//! Wave-15 RC hardening tests for the plugins crate.
//!
//! Covers: manifest parsing/validation, WASM loading configuration,
//! native plugin verification (code signing), capability checking,
//! plugin registry operations, and plugin isolation boundaries.

use std::path::Path;

use tempfile::tempdir;
use tokio::fs;
use uuid::Uuid;

use racing_wheel_plugins::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use racing_wheel_plugins::manifest::{
    Capability, EntryPoints, ManifestValidator, PluginConstraints, PluginManifest, PluginOperation,
};
use racing_wheel_plugins::native::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginConfig, NativePluginHost,
    SignatureVerificationConfig, check_abi_compatibility,
};
use racing_wheel_plugins::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
use racing_wheel_plugins::registry::{
    PluginCatalog, PluginId, PluginMetadata, VersionCompatibility, check_compatibility,
};
use racing_wheel_plugins::wasm::{ResourceLimits, WasmRuntime};
use racing_wheel_plugins::{PluginClass, PluginError};

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

/// WAT: minimal passthrough plugin.
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WAT: plugin that traps on process.
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// WAT: plugin missing required "process" export.
const NO_PROCESS_WAT: &str = r#"
(module
    (memory (export "memory") 1)
)
"#;

/// WAT: plugin missing required "memory" export.
const NO_MEMORY_WAT: &str = r#"
(module
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

// ===================================================================
// 1. Plugin Manifest Parsing & Validation
// ===================================================================

#[test]
fn manifest_safe_boundary_constraints_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    // Exactly at the Safe limits
    m.constraints.max_execution_time_us = 5000;
    m.constraints.max_memory_bytes = 16 * 1024 * 1024;
    m.constraints.update_rate_hz = 200;
    validator.validate(&m)
}

#[test]
fn manifest_safe_one_over_execution_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_execution_time_us = 5001;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_one_over_memory_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.max_memory_bytes = 16 * 1024 * 1024 + 1;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_safe_one_over_update_rate_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.constraints.update_rate_hz = 201;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_fast_boundary_constraints_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn manifest_fast_one_over_execution_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 201;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_fast_one_over_memory_limit_rejected() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.constraints.max_execution_time_us = 200;
    m.constraints.max_memory_bytes = 4 * 1024 * 1024 + 1;
    m.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&m).is_err());
}

#[test]
fn manifest_empty_name_and_author_each_rejected() {
    let validator = ManifestValidator::default();

    let mut m = make_manifest(PluginClass::Safe);
    m.name = String::new();
    let err_msg = format!(
        "{}",
        validator
            .validate(&m)
            .err()
            .unwrap_or_else(|| unreachable!())
    );
    assert!(
        err_msg.contains("name"),
        "error should mention 'name': {err_msg}"
    );

    let mut m = make_manifest(PluginClass::Safe);
    m.author = String::new();
    let err_msg = format!(
        "{}",
        validator
            .validate(&m)
            .err()
            .unwrap_or_else(|| unreachable!())
    );
    assert!(
        err_msg.contains("author"),
        "error should mention 'author': {err_msg}"
    );
}

#[test]
fn manifest_empty_capabilities_accepted() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![];
    validator.validate(&m)
}

#[tokio::test]
async fn manifest_yaml_file_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let manifest = make_manifest(PluginClass::Safe);
    let yaml = serde_yaml::to_string(&manifest)?;
    let path = temp.path().join("plugin.yaml");
    fs::write(&path, &yaml).await?;

    let loaded = racing_wheel_plugins::manifest::load_manifest(&path).await?;
    assert_eq!(loaded.id, manifest.id);
    assert_eq!(loaded.name, manifest.name);
    assert_eq!(loaded.class, manifest.class);
    Ok(())
}

#[tokio::test]
async fn manifest_load_from_nonexistent_path_errors() {
    let result =
        racing_wheel_plugins::manifest::load_manifest(Path::new("nonexistent/plugin.yaml")).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn manifest_load_malformed_yaml_errors() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let path = temp.path().join("plugin.yaml");
    fs::write(&path, "this is not: valid yaml: [[[").await?;

    let result = racing_wheel_plugins::manifest::load_manifest(&path).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn manifest_load_valid_yaml_but_invalid_constraints_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let mut manifest = make_manifest(PluginClass::Safe);
    manifest.constraints.max_execution_time_us = 999_999;
    let yaml = serde_yaml::to_string(&manifest)?;
    let path = temp.path().join("plugin.yaml");
    fs::write(&path, &yaml).await?;

    let result = racing_wheel_plugins::manifest::load_manifest(&path).await;
    assert!(result.is_err());
    Ok(())
}

#[test]
fn manifest_json_serialization_preserves_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = make_manifest(PluginClass::Safe);
    let json = serde_json::to_string(&manifest)?;
    let restored: PluginManifest = serde_json::from_str(&json)?;
    assert_eq!(restored.id, manifest.id);
    assert_eq!(restored.name, manifest.name);
    assert_eq!(restored.version, manifest.version);
    assert_eq!(restored.class, manifest.class);
    assert_eq!(restored.capabilities, manifest.capabilities);
    assert_eq!(restored.operations, manifest.operations);
    assert_eq!(
        restored.entry_points.main_function,
        manifest.entry_points.main_function
    );
    Ok(())
}

// ===================================================================
// 2. WASM Plugin Loading Configuration
// ===================================================================

#[test]
fn wasm_resource_limits_defaults_are_sensible() {
    let limits = ResourceLimits::default();
    assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 10_000_000);
    assert_eq!(limits.max_table_elements, 10_000);
    assert_eq!(limits.max_instances, 32);
}

#[test]
fn wasm_resource_limits_builder_customizes_all_fields() {
    let limits = ResourceLimits::default()
        .with_memory(4 * 1024 * 1024)
        .with_fuel(1_000_000)
        .with_table_elements(500)
        .with_max_instances(4);

    assert_eq!(limits.max_memory_bytes, 4 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 1_000_000);
    assert_eq!(limits.max_table_elements, 500);
    assert_eq!(limits.max_instances, 4);
}

#[test]
fn wasm_runtime_created_with_default_limits() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = WasmRuntime::new()?;
    assert_eq!(runtime.instance_count(), 0);
    assert_eq!(runtime.resource_limits().max_instances, 32);
    Ok(())
}

#[test]
fn wasm_runtime_created_with_custom_limits() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_max_instances(8);
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_instances, 8);
    Ok(())
}

#[test]
fn wasm_runtime_rejects_invalid_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"not wasm", vec![]);
    assert!(result.is_err());
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn wasm_runtime_rejects_module_without_memory() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(NO_MEMORY_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();
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
fn wasm_runtime_rejects_module_without_process() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(NO_PROCESS_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();
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
fn wasm_runtime_max_instances_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_max_instances(1);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id1 = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();
    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;

    let result = runtime.load_plugin_from_bytes(id2, &wasm, vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().unwrap_or_else(|| unreachable!()));
    assert!(msg.contains("Maximum plugin instances"));
    Ok(())
}

#[test]
fn wasm_hot_reload_replaces_plugin_safely() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output_before = runtime.process(&id, 0.5, 0.001)?;
    assert!((output_before - 0.5).abs() < f32::EPSILON);

    // Reload with fresh bytes
    let wasm_v2 = wat::parse_str(PASSTHROUGH_WAT)?;
    runtime.reload_plugin(&id, &wasm_v2, vec![])?;
    let output_after = runtime.process(&id, 0.9, 0.001)?;
    assert!((output_after - 0.9).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wasm_hot_reload_with_invalid_bytes_preserves_old() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.reload_plugin(&id, b"broken", vec![]);
    assert!(result.is_err());

    // Old plugin must still function
    assert!(runtime.has_plugin(&id));
    let output = runtime.process(&id, 0.3, 0.001)?;
    assert!((output - 0.3).abs() < f32::EPSILON);
    Ok(())
}

// ===================================================================
// 3. Native Plugin Verification (Code Signing, Security)
// ===================================================================

#[test]
fn native_config_strict_rejects_unsigned() {
    let strict = NativePluginConfig::strict();
    assert!(!strict.allow_unsigned);
    assert!(strict.require_signatures);
}

#[test]
fn native_config_development_allows_unsigned() {
    let dev = NativePluginConfig::development();
    assert!(dev.allow_unsigned);
    assert!(!dev.require_signatures);
}

#[test]
fn native_config_permissive_allows_unsigned_but_requires_sig() {
    let perm = NativePluginConfig::permissive();
    assert!(perm.allow_unsigned);
    assert!(perm.require_signatures);
}

#[test]
fn native_config_default_is_strict() {
    let def = NativePluginConfig::default();
    assert!(!def.allow_unsigned);
    assert!(def.require_signatures);
}

#[test]
fn native_config_converts_to_signature_config() {
    let config = NativePluginConfig::strict();
    let sig_config = config.to_signature_config();
    assert_eq!(sig_config.require_signatures, config.require_signatures);
    assert_eq!(sig_config.allow_unsigned, config.allow_unsigned);
}

#[test]
fn signature_verification_config_strict_matches_expectations() {
    let strict = SignatureVerificationConfig::strict();
    assert!(strict.require_signatures);
    assert!(!strict.allow_unsigned);
}

#[test]
fn signature_verification_config_development_relaxed() {
    let dev = SignatureVerificationConfig::development();
    assert!(!dev.require_signatures);
    assert!(dev.allow_unsigned);
}

#[test]
fn abi_compatibility_current_version_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert!(matches!(result, AbiCheckResult::Compatible));
}

#[test]
fn abi_compatibility_wrong_version_mismatch() {
    let wrong = CURRENT_ABI_VERSION.wrapping_add(1);
    let result = check_abi_compatibility(wrong);
    match result {
        AbiCheckResult::Mismatch { expected, actual } => {
            assert_eq!(expected, CURRENT_ABI_VERSION);
            assert_eq!(actual, wrong);
        }
        AbiCheckResult::Compatible => panic!("expected Mismatch for wrong ABI version"),
    }
}

#[tokio::test]
async fn native_host_rejects_unsigned_plugin_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("unsigned-native");
    fs::create_dir_all(&plugin_dir).await?;

    let lib_name = if cfg!(windows) {
        "plugin.dll"
    } else if cfg!(target_os = "macos") {
        "plugin.dylib"
    } else {
        "plugin.so"
    };

    fs::write(plugin_dir.join(lib_name), b"fake binary data").await?;

    let host = NativePluginHost::new_with_defaults();
    let fake_path = plugin_dir.join(lib_name);
    let result = host
        .load_plugin(
            uuid::Uuid::new_v4(),
            "unsigned-test".to_string(),
            &fake_path,
            1000,
        )
        .await;

    assert!(
        result.is_err(),
        "Strict config should reject unsigned native plugin"
    );
    Ok(())
}

// ===================================================================
// 4. Plugin Capability Checking
// ===================================================================

#[test]
fn capability_checker_empty_denies_everything() {
    let checker = CapabilityChecker::new(vec![]);
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    assert!(checker.check_file_access(Path::new("/tmp/x")).is_err());
    assert!(checker.check_network_access("any.com").is_err());
}

#[test]
fn capability_checker_grants_only_requested() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);
    checker.check_telemetry_read()?;
    checker.check_led_control()?;
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    Ok(())
}

#[test]
fn capability_filesystem_nested_paths_allowed_siblings_denied() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/data/plugins".to_string()],
    }]);
    checker.check_file_access(Path::new("/data/plugins/sub/deep/file.bin"))?;
    assert!(
        checker
            .check_file_access(Path::new("/data/other/file.txt"))
            .is_err()
    );
    assert!(checker.check_file_access(Path::new("/etc/passwd")).is_err());
    Ok(())
}

#[test]
fn capability_multiple_filesystem_roots() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string(), "/data".to_string()],
    }]);
    checker.check_file_access(Path::new("/tmp/foo.txt"))?;
    checker.check_file_access(Path::new("/data/bar.bin"))?;
    assert!(checker.check_file_access(Path::new("/etc/shadow")).is_err());
    Ok(())
}

#[test]
fn capability_network_specific_hosts_only() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec!["api.example.com".to_string(), "cdn.example.com".to_string()],
    }]);
    checker.check_network_access("api.example.com")?;
    checker.check_network_access("cdn.example.com")?;
    assert!(checker.check_network_access("evil.example.com").is_err());
    Ok(())
}

#[test]
fn capability_error_messages_contain_context() {
    let checker = CapabilityChecker::new(vec![]);

    let err = checker
        .check_telemetry_read()
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(format!("{err}").contains("ReadTelemetry"));

    let err = checker
        .check_dsp_processing()
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(format!("{err}").contains("ProcessDsp"));

    let err = checker
        .check_file_access(Path::new("/secret"))
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(format!("{err}").contains("/secret"));

    let err = checker
        .check_network_access("bad.host")
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(format!("{err}").contains("bad.host"));
}

#[test]
fn capability_has_capability_bool_check() {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::ProcessDsp]);
    assert!(checker.has_capability(&Capability::ReadTelemetry));
    assert!(checker.has_capability(&Capability::ProcessDsp));
    assert!(!checker.has_capability(&Capability::ModifyTelemetry));
    assert!(!checker.has_capability(&Capability::ControlLeds));
}

#[test]
fn wasm_capability_enforcer_delegates_to_inner_checker() -> Result<(), PluginError> {
    let enforcer =
        WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);
    let checker = enforcer.checker();
    checker.check_telemetry_read()?;
    checker.check_led_control()?;
    assert!(checker.check_dsp_processing().is_err());
    Ok(())
}

#[test]
fn manifest_safe_plugin_cannot_have_process_dsp() {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::ProcessDsp];
    let err = validator
        .validate(&m)
        .err()
        .unwrap_or_else(|| unreachable!());
    assert!(format!("{err}").contains("ProcessDsp"));
}

#[test]
fn manifest_fast_plugin_can_have_process_dsp() -> Result<(), PluginError> {
    let validator = ManifestValidator::default();
    let mut m = make_manifest(PluginClass::Fast);
    m.capabilities = vec![Capability::ProcessDsp];
    m.constraints.max_execution_time_us = 100;
    m.constraints.max_memory_bytes = 2 * 1024 * 1024;
    m.constraints.update_rate_hz = 1000;
    validator.validate(&m)
}

#[test]
fn manifest_neither_class_allows_filesystem() {
    let validator = ManifestValidator::default();

    let mut safe = make_manifest(PluginClass::Safe);
    safe.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    assert!(validator.validate(&safe).is_err());

    let mut fast = make_manifest(PluginClass::Fast);
    fast.capabilities = vec![Capability::FileSystem {
        paths: vec!["/tmp".to_string()],
    }];
    fast.constraints.max_execution_time_us = 100;
    fast.constraints.max_memory_bytes = 2 * 1024 * 1024;
    fast.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&fast).is_err());
}

#[test]
fn manifest_neither_class_allows_network() {
    let validator = ManifestValidator::default();

    let mut safe = make_manifest(PluginClass::Safe);
    safe.capabilities = vec![Capability::Network {
        hosts: vec!["x.com".to_string()],
    }];
    assert!(validator.validate(&safe).is_err());

    let mut fast = make_manifest(PluginClass::Fast);
    fast.capabilities = vec![Capability::Network {
        hosts: vec!["x.com".to_string()],
    }];
    fast.constraints.max_execution_time_us = 100;
    fast.constraints.max_memory_bytes = 2 * 1024 * 1024;
    fast.constraints.update_rate_hz = 1000;
    assert!(validator.validate(&fast).is_err());
}

// ===================================================================
// 5. Plugin Registry Operations
// ===================================================================

#[test]
fn registry_add_and_retrieve_plugin() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("FFB Filter", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    assert!(catalog.contains(&id));
    assert_eq!(catalog.plugin_count(), 1);

    let retrieved = catalog.get_plugin(&id, None);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.map(|m| &*m.name), Some("FFB Filter"));
    Ok(())
}

#[test]
fn registry_multiple_versions_latest_returned() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();

    let v1 = make_registry_metadata("My Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("My Plugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    let mut v1_5 = make_registry_metadata("My Plugin", "1.5.0");
    v1_5.id = id.clone();
    catalog.add_plugin(v1_5)?;

    assert_eq!(catalog.plugin_count(), 1);
    assert_eq!(catalog.version_count(), 3);

    let latest = catalog.get_plugin(&id, None);
    assert_eq!(
        latest.map(|m| m.version.to_string()),
        Some("2.0.0".to_string())
    );
    Ok(())
}

#[test]
fn registry_get_specific_version() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();

    let v1 = make_registry_metadata("My Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("My Plugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    let specific = catalog.get_plugin(&id, Some(&semver::Version::new(1, 0, 0)));
    assert!(specific.is_some());
    assert_eq!(
        specific.map(|m| m.version.to_string()),
        Some("1.0.0".to_string())
    );
    Ok(())
}

#[test]
fn registry_remove_all_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("Removable", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    assert!(catalog.remove_plugin(&id, None));
    assert!(!catalog.contains(&id));
    assert_eq!(catalog.plugin_count(), 0);
    Ok(())
}

#[test]
fn registry_remove_specific_version() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let v1 = make_registry_metadata("Plugin", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("Plugin", "2.0.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    assert!(catalog.remove_plugin(&id, Some(&semver::Version::new(1, 0, 0))));
    assert!(catalog.contains(&id));
    assert_eq!(catalog.version_count(), 1);

    let latest = catalog.get_plugin(&id, None);
    assert_eq!(
        latest.map(|m| m.version.to_string()),
        Some("2.0.0".to_string())
    );
    Ok(())
}

#[test]
fn registry_remove_nonexistent_returns_false() {
    let mut catalog = PluginCatalog::new();
    let fake_id = PluginId::new();
    assert!(!catalog.remove_plugin(&fake_id, None));
}

#[test]
fn registry_search_by_name_case_insensitive() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("FFB Filter", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("LED Controller", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Telemetry Logger", "1.0.0"))?;

    let results = catalog.search("ffb");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "FFB Filter");

    let results = catalog.search("LED");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "LED Controller");
    Ok(())
}

#[test]
fn registry_search_by_description() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let p1 = PluginMetadata::new(
        "Plugin A",
        semver::Version::new(1, 0, 0),
        "Author",
        "Provides force feedback enhancements",
        "MIT",
    );
    catalog.add_plugin(p1)?;

    let results = catalog.search("force feedback");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Plugin A");
    Ok(())
}

#[test]
fn registry_list_all_sorted_by_name() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("Zebra", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Alpha", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("Mid", "1.0.0"))?;

    let all = catalog.list_all();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].name, "Alpha");
    assert_eq!(all[1].name, "Mid");
    assert_eq!(all[2].name, "Zebra");
    Ok(())
}

#[test]
fn registry_contains_version_checks() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("Plugin", "1.2.3");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    assert!(catalog.contains_version(&id, &semver::Version::new(1, 2, 3)));
    assert!(!catalog.contains_version(&id, &semver::Version::new(9, 9, 9)));
    Ok(())
}

#[test]
fn registry_empty_name_rejected() {
    let mut catalog = PluginCatalog::new();
    let meta = PluginMetadata::new("", semver::Version::new(1, 0, 0), "Author", "Desc", "MIT");
    assert!(catalog.add_plugin(meta).is_err());
}

#[test]
fn registry_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = PluginCatalog::new();
    let meta = make_registry_metadata("Roundtrip Plugin", "2.1.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;

    let json = serde_json::to_string(&catalog)?;
    let mut restored: PluginCatalog = serde_json::from_str(&json)?;
    restored.rebuild_index();

    assert!(restored.contains(&id));
    assert_eq!(restored.plugin_count(), 1);
    Ok(())
}

#[test]
fn registry_find_compatible_version() -> Result<(), PluginError> {
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
fn semver_same_major_higher_minor_compatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 0, 0),
            &semver::Version::new(1, 3, 0)
        ),
        VersionCompatibility::Compatible,
    );
}

#[test]
fn semver_different_major_incompatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 0, 0),
            &semver::Version::new(2, 0, 0)
        ),
        VersionCompatibility::Incompatible,
    );
}

#[test]
fn semver_lower_available_incompatible() {
    assert_eq!(
        check_compatibility(
            &semver::Version::new(1, 5, 0),
            &semver::Version::new(1, 2, 0)
        ),
        VersionCompatibility::Incompatible,
    );
}

#[test]
fn semver_prerelease_exact_match_only() -> Result<(), Box<dyn std::error::Error>> {
    let alpha1 = semver::Version::parse("1.0.0-alpha")?;
    let alpha2 = semver::Version::parse("1.0.0-alpha")?;
    let beta = semver::Version::parse("1.0.0-beta")?;

    assert_eq!(
        check_compatibility(&alpha1, &alpha2),
        VersionCompatibility::Compatible
    );
    assert_eq!(
        check_compatibility(&alpha1, &beta),
        VersionCompatibility::Incompatible
    );
    Ok(())
}

#[test]
fn semver_zero_major_requires_exact_minor() {
    // 0.x versions: require exact minor match
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
fn registry_metadata_builder_pattern() {
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
    .with_download_url("https://dl.example.com/p.zip")
    .with_package_hash("deadbeef");

    assert_eq!(meta.homepage, Some("https://example.com".to_string()));
    assert_eq!(meta.capabilities.len(), 1);
    assert_eq!(meta.signature_fingerprint, Some("abc123".to_string()));
    assert_eq!(
        meta.download_url,
        Some("https://dl.example.com/p.zip".to_string())
    );
    assert_eq!(meta.package_hash, Some("deadbeef".to_string()));
}

// ===================================================================
// 6. Plugin Isolation Boundaries
// ===================================================================

#[test]
fn isolation_trap_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let bad_wasm = wat::parse_str(TRAP_WAT)?;
    let good_wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let bad_id = uuid::Uuid::new_v4();
    let good_id = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(bad_id, &bad_wasm, vec![])?;
    runtime.load_plugin_from_bytes(good_id, &good_wasm, vec![])?;

    // Crash the bad plugin
    let crash_result = runtime.process(&bad_id, 0.5, 0.001);
    assert!(crash_result.is_err());

    // Good plugin is unaffected
    let output = runtime.process(&good_id, 0.75, 0.001)?;
    assert!((output - 0.75).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn isolation_trap_disables_crashing_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.5, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn isolation_disabled_plugin_can_be_reenabled() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 0.5, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn isolation_unload_removes_plugin_completely() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 0);

    // Processing after unload should fail
    let result = runtime.process(&id, 0.5, 0.001);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn isolation_quarantine_independent_per_plugin() -> Result<(), PluginError> {
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
fn isolation_quarantine_escalation_across_releases() -> Result<(), PluginError> {
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
fn isolation_budget_violations_trigger_quarantine() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_budget_violations: 2,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::BudgetViolation, "v1".to_string())?;
    assert!(!manager.is_quarantined(id));

    manager.record_violation(id, ViolationType::BudgetViolation, "v2".to_string())?;
    assert!(manager.is_quarantined(id));
    Ok(())
}

#[test]
fn isolation_multiple_plugins_wasm_instance_counts() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let id1 = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();
    let id3 = uuid::Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id3, &wasm, vec![])?;
    assert_eq!(runtime.instance_count(), 3);

    runtime.unload_plugin(&id2)?;
    assert_eq!(runtime.instance_count(), 2);
    assert!(runtime.has_plugin(&id1));
    assert!(!runtime.has_plugin(&id2));
    assert!(runtime.has_plugin(&id3));
    Ok(())
}

// ===================================================================
// Plugin Host Integration
// ===================================================================

#[tokio::test]
async fn host_empty_directory_has_empty_registry() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert!(registry.is_empty());
    Ok(())
}

#[tokio::test]
async fn host_discovers_plugin_from_manifest_file() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("my-plugin");
    fs::create_dir_all(&plugin_dir).await?;

    let manifest = make_manifest(PluginClass::Safe);
    let plugin_id = manifest.id;
    let yaml = serde_yaml::to_string(&manifest)?;
    fs::write(plugin_dir.join("plugin.yaml"), &yaml).await?;
    fs::write(plugin_dir.join("plugin.wasm"), b"fake wasm content").await?;

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    let registry = host.get_registry().await;
    assert_eq!(registry.len(), 1);

    let entry = registry.get(&plugin_id).unwrap_or_else(|| unreachable!());
    assert_eq!(entry.manifest.name, "Test Plugin");
    assert!(!entry.is_loaded);
    assert!(entry.is_enabled);
    Ok(())
}

#[tokio::test]
async fn host_skips_invalid_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("broken");
    fs::create_dir_all(&plugin_dir).await?;
    fs::write(plugin_dir.join("plugin.yaml"), "bad: yaml: [[[").await?;

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    assert!(host.get_registry().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn host_load_nonexistent_plugin_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;
    let result = host.load_plugin(Uuid::new_v4()).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn host_enable_disable_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let plugin_dir = temp.path().join("toggle");
    fs::create_dir_all(&plugin_dir).await?;

    let manifest = make_manifest(PluginClass::Safe);
    let plugin_id = manifest.id;
    let yaml = serde_yaml::to_string(&manifest)?;
    fs::write(plugin_dir.join("plugin.yaml"), &yaml).await?;
    fs::write(plugin_dir.join("plugin.wasm"), b"fake").await?;

    let host = racing_wheel_plugins::host::PluginHost::new(temp.path().to_path_buf()).await?;

    host.set_plugin_enabled(plugin_id, false).await?;
    let registry = host.get_registry().await;
    let entry = registry.get(&plugin_id).unwrap_or_else(|| unreachable!());
    assert!(!entry.is_enabled);

    host.set_plugin_enabled(plugin_id, true).await?;
    let registry = host.get_registry().await;
    let entry = registry.get(&plugin_id).unwrap_or_else(|| unreachable!());
    assert!(entry.is_enabled);
    Ok(())
}
