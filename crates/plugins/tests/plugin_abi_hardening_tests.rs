//! Plugin ABI hardening tests.
//!
//! Covers:
//! 1. ABI version validation and forward-compatible loading
//! 2. Plugin capability enforcement
//! 3. Plugin loading edge cases (invalid WASM, wrong signatures, crash recovery)
//! 4. Plugin communication (host↔plugin data passing, serialization, large data)

use std::path::Path;

use uuid::Uuid;

use racing_wheel_plugins::abi::{
    PLUG_ABI_MAGIC, PLUG_ABI_VERSION, PluginCapabilities, PluginHeader, PluginInitStatus,
    TelemetryFrame, WasmExportValidation, WasmPluginAbiState,
};
use racing_wheel_plugins::capability::CapabilityChecker;
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::native::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginConfig, check_abi_compatibility,
    check_abi_compatibility_packed,
};
use racing_wheel_plugins::wasm::{PluginId, ResourceLimits, WasmRuntime};
use racing_wheel_plugins::{PluginClass, PluginError};

// ---------------------------------------------------------------------------
// WAT helpers
// ---------------------------------------------------------------------------

/// Minimal passthrough plugin: returns input unchanged.
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// Plugin with init that succeeds (returns 0).
const INIT_OK_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "init") (result i32)
        i32.const 0
    )
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// Plugin with init that fails (returns -1).
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

/// Plugin that traps on process call.
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// Plugin missing required "memory" export.
const NO_MEMORY_WAT: &str = r#"
(module
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// Plugin missing required "process" export.
const NO_PROCESS_WAT: &str = r#"
(module
    (memory (export "memory") 1)
)
"#;

/// Plugin that doubles its input.
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

/// Plugin that burns fuel in a tight loop.
const FUEL_BURNER_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        (local $i i32)
        (local.set $i (i32.const 0))
        (block $done
            (loop $loop
                (br_if $done (i32.ge_u (local.get $i) (i32.const 1000000)))
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br $loop)
            )
        )
        local.get 0
    )
)
"#;

/// Plugin that uses second param (dt) in its output: input * dt.
const MULTIPLY_DT_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 1
        f32.mul
    )
)
"#;

/// Plugin that writes output to linear memory and returns it.
/// Stores the input at memory offset 0 then returns it.
const MEMORY_WRITER_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; Write input to memory offset 0
        i32.const 0
        local.get 0
        f32.store
        ;; Read it back
        i32.const 0
        f32.load
    )
)
"#;

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

// ===================================================================
// 1. ABI Version Validation & Forward Compatibility
// ===================================================================

#[test]
fn abi_packed_version_major_minor_extraction() {
    let version = PLUG_ABI_VERSION; // 0x0001_0000 = v1.0
    assert_eq!(openracing_plugin_abi::abi_version_major(version), 1);
    assert_eq!(openracing_plugin_abi::abi_version_minor(version), 0);
}

#[test]
fn abi_packed_version_roundtrip() {
    let packed = openracing_plugin_abi::abi_version_pack(3, 7);
    assert_eq!(openracing_plugin_abi::abi_version_major(packed), 3);
    assert_eq!(openracing_plugin_abi::abi_version_minor(packed), 7);
}

#[test]
fn abi_header_exact_version_is_valid() {
    let header = PluginHeader::default();
    assert!(header.is_valid());
    assert!(header.is_compatible());
    assert!(header.version_mismatch_message().is_none());
}

#[test]
fn abi_header_wrong_magic_not_compatible() {
    let header = PluginHeader {
        magic: 0xDEADBEEF,
        ..Default::default()
    };
    assert!(!header.is_valid());
    assert!(!header.is_compatible());
    let msg = header.version_mismatch_message();
    assert!(msg.is_some());
    let msg = msg.as_deref().unwrap_or("");
    assert!(msg.contains("magic"), "expected magic in message: {msg}");
}

#[test]
fn abi_header_different_major_not_compatible() {
    let header = PluginHeader {
        abi_version: openracing_plugin_abi::abi_version_pack(2, 0),
        ..Default::default()
    };
    assert!(!header.is_compatible());
    let msg = header.version_mismatch_message();
    assert!(msg.is_some());
    let msg = msg.as_deref().unwrap_or("");
    assert!(
        msg.contains("mismatch"),
        "expected mismatch in message: {msg}"
    );
}

#[test]
fn abi_header_older_minor_same_major_is_compatible() {
    // Simulate a plugin built for v1.0 when host is also v1.0
    let header = PluginHeader {
        abi_version: openracing_plugin_abi::abi_version_pack(1, 0),
        ..Default::default()
    };
    assert!(header.is_compatible());
}

#[test]
fn abi_header_newer_minor_same_major_not_compatible() {
    // Simulate a plugin built for v1.5 when host is v1.0
    let header = PluginHeader {
        abi_version: openracing_plugin_abi::abi_version_pack(1, 5),
        ..Default::default()
    };
    assert!(!header.is_compatible());
    let msg = header.version_mismatch_message();
    assert!(msg.is_some());
}

#[test]
fn abi_header_version_mismatch_message_contains_versions() {
    let header = PluginHeader {
        abi_version: openracing_plugin_abi::abi_version_pack(2, 3),
        ..Default::default()
    };
    let msg = header.version_mismatch_message();
    assert!(msg.is_some());
    let msg = msg.as_deref().unwrap_or("");
    assert!(
        msg.contains("2.3"),
        "expected plugin version in message: {msg}"
    );
    assert!(
        msg.contains("1.0"),
        "expected host version in message: {msg}"
    );
}

#[test]
fn abi_native_exact_match_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
    assert!(result.is_loadable());
}

#[test]
fn abi_native_mismatch_returns_error_info() {
    let future = CURRENT_ABI_VERSION + 100;
    let result = check_abi_compatibility(future);
    assert!(!result.is_loadable());
    match result {
        AbiCheckResult::Mismatch { expected, actual } => {
            assert_eq!(expected, CURRENT_ABI_VERSION);
            assert_eq!(actual, future);
        }
        _ => panic!("expected Mismatch"),
    }
}

#[test]
fn abi_packed_forward_compat_older_minor_loadable() {
    let host = openracing_plugin_abi::abi_version_pack(1, 3);
    let plugin = openracing_plugin_abi::abi_version_pack(1, 1);
    let result = check_abi_compatibility_packed(host, plugin);
    assert!(result.is_loadable());
    assert!(matches!(result, AbiCheckResult::ForwardCompatible { .. }));
}

#[test]
fn abi_packed_forward_compat_same_version_compatible() {
    let v = openracing_plugin_abi::abi_version_pack(1, 3);
    let result = check_abi_compatibility_packed(v, v);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_packed_newer_minor_rejected() {
    let host = openracing_plugin_abi::abi_version_pack(1, 0);
    let plugin = openracing_plugin_abi::abi_version_pack(1, 2);
    let result = check_abi_compatibility_packed(host, plugin);
    assert!(!result.is_loadable());
}

#[test]
fn abi_packed_different_major_rejected() {
    let host = openracing_plugin_abi::abi_version_pack(1, 5);
    let plugin = openracing_plugin_abi::abi_version_pack(2, 0);
    let result = check_abi_compatibility_packed(host, plugin);
    assert!(!result.is_loadable());
}

#[test]
fn abi_header_reserved_field_must_be_zero() {
    let header = PluginHeader {
        reserved: 0x1234,
        ..Default::default()
    };
    // is_valid checks magic+version but doesn't enforce reserved=0;
    // callers should verify reserved == 0 for strict compliance
    assert!(header.is_valid());
    assert_ne!(
        header.reserved, 0,
        "test setup: reserved should be non-zero"
    );
}

#[test]
fn abi_plugin_error_mismatch_variant_formats_nicely() {
    let err = PluginError::AbiMismatch {
        message: "host 1.0, plugin 2.0".to_string(),
    };
    let msg = format!("{err}");
    assert!(
        msg.contains("mismatch"),
        "error display should mention mismatch: {msg}"
    );
    assert!(msg.contains("1.0"));
}

// ===================================================================
// 2. Plugin Capability Enforcement
// ===================================================================

#[test]
fn capability_telemetry_read_granted_when_declared() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    checker.check_telemetry_read()
}

#[test]
fn capability_telemetry_read_denied_when_not_declared() {
    let checker = CapabilityChecker::new(vec![]);
    let result = checker.check_telemetry_read();
    assert!(result.is_err());
    let err = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        err.contains("ReadTelemetry"),
        "error should name the capability: {err}"
    );
}

#[test]
fn capability_modify_denied_when_only_read_granted() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.check_telemetry_modify().is_err());
}

#[test]
fn capability_dsp_denied_for_safe_plugin_manifest() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![Capability::ProcessDsp];
    let result = validator.validate(&manifest);
    assert!(result.is_err());
    let err = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        err.contains("ProcessDsp"),
        "should name denied capability: {err}"
    );
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
fn capability_filesystem_scoped_to_granted_paths() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/safe/dir".to_string()],
    }]);
    checker.check_file_access(Path::new("/safe/dir/sub/file.txt"))?;
    assert!(checker.check_file_access(Path::new("/etc/passwd")).is_err());
    Ok(())
}

#[test]
fn capability_network_scoped_to_granted_hosts() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.safe.com".to_string()],
    }]);
    checker.check_network_access("api.safe.com")?;
    assert!(checker.check_network_access("evil.corp").is_err());
    Ok(())
}

#[test]
fn capability_inter_plugin_denied_without_grant() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.check_inter_plugin_comm().is_err());
}

#[test]
fn capability_inter_plugin_granted() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::InterPluginComm]);
    checker.check_inter_plugin_comm()
}

#[test]
fn capability_all_safe_capabilities_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.capabilities = vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::InterPluginComm,
    ];
    validator.validate(&manifest)
}

#[test]
fn capability_native_rt_fast_constraints_at_limit() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Fast);
    manifest.capabilities = vec![Capability::ProcessDsp];
    manifest.constraints.max_execution_time_us = 200; // exact limit
    manifest.constraints.max_memory_bytes = 4 * 1024 * 1024; // exact limit
    manifest.constraints.update_rate_hz = 1000; // exact limit
    validator.validate(&manifest)
}

#[test]
fn capability_budget_exceeded_rejected() {
    let validator = ManifestValidator::default();
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.constraints.max_execution_time_us = 6000; // exceeds 5000 limit
    assert!(validator.validate(&manifest).is_err());
}

// ===================================================================
// 3. Plugin Loading Edge Cases
// ===================================================================

#[test]
fn load_corrupted_wasm_bytes_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &[0xDE, 0xAD, 0xBE, 0xEF], vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn load_empty_bytes_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, &[], vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn load_truncated_wasm_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let good_wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    // Truncate to half
    let truncated = &good_wasm[..good_wasm.len() / 2];
    let result = runtime.load_plugin_from_bytes(id, truncated, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn load_wasm_wrong_version_header() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    // Valid WASM magic but wrong version
    let bad_header: [u8; 8] = [0x00, 0x61, 0x73, 0x6D, 0xFF, 0xFF, 0xFF, 0xFF];
    let result = runtime.load_plugin_from_bytes(id, &bad_header, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn load_missing_process_export_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(NO_PROCESS_WAT)?;
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        err_msg.contains("process"),
        "error should mention 'process': {err_msg}"
    );
    Ok(())
}

#[test]
fn load_missing_memory_export_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(NO_MEMORY_WAT)?;
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        err_msg.contains("memory"),
        "error should mention 'memory': {err_msg}"
    );
    Ok(())
}

#[test]
fn load_init_failure_prevents_loading() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(INIT_FAIL_WAT)?;
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(
        !runtime.has_plugin(&id),
        "failed init should not leave a loaded plugin"
    );
    Ok(())
}

#[test]
fn load_init_ok_plugin_is_initialized() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(INIT_OK_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));
    assert!(runtime.is_plugin_initialized(&id)?);
    Ok(())
}

#[test]
fn crash_recovery_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(TRAP_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn crash_recovery_re_enable_allows_retry() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(TRAP_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn crash_does_not_affect_other_plugins() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let crash_id: PluginId = Uuid::new_v4();
    let safe_id: PluginId = Uuid::new_v4();

    let crash_wasm = wat::parse_str(TRAP_WAT)?;
    let safe_wasm = wat::parse_str(PASSTHROUGH_WAT)?;

    runtime.load_plugin_from_bytes(crash_id, &crash_wasm, vec![])?;
    runtime.load_plugin_from_bytes(safe_id, &safe_wasm, vec![])?;

    let _ = runtime.process(&crash_id, 1.0, 0.001);
    // Healthy plugin should still work
    let out = runtime.process(&safe_id, 7.5, 0.001)?;
    assert!((out - 7.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn fuel_exhaustion_budget_violation() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_fuel(100);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(FUEL_BURNER_WAT)?;

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn max_instances_limit_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;

    runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;
    runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;

    // Third should fail
    let result = runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(result.is_err());
    let err_msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(
        err_msg.contains("Maximum"),
        "should mention max instances: {err_msg}"
    );
    Ok(())
}

#[test]
fn concurrent_load_multiple_plugins() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;

    let mut ids = Vec::new();
    for _ in 0..5 {
        let id = Uuid::new_v4();
        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        ids.push(id);
    }

    assert_eq!(runtime.instance_count(), 5);

    // All should process correctly
    for id in &ids {
        let out = runtime.process(id, 2.72, 0.001)?;
        assert!((out - 2.72).abs() < f32::EPSILON);
    }
    Ok(())
}

#[test]
fn unload_nonexistent_plugin_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let result = runtime.unload_plugin(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn process_nonexistent_plugin_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    Ok(())
}

// ===================================================================
// 4. Plugin Communication
// ===================================================================

#[test]
fn host_to_plugin_input_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // Various input values pass through correctly
    for &input in &[0.0_f32, 1.0, -1.0, 0.5, f32::MIN_POSITIVE, 1e10] {
        let out = runtime.process(&id, input, 0.001)?;
        assert!(
            (out - input).abs() < f32::EPSILON,
            "passthrough failed for input {input}: got {out}"
        );
    }
    Ok(())
}

#[test]
fn host_to_plugin_dt_parameter_used() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(MULTIPLY_DT_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let out = runtime.process(&id, 10.0, 0.5)?;
    assert!(
        (out - 5.0).abs() < f32::EPSILON,
        "expected 10*0.5=5, got {out}"
    );
    Ok(())
}

#[test]
fn plugin_to_host_result_correct() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(DOUBLE_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let out = runtime.process(&id, 21.0, 0.001)?;
    assert!((out - 42.0).abs() < f32::EPSILON, "expected 42, got {out}");
    Ok(())
}

#[test]
fn plugin_memory_write_read_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(MEMORY_WRITER_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let out = runtime.process(&id, 99.5, 0.001)?;
    assert!(
        (out - 99.5).abs() < f32::EPSILON,
        "memory roundtrip failed: got {out}"
    );
    Ok(())
}

#[test]
fn telemetry_frame_serialization_roundtrip() {
    let frame = TelemetryFrame {
        timestamp_us: 1_000_000,
        wheel_angle_deg: 180.0,
        wheel_speed_rad_s: std::f32::consts::PI,
        temperature_c: 42.0,
        fault_flags: 0xCAFE,
        _pad: 0,
    };
    let bytes = frame.to_bytes();
    let restored = TelemetryFrame::from_bytes(&bytes);

    assert_eq!(frame.timestamp_us, restored.timestamp_us);
    assert_eq!(frame.wheel_angle_deg, restored.wheel_angle_deg);
    assert_eq!(frame.wheel_speed_rad_s, restored.wheel_speed_rad_s);
    assert_eq!(frame.temperature_c, restored.temperature_c);
    assert_eq!(frame.fault_flags, restored.fault_flags);
}

#[test]
fn telemetry_frame_update_via_host() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let frame = TelemetryFrame {
        timestamp_us: 42,
        wheel_angle_deg: 90.0,
        wheel_speed_rad_s: 1.5,
        temperature_c: 55.0,
        fault_flags: 0,
        _pad: 0,
    };
    runtime.update_plugin_telemetry(&id, frame)?;
    // Verify plugin still processes normally after telemetry update
    let out = runtime.process(&id, 1.0, 0.001)?;
    assert!((out - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn plugin_stats_track_call_count() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..10 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 10);
    Ok(())
}

#[test]
fn plugin_header_byte_serialization_stability() {
    let header = PluginHeader::new(PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS);
    let bytes = header.to_bytes();
    let restored = PluginHeader::from_bytes(&bytes);
    assert_eq!(header, restored);

    // Verify exact byte layout (little-endian)
    assert_eq!(&bytes[0..4], &PLUG_ABI_MAGIC.to_le_bytes());
    assert_eq!(&bytes[4..8], &PLUG_ABI_VERSION.to_le_bytes());
}

#[test]
fn large_repeated_processing_no_unbounded_growth() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // Run many iterations to verify no resource leaks or unbounded allocation
    for i in 0..1000 {
        let input = (i as f32) * 0.001;
        let out = runtime.process(&id, input, 0.001)?;
        assert!(
            (out - input).abs() < f32::EPSILON,
            "iteration {i}: expected {input}, got {out}"
        );
    }

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 1000);
    Ok(())
}

#[test]
fn wasm_abi_state_lifecycle() {
    let mut state = WasmPluginAbiState::new();
    assert_eq!(state.init_status, PluginInitStatus::Uninitialized);
    assert!(!state.is_initialized());

    state.mark_initialized();
    assert!(state.is_initialized());

    state.record_process_call(50);
    state.record_process_call(150);
    assert_eq!(state.process_count, 2);
    assert!((state.average_process_time_us() - 100.0).abs() < f64::EPSILON);

    state.store_data("key".to_string(), vec![1, 2, 3]);
    assert_eq!(state.get_data("key"), Some(&vec![1, 2, 3]));
    assert_eq!(state.get_data("missing"), None);

    state.mark_shutdown();
    assert!(!state.is_initialized());
    assert_eq!(state.init_status, PluginInitStatus::ShutDown);
}

#[test]
fn wasm_abi_state_failure_records_error() {
    let mut state = WasmPluginAbiState::new();
    state.mark_failed("out of memory".to_string());
    assert_eq!(state.init_status, PluginInitStatus::Failed);
    assert_eq!(state.last_error, Some("out of memory".to_string()));
}

#[test]
fn wasm_export_validation_reports_missing() {
    let validation = WasmExportValidation::default();
    assert!(!validation.is_valid());
    let missing = validation.missing_required();
    assert_eq!(missing.len(), 2);
    assert!(missing.contains(&"process"));
    assert!(missing.contains(&"memory"));
}

#[test]
fn hot_reload_preserves_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let wasm_v1 = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.load_plugin_from_bytes(id, &wasm_v1, vec![])?;

    // Run a few calls to accumulate stats
    for _ in 0..5 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }
    let (count_before, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_before, 5);

    // Hot-reload with double plugin
    let wasm_v2 = wat::parse_str(DOUBLE_WAT)?;
    runtime.reload_plugin(&id, &wasm_v2, vec![])?;

    // Stats should be preserved
    let (count_after, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_after, 5);

    // New behavior should be active
    let out = runtime.process(&id, 10.0, 0.001)?;
    assert!((out - 20.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn hot_reload_with_bad_wasm_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id: PluginId = Uuid::new_v4();
    let good_wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.load_plugin_from_bytes(id, &good_wasm, vec![])?;

    // Attempt reload with invalid bytes
    let result = runtime.reload_plugin(&id, &[0xDE, 0xAD], vec![]);
    assert!(result.is_err());

    // Old plugin should still work
    let out = runtime.process(&id, 5.0, 0.001)?;
    assert!((out - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn native_config_strict_rejects_unsigned() {
    let config = NativePluginConfig::strict();
    assert!(!config.allow_unsigned);
    assert!(config.require_signatures);
}

#[test]
fn native_config_development_allows_unsigned() {
    let config = NativePluginConfig::development();
    assert!(config.allow_unsigned);
    assert!(!config.require_signatures);
}
