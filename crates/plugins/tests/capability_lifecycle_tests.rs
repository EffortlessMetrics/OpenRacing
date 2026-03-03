//! Comprehensive tests for the plugins crate: capabilities, manifest parsing,
//! plugin lifecycle, WASM runtime configuration, and error handling.

use std::path::Path;
use tempfile::tempdir;
use tokio::fs;
use uuid::Uuid;

use racing_wheel_plugins::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::quarantine::{
    FailureTracker, QuarantineManager, QuarantinePolicy, ViolationType,
};
use racing_wheel_plugins::wasm::{PluginId, ResourceLimits, WasmRuntime};
use racing_wheel_plugins::{
    PluginClass, PluginContext, PluginDspOutput, PluginError, PluginLedOutput, PluginOutput,
    PluginStats, PluginTelemetryOutput,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a valid test manifest with sensible defaults for the given class.
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

/// WAT: minimal passthrough plugin (returns input unchanged).
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT: plugin with init that returns 0 (success).
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

/// WAT: plugin with init that returns -1 (failure).
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

/// WAT: plugin with shutdown export.
const WITH_SHUTDOWN_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
    (func (export "shutdown"))
)
"#;

/// WAT: plugin missing the required "memory" export.
const NO_MEMORY_WAT: &str = r#"
(module
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT: plugin missing the required "process" export.
const NO_PROCESS_WAT: &str = r#"
(module
    (memory (export "memory") 1)
)
"#;

/// WAT: plugin that traps on process call.
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

// ===================================================================
// Capability System Tests
// ===================================================================

#[test]
fn capability_checker_no_capabilities_denies_all() {
    let checker = CapabilityChecker::new(vec![]);

    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    assert!(
        checker
            .check_file_access(Path::new("/tmp/anything"))
            .is_err()
    );
    assert!(checker.check_network_access("example.com").is_err());
}

#[test]
fn capability_checker_all_individual_capabilities() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::ProcessDsp,
        Capability::InterPluginComm,
    ]);

    checker.check_telemetry_read()?;
    checker.check_telemetry_modify()?;
    checker.check_led_control()?;
    checker.check_dsp_processing()?;
    checker.check_inter_plugin_comm()?;
    Ok(())
}

#[test]
fn capability_checker_filesystem_nested_path_allowed() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/data/plugins".to_string()],
    }]);

    checker.check_file_access(Path::new("/data/plugins/sub/deep/file.bin"))?;
    Ok(())
}

#[test]
fn capability_checker_filesystem_sibling_path_denied() {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/data/plugins".to_string()],
    }]);

    assert!(
        checker
            .check_file_access(Path::new("/data/other/file.txt"))
            .is_err()
    );
}

#[test]
fn capability_checker_multiple_filesystem_paths() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string(), "/data".to_string()],
    }]);

    checker.check_file_access(Path::new("/tmp/foo.txt"))?;
    checker.check_file_access(Path::new("/data/bar.bin"))?;
    assert!(checker.check_file_access(Path::new("/etc/passwd")).is_err());
    Ok(())
}

#[test]
fn capability_checker_multiple_network_hosts() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.example.com".to_string(), "cdn.example.com".to_string()],
    }]);

    checker.check_network_access("api.example.com")?;
    checker.check_network_access("cdn.example.com")?;
    assert!(checker.check_network_access("evil.example.com").is_err());
    Ok(())
}

#[test]
fn capability_checker_has_capability_returns_correct_bool() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);
    assert!(checker.has_capability(&Capability::ReadTelemetry));
    assert!(!checker.has_capability(&Capability::ModifyTelemetry));
    assert!(!checker.has_capability(&Capability::ControlLeds));
}

#[test]
fn capability_error_message_contains_capability_name() {
    let checker = CapabilityChecker::new(vec![]);

    let err = checker.check_telemetry_read().err();
    assert!(err.is_some());
    let msg = format!("{}", err.unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("ReadTelemetry"));

    let err = checker.check_dsp_processing().err();
    assert!(err.is_some());
    let msg = format!("{}", err.unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("ProcessDsp"));
}

#[test]
fn capability_file_access_error_contains_path() {
    let checker = CapabilityChecker::new(vec![]);
    let err = checker.check_file_access(Path::new("/secret/file")).err();
    assert!(err.is_some());
    let msg = format!("{}", err.unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("/secret/file"));
}

#[test]
fn capability_network_error_contains_host() {
    let checker = CapabilityChecker::new(vec![]);
    let err = checker.check_network_access("bad.host.com").err();
    assert!(err.is_some());
    let msg = format!("{}", err.unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("bad.host.com"));
}

// ===================================================================
// WasmCapabilityEnforcer Tests
// ===================================================================

#[test]
fn wasm_enforcer_delegates_to_checker() -> Result<(), PluginError> {
    let enforcer =
        WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);
    let checker = enforcer.checker();

    checker.check_telemetry_read()?;
    checker.check_led_control()?;
    assert!(checker.check_dsp_processing().is_err());
    Ok(())
}

#[test]
fn wasm_enforcer_empty_capabilities() {
    let enforcer = WasmCapabilityEnforcer::new(vec![]);
    let checker = enforcer.checker();
    assert!(checker.check_telemetry_read().is_err());
}

// ===================================================================
// Manifest Validation Tests
// ===================================================================

#[test]
fn manifest_validator_safe_allows_all_safe_capabilities() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![
        Capability::ReadTelemetry,
        Capability::ModifyTelemetry,
        Capability::ControlLeds,
        Capability::InterPluginComm,
    ];
    validator.validate(&m)
}

#[test]
fn manifest_validator_fast_allows_process_dsp() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.capabilities = vec![Capability::ProcessDsp];
    m.constraints.max_execution_time_us = 100;
    m.constraints.max_memory_bytes = 2 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn manifest_validator_safe_rejects_filesystem_capability() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_validator_safe_rejects_network_capability() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::Network {
        hosts: vec!["example.com".to_string()],
    }];
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_validator_fast_rejects_filesystem_capability() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.capabilities = vec![Capability::FileSystem {
        paths: vec!["/data".to_string()],
    }];
    m.constraints.max_execution_time_us = 100;
    m.constraints.max_memory_bytes = 2 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_validator_fast_constraint_limits() {
    let validator = ManifestValidator::default();

    // Fast max execution time is 200μs
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 201;
    m.constraints.max_memory_bytes = 2 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());

    // Fast max memory is 4MB
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 100;
    m.constraints.max_memory_bytes = 5 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());

    // Fast max update rate is 1000Hz
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 100;
    m.constraints.max_memory_bytes = 2 * 1024 * 1024;
    m.constraints.update_rate_hz = 1001;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_validator_fast_at_exact_limits_passes() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn manifest_empty_capabilities_passes_validation() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![];
    validator.validate(&m)
}

// ===================================================================
// Manifest YAML Parsing Tests
// ===================================================================

#[tokio::test]
async fn load_manifest_from_valid_yaml_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    let path = temp.path().join("plugin.yaml");
    fs::write(&path, &yaml).await?;

    let loaded = racing_wheel_plugins::manifest::load_manifest(&path).await?;
    assert_eq!(loaded.name, "Test Plugin");
    assert_eq!(loaded.id, manifest.id);
    Ok(())
}

#[tokio::test]
async fn load_manifest_from_invalid_yaml_returns_error() {
    let temp = tempdir();
    assert!(temp.is_ok());
    let temp = temp.unwrap_or_else(|_| unreachable!());
    let path = temp.path().join("plugin.yaml");
    let write_result = fs::write(&path, "not: valid: yaml: [[[").await;
    assert!(write_result.is_ok());

    let result = racing_wheel_plugins::manifest::load_manifest(&path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn load_manifest_from_nonexistent_file_returns_error() {
    let result =
        racing_wheel_plugins::manifest::load_manifest(Path::new("/nonexistent/plugin.yaml")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn load_manifest_rejects_invalid_constraints() {
    let temp = tempdir();
    assert!(temp.is_ok());
    let temp = temp.unwrap_or_else(|_| unreachable!());
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.constraints.max_execution_time_us = 99999; // exceeds Safe limit
    let yaml = serde_yaml::to_string(&manifest);
    assert!(yaml.is_ok());
    let yaml = yaml.unwrap_or_else(|_| unreachable!());
    let path = temp.path().join("plugin.yaml");
    let write_result = fs::write(&path, &yaml).await;
    assert!(write_result.is_ok());

    let result = racing_wheel_plugins::manifest::load_manifest(&path).await;
    assert!(result.is_err());
}

// ===================================================================
// Plugin Types Serialization Tests
// ===================================================================

#[test]
fn plugin_class_json_roundtrip() -> Result<(), serde_json::Error> {
    for class in [PluginClass::Safe, PluginClass::Fast] {
        let json = serde_json::to_string(&class)?;
        let restored: PluginClass = serde_json::from_str(&json)?;
        assert_eq!(class, restored);
    }
    Ok(())
}

#[test]
fn plugin_context_serialization_roundtrip() -> Result<(), serde_json::Error> {
    let ctx = PluginContext {
        plugin_id: Uuid::new_v4(),
        class: PluginClass::Fast,
        update_rate_hz: 1000,
        budget_us: 200,
        capabilities: vec!["ReadTelemetry".to_string(), "ProcessDsp".to_string()],
    };
    let json = serde_json::to_string(&ctx)?;
    let restored: PluginContext = serde_json::from_str(&json)?;
    assert_eq!(restored.class, ctx.class);
    assert_eq!(restored.update_rate_hz, ctx.update_rate_hz);
    assert_eq!(restored.budget_us, ctx.budget_us);
    assert_eq!(restored.capabilities, ctx.capabilities);
    Ok(())
}

#[test]
fn plugin_output_variants_serialize() -> Result<(), serde_json::Error> {
    let telemetry_out = PluginOutput::Telemetry(PluginTelemetryOutput {
        modified_telemetry: None,
        custom_data: serde_json::json!({"key": "value"}),
    });
    let json = serde_json::to_string(&telemetry_out)?;
    let _: PluginOutput = serde_json::from_str(&json)?;

    let led_out = PluginOutput::Led(PluginLedOutput {
        led_pattern: vec![255, 0, 0],
        brightness: 0.8,
        duration_ms: 100,
    });
    let json = serde_json::to_string(&led_out)?;
    let _: PluginOutput = serde_json::from_str(&json)?;

    let dsp_out = PluginOutput::Dsp(PluginDspOutput {
        modified_ffb: 0.5,
        filter_state: serde_json::json!({}),
    });
    let json = serde_json::to_string(&dsp_out)?;
    let _: PluginOutput = serde_json::from_str(&json)?;

    Ok(())
}

#[test]
fn plugin_stats_default_is_zeroed() {
    let stats = PluginStats::default();
    assert_eq!(stats.executions, 0);
    assert_eq!(stats.total_time_us, 0);
    assert_eq!(stats.avg_time_us, 0.0);
    assert_eq!(stats.max_time_us, 0);
    assert_eq!(stats.budget_violations, 0);
    assert_eq!(stats.crashes, 0);
    assert!(stats.last_execution.is_none());
}

// ===================================================================
// Plugin Error Display Tests
// ===================================================================

#[test]
fn plugin_error_display_messages() {
    let err = PluginError::ManifestValidation("bad field".to_string());
    assert!(format!("{err}").contains("bad field"));

    let err = PluginError::LoadingFailed("not found".to_string());
    assert!(format!("{err}").contains("not found"));

    let err = PluginError::ExecutionTimeout {
        duration: std::time::Duration::from_millis(50),
    };
    assert!(format!("{err}").contains("timeout"));

    let err = PluginError::BudgetViolation {
        used_us: 500,
        budget_us: 200,
    };
    let msg = format!("{err}");
    assert!(msg.contains("500"));
    assert!(msg.contains("200"));

    let err = PluginError::Crashed {
        reason: "segfault".to_string(),
    };
    assert!(format!("{err}").contains("segfault"));

    let id = Uuid::new_v4();
    let err = PluginError::Quarantined { plugin_id: id };
    assert!(format!("{err}").contains(&id.to_string()));

    let err = PluginError::CapabilityViolation {
        capability: "ProcessDsp".to_string(),
    };
    assert!(format!("{err}").contains("ProcessDsp"));

    let err = PluginError::NativePluginLoad("abi mismatch".to_string());
    assert!(format!("{err}").contains("abi mismatch"));

    let err = PluginError::Ipc("connection refused".to_string());
    assert!(format!("{err}").contains("connection refused"));
}

// ===================================================================
// WASM Runtime – Plugin Lifecycle Tests
// ===================================================================

#[test]
fn wasm_runtime_load_valid_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 1);
    Ok(())
}

#[test]
fn wasm_runtime_plugin_with_successful_init() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(INIT_OK_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);
    Ok(())
}

#[test]
fn wasm_runtime_plugin_with_failed_init_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(INIT_FAIL_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn wasm_runtime_load_missing_memory_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(NO_MEMORY_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("memory"));
    Ok(())
}

#[test]
fn wasm_runtime_load_missing_process_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(NO_PROCESS_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("process"));
    Ok(())
}

#[test]
fn wasm_runtime_load_invalid_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    let result = runtime.load_plugin_from_bytes(id, b"not wasm bytes at all", vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn wasm_runtime_process_passthrough() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, 0.42, 0.001)?;
    assert!((output - 0.42).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wasm_runtime_process_updates_stats() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    runtime.process(&id, 0.1, 0.001)?;
    runtime.process(&id, 0.2, 0.001)?;
    runtime.process(&id, 0.3, 0.001)?;

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 3);
    Ok(())
}

#[test]
fn wasm_runtime_unload_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn wasm_runtime_unload_with_shutdown_export() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(WITH_SHUTDOWN_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn wasm_runtime_unload_nonexistent_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();
    let result = runtime.unload_plugin(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn wasm_runtime_process_nonexistent_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();
    let result = runtime.process(&id, 0.5, 0.001);
    assert!(result.is_err());
    Ok(())
}

// ===================================================================
// WASM Runtime – Crash Isolation
// ===================================================================

#[test]
fn wasm_trap_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.5, 0.001);
    assert!(result.is_err());

    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn wasm_trap_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let bad_wasm = wat::parse_str(TRAP_WAT)?;
    let good_wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let bad_id = PluginId::new_v4();
    let good_id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(bad_id, &bad_wasm, vec![])?;
    runtime.load_plugin_from_bytes(good_id, &good_wasm, vec![])?;

    // Crash the bad plugin
    let crash_result = runtime.process(&bad_id, 0.5, 0.001);
    assert!(crash_result.is_err());

    // Good plugin still works
    let good_result = runtime.process(&good_id, 0.75, 0.001)?;
    assert!((good_result - 0.75).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wasm_disabled_plugin_can_be_reenabled() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 0.5, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn wasm_reenable_non_disabled_plugin_returns_false() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(!was_disabled);
    Ok(())
}

// ===================================================================
// WASM Runtime – Resource Limits
// ===================================================================

#[test]
fn wasm_runtime_max_instance_limit_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;

    let id1 = PluginId::new_v4();
    let id2 = PluginId::new_v4();
    let id3 = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

    // Third should be rejected
    let result = runtime.load_plugin_from_bytes(id3, &wasm, vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("Maximum plugin instances"));
    Ok(())
}

#[test]
fn wasm_resource_limits_builder_chaining() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default()
        .with_memory(8 * 1024 * 1024)
        .with_fuel(5_000_000)
        .with_table_elements(5_000)
        .with_max_instances(16);

    assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 5_000_000);
    assert_eq!(limits.max_table_elements, 5_000);
    assert_eq!(limits.max_instances, 16);

    // Verify runtime can be created with custom limits
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_instances, 16);
    Ok(())
}

// ===================================================================
// WASM Runtime – Hot Reload
// ===================================================================

#[test]
fn wasm_hot_reload_replaces_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_v1 = wat::parse_str(PASSTHROUGH_WAT)?;
    // v2 is still passthrough, just a fresh reload
    let wasm_v2 = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm_v1, vec![])?;
    runtime.process(&id, 0.5, 0.001)?;

    // Reload
    runtime.reload_plugin(&id, &wasm_v2, vec![])?;
    assert!(runtime.has_plugin(&id));

    let output = runtime.process(&id, 0.9, 0.001)?;
    assert!((output - 0.9).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wasm_hot_reload_invalid_bytes_keeps_old() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    runtime.process(&id, 0.5, 0.001)?;

    // Attempt reload with invalid bytes
    let result = runtime.reload_plugin(&id, b"invalid wasm", vec![]);
    assert!(result.is_err());

    // Old plugin must still work
    assert!(runtime.has_plugin(&id));
    let output = runtime.process(&id, 0.3, 0.001)?;
    assert!((output - 0.3).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// WASM Runtime – Telemetry Update
// ===================================================================

#[test]
fn wasm_runtime_update_telemetry_nonexistent_plugin_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();
    let frame = racing_wheel_plugins::abi::TelemetryFrame::default();
    let result = runtime.update_plugin_telemetry(&id, frame);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn wasm_runtime_update_telemetry_for_loaded_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let frame = racing_wheel_plugins::abi::TelemetryFrame {
        timestamp_us: 42000,
        wheel_angle_deg: 90.0,
        wheel_speed_rad_s: std::f32::consts::PI,
        temperature_c: 55.0,
        fault_flags: 0,
        _pad: 0,
    };
    runtime.update_plugin_telemetry(&id, frame)?;
    Ok(())
}

// ===================================================================
// Quarantine – Additional Edge Cases
// ===================================================================

#[test]
fn quarantine_escalation_increases_level() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        quarantine_duration_minutes: 5,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    // First quarantine
    manager.record_violation(id, ViolationType::Crash, "crash 1".to_string())?;
    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    assert_eq!(state.unwrap_or_else(|| unreachable!()).escalation_level, 1);

    // Release and crash again
    manager.release_from_quarantine(id)?;
    manager.record_violation(id, ViolationType::Crash, "crash 2".to_string())?;
    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    assert_eq!(state.unwrap_or_else(|| unreachable!()).escalation_level, 2);
    Ok(())
}

#[test]
fn quarantine_timeout_violation_does_not_increment_counters() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy::default());
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::TimeoutViolation, "timed out".to_string())?;

    let state = manager.get_quarantine_state(id);
    assert!(state.is_some());
    let s = state.unwrap_or_else(|| unreachable!());
    assert_eq!(s.total_crashes, 0);
    assert_eq!(s.total_budget_violations, 0);
    assert_eq!(s.recent_violations.len(), 1);
    Ok(())
}

#[test]
fn quarantine_multiple_plugins_independent() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_crashes: 1,
        ..QuarantinePolicy::default()
    });

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    manager.record_violation(id_a, ViolationType::Crash, "crash a".to_string())?;
    assert!(manager.is_quarantined(id_a));
    assert!(!manager.is_quarantined(id_b));

    // Plugin B is still fine
    let stats = manager.get_quarantine_stats();
    assert!(stats.contains_key(&id_a));
    assert!(!stats.contains_key(&id_b));
    Ok(())
}

#[test]
fn failure_tracker_separate_plugins() {
    let mut tracker = FailureTracker::new();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    tracker.record_execution(id_a, 100, true);
    tracker.record_execution(id_b, 200, false);

    let stats_a = tracker.get_stats(id_a);
    assert!(stats_a.is_some());
    assert_eq!(stats_a.unwrap_or_else(|| unreachable!()).crashes, 0);

    let stats_b = tracker.get_stats(id_b);
    assert!(stats_b.is_some());
    assert_eq!(stats_b.unwrap_or_else(|| unreachable!()).crashes, 1);
}

// ===================================================================
// Plugin Host – Integration Tests
// ===================================================================

#[tokio::test]
async fn plugin_host_empty_directory() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;

    let registry = host.get_registry().await;
    assert!(registry.is_empty());

    let stats = host.get_quarantine_stats().await;
    assert!(stats.is_empty());
    Ok(())
}

#[tokio::test]
async fn plugin_host_load_nonexistent_plugin_returns_error()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;

    let result = host.load_plugin(Uuid::new_v4()).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn plugin_host_discovers_plugin_from_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("my-plugin");
    fs::create_dir_all(&plugin_dir).await?;

    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    fs::write(plugin_dir.join("plugin.yaml"), &yaml).await?;
    fs::write(plugin_dir.join("plugin.wasm"), b"fake wasm").await?;

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert_eq!(registry.len(), 1);

    let (_, entry) = registry.iter().next().unwrap_or_else(|| unreachable!());
    assert_eq!(entry.manifest.name, "Test Plugin");
    assert!(!entry.is_loaded);
    assert!(entry.is_enabled);
    Ok(())
}

#[tokio::test]
async fn plugin_host_skips_invalid_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("broken-plugin");
    fs::create_dir_all(&plugin_dir).await?;

    fs::write(plugin_dir.join("plugin.yaml"), "invalid: yaml: [[[").await?;

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert!(registry.is_empty());
    Ok(())
}

#[tokio::test]
async fn plugin_host_skips_plugin_without_wasm_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("no-wasm");
    fs::create_dir_all(&plugin_dir).await?;

    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    fs::write(plugin_dir.join("plugin.yaml"), &yaml).await?;
    // Intentionally do NOT create plugin.wasm

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert!(registry.is_empty());
    Ok(())
}

#[tokio::test]
async fn plugin_host_set_enabled_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("toggle-plugin");
    fs::create_dir_all(&plugin_dir).await?;

    let manifest = make_manifest(PluginClass::Safe);
    let plugin_id = manifest.id;
    let yaml = serde_yaml::to_string(&manifest)?;
    fs::write(plugin_dir.join("plugin.yaml"), &yaml).await?;
    fs::write(plugin_dir.join("plugin.wasm"), b"fake").await?;

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;

    // Disable the plugin
    host.set_plugin_enabled(plugin_id, false).await?;
    let registry = host.get_registry().await;
    let entry = registry.get(&plugin_id).unwrap_or_else(|| unreachable!());
    assert!(!entry.is_enabled);

    // Re-enable
    host.set_plugin_enabled(plugin_id, true).await?;
    Ok(())
}

// ===================================================================
// Capability Enum – Equality & Hashing
// ===================================================================

#[test]
fn capability_equality_and_hashing() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(Capability::ReadTelemetry);
    set.insert(Capability::ReadTelemetry); // duplicate
    set.insert(Capability::ModifyTelemetry);
    assert_eq!(set.len(), 2);

    // FileSystem with same paths are equal
    let fs1 = Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    };
    let fs2 = Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    };
    assert_eq!(fs1, fs2);

    // Network with different hosts are not equal
    let net1 = Capability::Network {
        hosts: vec!["a.com".to_string()],
    };
    let net2 = Capability::Network {
        hosts: vec!["b.com".to_string()],
    };
    assert_ne!(net1, net2);
}

// ===================================================================
// PluginOperation Enum Tests
// ===================================================================

#[test]
fn plugin_operation_equality() {
    assert_eq!(
        PluginOperation::TelemetryProcessor,
        PluginOperation::TelemetryProcessor
    );
    assert_ne!(PluginOperation::LedMapper, PluginOperation::DspFilter);
    assert_ne!(
        PluginOperation::TelemetrySource,
        PluginOperation::TelemetryProcessor
    );
}

// ===================================================================
// Full Lifecycle: load → process → reload → unload
// ===================================================================

#[test]
fn full_plugin_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = PluginId::new_v4();

    // Load
    runtime.load_plugin_from_bytes(id, &wasm, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));
    assert!(runtime.is_plugin_initialized(&id)?);

    // Process several times
    for i in 0..5 {
        let input = i as f32 * 0.1;
        let output = runtime.process(&id, input, 0.001)?;
        assert!((output - input).abs() < f32::EPSILON);
    }

    let (count, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 5);

    // Hot-reload
    let wasm_v2 = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.reload_plugin(&id, &wasm_v2, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));

    // Process after reload
    let output = runtime.process(&id, 0.99, 0.001)?;
    assert!((output - 0.99).abs() < f32::EPSILON);

    // Unload
    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 0);

    Ok(())
}
