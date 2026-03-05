//! Plugin Ecosystem Safety Tests for 1.0 RC
//!
//! Covers safety-critical plugin subsystems:
//!  1. WASM Sandbox Boundary Tests
//!  2. Native Plugin ABI Tests
//!  3. Capability Model Tests
//!  4. Budget Enforcement Tests
//!  5. Plugin Registry Lifecycle Tests
//!  6. Plugin Signing & Trust Store Tests

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
use racing_wheel_plugins::{PluginClass, PluginError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compile_wat(wat: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(wat)?)
}

fn make_manifest(class: PluginClass) -> PluginManifest {
    PluginManifest {
        id: Uuid::new_v4(),
        name: "Ecosystem Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "Plugin for ecosystem safety tests".to_string(),
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

fn sign_bytes(
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

// -- WAT modules --

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

#[allow(dead_code)]
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

/// WASM module that allocates a huge memory (tries to grow past the limit).
#[allow(dead_code)]
const MEMORY_HOG_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; Try to grow memory by 256 pages (16 MB)
        i32.const 256
        memory.grow
        drop
        local.get 0
    )
)
"#;

/// WASM module that does many iterations to consume fuel.
const FUEL_CONSUMER_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        (local $i i32)
        (local.set $i (i32.const 0))
        (block $break
            (loop $loop
                (br_if $break (i32.ge_u (local.get $i) (i32.const 50000)))
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br $loop)
            )
        )
        local.get 0
    )
)
"#;

/// WASM module with stack overflow (deep recursion).
const STACK_OVERFLOW_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func $recurse (param f32 f32) (result f32)
        local.get 0
        local.get 1
        call $recurse
    )
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 1
        call $recurse
    )
)
"#;

/// WASM module that writes to memory offset 0.
const MEMORY_WRITER_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; Write 42 to address 0
        i32.const 0
        i32.const 42
        i32.store
        local.get 0
    )
)
"#;

// ===================================================================
// 1. WASM Sandbox Boundary Tests
// ===================================================================

#[test]
fn sandbox_wasm_cannot_access_host_filesystem() -> Result<(), Box<dyn std::error::Error>> {
    // Plugins without FileSystem capability have no preopened dirs
    let enforcer = WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry]);
    let checker = enforcer.checker();
    let result = checker.check_file_access(Path::new("/etc/passwd"));
    assert!(result.is_err(), "plugin must not access host filesystem");
    Ok(())
}

#[test]
fn sandbox_wasm_cannot_access_arbitrary_network() -> Result<(), Box<dyn std::error::Error>> {
    let enforcer = WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry]);
    let checker = enforcer.checker();
    let result = checker.check_network_access("evil.example.com");
    assert!(result.is_err(), "plugin must not access arbitrary network");
    Ok(())
}

#[test]
fn sandbox_fuel_limit_terminates_infinite_loop() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(500);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "infinite loop must be terminated by fuel");
    Ok(())
}

#[test]
fn sandbox_stack_overflow_caught() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(STACK_OVERFLOW_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "stack overflow must be caught");
    assert!(
        runtime.is_plugin_disabled(&id)?,
        "plugin must be disabled after stack overflow"
    );
    Ok(())
}

#[test]
fn sandbox_unreachable_trap_caught_and_disables() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);

    // Subsequent calls must also fail
    let second = runtime.process(&id, 1.0, 0.001);
    assert!(second.is_err(), "disabled plugin must reject calls");
    Ok(())
}

#[test]
fn sandbox_memory_write_stays_in_linear_memory() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(MEMORY_WRITER_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, 7.5, 0.001)?;
    // Plugin writes to its own memory but returns the input unchanged
    assert!((output - 7.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn sandbox_invalid_wasm_magic_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"\x00\x61\x73\x6d\xff\xff\xff\xff", vec![]);
    assert!(result.is_err(), "invalid WASM magic must be rejected");
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn sandbox_empty_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"", vec![]);
    assert!(result.is_err(), "empty bytes must be rejected");
    Ok(())
}

#[test]
fn sandbox_truncated_wasm_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let truncated = &wasm[..wasm.len() / 2];
    let result = runtime.load_plugin_from_bytes(id, truncated, vec![]);
    assert!(result.is_err(), "truncated WASM must be rejected");
    Ok(())
}

#[test]
fn sandbox_missing_memory_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(NO_MEMORY_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err(), "missing memory export must be rejected");
    Ok(())
}

#[test]
fn sandbox_missing_process_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(NO_PROCESS_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err(), "missing process export must be rejected");
    Ok(())
}

#[test]
fn sandbox_crash_in_one_plugin_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let trap_id = Uuid::new_v4();
    let pass_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(trap_id, &trap_wasm, vec![])?;
    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;

    let _ = runtime.process(&trap_id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&trap_id)?);

    let output = runtime.process(&pass_id, 3.125, 0.001)?;
    assert!(
        (output - 3.125).abs() < f32::EPSILON,
        "sibling must be unaffected"
    );
    assert!(!runtime.is_plugin_disabled(&pass_id)?);
    Ok(())
}

// ===================================================================
// 2. Native Plugin ABI Tests
// ===================================================================

#[test]
fn abi_current_version_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_version_zero_mismatch() {
    if CURRENT_ABI_VERSION != 0 {
        let result = check_abi_compatibility(0);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn abi_future_version_mismatch() {
    let future = CURRENT_ABI_VERSION + 1;
    let result = check_abi_compatibility(future);
    match result {
        AbiCheckResult::Mismatch { expected, actual } => {
            assert_eq!(expected, CURRENT_ABI_VERSION);
            assert_eq!(actual, future);
        }
        _ => panic!("future version should mismatch"),
    }
}

#[test]
fn abi_u32_max_mismatch() {
    let result = check_abi_compatibility(u32::MAX);
    assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
}

#[test]
fn abi_adjacent_below_mismatch() {
    let below = CURRENT_ABI_VERSION.saturating_sub(1);
    if below != CURRENT_ABI_VERSION {
        let result = check_abi_compatibility(below);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn abi_native_host_permissive_constructible() {
    let host = NativePluginHost::new_permissive_for_development();
    drop(host);
}

#[test]
fn abi_signature_config_strict_defaults() {
    let config = SignatureVerificationConfig::default();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn abi_signature_config_development_relaxes() {
    let config = SignatureVerificationConfig::development();
    assert!(!config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn abi_signature_config_permissive_still_requires_but_allows_unsigned() {
    let config = SignatureVerificationConfig::permissive();
    assert!(config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn abi_plugin_header_size_stable() {
    use openracing_plugin_abi::PluginHeader;
    assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
    assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
}

#[test]
fn abi_telemetry_frame_size_stable() {
    use openracing_plugin_abi::TelemetryFrame;
    assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
    assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
}

#[test]
fn abi_plugin_header_roundtrip() {
    use openracing_plugin_abi::{
        PLUG_ABI_MAGIC, PLUG_ABI_VERSION, PluginCapabilities, PluginHeader,
    };

    let header = PluginHeader {
        magic: PLUG_ABI_MAGIC,
        abi_version: PLUG_ABI_VERSION,
        capabilities: (PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS).bits(),
        reserved: 0,
    };
    let bytes = header.to_bytes();
    let restored = PluginHeader::from_bytes(&bytes);
    assert_eq!(header, restored);
}

#[test]
fn abi_telemetry_frame_roundtrip() {
    use openracing_plugin_abi::TelemetryFrame;

    let frame = TelemetryFrame {
        timestamp_us: 9999,
        wheel_angle_deg: -45.0,
        wheel_speed_rad_s: 2.0,
        temperature_c: 70.0,
        fault_flags: 0xABCD,
        _pad: 0,
    };
    let bytes = frame.to_bytes();
    let restored = TelemetryFrame::from_bytes(&bytes);
    assert_eq!(frame.timestamp_us, restored.timestamp_us);
    assert_eq!(frame.wheel_angle_deg, restored.wheel_angle_deg);
    assert_eq!(frame.fault_flags, restored.fault_flags);
}

// ===================================================================
// 3. Capability Model Tests
// ===================================================================

#[test]
fn cap_empty_grants_deny_all() {
    let checker = CapabilityChecker::new(vec![]);
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
}

#[test]
fn cap_single_grant_selective() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::ControlLeds]);
    checker.check_led_control()?;
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_dsp_processing().is_err());
    Ok(())
}

#[test]
fn cap_file_access_scoped_to_granted_path() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/sandbox".to_string()],
    }]);
    checker.check_file_access(Path::new("/sandbox/data.bin"))?;
    checker.check_file_access(Path::new("/sandbox/subdir/file.txt"))?;
    assert!(checker.check_file_access(Path::new("/etc/shadow")).is_err());
    assert!(checker.check_file_access(Path::new("/home/user")).is_err());
    Ok(())
}

#[test]
fn cap_network_scoped_to_granted_host() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.example.com".to_string(), "cdn.example.com".to_string()],
    }]);
    checker.check_network_access("api.example.com")?;
    checker.check_network_access("cdn.example.com")?;
    assert!(checker.check_network_access("evil.com").is_err());
    Ok(())
}

#[test]
fn cap_dsp_denied_for_safe_class() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::ProcessDsp];
    assert!(
        validator.validate(&manifest).is_err(),
        "ProcessDsp must be denied for Safe plugins"
    );
}

#[test]
fn cap_dsp_allowed_for_fast_class() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Fast);
    manifest.capabilities = vec![Capability::ProcessDsp];
    manifest.constraints.max_execution_time_us = 100;
    manifest.constraints.max_memory_bytes = 2 * 1024 * 1024;
    manifest.constraints.update_rate_hz = 1000;
    validator.validate(&manifest)
}

#[test]
fn cap_network_denied_for_safe_class() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::Network {
        hosts: vec!["any.host".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn cap_filesystem_denied_for_safe_class() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&manifest).is_err());
}

#[test]
fn cap_enforcer_delegates_correctly() -> Result<(), PluginError> {
    let enforcer =
        WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);
    let checker = enforcer.checker();
    checker.check_telemetry_read()?;
    checker.check_led_control()?;
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    Ok(())
}

#[test]
fn cap_inter_plugin_comm_opt_in() -> Result<(), PluginError> {
    let without = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(without.check_inter_plugin_comm().is_err());

    let with = CapabilityChecker::new(vec![Capability::InterPluginComm]);
    with.check_inter_plugin_comm()?;
    Ok(())
}

#[test]
fn cap_two_plugins_independent_capabilities() -> Result<(), PluginError> {
    let a = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    let b = CapabilityChecker::new(vec![Capability::ControlLeds]);

    assert!(a.check_telemetry_read().is_ok());
    assert!(a.check_led_control().is_err());
    assert!(b.check_led_control().is_ok());
    assert!(b.check_telemetry_read().is_err());
    Ok(())
}

// ===================================================================
// 4. Budget Enforcement Tests
// ===================================================================

#[test]
fn budget_fuel_exhaustion_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn budget_generous_fuel_allows_computation() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(FUEL_CONSUMER_WAT)?;
    let limits = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let out = runtime.process(&id, 42.0, 0.001)?;
    assert!((out - 42.0).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn budget_tight_fuel_rejects_heavy_computation() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(FUEL_CONSUMER_WAT)?;
    let limits = ResourceLimits::default().with_fuel(100);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "tight fuel must reject heavy computation");
    Ok(())
}

#[test]
fn budget_max_instances_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_max_instances(3);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    for _ in 0..3 {
        runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;
    }
    assert_eq!(runtime.instance_count(), 3);

    let result = runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(result.is_err(), "4th instance must be rejected (limit 3)");
    assert_eq!(runtime.instance_count(), 3);
    Ok(())
}

#[test]
fn budget_memory_limit_configurable() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_memory(4 * 1024 * 1024);
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_memory_bytes, 4 * 1024 * 1024);
    Ok(())
}

#[test]
fn budget_quarantine_after_max_crashes() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 2,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::Crash, "c1".to_string())?;
    assert!(!manager.is_quarantined(id));

    manager.record_violation(id, ViolationType::Crash, "c2".to_string())?;
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn budget_quarantine_after_max_budget_violations() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_budget_violations: 3,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    for i in 0..2 {
        manager.record_violation(id, ViolationType::BudgetViolation, format!("v{i}"))?;
        assert!(!manager.is_quarantined(id));
    }
    manager.record_violation(id, ViolationType::BudgetViolation, "v2".to_string())?;
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn budget_capability_violations_dont_count_as_crashes() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 2,
        max_budget_violations: 10,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    for _ in 0..10 {
        manager.record_violation(id, ViolationType::CapabilityViolation, "cap".to_string())?;
    }
    assert!(
        !manager.is_quarantined(id),
        "cap violations alone must not quarantine"
    );

    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    Ok(())
}

#[test]
fn budget_failure_tracker_tracks_peak_time() {
    let mut tracker = FailureTracker::new();
    let id = Uuid::new_v4();

    tracker.record_execution(id, 100, true);
    tracker.record_execution(id, 500, true);
    tracker.record_execution(id, 200, false);

    let stats = tracker.get_stats(id);
    assert!(stats.is_some());
    if let Some(s) = stats {
        assert_eq!(s.max_time_us, 500);
        assert_eq!(s.executions, 3);
        assert_eq!(s.crashes, 1);
    }
}

#[test]
fn budget_fuel_per_call_replenished() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // Multiple calls should all succeed because fuel is reset per call
    for i in 0..10 {
        let out = runtime.process(&id, i as f32, 0.001)?;
        assert!((out - i as f32).abs() < f32::EPSILON);
    }
    Ok(())
}

#[test]
fn budget_disabled_plugin_rejects_subsequent_calls() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    for _ in 0..3 {
        let result = runtime.process(&id, 1.0, 0.001);
        assert!(result.is_err());
    }
    Ok(())
}

// ===================================================================
// 5. Plugin Registry Lifecycle Tests
// ===================================================================

#[test]
fn registry_add_search_find() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("FFB Filter Pro", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    let results = catalog.search("FFB");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "FFB Filter Pro");

    assert!(catalog.contains(&id));
    Ok(())
}

#[test]
fn registry_search_case_insensitive() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("LED Controller", "1.0.0"))?;

    let results = catalog.search("led controller");
    assert_eq!(results.len(), 1);

    let results_upper = catalog.search("LED CONTROLLER");
    assert_eq!(results_upper.len(), 1);
    Ok(())
}

#[test]
fn registry_search_by_description() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("MyPlugin", "1.0.0");
    // Description contains "MyPlugin" by default
    catalog.add_plugin(meta)?;

    let results = catalog.search("Description");
    assert_eq!(results.len(), 1);
    Ok(())
}

#[test]
fn registry_multiple_versions_latest_first() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("TestPlugin", "1.0.0");
    let id = v1.id.clone();

    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("TestPlugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    let mut v15 = make_registry_metadata("TestPlugin", "1.5.0");
    v15.id = id.clone();
    catalog.add_plugin(v15)?;

    assert_eq!(catalog.version_count(), 3);
    assert_eq!(catalog.plugin_count(), 1);

    // Latest should be 2.0.0
    let latest = catalog.get_plugin(&id, None);
    assert!(latest.is_some());
    if let Some(m) = latest {
        assert_eq!(m.version.to_string(), "2.0.0");
    }
    Ok(())
}

#[test]
fn registry_remove_specific_version() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("TestPlugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("TestPlugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    // Remove v1 only
    assert!(catalog.remove_plugin(&id, Some(&semver::Version::new(1, 0, 0))));
    assert!(catalog.contains(&id));
    assert_eq!(catalog.version_count(), 1);
    Ok(())
}

#[test]
fn registry_remove_all_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("TestPlugin", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    assert!(catalog.remove_plugin(&id, None));
    assert!(!catalog.contains(&id));
    assert_eq!(catalog.plugin_count(), 0);
    Ok(())
}

#[test]
fn registry_remove_nonexistent_returns_false() {
    let mut catalog = PluginCatalog::new();
    let fake_id = racing_wheel_plugins::registry::PluginId::new();
    assert!(!catalog.remove_plugin(&fake_id, None));
}

#[test]
fn registry_find_compatible_version_semver() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v100 = make_registry_metadata("PlugX", "1.0.0");
    let id = v100.id.clone();
    catalog.add_plugin(v100)?;

    let mut v120 = make_registry_metadata("PlugX", "1.2.0");
    v120.id = id.clone();
    catalog.add_plugin(v120)?;

    let mut v200 = make_registry_metadata("PlugX", "2.0.0");
    v200.id = id.clone();
    catalog.add_plugin(v200)?;

    // Requiring 1.0.0 → highest compatible in major 1 is 1.2.0
    let compat = catalog.find_compatible_version(&id, &semver::Version::new(1, 0, 0));
    assert!(compat.is_some());
    if let Some(m) = compat {
        assert_eq!(m.version, semver::Version::new(1, 2, 0));
    }
    Ok(())
}

#[test]
fn registry_version_compat_same_major_higher_minor() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 0, 0),
            &semver::Version::new(1, 3, 0)
        ),
        VersionCompatibility::Compatible,
    );
}

#[test]
fn registry_version_compat_different_major_incompatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 0, 0),
            &semver::Version::new(2, 0, 0)
        ),
        VersionCompatibility::Incompatible,
    );
}

#[test]
fn registry_version_compat_lower_available_incompatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 5, 0),
            &semver::Version::new(1, 2, 0)
        ),
        VersionCompatibility::Incompatible,
    );
}

#[test]
fn registry_version_compat_prerelease_exact_match_only() {
    let pre1 = semver::Version::parse("1.0.0-alpha.1").unwrap_or_else(|_| unreachable!());
    let pre2 = semver::Version::parse("1.0.0-alpha.2").unwrap_or_else(|_| unreachable!());

    assert_eq!(
        check_compatibility(&pre1, &pre1),
        VersionCompatibility::Compatible
    );
    assert_eq!(
        check_compatibility(&pre1, &pre2),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn registry_version_compat_0x_strict() {
    // 0.x versions require exact minor match
    assert_eq!(
        check_compatibility(
            &semver::Version::new(0, 1, 0),
            &semver::Version::new(0, 1, 5)
        ),
        VersionCompatibility::Compatible,
    );
    assert_eq!(
        check_compatibility(
            &semver::Version::new(0, 1, 0),
            &semver::Version::new(0, 2, 0)
        ),
        VersionCompatibility::Incompatible,
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
fn registry_metadata_validation_rejects_empty_name() {
    let meta = PluginMetadata::new(
        "",
        semver::Version::new(1, 0, 0),
        "Author",
        "Description",
        "MIT",
    );
    assert!(meta.validate().is_err());
}

#[test]
fn registry_metadata_validation_rejects_empty_author() {
    let meta = PluginMetadata::new(
        "Name",
        semver::Version::new(1, 0, 0),
        "",
        "Description",
        "MIT",
    );
    assert!(meta.validate().is_err());
}

#[test]
fn registry_metadata_validation_rejects_empty_license() {
    let meta = PluginMetadata::new(
        "Name",
        semver::Version::new(1, 0, 0),
        "Author",
        "Description",
        "",
    );
    assert!(meta.validate().is_err());
}

#[test]
fn registry_metadata_builder_methods() {
    let meta = PluginMetadata::new(
        "Plugin",
        semver::Version::new(1, 0, 0),
        "Author",
        "Desc",
        "MIT",
    )
    .with_homepage("https://example.com")
    .with_capabilities(vec![Capability::ReadTelemetry])
    .with_signature_fingerprint("abc123")
    .with_download_url("https://example.com/plugin.tar.gz")
    .with_package_hash("deadbeef");

    assert_eq!(meta.homepage, Some("https://example.com".to_string()));
    assert_eq!(meta.capabilities.len(), 1);
    assert_eq!(meta.signature_fingerprint, Some("abc123".to_string()));
    assert_eq!(
        meta.download_url,
        Some("https://example.com/plugin.tar.gz".to_string())
    );
    assert_eq!(meta.package_hash, Some("deadbeef".to_string()));
}

// ===================================================================
// 6. Plugin Signing & Trust Store Tests
// ===================================================================

#[test]
fn sign_roundtrip_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"plugin binary content";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(data, &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn sign_tampered_data_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"original content";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut tampered = data.to_vec();
    tampered[0] ^= 0xFF;
    assert!(!Ed25519Verifier::verify(&tampered, &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn sign_wrong_key_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;
    let data = b"signed by kp1 only";
    let sig = Ed25519Signer::sign(data, &kp1.signing_key)?;
    assert!(!Ed25519Verifier::verify(data, &sig, &kp2.public_key)?);
    Ok(())
}

#[test]
fn sign_empty_payload() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"", &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(b"", &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn sign_large_payload() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = vec![0xCD; 512 * 1024]; // 512KB
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(&data, &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn sign_unique_keys_each_generation() -> Result<(), Box<dyn std::error::Error>> {
    let mut fps = std::collections::HashSet::new();
    for _ in 0..5 {
        let kp = gen_keypair()?;
        assert!(fps.insert(kp.fingerprint()));
    }
    Ok(())
}

#[test]
fn sign_detached_signature_file_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("test.wasm");
    let content = b"wasm binary";
    std::fs::write(&path, content)?;

    let kp = gen_keypair()?;
    let meta = sign_bytes(content, &kp, "Tester")?;
    openracing_crypto::utils::create_detached_signature(&path, &meta)?;

    let sig_path = openracing_crypto::utils::get_signature_path(&path);
    assert!(sig_path.exists());

    let extracted = openracing_crypto::utils::extract_signature_metadata(&path)?;
    assert!(extracted.is_some());
    if let Some(m) = extracted {
        assert_eq!(m.signer, "Tester");
    }
    Ok(())
}

#[test]
fn trust_store_add_and_lookup() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;

    store.add_key(
        kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("Test key".to_string()),
    )?;

    assert_eq!(
        store.get_trust_level(&kp.fingerprint()),
        TrustLevel::Trusted
    );
    assert!(store.is_key_trusted(&kp.fingerprint()));
    Ok(())
}

#[test]
fn trust_store_unknown_key_returns_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    assert_eq!(
        store.get_trust_level(&kp.fingerprint()),
        TrustLevel::Unknown
    );
    assert!(!store.is_key_trusted(&kp.fingerprint()));
    Ok(())
}

#[test]
fn trust_store_distrusted_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;

    store.add_key(
        kp.public_key.clone(),
        TrustLevel::Distrusted,
        Some("Compromised".to_string()),
    )?;

    assert_eq!(
        store.get_trust_level(&kp.fingerprint()),
        TrustLevel::Distrusted
    );
    assert!(!store.is_key_trusted(&kp.fingerprint()));
    Ok(())
}

#[test]
fn trust_store_fail_closed_rejects_all() {
    let store = TrustStore::new_fail_closed("test failure");
    assert!(store.is_failed());

    let fake_fp = "0000000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(store.get_trust_level(fake_fp), TrustLevel::Distrusted);
    assert!(store.get_public_key(fake_fp).is_none());
}

#[test]
fn trust_store_remove_user_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();

    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;
    assert!(store.is_key_trusted(&fp));

    let removed = store.remove_key(&fp)?;
    assert!(removed);
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Unknown);
    Ok(())
}

#[test]
fn trust_store_stats() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;

    store.add_key(kp1.public_key.clone(), TrustLevel::Trusted, None)?;
    store.add_key(kp2.public_key.clone(), TrustLevel::Distrusted, None)?;

    let stats = store.get_stats();
    // 1 default system key (placeholder) + 1 user trusted
    assert!(stats.trusted_keys >= 2);
    assert!(stats.distrusted_keys >= 1);
    Ok(())
}

#[test]
fn sign_native_unsigned_rejected_strict() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("unsigned.dll");
    std::fs::write(&path, b"fake binary")?;

    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());

    let result = verifier.verify(&path);
    assert!(result.is_err(), "strict must reject unsigned");
    Ok(())
}

#[test]
fn sign_native_unsigned_allowed_dev() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("unsigned.dll");
    std::fs::write(&path, b"fake binary")?;

    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::development());

    let result = verifier.verify(&path)?;
    assert!(result.verified, "dev must allow unsigned");
    assert!(!result.is_signed);
    Ok(())
}

#[test]
fn sign_native_signed_trusted_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("signed.dll");
    let content = b"signed plugin binary";
    std::fs::write(&path, content)?;

    let kp = gen_keypair()?;
    let meta = sign_bytes(content, &kp, "Trusted Author")?;
    let sig_path = path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&meta)?)?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path)?;

    assert!(result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Trusted);
    Ok(())
}

#[test]
fn sign_native_tampered_content_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("tampered.dll");
    let original = b"original binary";
    std::fs::write(&path, original)?;

    let kp = gen_keypair()?;
    let meta = sign_bytes(original, &kp, "Author")?;
    let sig_path = path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&meta)?)?;

    // Tamper after signing
    std::fs::write(&path, b"TAMPERED binary")?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(result.is_err(), "tampered content must fail");
    Ok(())
}

#[test]
fn sign_native_distrusted_key_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("evil.dll");
    let content = b"evil plugin";
    std::fs::write(&path, content)?;

    let kp = gen_keypair()?;
    let meta = sign_bytes(content, &kp, "Evil Signer")?;
    let sig_path = path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&meta)?)?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Distrusted, None)?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(result.is_err(), "distrusted key must be rejected");
    Ok(())
}

#[test]
fn sign_native_unknown_key_strict_rejects_or_unverified() -> Result<(), Box<dyn std::error::Error>>
{
    let temp = TempDir::new()?;
    let path = temp.path().join("unknown.dll");
    let content = b"unknown signer plugin";
    std::fs::write(&path, content)?;

    let kp = gen_keypair()?;
    let meta = sign_bytes(content, &kp, "Unknown Author")?;
    let sig_path = path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&meta)?)?;

    // Empty trust store — key unknown
    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);

    match result {
        Err(_) => { /* rejection is valid */ }
        Ok(r) => {
            assert!(
                !r.verified,
                "unknown key in strict mode must not be verified"
            );
        }
    }
    Ok(())
}

#[test]
fn sign_fail_closed_trust_store_rejects_signed_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let path = temp.path().join("fc_test.dll");
    let content = b"plugin for fail-closed test";
    std::fs::write(&path, content)?;

    let kp = gen_keypair()?;
    let meta = sign_bytes(content, &kp, "Author")?;
    let sig_path = path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&meta)?)?;

    let store = TrustStore::new_fail_closed("simulated failure");
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);

    match result {
        Err(_) => { /* rejection is correct for fail-closed */ }
        Ok(r) => {
            assert!(
                !r.verified || r.trust_level == TrustLevel::Distrusted,
                "fail-closed must not produce trusted+verified"
            );
        }
    }
    Ok(())
}

// ===================================================================
// Additional: Lifecycle & manifest edge cases
// ===================================================================

#[test]
fn lifecycle_init_failure_prevents_loading() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INIT_FAIL_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err(), "init failure must prevent loading");
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn lifecycle_re_enable_after_crash() -> Result<(), Box<dyn std::error::Error>> {
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
fn lifecycle_hot_reload_preserves_stats() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![])?;
    for _ in 0..5 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }
    let (count_before, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_before, 5);

    runtime.reload_plugin(&id, &double_wasm, vec![])?;

    let out = runtime.process(&id, 3.0, 0.001)?;
    assert!((out - 6.0).abs() < f32::EPSILON, "reloaded to doubler");

    let (count_after, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_after, 6, "5 before + 1 after reload");
    Ok(())
}

#[test]
fn lifecycle_reload_invalid_keeps_old() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.reload_plugin(&id, b"garbage", vec![]);
    assert!(result.is_err());

    // Old plugin must still work
    let out = runtime.process(&id, 5.0, 0.001)?;
    assert!((out - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn lifecycle_unload_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let result = runtime.unload_plugin(&Uuid::new_v4());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn lifecycle_process_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let result = runtime.process(&Uuid::new_v4(), 1.0, 0.001);
    assert!(result.is_err());
    Ok(())
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
fn manifest_safe_over_execution_limit() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_execution_time_us = 6000; // safe max 5000
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_over_memory_limit() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_memory_bytes = 32 * 1024 * 1024; // safe max 16MB
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_over_update_rate() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.update_rate_hz = 300; // safe max 200
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_fast_over_execution_limit() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 500; // fast max 200
    m.constraints.max_memory_bytes = 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_at_exact_safe_limits() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_execution_time_us = 5000;
    m.constraints.max_memory_bytes = 16 * 1024 * 1024;
    m.constraints.update_rate_hz = 200;
    validator.validate(&m)
}

#[test]
fn manifest_at_exact_fast_limits() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn quarantine_manual_and_release() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.manual_quarantine(id, 30)?;
    assert!(manager.is_quarantined(id));

    manager.release_from_quarantine(id)?;
    assert!(!manager.is_quarantined(id));
    Ok(())
}

#[test]
fn quarantine_release_unknown_fails() {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let result = manager.release_from_quarantine(Uuid::new_v4());
    assert!(result.is_err());
}

#[test]
fn quarantine_only_affects_target_plugin() -> Result<(), PluginError> {
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
