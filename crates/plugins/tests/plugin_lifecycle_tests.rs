//! Deep plugin lifecycle tests.
//!
//! Covers 15 test scenarios:
//!  1. WASM plugin loading and unloading lifecycle
//!  2. WASM capability-based permission system
//!  3. WASM plugin crash recovery
//!  4. WASM plugin resource budgets (memory, CPU time)
//!  5. Native plugin Ed25519 signature verification
//!  6. Native plugin isolation boundaries
//!  7. Plugin discovery and enumeration
//!  8. Plugin manifest parsing and validation
//!  9. Plugin version compatibility checking
//! 10. Multiple plugins loaded simultaneously
//! 11. Plugin priority and conflict resolution
//! 12. Plugin hot-reload
//! 13. Plugin communication channels
//! 14. Plugin error propagation
//! 15. Plugin sandboxing boundary tests

use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use openracing_crypto::TrustLevel;
use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::ContentType;

use racing_wheel_plugins::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use racing_wheel_plugins::host::PluginHost;
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::native::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginConfig, NativePluginHost,
    SignatureVerificationConfig, SignatureVerifier, check_abi_compatibility,
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

fn make_manifest(class: PluginClass) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
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

fn compile_wat(wat: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(wat)?)
}

/// WAT: minimal passthrough plugin.
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT: plugin that traps on process (simulates crash).
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// WAT: plugin with init + shutdown + process (lifecycle).
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

/// WAT: infinite loop to exhaust fuel budget.
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

/// WAT: plugin that doubles input (for hot-reload differentiation).
const DOUBLE_OUTPUT_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 0
        f32.add
    )
)
"#;

/// WAT: plugin that halves input (for multi-plugin / priority tests).
const HALF_OUTPUT_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        f32.const 0.5
        f32.mul
    )
)
"#;

/// WAT: plugin requesting a large linear memory (8 pages = 512 KiB).
const LARGE_MEMORY_WAT: &str = r#"
(module
    (memory (export "memory") 8)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT: missing the required "process" export.
const NO_PROCESS_WAT: &str = r#"
(module
    (memory (export "memory") 1)
)
"#;

/// WAT: missing the required "memory" export.
const NO_MEMORY_WAT: &str = r#"
(module
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

// ===================================================================
// 1. WASM plugin loading and unloading lifecycle
// ===================================================================

#[test]
fn lifecycle_load_process_unload() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));

    let output = runtime.process(&id, 0.42, 0.001)?;
    assert!(
        (output - 0.42).abs() < f32::EPSILON,
        "passthrough should return input unchanged"
    );

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn lifecycle_full_with_init_and_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(LIFECYCLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![Capability::ReadTelemetry])?;
    assert!(runtime.is_plugin_initialized(&id)?);

    let output = runtime.process(&id, 7.5, 0.001)?;
    assert!((output - 7.5).abs() < f32::EPSILON);

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 1);

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn lifecycle_process_after_unload_fails() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    runtime.unload_plugin(&id)?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "process after unload must fail");
    Ok(())
}

#[test]
fn lifecycle_unload_nonexistent_plugin_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.unload_plugin(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn lifecycle_stats_accumulate_across_calls() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..10 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }

    let (count, avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 10);
    assert!(avg >= 0.0, "average time must be non-negative");
    Ok(())
}

// ===================================================================
// 2. WASM capability-based permission system
// ===================================================================

#[test]
fn capability_read_telemetry_only_denies_modify() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    checker.check_telemetry_read()?;
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    Ok(())
}

#[test]
fn capability_empty_grants_denies_all() {
    let checker = CapabilityChecker::new(vec![]);

    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    assert!(checker.check_file_access(Path::new("/tmp")).is_err());
    assert!(checker.check_network_access("any.host").is_err());
}

#[test]
fn capability_filesystem_only_allows_granted_paths() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/sandbox/data".to_string(), "/tmp/plugins".to_string()],
    }]);

    checker.check_file_access(Path::new("/sandbox/data/config.json"))?;
    checker.check_file_access(Path::new("/tmp/plugins/state.bin"))?;
    assert!(checker.check_file_access(Path::new("/etc/passwd")).is_err());
    assert!(checker.check_file_access(Path::new("/home/user")).is_err());
    Ok(())
}

#[test]
fn capability_network_only_allows_granted_hosts() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.openracing.io".to_string()],
    }]);

    checker.check_network_access("api.openracing.io")?;
    assert!(checker.check_network_access("evil.com").is_err());
    assert!(checker.check_network_access("localhost").is_err());
    Ok(())
}

#[test]
fn capability_inter_plugin_comm_requires_explicit_grant() -> Result<(), PluginError> {
    let without = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(without.check_inter_plugin_comm().is_err());

    let with = CapabilityChecker::new(vec![Capability::InterPluginComm]);
    with.check_inter_plugin_comm()?;
    Ok(())
}

#[test]
fn capability_enforcer_delegates_correctly() -> Result<(), PluginError> {
    let enforcer =
        WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);
    let checker = enforcer.checker();

    checker.check_telemetry_read()?;
    checker.check_led_control()?;
    assert!(checker.check_dsp_processing().is_err());
    Ok(())
}

#[test]
fn capability_error_contains_violated_capability_name() {
    let checker = CapabilityChecker::new(vec![]);

    let err = checker.check_telemetry_read();
    assert!(err.is_err());
    let msg = format!("{}", err.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("ReadTelemetry"));

    let err = checker.check_file_access(Path::new("/secret"));
    assert!(err.is_err());
    let msg = format!("{}", err.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("/secret"));
}

// ===================================================================
// 3. WASM plugin crash recovery
// ===================================================================

#[test]
fn crash_trapping_plugin_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "trapping plugin must return error");
    Ok(())
}

#[test]
fn crash_disables_trapping_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);

    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn crash_disabled_plugin_info_available() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);

    let info = runtime.get_plugin_disabled_info(&id)?;
    assert!(info.is_some(), "disabled info should be available");
    let info = info.unwrap_or_else(|| unreachable!());
    assert!(!info.reason.is_empty(), "reason should not be empty");
    Ok(())
}

#[test]
fn crash_reenable_allows_retry() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn crash_does_not_affect_sibling_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let bad_wasm = compile_wat(TRAP_WAT)?;
    let good_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let bad_id = Uuid::new_v4();
    let good_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(bad_id, &bad_wasm, vec![])?;
    runtime.load_plugin_from_bytes(good_id, &good_wasm, vec![])?;

    let _ = runtime.process(&bad_id, 1.0, 0.001);

    // Good plugin must still work
    let output = runtime.process(&good_id, 42.0, 0.001)?;
    assert!((output - 42.0).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&good_id)?);
    assert!(runtime.is_plugin_disabled(&bad_id)?);
    Ok(())
}

#[test]
fn crash_quarantine_after_repeated_crashes() -> Result<(), PluginError> {
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

// ===================================================================
// 4. WASM plugin resource budgets (memory, CPU time)
// ===================================================================

#[test]
fn budget_fuel_exhaustion_terminates_infinite_loop() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "infinite-loop plugin must be terminated");

    let disabled = runtime.is_plugin_disabled(&id)?;
    assert!(disabled, "plugin must be disabled after fuel exhaustion");
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
    assert!(result.is_err(), "third instance should exceed limit");
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("Maximum plugin instances"));
    Ok(())
}

#[test]
fn budget_custom_fuel_limit_applied() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_fuel(5_000_000);
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_fuel, 5_000_000);
    Ok(())
}

#[test]
fn budget_custom_memory_limit_applied() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_memory(4 * 1024 * 1024);
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_memory_bytes, 4 * 1024 * 1024);
    Ok(())
}

#[test]
fn budget_defaults_are_sensible() {
    let limits = ResourceLimits::default();
    assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 10_000_000);
    assert_eq!(limits.max_table_elements, 10_000);
    assert_eq!(limits.max_instances, 32);
}

#[test]
fn budget_violation_tracking_in_quarantine() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_budget_violations: 3,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    for i in 0..2 {
        manager.record_violation(
            id,
            ViolationType::BudgetViolation,
            format!("budget violation {i}"),
        )?;
        assert!(!manager.is_quarantined(id));
    }

    manager.record_violation(
        id,
        ViolationType::BudgetViolation,
        "budget violation 2".to_string(),
    )?;
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn budget_large_memory_plugin_loads_within_default_limits() -> Result<(), Box<dyn std::error::Error>>
{
    let wasm = compile_wat(LARGE_MEMORY_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, 3.125, 0.001)?;
    assert!((output - 3.125).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// 5. Native plugin Ed25519 signature verification
// ===================================================================

#[test]
fn native_sig_strict_rejects_unsigned() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("unsigned_plugin.dll");
    std::fs::write(&plugin_path, b"fake DLL content")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path);
    assert!(
        result.is_err(),
        "unsigned plugin must be rejected in strict mode"
    );
    Ok(())
}

#[test]
fn native_sig_permissive_allows_unsigned() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("dev_plugin.dll");
    std::fs::write(&plugin_path, b"dev plugin content")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::permissive();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path)?;
    assert!(!result.is_signed);
    assert!(result.verified, "permissive must allow unsigned plugins");
    Ok(())
}

#[test]
fn native_sig_tampered_content_detected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("tampered.dll");
    let original_content = b"original DLL content";
    std::fs::write(&plugin_path, original_content)?;

    let keypair = KeyPair::generate()?;
    let metadata = Ed25519Signer::sign_with_metadata(
        original_content,
        &keypair,
        "Test Signer",
        ContentType::Plugin,
        None,
    )?;

    let sig_path = plugin_path.with_extension("dll.sig");
    let sig_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&sig_path, &sig_json)?;

    // Tamper with content
    std::fs::write(&plugin_path, b"TAMPERED content")?;

    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(
        keypair.public_key.clone(),
        TrustLevel::Trusted,
        Some("test key".to_string()),
    )?;

    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path);
    assert!(result.is_err(), "tampered content must fail verification");
    Ok(())
}

#[test]
fn native_sig_config_modes_differ() {
    let strict = NativePluginConfig::strict();
    assert!(!strict.allow_unsigned);
    assert!(strict.require_signatures);

    let dev = NativePluginConfig::development();
    assert!(dev.allow_unsigned);
    assert!(!dev.require_signatures);

    let default = NativePluginConfig::default();
    assert!(!default.allow_unsigned);
    assert!(default.require_signatures);
}

#[test]
fn native_sig_abi_current_version_compatible() {
    assert!(matches!(
        check_abi_compatibility(CURRENT_ABI_VERSION),
        AbiCheckResult::Compatible
    ));
}

#[test]
fn native_sig_abi_wrong_version_mismatch() {
    let wrong = CURRENT_ABI_VERSION.wrapping_add(1);
    match check_abi_compatibility(wrong) {
        AbiCheckResult::Mismatch { expected, actual } => {
            assert_eq!(expected, CURRENT_ABI_VERSION);
            assert_eq!(actual, wrong);
        }
        AbiCheckResult::Compatible => panic!("expected Mismatch for wrong ABI version"),
    }
}

// ===================================================================
// 6. Native plugin isolation boundaries
// ===================================================================

#[tokio::test]
async fn native_isolation_strict_rejects_unsigned_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_dir = temp.path().join("unsigned-native");
    tokio::fs::create_dir_all(&plugin_dir).await?;

    let lib_name = if cfg!(windows) {
        "plugin.dll"
    } else if cfg!(target_os = "macos") {
        "plugin.dylib"
    } else {
        "plugin.so"
    };

    tokio::fs::write(plugin_dir.join(lib_name), b"fake binary").await?;

    let host = NativePluginHost::new_with_defaults();
    let result = host
        .load_plugin(
            Uuid::new_v4(),
            "unsigned-test".to_string(),
            &plugin_dir.join(lib_name),
            1000,
        )
        .await;

    assert!(
        result.is_err(),
        "strict config must reject unsigned native plugin"
    );
    Ok(())
}

#[test]
fn native_isolation_abi_version_zero_rejected() {
    let result = check_abi_compatibility(0);
    assert!(
        matches!(result, AbiCheckResult::Mismatch { .. }),
        "ABI version 0 must be rejected"
    );
}

#[test]
fn native_isolation_abi_far_future_rejected() {
    let result = check_abi_compatibility(999);
    assert!(
        matches!(result, AbiCheckResult::Mismatch { expected, actual } if expected == CURRENT_ABI_VERSION && actual == 999),
        "far-future ABI version must be rejected"
    );
}

// ===================================================================
// 7. Plugin discovery and enumeration
// ===================================================================

#[tokio::test]
async fn discovery_empty_directory_yields_zero_plugins() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let host = PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert_eq!(registry.len(), 0);
    Ok(())
}

#[tokio::test]
async fn discovery_finds_plugin_with_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_dir = temp.path().join("test-plugin");
    tokio::fs::create_dir_all(&plugin_dir).await?;

    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    tokio::fs::write(plugin_dir.join("plugin.yaml"), &yaml).await?;
    tokio::fs::write(plugin_dir.join("plugin.wasm"), b"mock wasm").await?;

    let host = PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert_eq!(registry.len(), 1);

    let (_, entry) = registry.iter().next().unwrap_or_else(|| unreachable!());
    assert_eq!(entry.manifest.name, "Test Plugin");
    assert!(!entry.is_loaded);
    assert!(entry.is_enabled);
    Ok(())
}

#[tokio::test]
async fn discovery_skips_invalid_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;

    // Valid plugin
    let valid_dir = temp.path().join("valid");
    tokio::fs::create_dir_all(&valid_dir).await?;
    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    tokio::fs::write(valid_dir.join("plugin.yaml"), &yaml).await?;
    tokio::fs::write(valid_dir.join("plugin.wasm"), b"mock").await?;

    // Invalid plugin (bad YAML)
    let invalid_dir = temp.path().join("invalid");
    tokio::fs::create_dir_all(&invalid_dir).await?;
    tokio::fs::write(invalid_dir.join("plugin.yaml"), "invalid: [[[yaml").await?;

    let host = PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    // Only valid plugin discovered
    assert_eq!(registry.len(), 1);
    Ok(())
}

#[tokio::test]
async fn discovery_catalog_search_finds_by_name() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("FFB Filter", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("LED Controller", "2.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Telemetry Logger", "1.5.0"))?;

    let results = catalog.search("ffb");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "FFB Filter");

    let results = catalog.search("controller");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "LED Controller");
    Ok(())
}

#[test]
fn discovery_catalog_list_all_sorted() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("Zebra", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Alpha", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Middle", "1.0.0"))?;

    let all = catalog.list_all();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].name, "Alpha");
    assert_eq!(all[1].name, "Middle");
    assert_eq!(all[2].name, "Zebra");
    Ok(())
}

// ===================================================================
// 8. Plugin manifest parsing and validation
// ===================================================================

#[test]
fn manifest_valid_safe_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    validator.validate(&make_manifest(PluginClass::Safe))
}

#[test]
fn manifest_valid_fast_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn manifest_empty_name_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.name = String::new();
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_empty_author_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.author = String::new();
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_cannot_have_process_dsp() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::ProcessDsp];
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_cannot_have_filesystem() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_cannot_have_network() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::Network {
        hosts: vec!["evil.com".to_string()],
    }];
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_exceeding_execution_time_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_execution_time_us = 5001; // Safe max is 5000
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_exceeding_memory_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_memory_bytes = 16 * 1024 * 1024 + 1; // Safe max is 16MB
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_exceeding_update_rate_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.update_rate_hz = 201; // Safe max is 200
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_at_exact_limits_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_execution_time_us = 5000;
    m.constraints.max_memory_bytes = 16 * 1024 * 1024;
    m.constraints.update_rate_hz = 200;
    validator.validate(&m)
}

#[test]
fn manifest_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = make_manifest(PluginClass::Safe);
    let json = serde_json::to_string(&manifest)?;
    let restored: PluginManifest = serde_json::from_str(&json)?;
    assert_eq!(restored.id, manifest.id);
    assert_eq!(restored.name, manifest.name);
    assert_eq!(restored.version, manifest.version);
    assert_eq!(restored.class, manifest.class);
    assert_eq!(restored.capabilities, manifest.capabilities);
    Ok(())
}

#[tokio::test]
async fn manifest_yaml_file_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    let path = temp.path().join("plugin.yaml");
    tokio::fs::write(&path, &yaml).await?;

    let loaded = racing_wheel_plugins::manifest::load_manifest(&path).await?;
    assert_eq!(loaded.id, manifest.id);
    assert_eq!(loaded.name, manifest.name);
    Ok(())
}

#[tokio::test]
async fn manifest_load_nonexistent_file_errors() {
    let result =
        racing_wheel_plugins::manifest::load_manifest(Path::new("nonexistent/plugin.yaml")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn manifest_load_malformed_yaml_errors() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("plugin.yaml");
    tokio::fs::write(&path, "not: valid: yaml: [[[").await?;
    assert!(
        racing_wheel_plugins::manifest::load_manifest(&path)
            .await
            .is_err()
    );
    Ok(())
}

// ===================================================================
// 9. Plugin version compatibility checking
// ===================================================================

#[test]
fn version_same_major_higher_minor_compatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 0, 0),
            &semver::Version::new(1, 3, 0)
        ),
        VersionCompatibility::Compatible
    );
}

#[test]
fn version_same_major_same_minor_higher_patch_compatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 2, 0),
            &semver::Version::new(1, 2, 5)
        ),
        VersionCompatibility::Compatible
    );
}

#[test]
fn version_different_major_incompatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 0, 0),
            &semver::Version::new(2, 0, 0)
        ),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn version_lower_available_incompatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 5, 0),
            &semver::Version::new(1, 2, 0)
        ),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn version_prerelease_exact_match_required() -> Result<(), Box<dyn std::error::Error>> {
    let alpha = semver::Version::parse("1.0.0-alpha")?;
    let beta = semver::Version::parse("1.0.0-beta")?;

    assert_eq!(
        check_compatibility(&alpha, &alpha),
        VersionCompatibility::Compatible
    );
    assert_eq!(
        check_compatibility(&alpha, &beta),
        VersionCompatibility::Incompatible
    );
    Ok(())
}

#[test]
fn version_zero_major_requires_exact_minor() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(0, 1, 0),
            &semver::Version::new(0, 1, 5)
        ),
        VersionCompatibility::Compatible
    );
    assert_eq!(
        check_compatibility(
            &semver::Version::new(0, 1, 0),
            &semver::Version::new(0, 2, 0)
        ),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn version_catalog_find_compatible() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v1_2 = make_registry_metadata("Plugin", "1.2.0");
    v1_2.id = id.clone();
    catalog.add_plugin(v1_2)?;

    let mut v2 = make_registry_metadata("Plugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    // Requiring 1.0.0 should find 1.2.0 (highest compatible in major 1)
    let compat = catalog.find_compatible_version(&id, &semver::Version::new(1, 0, 0));
    assert!(compat.is_some());
    assert_eq!(
        compat.map(|m| m.version.to_string()),
        Some("1.2.0".to_string())
    );
    Ok(())
}

#[test]
fn version_catalog_no_compatible_returns_none() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    // Requiring 2.0.0 when only 1.0.0 exists
    let compat = catalog.find_compatible_version(&id, &semver::Version::new(2, 0, 0));
    assert!(compat.is_none());
    Ok(())
}

// ===================================================================
// 10. Multiple plugins loaded simultaneously
// ===================================================================

#[test]
fn multi_load_and_process_independently() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let half_wasm = compile_wat(HALF_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let pass_id = Uuid::new_v4();
    let double_id = Uuid::new_v4();
    let half_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;
    runtime.load_plugin_from_bytes(double_id, &double_wasm, vec![])?;
    runtime.load_plugin_from_bytes(half_id, &half_wasm, vec![])?;

    assert_eq!(runtime.instance_count(), 3);

    let pass_out = runtime.process(&pass_id, 10.0, 0.001)?;
    let double_out = runtime.process(&double_id, 10.0, 0.001)?;
    let half_out = runtime.process(&half_id, 10.0, 0.001)?;

    assert!((pass_out - 10.0).abs() < f32::EPSILON);
    assert!((double_out - 20.0).abs() < f32::EPSILON);
    assert!((half_out - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn multi_unload_one_others_remain() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id3, &wasm, vec![])?;

    runtime.unload_plugin(&id2)?;
    assert_eq!(runtime.instance_count(), 2);
    assert!(runtime.has_plugin(&id1));
    assert!(!runtime.has_plugin(&id2));
    assert!(runtime.has_plugin(&id3));

    // Remaining plugins still work
    let o1 = runtime.process(&id1, 1.0, 0.001)?;
    let o3 = runtime.process(&id3, 3.0, 0.001)?;
    assert!((o1 - 1.0).abs() < f32::EPSILON);
    assert!((o3 - 3.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn multi_crash_one_others_unaffected() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let good1 = Uuid::new_v4();
    let bad = Uuid::new_v4();
    let good2 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(good1, &pass_wasm, vec![])?;
    runtime.load_plugin_from_bytes(bad, &trap_wasm, vec![])?;
    runtime.load_plugin_from_bytes(good2, &pass_wasm, vec![])?;

    // Crash the bad plugin
    let _ = runtime.process(&bad, 1.0, 0.001);

    // Good plugins still work
    let o1 = runtime.process(&good1, 5.0, 0.001)?;
    let o2 = runtime.process(&good2, 7.0, 0.001)?;
    assert!((o1 - 5.0).abs() < f32::EPSILON);
    assert!((o2 - 7.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn multi_independent_stats_per_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id_a, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id_b, &wasm, vec![])?;

    // Call plugin A 5 times, plugin B 3 times
    for _ in 0..5 {
        let _ = runtime.process(&id_a, 1.0, 0.001)?;
    }
    for _ in 0..3 {
        let _ = runtime.process(&id_b, 1.0, 0.001)?;
    }

    let (count_a, _) = runtime.get_plugin_stats(&id_a)?;
    let (count_b, _) = runtime.get_plugin_stats(&id_b)?;
    assert_eq!(count_a, 5);
    assert_eq!(count_b, 3);
    Ok(())
}

// ===================================================================
// 11. Plugin priority and conflict resolution
// ===================================================================

#[test]
fn priority_chain_passthrough_then_double() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let pass_id = Uuid::new_v4();
    let double_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;
    runtime.load_plugin_from_bytes(double_id, &double_wasm, vec![])?;

    // Simulate priority chain: passthrough first, then doubler
    let input = 5.0;
    let after_pass = runtime.process(&pass_id, input, 0.001)?;
    let after_double = runtime.process(&double_id, after_pass, 0.001)?;

    assert!((after_pass - 5.0).abs() < f32::EPSILON);
    assert!((after_double - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn priority_chain_order_matters() -> Result<(), Box<dyn std::error::Error>> {
    let double_wasm = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let half_wasm = compile_wat(HALF_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let double_id = Uuid::new_v4();
    let half_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(double_id, &double_wasm, vec![])?;
    runtime.load_plugin_from_bytes(half_id, &half_wasm, vec![])?;

    // Order A: double first, then half → 10 * 2 * 0.5 = 10
    let input = 10.0;
    let r1 = runtime.process(&double_id, input, 0.001)?;
    let r2 = runtime.process(&half_id, r1, 0.001)?;
    assert!((r2 - 10.0).abs() < f32::EPSILON);

    // Order B: half first, then double → 10 * 0.5 * 2 = 10
    let r3 = runtime.process(&half_id, input, 0.001)?;
    let r4 = runtime.process(&double_id, r3, 0.001)?;
    assert!((r4 - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn priority_manifest_validator_rejects_conflicting_class_caps() {
    let validator = ManifestValidator::default();

    // Safe plugin cannot request Fast-only capability (ProcessDsp)
    let mut safe = make_manifest(PluginClass::Safe);
    safe.capabilities = vec![Capability::ProcessDsp];
    assert!(
        validator.validate(&safe).is_err(),
        "Safe plugin with ProcessDsp must be rejected"
    );
}

// ===================================================================
// 12. Plugin hot-reload
// ===================================================================

#[test]
fn hot_reload_swaps_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let v1 = compile_wat(PASSTHROUGH_WAT)?;
    let v2 = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &v1, vec![])?;
    let output_v1 = runtime.process(&id, 3.0, 0.001)?;
    assert!((output_v1 - 3.0).abs() < f32::EPSILON);

    runtime.reload_plugin(&id, &v2, vec![])?;
    let output_v2 = runtime.process(&id, 3.0, 0.001)?;
    assert!(
        (output_v2 - 6.0).abs() < f32::EPSILON,
        "hot-reloaded plugin must use new code"
    );
    Ok(())
}

#[test]
fn hot_reload_preserves_stats() -> Result<(), Box<dyn std::error::Error>> {
    let v1 = compile_wat(PASSTHROUGH_WAT)?;
    let v2 = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &v1, vec![])?;
    for _ in 0..7 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }
    let (count_before, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_before, 7);

    runtime.reload_plugin(&id, &v2, vec![])?;
    let (count_after, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_after, count_before, "stats must be preserved");
    Ok(())
}

#[test]
fn hot_reload_failure_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let valid = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &valid, vec![])?;

    let result = runtime.reload_plugin(&id, b"invalid wasm bytes", vec![]);
    assert!(result.is_err(), "reload with invalid WASM must fail");

    // Old plugin must still work
    assert!(runtime.has_plugin(&id));
    let output = runtime.process(&id, 99.0, 0.001)?;
    assert!((output - 99.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn hot_reload_missing_plugin_still_loads() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // reload_plugin on a non-existent id should still create the instance
    // (since the old instance is not found, it just creates a new one)
    runtime.reload_plugin(&id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));

    let output = runtime.process(&id, 5.0, 0.001)?;
    assert!((output - 5.0).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// 13. Plugin communication channels
// ===================================================================

#[test]
fn comm_inter_plugin_capability_required() -> Result<(), PluginError> {
    let without = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(without.check_inter_plugin_comm().is_err());

    let with = CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::InterPluginComm]);
    with.check_inter_plugin_comm()?;
    Ok(())
}

#[test]
fn comm_manifest_allows_inter_plugin_for_safe() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::InterPluginComm];
    validator.validate(&m)
}

#[test]
fn comm_manifest_allows_inter_plugin_for_fast() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.capabilities = vec![Capability::InterPluginComm];
    m.constraints.max_execution_time_us = 100;
    m.constraints.max_memory_bytes = 2 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn comm_plugin_context_carries_capabilities() {
    let context = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Safe,
        update_rate_hz: 60,
        budget_us: 5000,
        capabilities: vec!["ReadTelemetry".to_string(), "InterPluginComm".to_string()],
    };

    assert_eq!(context.capabilities.len(), 2);
    assert!(
        context
            .capabilities
            .contains(&"InterPluginComm".to_string())
    );
}

// ===================================================================
// 14. Plugin error propagation
// ===================================================================

#[test]
fn error_loading_invalid_wasm() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, b"not wasm", vec![]);
    assert!(result.is_err());
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn error_missing_process_export() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(NO_PROCESS_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        msg.contains("process"),
        "error should mention 'process': {msg}"
    );
    Ok(())
}

#[test]
fn error_missing_memory_export() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(NO_MEMORY_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        msg.contains("memory"),
        "error should mention 'memory': {msg}"
    );
    Ok(())
}

#[test]
fn error_quarantine_blocks_loading() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::Crash, "fatal crash".to_string())?;
    assert!(manager.is_quarantined(id));

    // Quarantine state has information
    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    let state = state.unwrap_or_else(|| unreachable!());
    assert_eq!(state.total_crashes, 1);
    assert!(state.is_quarantined);
    Ok(())
}

#[test]
fn error_release_unknown_plugin_fails() {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let result = manager.release_from_quarantine(Uuid::new_v4());
    assert!(result.is_err());
}

#[test]
fn error_capability_violation_messages_descriptive() {
    let checker = CapabilityChecker::new(vec![]);

    let err = checker.check_dsp_processing();
    let msg = format!("{}", err.err().unwrap_or_else(|| unreachable!()));
    assert!(
        msg.contains("ProcessDsp"),
        "msg should mention ProcessDsp: {msg}"
    );

    let err = checker.check_network_access("evil.com");
    let msg = format!("{}", err.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("evil.com"), "msg should mention host: {msg}");
}

#[test]
fn error_failure_tracker_records_crashes() {
    let mut tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    tracker.record_execution(id, 100, true);
    tracker.record_execution(id, 200, false);
    tracker.record_execution(id, 150, true);

    let stats = tracker.get_stats(id);
    assert!(stats.is_some());
    let stats = stats.unwrap_or_else(|| unreachable!());
    assert_eq!(stats.executions, 3);
    assert_eq!(stats.crashes, 1);
    assert_eq!(stats.max_time_us, 200);
}

#[test]
fn error_plugin_error_variants() {
    // Verify error type variants construct correctly
    let err = PluginError::ManifestValidation("bad manifest".to_string());
    assert!(format!("{err}").contains("bad manifest"));

    let err = PluginError::LoadingFailed("missing file".to_string());
    assert!(format!("{err}").contains("missing file"));

    let err = PluginError::Crashed {
        reason: "unreachable trap".to_string(),
    };
    assert!(format!("{err}").contains("unreachable trap"));

    let err = PluginError::CapabilityViolation {
        capability: "ProcessDsp".to_string(),
    };
    assert!(format!("{err}").contains("ProcessDsp"));

    let id = Uuid::new_v4();
    let err = PluginError::Quarantined { plugin_id: id };
    assert!(format!("{err}").contains(&id.to_string()));
}

// ===================================================================
// 15. Plugin sandboxing boundary tests
// ===================================================================

#[test]
fn sandbox_no_filesystem_without_capability() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(checker.check_file_access(Path::new("/tmp")).is_err());
    assert!(checker.check_file_access(Path::new("/etc/hosts")).is_err());
    assert!(checker.check_file_access(Path::new("C:\\Windows")).is_err());
    assert!(
        checker
            .check_file_access(Path::new("/proc/self/status"))
            .is_err()
    );
}

#[test]
fn sandbox_no_network_without_capability() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(checker.check_network_access("api.example.com").is_err());
    assert!(checker.check_network_access("localhost").is_err());
    assert!(checker.check_network_access("127.0.0.1").is_err());
}

#[test]
fn sandbox_filesystem_path_containment() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/sandbox".to_string()],
    }]);

    checker.check_file_access(Path::new("/sandbox/file.txt"))?;
    checker.check_file_access(Path::new("/sandbox/sub/deep/file.bin"))?;
    assert!(checker.check_file_access(Path::new("/etc/shadow")).is_err());
    assert!(
        checker
            .check_file_access(Path::new("/sandboxed-escape"))
            .is_err()
    );
    Ok(())
}

#[test]
fn sandbox_network_host_containment() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["trusted.api.com".to_string()],
    }]);

    checker.check_network_access("trusted.api.com")?;
    assert!(checker.check_network_access("untrusted.api.com").is_err());
    assert!(
        checker
            .check_network_access("trusted.api.com.evil.com")
            .is_err()
    );
    Ok(())
}

#[test]
fn sandbox_wasm_crash_isolation() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let trap_id = Uuid::new_v4();
    let safe_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(trap_id, &trap_wasm, vec![])?;
    runtime.load_plugin_from_bytes(safe_id, &pass_wasm, vec![])?;

    // Crash the trapping plugin
    let result = runtime.process(&trap_id, 1.0, 0.001);
    assert!(result.is_err());

    // Safe plugin completely unaffected
    let output = runtime.process(&safe_id, 123.0, 0.001)?;
    assert!((output - 123.0).abs() < f32::EPSILON);

    // Trapping plugin disabled, safe plugin not
    assert!(runtime.is_plugin_disabled(&trap_id)?);
    assert!(!runtime.is_plugin_disabled(&safe_id)?);
    Ok(())
}

#[test]
fn sandbox_fuel_exhaustion_isolated() -> Result<(), Box<dyn std::error::Error>> {
    let loop_wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let loop_id = Uuid::new_v4();
    let pass_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(loop_id, &loop_wasm, vec![])?;
    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;

    // Infinite loop plugin gets terminated
    let result = runtime.process(&loop_id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&loop_id)?);

    // Passthrough plugin still works
    let output = runtime.process(&pass_id, 7.5, 0.001)?;
    assert!((output - 7.5).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&pass_id)?);
    Ok(())
}

#[test]
fn sandbox_quarantine_isolation_per_plugin() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        ..QuarantinePolicy::default()
    });

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    manager.record_violation(id_a, ViolationType::Crash, "crash a".to_string())?;
    assert!(manager.is_quarantined(id_a));
    assert!(!manager.is_quarantined(id_b));
    Ok(())
}

#[test]
fn sandbox_quarantine_escalation() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        quarantine_duration_minutes: 5,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::Crash, "crash 1".to_string())?;
    let state = manager
        .get_quarantine_state(id)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(state.escalation_level, 1);

    manager.release_from_quarantine(id)?;
    manager.record_violation(id, ViolationType::Crash, "crash 2".to_string())?;
    let state = manager
        .get_quarantine_state(id)
        .unwrap_or_else(|| unreachable!());
    assert_eq!(state.escalation_level, 2);
    Ok(())
}

#[test]
fn sandbox_manual_quarantine_and_release() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.manual_quarantine(id, 30)?;
    assert!(manager.is_quarantined(id));

    manager.release_from_quarantine(id)?;
    assert!(!manager.is_quarantined(id));
    Ok(())
}

// ===================================================================
// Supplementary: PluginHost integration
// ===================================================================

#[tokio::test]
async fn host_load_and_unload_all_noop_on_empty() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let host = PluginHost::new(temp.path().to_path_buf()).await?;

    host.load_all_plugins().await?;
    host.unload_all_plugins().await?;

    let stats = host.get_quarantine_stats().await;
    assert_eq!(stats.len(), 0);
    Ok(())
}

#[tokio::test]
async fn host_quarantine_stats_empty_initially() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let host = PluginHost::new(temp.path().to_path_buf()).await?;
    let stats = host.get_quarantine_stats().await;
    assert!(stats.is_empty());
    Ok(())
}

// ===================================================================
// Supplementary: Registry CRUD operations
// ===================================================================

#[test]
fn registry_add_retrieve_remove() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("My Plugin", "1.0.0");
    let id = meta.id.clone();

    catalog.add_plugin(meta)?;
    assert!(catalog.contains(&id));
    assert_eq!(catalog.plugin_count(), 1);

    assert!(catalog.remove_plugin(&id, None));
    assert!(!catalog.contains(&id));
    assert_eq!(catalog.plugin_count(), 0);
    Ok(())
}

#[test]
fn registry_multiple_versions_tracked() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("Plugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    assert_eq!(catalog.plugin_count(), 1); // One unique plugin
    assert_eq!(catalog.version_count(), 2); // Two versions

    // Latest version returned by default
    let latest = catalog.get_plugin(&id, None);
    assert_eq!(
        latest.map(|m| m.version.to_string()),
        Some("2.0.0".to_string())
    );

    // Specific version available
    let specific = catalog.get_plugin(&id, Some(&semver::Version::new(1, 0, 0)));
    assert!(specific.is_some());
    Ok(())
}

#[test]
fn registry_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("Roundtrip", "2.1.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    let json = serde_json::to_string(&catalog)?;
    let mut restored: PluginCatalog = serde_json::from_str(&json)?;
    restored.rebuild_index();

    assert!(restored.contains(&id));
    assert_eq!(restored.plugin_count(), 1);
    Ok(())
}

// ===========================================================================
// 16. Additional lifecycle edge cases
// ===========================================================================

#[test]
fn lifecycle_double_unload_fails() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    runtime.unload_plugin(&id)?;

    let result = runtime.unload_plugin(&id);
    assert!(result.is_err(), "double unload must fail");
    Ok(())
}

#[test]
fn lifecycle_load_same_id_twice_replaces() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    // Loading with the same ID replaces the existing instance
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_ok(), "loading same id should succeed (replace)");
    // Plugin should still work after replacement
    let output = runtime.process(&id, 1.0, 0.001)?;
    assert!((output - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn lifecycle_process_with_zero_dt() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, 5.0, 0.0)?;
    assert!((output - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn lifecycle_process_negative_input() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, -100.0, 0.001)?;
    assert!((output - (-100.0)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn lifecycle_process_nan_input() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, f32::NAN, 0.001)?;
    assert!(output.is_nan());
    Ok(())
}

#[test]
fn lifecycle_process_infinity_input() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, f32::INFINITY, 0.001)?;
    assert!(output.is_infinite());
    Ok(())
}

// ===========================================================================
// 17. PluginClass and PluginContext
// ===========================================================================

#[test]
fn plugin_class_debug_display() {
    let safe = PluginClass::Safe;
    let fast = PluginClass::Fast;
    let safe_str = format!("{safe:?}");
    let fast_str = format!("{fast:?}");
    assert!(safe_str.contains("Safe"));
    assert!(fast_str.contains("Fast"));
}

#[test]
fn plugin_context_creation_and_access() {
    let ctx = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Safe,
        update_rate_hz: 60,
        budget_us: 5000,
        capabilities: vec!["ReadTelemetry".to_string(), "ControlLeds".to_string()],
    };

    assert_eq!(ctx.capabilities.len(), 2);
    assert!(ctx.capabilities.contains(&"ReadTelemetry".to_string()));
}

#[test]
fn plugin_context_empty_capabilities() {
    let ctx = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Safe,
        update_rate_hz: 60,
        budget_us: 5000,
        capabilities: vec![],
    };

    assert!(ctx.capabilities.is_empty());
}

#[test]
fn plugin_context_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Fast,
        update_rate_hz: 1000,
        budget_us: 200,
        capabilities: vec!["ProcessDsp".to_string()],
    };

    let json = serde_json::to_string(&ctx)?;
    let restored: PluginContext = serde_json::from_str(&json)?;
    assert_eq!(restored.plugin_id, ctx.plugin_id);
    assert_eq!(restored.class, PluginClass::Fast);
    assert_eq!(restored.update_rate_hz, 1000);
    Ok(())
}

// ===========================================================================
// 18. Manifest edge cases
// ===========================================================================

#[test]
fn manifest_all_allowed_capabilities_on_fast_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    m.capabilities = vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::ProcessDsp,
        Capability::InterPluginComm,
    ];
    validator.validate(&m)?;
    Ok(())
}

#[test]
fn manifest_fast_exceeds_execution_time_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 999;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_fast_exceeds_memory_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 64 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_fast_exceeds_update_rate_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 10_000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_with_only_inter_plugin_comm() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::InterPluginComm];
    validator.validate(&m)?;
    Ok(())
}

#[test]
fn manifest_safe_filesystem_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_fast_filesystem_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    m.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&m).is_err());
}

// ===========================================================================
// 19. Quarantine and failure tracking edge cases
// ===========================================================================

#[test]
fn quarantine_default_policy_sensible() {
    let policy = QuarantinePolicy::default();
    assert!(policy.max_crashes > 0);
    assert!(policy.quarantine_duration_minutes > 0);
}

#[test]
fn quarantine_multiple_plugins_tracked_independently() {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 2,
        ..QuarantinePolicy::default()
    });

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    let _ = manager.record_violation(id_a, ViolationType::Crash, "crash 1".to_string());
    let _ = manager.record_violation(id_a, ViolationType::Crash, "crash 2".to_string());
    let _ = manager.record_violation(id_b, ViolationType::Crash, "crash b".to_string());

    assert!(manager.is_quarantined(id_a));
    assert!(!manager.is_quarantined(id_b));
}

#[test]
fn quarantine_release_then_re_quarantine() {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        ..QuarantinePolicy::default()
    });

    let id = Uuid::new_v4();
    let _ = manager.record_violation(id, ViolationType::Crash, "crash".to_string());
    assert!(manager.is_quarantined(id));

    let _ = manager.release_from_quarantine(id);
    assert!(!manager.is_quarantined(id));

    let _ = manager.record_violation(id, ViolationType::Crash, "crash again".to_string());
    assert!(manager.is_quarantined(id));
}

#[test]
fn failure_tracker_empty_returns_none() {
    let tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    let stats = tracker.get_stats(id);
    assert!(stats.is_none());
}

#[test]
fn failure_tracker_multiple_plugins_independent() {
    let mut tracker = FailureTracker::new();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    tracker.record_execution(id_a, 100, true);
    tracker.record_execution(id_a, 200, true);
    tracker.record_execution(id_b, 300, false);

    let stats_a = tracker.get_stats(id_a).unwrap_or_else(|| unreachable!());
    let stats_b = tracker.get_stats(id_b).unwrap_or_else(|| unreachable!());
    assert_eq!(stats_a.executions, 2);
    assert_eq!(stats_b.executions, 1);
}

#[test]
fn failure_tracker_crash_count_accurate() {
    let mut tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    tracker.record_execution(id, 100, true);
    tracker.record_execution(id, 200, false);
    tracker.record_execution(id, 150, true);

    let stats = tracker.get_stats(id).unwrap_or_else(|| unreachable!());
    assert_eq!(stats.executions, 3);
    assert_eq!(stats.crashes, 1);
}

// ===========================================================================
// 20. PluginError variants (additional)
// ===========================================================================

#[test]
fn plugin_error_loading_failed_display() {
    let err = PluginError::LoadingFailed("corrupt binary".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("corrupt binary"));
}

#[test]
fn plugin_error_capability_violation_struct() {
    let err = PluginError::CapabilityViolation {
        capability: "ReadTelemetry".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("ReadTelemetry"));
}

#[test]
fn plugin_error_quarantined_struct() {
    let id = Uuid::new_v4();
    let err = PluginError::Quarantined { plugin_id: id };
    let msg = format!("{err}");
    assert!(msg.contains(&id.to_string()));
}

#[test]
fn plugin_error_execution_timeout() {
    let err = PluginError::ExecutionTimeout {
        duration: std::time::Duration::from_millis(500),
    };
    let msg = format!("{err}");
    assert!(!msg.is_empty());
}

#[test]
fn plugin_error_budget_violation() {
    let err = PluginError::BudgetViolation {
        used_us: 2000,
        budget_us: 1000,
    };
    let msg = format!("{err}");
    assert!(msg.contains("2000"));
    assert!(msg.contains("1000"));
}

#[test]
fn plugin_error_ipc() {
    let err = PluginError::Ipc("connection lost".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("connection lost"));
}

#[test]
fn plugin_error_native_plugin_load() {
    let err = PluginError::NativePluginLoad("missing symbol".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("missing symbol"));
}

// ===========================================================================
// 21. Version compatibility edge cases
// ===========================================================================

#[test]
fn version_exact_match_compatible() {
    let v = semver::Version::new(1, 2, 3);
    let compat = check_compatibility(&v, &v);
    assert_eq!(compat, VersionCompatibility::Compatible);
}

#[test]
fn version_zero_minor_bump_incompatible() {
    let required = semver::Version::new(0, 1, 0);
    let available = semver::Version::new(0, 2, 0);
    let compat = check_compatibility(&required, &available);
    assert_ne!(compat, VersionCompatibility::Compatible);
}

#[test]
fn version_major_bump_incompatible() {
    let required = semver::Version::new(1, 0, 0);
    let available = semver::Version::new(2, 0, 0);
    let compat = check_compatibility(&required, &available);
    assert_eq!(compat, VersionCompatibility::Incompatible);
}

// ===========================================================================
// 22. Registry / catalog edge cases
// ===========================================================================

#[test]
fn registry_empty_catalog_contains_nothing() {
    let catalog = PluginCatalog::new();
    assert_eq!(catalog.plugin_count(), 0);

    let id = PluginMetadata::new("X", semver::Version::new(1, 0, 0), "a", "d", "MIT").id;
    assert!(!catalog.contains(&id));
}

#[test]
fn registry_remove_nonexistent_returns_false() {
    let mut catalog = PluginCatalog::new();
    let id = PluginMetadata::new("X", semver::Version::new(1, 0, 0), "a", "d", "MIT").id;
    let removed = catalog.remove_plugin(&id, None);
    assert!(!removed);
}

#[test]
fn registry_search_empty_query_returns_all() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("Alpha", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Beta", "1.0.0"))?;

    let results = catalog.search("");
    assert_eq!(results.len(), 2);
    Ok(())
}

#[test]
fn registry_search_filters_by_name() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("AlphaPlugin", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("BetaPlugin", "1.0.0"))?;

    let results = catalog.search("Alpha");
    assert_eq!(results.len(), 1);
    Ok(())
}

#[test]
fn registry_get_all_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("Multi", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("Multi", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    let versions = catalog.get_all_versions(&id);
    assert!(versions.is_some());
    assert_eq!(versions.map(|v| v.len()), Some(2));
    Ok(())
}

#[test]
fn registry_remove_specific_version() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("VerPlugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("VerPlugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    assert_eq!(catalog.version_count(), 2);

    let removed = catalog.remove_plugin(&id, Some(&semver::Version::new(1, 0, 0)));
    assert!(removed);
    assert_eq!(catalog.version_count(), 1);
    Ok(())
}

// ===========================================================================
// 23. Resource limits
// ===========================================================================

#[test]
fn resource_limits_default_values() {
    let limits = ResourceLimits::default();
    assert!(limits.max_memory_bytes > 0);
    assert!(limits.max_fuel > 0);
    assert!(limits.max_table_elements > 0);
    assert!(limits.max_instances > 0);
}

#[test]
fn resource_limits_custom_values() {
    let limits = ResourceLimits::new(
        8 * 1024 * 1024, // 8 MiB
        5_000_000,
        5_000,
        16,
    );
    assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 5_000_000);
    assert_eq!(limits.max_table_elements, 5_000);
    assert_eq!(limits.max_instances, 16);
}

// ===========================================================================
// 24. Native plugin configuration
// ===========================================================================

#[test]
fn native_config_strict_requires_signatures() {
    let config = NativePluginConfig::strict();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn native_config_permissive_allows_unsigned() {
    let config = NativePluginConfig::permissive();
    assert!(config.allow_unsigned);
}

#[test]
fn native_config_default_is_strict() {
    let config = NativePluginConfig::default();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

// ===========================================================================
// 25. Capability checker edge cases
// ===========================================================================

#[test]
fn capability_all_seven_capabilities_granted() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::ProcessDsp,
        Capability::FileSystem {
            paths: vec!["/tmp".to_string()],
        },
        Capability::Network {
            hosts: vec!["localhost".to_string()],
        },
        Capability::InterPluginComm,
    ]);

    checker.check_telemetry_read()?;
    checker.check_telemetry_modify()?;
    checker.check_led_control()?;
    checker.check_dsp_processing()?;
    checker.check_file_access(Path::new("/tmp/data.json"))?;
    checker.check_network_access("localhost")?;
    checker.check_inter_plugin_comm()?;
    Ok(())
}

#[test]
fn capability_enforcer_delegates_to_checker() -> Result<(), PluginError> {
    let enforcer = WasmCapabilityEnforcer::new(vec![
        Capability::ReadTelemetry,
        Capability::FileSystem {
            paths: vec!["/sandbox".to_string()],
        },
    ]);
    let checker = enforcer.checker();

    checker.check_telemetry_read()?;
    checker.check_file_access(Path::new("/sandbox/test.txt"))?;
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_network_access("evil.com").is_err());
    Ok(())
}

#[test]
fn capability_has_capability_simple_types() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);

    assert!(checker.has_capability(&Capability::ReadTelemetry));
    assert!(checker.has_capability(&Capability::ControlLeds));
    assert!(!checker.has_capability(&Capability::ProcessDsp));
    assert!(!checker.has_capability(&Capability::ModifyTelemetry));
}

// ===========================================================================
// 26. WasmRuntime instance tracking
// ===========================================================================

#[test]
fn runtime_instance_count_starts_at_zero() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = WasmRuntime::new()?;
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn runtime_has_plugin_check() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    assert!(!runtime.has_plugin(&id));
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

// ===========================================================================
// 27. Native ABI compatibility
// ===========================================================================

#[test]
fn abi_current_version_is_defined() {
    // Ensure the ABI version constant exists and is usable
    let version = CURRENT_ABI_VERSION;
    assert_ne!(version, 0);
}

#[test]
fn abi_check_compatible_returns_ok() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_check_zero_returns_incompatible() {
    let result = check_abi_compatibility(0);
    assert_ne!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_check_very_old_returns_incompatible() {
    if CURRENT_ABI_VERSION > 1 {
        let result = check_abi_compatibility(1);
        assert_ne!(result, AbiCheckResult::Compatible);
    }
}
