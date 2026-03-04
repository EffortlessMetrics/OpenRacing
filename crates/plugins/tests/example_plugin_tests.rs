//! Example plugin tests exercising the real plugin infrastructure.
//!
//! Covers:
//!  1. WASM plugin loading with mock bytes
//!  2. Capability model: grant and enforcement
//!  3. Budget enforcement: fuel limits and exhaustion
//!  4. Plugin registry: register, discover, unregister
//!  5. Plugin catalog serialization
//!  6. Multiple plugin tiers (Safe vs Fast) coexistence

use std::path::Path;

use uuid::Uuid;

use racing_wheel_plugins::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use racing_wheel_plugins::manifest::Capability;
use racing_wheel_plugins::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
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

/// Minimal passthrough plugin.
const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// Plugin that traps immediately (simulates crash).
const TRAP_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// Infinite loop to exhaust fuel budget.
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

/// Plugin that doubles input.
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

// ===================================================================
// 1. WASM plugin loading with mock bytes
// ===================================================================

#[test]
fn example_load_minimal_wasm_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![Capability::ReadTelemetry])?;
    assert!(runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 1);

    let output = runtime.process(&id, 0.75, 0.001)?;
    assert!(
        (output - 0.75).abs() < f32::EPSILON,
        "passthrough should return input unchanged"
    );

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn example_invalid_wasm_bytes_rejected() {
    let mut runtime = match WasmRuntime::new() {
        Ok(r) => r,
        Err(e) => {
            panic!("Failed to create runtime: {e}");
        }
    };
    let id = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id, b"not valid wasm", vec![]);
    assert!(result.is_err(), "invalid WASM bytes must be rejected");
}

// ===================================================================
// 2. Capability model: grant and enforcement
// ===================================================================

#[test]
fn example_capability_grant_and_deny() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry, Capability::ControlLeds]);

    // Granted capabilities succeed
    checker.check_telemetry_read()?;
    checker.check_led_control()?;

    // Non-granted capabilities fail
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(checker.check_inter_plugin_comm().is_err());
    Ok(())
}

#[test]
fn example_capability_filesystem_scope() -> Result<(), PluginError> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/sandbox".to_string()],
    }]);

    checker.check_file_access(Path::new("/sandbox/data.bin"))?;
    assert!(checker.check_file_access(Path::new("/etc/shadow")).is_err());
    Ok(())
}

#[test]
fn example_capability_enforcer_wasi_integration() -> Result<(), PluginError> {
    let enforcer =
        WasmCapabilityEnforcer::new(vec![Capability::ReadTelemetry, Capability::ProcessDsp]);
    let inner = enforcer.checker();

    inner.check_telemetry_read()?;
    inner.check_dsp_processing()?;
    assert!(inner.check_led_control().is_err());
    Ok(())
}

// ===================================================================
// 3. Budget enforcement: fuel limits and exhaustion
// ===================================================================

#[test]
fn example_fuel_exhaustion_terminates_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(
        result.is_err(),
        "infinite-loop must be terminated by fuel exhaustion"
    );

    assert!(
        runtime.is_plugin_disabled(&id)?,
        "plugin must be disabled after fuel exhaustion"
    );
    Ok(())
}

#[test]
fn example_fuel_limit_applied_to_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_fuel(2_500_000);
    let runtime = WasmRuntime::with_limits(limits)?;
    assert_eq!(runtime.resource_limits().max_fuel, 2_500_000);
    Ok(())
}

#[test]
fn example_max_instances_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let limits = ResourceLimits::default().with_max_instances(1);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id1 = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;

    let id2 = Uuid::new_v4();
    let result = runtime.load_plugin_from_bytes(id2, &wasm, vec![]);
    assert!(result.is_err(), "second instance must exceed limit of 1");
    Ok(())
}

#[test]
fn example_budget_violation_quarantine() -> Result<(), PluginError> {
    let mut manager = QuarantineManager::new(QuarantinePolicy {
        max_budget_violations: 2,
        ..QuarantinePolicy::default()
    });
    let id = Uuid::new_v4();

    manager.record_violation(id, ViolationType::BudgetViolation, "v1".to_string())?;
    assert!(!manager.is_quarantined(id));

    manager.record_violation(id, ViolationType::BudgetViolation, "v2".to_string())?;
    assert!(
        manager.is_quarantined(id),
        "plugin must be quarantined after exceeding budget violation limit"
    );
    Ok(())
}

// ===================================================================
// 4. Plugin registry: register, discover, unregister
// ===================================================================

#[test]
fn example_registry_add_search_remove() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();
    assert_eq!(catalog.plugin_count(), 0);

    let meta = make_registry_metadata("FFB-Enhancer", "1.0.0");
    let id = meta.id.clone();
    catalog.add_plugin(meta)?;
    assert_eq!(catalog.plugin_count(), 1);

    let results = catalog.search("FFB");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "FFB-Enhancer");

    let removed = catalog.remove_plugin(&id, None);
    assert!(removed);
    assert_eq!(catalog.plugin_count(), 0);
    Ok(())
}

#[test]
fn example_registry_version_compat() {
    let req = semver::Version::new(1, 0, 0);
    let avail = semver::Version::new(1, 2, 0);
    assert_eq!(
        check_compatibility(&req, &avail),
        VersionCompatibility::Compatible
    );

    let breaking = semver::Version::new(2, 0, 0);
    assert_eq!(
        check_compatibility(&req, &breaking),
        VersionCompatibility::Incompatible
    );
}

#[test]
fn example_registry_multiple_versions() -> Result<(), PluginError> {
    let mut catalog = PluginCatalog::new();

    let v1 = make_registry_metadata("LedMapper", "1.0.0");
    let id = v1.id.clone();
    catalog.add_plugin(v1)?;

    let mut v2 = make_registry_metadata("LedMapper", "1.1.0");
    v2.id = id.clone();
    catalog.add_plugin(v2)?;

    assert_eq!(catalog.plugin_count(), 1, "same plugin ID, two versions");
    assert_eq!(catalog.version_count(), 2);

    let latest = catalog.get_plugin(&id, None);
    assert!(latest.is_some());

    let versions = catalog.get_all_versions(&id);
    assert!(versions.is_some());
    let versions = versions.unwrap_or_else(|| unreachable!());
    assert_eq!(versions.len(), 2);
    Ok(())
}

// ===================================================================
// 5. Plugin catalog serialization
// ===================================================================

#[test]
fn example_catalog_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = PluginCatalog::new();
    catalog.add_plugin(make_registry_metadata("PluginA", "1.0.0"))?;
    catalog.add_plugin(make_registry_metadata("PluginB", "2.1.0"))?;

    let json = serde_json::to_string(&catalog)?;
    let restored: PluginCatalog = serde_json::from_str(&json)?;

    assert_eq!(restored.plugin_count(), 2);
    Ok(())
}

#[test]
fn example_plugin_metadata_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let meta = make_registry_metadata("SerdeTest", "3.0.0")
        .with_capabilities(vec![Capability::ReadTelemetry, Capability::ControlLeds])
        .with_homepage("https://example.com");

    let json = serde_json::to_string(&meta)?;
    let restored: PluginMetadata = serde_json::from_str(&json)?;

    assert_eq!(restored.name, "SerdeTest");
    assert_eq!(restored.version, semver::Version::new(3, 0, 0));
    assert_eq!(restored.capabilities.len(), 2);
    assert!(restored.homepage.is_some());
    Ok(())
}

// ===================================================================
// 6. Multiple plugin tiers (Safe vs Fast) coexistence
// ===================================================================

#[test]
fn example_safe_and_fast_contexts_coexist() {
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

    assert_eq!(safe_ctx.class, PluginClass::Safe);
    assert_eq!(fast_ctx.class, PluginClass::Fast);
    assert_ne!(safe_ctx.class, fast_ctx.class);
    assert!(safe_ctx.budget_us > fast_ctx.budget_us);
}

#[test]
fn example_two_wasm_plugins_loaded_simultaneously() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let pass_id = Uuid::new_v4();
    let double_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![Capability::ReadTelemetry])?;
    runtime.load_plugin_from_bytes(double_id, &double_wasm, vec![Capability::ReadTelemetry])?;
    assert_eq!(runtime.instance_count(), 2);

    let pass_out = runtime.process(&pass_id, 5.0, 0.001)?;
    let double_out = runtime.process(&double_id, 5.0, 0.001)?;

    assert!(
        (pass_out - 5.0).abs() < f32::EPSILON,
        "passthrough returns input"
    );
    assert!(
        (double_out - 10.0).abs() < f32::EPSILON,
        "doubler returns 2x input"
    );
    Ok(())
}

#[test]
fn example_crash_isolation_between_plugins() -> Result<(), Box<dyn std::error::Error>> {
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
    let output = runtime.process(&pass_id, 3.125, 0.001)?;
    assert!((output - 3.125).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&pass_id)?);
    Ok(())
}

#[test]
fn example_plugin_class_serde_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let safe_json = serde_json::to_string(&PluginClass::Safe)?;
    let fast_json = serde_json::to_string(&PluginClass::Fast)?;

    let safe_restored: PluginClass = serde_json::from_str(&safe_json)?;
    let fast_restored: PluginClass = serde_json::from_str(&fast_json)?;

    assert_eq!(safe_restored, PluginClass::Safe);
    assert_eq!(fast_restored, PluginClass::Fast);
    Ok(())
}
