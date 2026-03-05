//! Comprehensive plugin system tests.
//!
//! Covers:
//!  1. Plugin manifest parsing and validation
//!  2. Capability model (required/denied permissions)
//!  3. WASM plugin instantiation and sandbox behavior
//!  4. Native plugin ABI compatibility checking
//!  5. Plugin budget enforcement (time/memory limits)
//!  6. Code signing verification (Ed25519)
//!  7. Plugin lifecycle (load, init, process, shutdown)
//!  8. Error handling (malformed plugins, signature mismatch, timeout)
//!  9. Plugin isolation (one plugin can't affect another)

use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use openracing_crypto::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::ContentType;
use openracing_crypto::{SignatureMetadata, TrustLevel};

use racing_wheel_plugins::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::native::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginHost, SignatureVerificationConfig,
    SignatureVerifier, check_abi_compatibility,
};
use racing_wheel_plugins::quarantine::{
    FailureTracker, QuarantineManager, QuarantinePolicy, ViolationType,
};
use racing_wheel_plugins::registry::{
    PluginCatalog, PluginMetadata, VersionCompatibility, check_compatibility,
};
use racing_wheel_plugins::wasm::{ResourceLimits, WasmRuntime};
use racing_wheel_plugins::{PluginClass, PluginContext, PluginError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compile_wat(wat: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(wat)?)
}

fn make_manifest(class: PluginClass) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: "Comprehensive Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "Plugin for comprehensive tests".to_string(),
        author: "Test Author".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class,
        capabilities: vec![Capability::ReadTelemetry],
        operations: vec![PluginOperation::TelemetryProcessor],
        constraints: PluginConstraints {
            max_execution_time_us: 100,
            max_memory_bytes: 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

fn gen_keypair() -> Result<KeyPair, Box<dyn std::error::Error>> {
    Ok(KeyPair::generate()?)
}

fn sign_plugin_bytes(
    content: &[u8],
    keypair: &KeyPair,
    signer_name: &str,
) -> Result<SignatureMetadata, Box<dyn std::error::Error>> {
    Ok(Ed25519Signer::sign_with_metadata(
        content,
        keypair,
        signer_name,
        ContentType::Plugin,
        None,
    )?)
}

fn make_registry_metadata(name: &str, version: &str) -> PluginMetadata {
    PluginMetadata::new(
        name,
        semver::Version::parse(version)
            .ok()
            .unwrap_or_else(|| semver::Version::new(1, 0, 0)),
        "Test Author",
        format!("Description for {name}"),
        "MIT",
    )
}

// WAT modules
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

const DOUBLE_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 0
        f32.add
    )
)
"#;

const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

const INFINITE_LOOP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        (loop $lp
            br $lp
        )
        local.get 0
    )
)
"#;

const LIFECYCLE_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "init") (result i32)
        i32.const 0
    )
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
    (func (export "shutdown")
        nop
    )
)
"#;

const INIT_FAIL_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "init") (result i32)
        i32.const -1
    )
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

const NO_MEMORY_WAT: &str = r#"
(module
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

const NO_PROCESS_WAT: &str = r#"
(module
    (memory (export "memory") 1)
)
"#;

const CLAMP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; clamp input to [-1.0, 1.0]: returns max(-1.0, min(1.0, input))
        local.get 0
        f32.const 1.0
        f32.min
        f32.const -1.0
        f32.max
    )
)
"#;

// ===================================================================
// 1. Plugin manifest parsing and validation
// ===================================================================

#[test]
fn manifest_valid_safe_plugin_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let manifest = make_manifest(PluginClass::Safe);
    validator.validate(&manifest)
}

#[test]
fn manifest_valid_fast_plugin_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Fast);
    manifest.constraints.max_execution_time_us = 150;
    manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn manifest_empty_name_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.name = String::new();
    let result = validator.validate(&manifest);
    assert!(result.is_err());
}

#[test]
fn manifest_empty_author_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.author = String::new();
    let result = validator.validate(&manifest);
    assert!(result.is_err());
}

#[test]
fn manifest_safe_execution_time_over_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    // Safe max is 5000us
    manifest.constraints.max_execution_time_us = 6000;
    let result = validator.validate(&manifest);
    assert!(result.is_err());
}

#[test]
fn manifest_safe_memory_over_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    // Safe max is 16MB
    manifest.constraints.max_memory_bytes = 32 * 1024 * 1024;
    let result = validator.validate(&manifest);
    assert!(result.is_err());
}

#[test]
fn manifest_safe_update_rate_over_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    // Safe max is 200Hz
    manifest.constraints.update_rate_hz = 300;
    let result = validator.validate(&manifest);
    assert!(result.is_err());
}

#[test]
fn manifest_fast_execution_time_over_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Fast);
    // Fast max is 200us
    manifest.constraints.max_execution_time_us = 500;
    manifest.constraints.max_memory_bytes = 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    let result = validator.validate(&manifest);
    assert!(result.is_err());
}

#[test]
fn manifest_at_exact_safe_limits_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.constraints.max_execution_time_us = 5000;
    manifest.constraints.max_memory_bytes = 16 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 200;
    validator.validate(&manifest)
}

#[test]
fn manifest_at_exact_fast_limits_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Fast);
    manifest.constraints.max_execution_time_us = 200;
    manifest.constraints.max_memory_bytes = 4 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn manifest_yaml_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    let deserialized: PluginManifest = serde_yaml::from_str(&yaml)?;
    assert_eq!(deserialized.name, manifest.name);
    assert_eq!(deserialized.version, manifest.version);
    assert_eq!(deserialized.class, manifest.class);
    Ok(())
}

#[test]
fn manifest_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = make_manifest(PluginClass::Fast);
    let json = serde_json::to_string(&manifest)?;
    let deserialized: PluginManifest = serde_json::from_str(&json)?;
    assert_eq!(deserialized.name, manifest.name);
    assert_eq!(deserialized.author, manifest.author);
    assert_eq!(deserialized.capabilities, manifest.capabilities);
    Ok(())
}

#[test]
fn manifest_malformed_yaml_produces_error() {
    let bad_yaml = "this is: [not valid: yaml for a manifest";
    let result: Result<PluginManifest, _> = serde_yaml::from_str(bad_yaml);
    assert!(result.is_err());
}

// ===================================================================
// 2. Capability model (required/denied permissions)
// ===================================================================

#[test]
fn capability_read_telemetry_granted() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    checker.check_telemetry_read()
}

#[test]
fn capability_modify_telemetry_denied_when_not_granted() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.check_telemetry_modify().is_err());
}

#[test]
fn capability_dsp_denied_for_safe_plugin() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::ProcessDsp];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn capability_dsp_allowed_for_fast_plugin() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Fast);
    manifest.capabilities = vec![Capability::ProcessDsp];
    manifest.constraints.max_execution_time_us = 100;
    manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn capability_network_denied_for_safe() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::Network {
        hosts: vec!["example.com".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn capability_filesystem_denied_for_safe() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn capability_file_access_scoped_to_granted_paths() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/sandbox".to_string()],
    }]);
    checker.check_file_access(Path::new("/sandbox/data.bin"))?;
    assert!(checker.check_file_access(Path::new("/etc/shadow")).is_err());
    Ok(())
}

#[test]
fn capability_network_access_scoped_to_granted_hosts() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.example.com".to_string()],
    }]);
    checker.check_network_access("api.example.com")?;
    assert!(checker.check_network_access("evil.com").is_err());
    Ok(())
}

#[test]
fn capability_inter_plugin_comm_denied_by_default() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.check_inter_plugin_comm().is_err());
}

#[test]
fn capability_inter_plugin_comm_granted_when_present() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::InterPluginComm]);
    checker.check_inter_plugin_comm()
}

#[test]
fn capability_multiple_all_checked() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
    ]);
    checker.check_telemetry_read()?;
    checker.check_telemetry_modify()?;
    checker.check_led_control()?;
    assert!(checker.check_dsp_processing().is_err());
    Ok(())
}

#[test]
fn capability_enforcer_delegates_to_checker() -> Result<(), PluginError> {
    let enforcer =
        WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry, Capability::ProcessDsp]);
    let inner = enforcer.checker();
    inner.check_telemetry_read()?;
    inner.check_dsp_processing()?;
    assert!(inner.check_led_control().is_err());
    Ok(())
}

#[test]
fn capability_empty_grants_denies_everything() {
    let checker = CapabilityChecker::new(vec![]);
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
}

// ===================================================================
// 3. WASM plugin instantiation and sandbox behavior
// ===================================================================

#[test]
fn wasm_load_minimal_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 1);

    let output = runtime.process(&id, 0.5, 0.001)?;
    assert!((output - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wasm_load_lifecycle_plugin_with_init_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(LIFECYCLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);

    let output = runtime.process(&id, 1.0, 0.001)?;
    assert!((output - 1.0).abs() < f32::EPSILON);

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn wasm_invalid_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"not valid wasm", vec![]);
    assert!(result.is_err());
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn wasm_missing_memory_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(NO_MEMORY_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn wasm_missing_process_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(NO_PROCESS_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn wasm_init_failure_prevents_loading() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INIT_FAIL_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn wasm_resource_limits_configurable() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default()
        .with_fuel(5_000_000)
        .with_memory(8 * 1024 * 1024)
        .with_max_instances(4);
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_fuel, 5_000_000);
    assert_eq!(runtime.resource_limits().max_memory_bytes, 8 * 1024 * 1024);
    assert_eq!(runtime.resource_limits().max_instances, 4);
    Ok(())
}

#[test]
fn wasm_process_output_varies_by_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_WAT)?;
    let clamp_wasm = compile_wat(CLAMP_WAT)?;

    let mut runtime = WasmRuntime::new()?;
    let pass_id = Uuid::new_v4();
    let double_id = Uuid::new_v4();
    let clamp_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;
    runtime.load_plugin_from_bytes(double_id, &double_wasm, vec![])?;
    runtime.load_plugin_from_bytes(clamp_id, &clamp_wasm, vec![])?;

    let pass_out = runtime.process(&pass_id, 3.0, 0.001)?;
    let double_out = runtime.process(&double_id, 3.0, 0.001)?;
    let clamp_out = runtime.process(&clamp_id, 3.0, 0.001)?;

    assert!((pass_out - 3.0).abs() < f32::EPSILON, "passthrough");
    assert!((double_out - 6.0).abs() < f32::EPSILON, "doubler");
    assert!((clamp_out - 1.0).abs() < f32::EPSILON, "clamp to 1.0");
    Ok(())
}

// ===================================================================
// 4. Native plugin ABI compatibility checking
// ===================================================================

#[test]
fn abi_current_version_is_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_version_zero_incompatible() {
    if CURRENT_ABI_VERSION != 0 {
        let result = check_abi_compatibility(0);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn abi_future_version_incompatible() {
    let future = CURRENT_ABI_VERSION + 1;
    let result = check_abi_compatibility(future);
    assert!(matches!(
        result,
        AbiCheckResult::Mismatch {
            expected: _,
            actual: _
        }
    ));
}

#[test]
fn abi_very_large_version_incompatible() {
    let result = check_abi_compatibility(u32::MAX);
    assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
}

#[test]
fn native_host_permissive_config_allows_unsigned() {
    let host = NativePluginHost::new_permissive_for_development();
    // Should construct without error - permissive mode for dev
    drop(host);
}

#[test]
fn native_signature_config_strict_defaults() {
    let config = SignatureVerificationConfig::strict();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn native_signature_config_development_mode() {
    let config = SignatureVerificationConfig::development();
    assert!(!config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn native_signature_config_permissive() {
    let config = SignatureVerificationConfig::permissive();
    assert!(config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn native_signature_verifier_unsigned_plugin_rejected_strict()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("unsigned.dll");
    std::fs::write(&plugin_path, b"fake plugin binary")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);

    // Strict mode returns an error for unsigned plugins
    let result = verifier.verify(&plugin_path);
    assert!(result.is_err(), "strict mode must reject unsigned plugins");
    Ok(())
}

#[test]
fn native_signature_verifier_unsigned_allowed_in_dev() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("unsigned.dll");
    std::fs::write(&plugin_path, b"fake plugin binary")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::development();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path)?;
    assert!(result.verified, "dev mode must allow unsigned plugins");
    Ok(())
}

// ===================================================================
// 5. Plugin budget enforcement (time/memory limits)
// ===================================================================

#[test]
fn budget_fuel_exhaustion_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "infinite loop must be terminated");
    assert!(
        runtime.is_plugin_disabled(&id)?,
        "plugin must be disabled after fuel exhaustion"
    );
    Ok(())
}

#[test]
fn budget_max_instances_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

    let result = runtime.load_plugin_from_bytes(id3, &wasm, vec![]);
    assert!(result.is_err(), "third instance must be rejected (limit 2)");
    assert_eq!(runtime.instance_count(), 2);
    Ok(())
}

#[test]
fn budget_quarantine_after_repeated_violations() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_budget_violations: 3,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    for i in 0..2 {
        manager.record_violation(id, ViolationType::BudgetViolation, format!("v{i}"))?;
        assert!(!manager.is_quarantined(id), "not quarantined at {i}");
    }

    manager.record_violation(id, ViolationType::BudgetViolation, "v2".to_string())?;
    assert!(
        manager.is_quarantined(id),
        "must be quarantined after 3 violations"
    );
    Ok(())
}

#[test]
fn budget_crash_violations_trigger_quarantine() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 2,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::Crash, "crash 1".to_string())?;
    assert!(!manager.is_quarantined(id));

    manager.record_violation(id, ViolationType::Crash, "crash 2".to_string())?;
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn budget_capability_violation_does_not_quarantine_as_crash() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 2,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    // Capability violations don't count as crashes
    for _ in 0..5 {
        manager.record_violation(
            id,
            ViolationType::CapabilityViolation,
            "cap violation".to_string(),
        )?;
    }
    assert!(!manager.is_quarantined(id));

    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    Ok(())
}

#[test]
fn budget_failure_tracker_records_stats() {
    let mut tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    tracker.record_execution(id, 100, true);
    tracker.record_execution(id, 200, true);
    tracker.record_execution(id, 500, false);

    let stats = tracker.get_stats(id);
    assert!(stats.is_some());
    if let Some(s) = stats {
        assert_eq!(s.executions, 3);
        assert_eq!(s.max_time_us, 500);
        assert_eq!(s.crashes, 1);
    }
}

#[test]
fn budget_disabled_plugin_cannot_process() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &trap_wasm, vec![])?;

    // First call traps and disables
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Second call must fail with Crashed error
    let second_result = runtime.process(&id, 1.0, 0.001);
    assert!(second_result.is_err());
    Ok(())
}

// ===================================================================
// 6. Code signing verification (Ed25519)
// ===================================================================

#[test]
fn signing_roundtrip_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"plugin binary content";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(data, &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn signing_tampered_data_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"original content";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut tampered = data.to_vec();
    tampered[0] ^= 0x01;
    assert!(!Ed25519Verifier::verify(&tampered, &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn signing_wrong_key_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;
    let data = b"signed by kp1";
    let sig = Ed25519Signer::sign(data, &kp1.signing_key)?;
    assert!(!Ed25519Verifier::verify(data, &sig, &kp2.public_key)?);
    Ok(())
}

#[test]
fn signing_unique_keys_generated() -> Result<(), Box<dyn std::error::Error>> {
    let mut fingerprints = std::collections::HashSet::new();
    for _ in 0..5 {
        let kp = gen_keypair()?;
        assert!(fingerprints.insert(kp.fingerprint()), "keys must be unique");
    }
    Ok(())
}

#[test]
fn signing_detached_signature_file_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("test_plugin.wasm");
    let content = b"wasm plugin binary";
    std::fs::write(&plugin_path, content)?;

    let kp = gen_keypair()?;
    let meta = sign_plugin_bytes(content, &kp, "Test Signer")?;
    openracing_crypto::utils::create_detached_signature(&plugin_path, &meta)?;

    let sig_path = openracing_crypto::utils::get_signature_path(&plugin_path);
    assert!(sig_path.exists(), "signature file must be created");

    let extracted = openracing_crypto::utils::extract_signature_metadata(&plugin_path)?;
    assert!(extracted.is_some());
    if let Some(extracted_meta) = extracted {
        assert_eq!(extracted_meta.signer, "Test Signer");
    }
    Ok(())
}

#[test]
fn signing_trust_store_integration() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;

    store.add_key(
        kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("Test key".to_string()),
    )?;

    let trust = store.get_trust_level(&kp.fingerprint());
    assert_eq!(trust, TrustLevel::Trusted);
    Ok(())
}

#[test]
fn signing_unknown_key_not_trusted() -> Result<(), Box<dyn std::error::Error>> {
    let store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    let trust = store.get_trust_level(&kp.fingerprint());
    assert_eq!(trust, TrustLevel::Unknown);
    Ok(())
}

#[test]
fn signing_empty_payload_can_be_signed() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"", &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(b"", &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn signing_large_payload_can_be_signed() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = vec![0xAB; 1024 * 1024]; // 1MB
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(&data, &sig, &kp.public_key)?);
    Ok(())
}

// ===================================================================
// 7. Plugin lifecycle (load, init, process, shutdown)
// ===================================================================

#[test]
fn lifecycle_load_process_unload() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load
    runtime.load_plugin_from_bytes(id, &wasm, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));

    // Process
    let out = runtime.process(&id, 0.42, 0.001)?;
    assert!((out - 0.42).abs() < f32::EPSILON);

    // Unload
    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn lifecycle_init_shutdown_called() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(LIFECYCLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);

    // Process several times
    for i in 0..5 {
        let out = runtime.process(&id, i as f32, 0.001)?;
        assert!((out - i as f32).abs() < f32::EPSILON);
    }

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 5);

    // Shutdown via unload
    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn lifecycle_hot_reload_preserves_state() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load passthrough, process some calls
    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![Capability::ReadTelemetry])?;
    for _ in 0..3 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }
    let (count_before, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_before, 3);

    // Hot-reload with doubler
    runtime.reload_plugin(&id, &double_wasm, vec![Capability::ReadTelemetry])?;

    // New behavior (doubler)
    let out = runtime.process(&id, 5.0, 0.001)?;
    assert!((out - 10.0).abs() < f32::EPSILON, "doubler after reload");

    // Stats preserved from before reload
    let (count_after, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_after, 4, "stats include pre-reload count + 1");
    Ok(())
}

#[test]
fn lifecycle_unload_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.unload_plugin(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn lifecycle_process_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn lifecycle_re_enable_after_crash() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &trap_wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Re-enable
    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled, "should have been disabled");
    assert!(!runtime.is_plugin_disabled(&id)?);

    // Will trap again since code hasn't changed
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn lifecycle_quarantine_manual_and_release() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.manual_quarantine(id, 30)?;
    assert!(manager.is_quarantined(id));

    manager.release_from_quarantine(id)?;
    assert!(!manager.is_quarantined(id));
    Ok(())
}

#[test]
fn lifecycle_release_unknown_plugin_fails() {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let result = manager.release_from_quarantine(Uuid::new_v4());
    assert!(result.is_err());
}

// ===================================================================
// 8. Error handling (malformed plugins, signature mismatch, timeout)
// ===================================================================

#[test]
fn error_malformed_wasm_produces_loading_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"\x00\x61\x73\x6d\xff\xff\xff\xff", vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn error_truncated_wasm_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Truncate to half length
    let truncated = &wasm[..wasm.len() / 2];
    let result = runtime.load_plugin_from_bytes(id, truncated, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn error_empty_wasm_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"", vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn error_trap_produces_crashed_error() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &trap_wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());

    // Verify it's the right kind of error
    if let Err(PluginError::Crashed { reason }) = result {
        assert!(!reason.is_empty());
    } else if let Err(PluginError::BudgetViolation { .. }) = result {
        // Also acceptable - fuel exhaustion
    } else {
        panic!("expected Crashed or BudgetViolation error");
    }
    Ok(())
}

#[test]
fn error_fuel_exhaustion_type() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(500);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn error_signature_mismatch_detected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"original";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    // Verify with wrong data
    assert!(!Ed25519Verifier::verify(b"tampered", &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn error_double_load_same_id_overwrites() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![])?;
    // Loading same ID again with different module
    runtime.load_plugin_from_bytes(id, &double_wasm, vec![])?;

    let out = runtime.process(&id, 3.0, 0.001)?;
    assert!((out - 6.0).abs() < f32::EPSILON, "second load took effect");
    assert_eq!(runtime.instance_count(), 1);
    Ok(())
}

#[test]
fn error_reload_with_invalid_wasm_keeps_old() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![])?;

    // Reload with garbage should fail, keeping old plugin
    let result = runtime.reload_plugin(&id, b"garbage bytes", vec![]);
    assert!(result.is_err());

    // Old plugin still works
    let out = runtime.process(&id, 2.0, 0.001)?;
    assert!((out - 2.0).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// 9. Plugin isolation (one plugin can't affect another)
// ===================================================================

#[test]
fn isolation_crash_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let trap_id = Uuid::new_v4();
    let pass_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(trap_id, &trap_wasm, vec![])?;
    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;

    // Crash the trap plugin
    let _ = runtime.process(&trap_id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&trap_id)?);

    // Healthy plugin unaffected
    let output = runtime.process(&pass_id, 7.5, 0.001)?;
    assert!((output - 7.5).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&pass_id)?);
    Ok(())
}

#[test]
fn isolation_fuel_exhaustion_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let loop_wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let loop_id = Uuid::new_v4();
    let pass_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(loop_id, &loop_wasm, vec![])?;
    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;

    // Exhaust fuel on loop plugin
    let _ = runtime.process(&loop_id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&loop_id)?);

    // Healthy plugin still works (gets its own fuel)
    let output = runtime.process(&pass_id, 2.5, 0.001)?;
    assert!((output - 2.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn isolation_independent_capabilities() -> Result<(), PluginError> {
    let checker_a = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    let checker_b = CapabilityChecker::new(vec![Capability::ControlLeds]);

    // A can read telemetry, B cannot
    assert!(checker_a.check_telemetry_read().is_ok());
    assert!(checker_b.check_telemetry_read().is_err());

    // B can control LEDs, A cannot
    assert!(checker_b.check_led_control().is_ok());
    assert!(checker_a.check_led_control().is_err());
    Ok(())
}

#[test]
fn isolation_multiple_plugins_process_independently() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_WAT)?;
    let clamp_wasm = compile_wat(CLAMP_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();

    runtime.load_plugin_from_bytes(ids[0], &pass_wasm, vec![])?;
    runtime.load_plugin_from_bytes(ids[1], &double_wasm, vec![])?;
    runtime.load_plugin_from_bytes(ids[2], &clamp_wasm, vec![])?;

    // Process in interleaved order to ensure no state leaks
    let p1 = runtime.process(&ids[0], 1.0, 0.001)?;
    let d1 = runtime.process(&ids[1], 1.0, 0.001)?;
    let c1 = runtime.process(&ids[2], 5.0, 0.001)?;
    let p2 = runtime.process(&ids[0], 2.0, 0.001)?;
    let d2 = runtime.process(&ids[1], 2.0, 0.001)?;

    assert!((p1 - 1.0).abs() < f32::EPSILON);
    assert!((d1 - 2.0).abs() < f32::EPSILON);
    assert!((c1 - 1.0).abs() < f32::EPSILON);
    assert!((p2 - 2.0).abs() < f32::EPSILON);
    assert!((d2 - 4.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn isolation_unload_one_leaves_others() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id3, &wasm, vec![])?;
    assert_eq!(runtime.instance_count(), 3);

    runtime.unload_plugin(&id2)?;
    assert_eq!(runtime.instance_count(), 2);
    assert!(runtime.has_plugin(&id1));
    assert!(!runtime.has_plugin(&id2));
    assert!(runtime.has_plugin(&id3));

    // Remaining plugins still work
    let out1 = runtime.process(&id1, 1.0, 0.001)?;
    let out3 = runtime.process(&id3, 3.0, 0.001)?;
    assert!((out1 - 1.0).abs() < f32::EPSILON);
    assert!((out3 - 3.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn isolation_quarantine_one_plugin_not_others() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        ..QuarantinePolicy::default()
    });
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    manager.record_violation(id_a, ViolationType::Crash, "crash".to_string())?;
    assert!(manager.is_quarantined(id_a));
    assert!(!manager.is_quarantined(id_b));
    Ok(())
}

// ===================================================================
// Bonus: Registry and version compatibility
// ===================================================================

#[test]
fn registry_add_search_remove() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("TestFFB", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;
    assert_eq!(catalog.plugin_count(), 1);

    let results = catalog.search("TestFFB");
    assert_eq!(results.len(), 1);

    assert!(catalog.remove_plugin(&id, None));
    assert_eq!(catalog.plugin_count(), 0);
    Ok(())
}

#[test]
fn registry_version_compat_same_major() {
    let req = semver::Version::new(1, 0, 0);
    let avail = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&req, &avail),
        VersionCompatibility::Compatible
    );
}

#[test]
fn registry_version_compat_different_major() {
    let req = semver::Version::new(1, 0, 0);
    let avail = semver::Version::new(2, 0, 0);
    assert_eq!(
        check_compatibility(&req, &avail),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn registry_catalog_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("PlugA", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("PlugB", "2.0.0"))?;

    let json = serde_json::to_string(&catalog)?;
    let restored: PluginCatalog = serde_json::from_str(&json)?;
    assert_eq!(restored.plugin_count(), 2);
    Ok(())
}

#[test]
fn plugin_class_context_coexistence() {
    let safe_ctx = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Safe,
        update_rate_hz: 60,
        budget_us: 5000,
        capabilities: vec!["ReadTelemetry".to_string()],
    };
    let fast_ctx = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Fast,
        update_rate_hz: 1000,
        budget_us: 100,
        capabilities: vec!["ProcessDsp".to_string()],
    };

    assert_ne!(safe_ctx.class, fast_ctx.class);
    assert!(safe_ctx.budget_us > fast_ctx.budget_us);
    assert!(fast_ctx.update_rate_hz > safe_ctx.update_rate_hz);
}
