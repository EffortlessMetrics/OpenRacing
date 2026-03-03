//! Example WASM plugin tests exercising the real WASM runtime infrastructure.
//!
//! Covers:
//!  1. Full plugin lifecycle: load → init → process → shutdown
//!  2. Hot-reload behavior
//!  3. Crash recovery from trapped WASM
//!  4. Test plugins as byte arrays (minimal WASM modules)

use uuid::Uuid;

use openracing_wasm_runtime::{
    HotReloader, PreservedPluginState, ResourceLimits, WasmRuntime,
};

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

/// Plugin with init + shutdown + process (full lifecycle).
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

/// Plugin that traps on process (simulates crash).
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

/// Plugin that doubles its input.
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

// ===================================================================
// 1. Full plugin lifecycle: load → init → process → shutdown
// ===================================================================

#[test]
fn wasm_example_load_process_unload() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));

    let output = runtime.process(&id, 0.42, 0.001)?;
    assert!(
        (output - 0.42).abs() < f32::EPSILON,
        "passthrough must return input unchanged"
    );

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn wasm_example_full_lifecycle_init_process_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(LIFECYCLE_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);

    let output = runtime.process(&id, 7.5, 0.001)?;
    assert!((output - 7.5).abs() < f32::EPSILON);

    let (count, avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 1);
    assert!(avg >= 0.0, "average time must be non-negative");

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn wasm_example_process_after_unload_fails() -> Result<(), Box<dyn std::error::Error>> {
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
fn wasm_example_stats_accumulate() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..5 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 5, "should have recorded 5 process calls");
    Ok(())
}

// ===================================================================
// 2. Hot-reload behavior
// ===================================================================

#[test]
fn wasm_example_hot_reload_changes_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load passthrough first
    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![])?;
    let output_v1 = runtime.process(&id, 4.0, 0.001)?;
    assert!((output_v1 - 4.0).abs() < f32::EPSILON, "v1 should passthrough");

    // Hot-reload with doubler
    runtime.reload_plugin(&id, &double_wasm, vec![])?;
    let output_v2 = runtime.process(&id, 4.0, 0.001)?;
    assert!((output_v2 - 8.0).abs() < f32::EPSILON, "v2 should double");
    Ok(())
}

#[test]
fn wasm_example_hot_reload_preserves_stats() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let double_wasm = compile_wat(DOUBLE_OUTPUT_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![])?;
    for _ in 0..3 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }

    runtime.reload_plugin(&id, &double_wasm, vec![])?;

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 3, "stats should be preserved across hot-reload");
    Ok(())
}

#[test]
fn wasm_example_hot_reload_invalid_keeps_old() -> Result<(), Box<dyn std::error::Error>> {
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &pass_wasm, vec![])?;

    // Try to reload with invalid bytes — should fail but keep old plugin
    let result = runtime.reload_plugin(&id, b"not valid wasm", vec![]);
    assert!(result.is_err(), "reload with invalid WASM must fail");

    // Old plugin should still work
    let output = runtime.process(&id, 2.0, 0.001)?;
    assert!((output - 2.0).abs() < f32::EPSILON, "old plugin should still work");
    Ok(())
}

#[test]
fn wasm_example_hot_reloader_tracking() {
    let mut reloader = HotReloader::new();
    assert_eq!(reloader.reload_count(), 0);
    assert_eq!(reloader.failed_reload_count(), 0);

    reloader.record_success();
    reloader.record_success();
    reloader.record_failure();

    assert_eq!(reloader.reload_count(), 2);
    assert_eq!(reloader.failed_reload_count(), 1);
    assert_eq!(reloader.total_attempts(), 3);

    let rate = reloader.success_rate();
    assert!((rate - 66.666).abs() < 1.0, "success rate should be ~66.7%");
}

#[test]
fn wasm_example_preserved_state_empty_and_nonempty() {
    let empty = PreservedPluginState::new();
    assert!(empty.is_empty());
    assert_eq!(empty.average_process_time_us(), 0.0);

    let filled = PreservedPluginState {
        plugin_data: {
            let mut m = std::collections::HashMap::new();
            m.insert("key".to_string(), vec![1, 2, 3]);
            m
        },
        process_count: 10,
        total_process_time_us: 500,
    };
    assert!(!filled.is_empty());
    assert!((filled.average_process_time_us() - 50.0).abs() < f64::EPSILON);
}

// ===================================================================
// 3. Crash recovery from trapped WASM
// ===================================================================

#[test]
fn wasm_example_trap_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "trapping plugin must return error");
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn wasm_example_disabled_info_available_after_trap() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);

    let info = runtime.get_plugin_disabled_info(&id)?;
    assert!(info.is_some(), "disabled info should be available after trap");
    Ok(())
}

#[test]
fn wasm_example_reenable_after_trap() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(TRAP_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled, "plugin should have been disabled");
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn wasm_example_crash_does_not_affect_sibling() -> Result<(), Box<dyn std::error::Error>> {
    let trap_wasm = compile_wat(TRAP_WAT)?;
    let pass_wasm = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;

    let trap_id = Uuid::new_v4();
    let pass_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(trap_id, &trap_wasm, vec![])?;
    runtime.load_plugin_from_bytes(pass_id, &pass_wasm, vec![])?;

    let _ = runtime.process(&trap_id, 1.0, 0.001);

    let output = runtime.process(&pass_id, 99.0, 0.001)?;
    assert!((output - 99.0).abs() < f32::EPSILON);
    assert!(runtime.is_plugin_disabled(&trap_id)?);
    assert!(!runtime.is_plugin_disabled(&pass_id)?);
    Ok(())
}

#[test]
fn wasm_example_fuel_exhaustion_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = compile_wat(INFINITE_LOOP_WAT)?;
    let limits = ResourceLimits::default().with_fuel(500);
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

// ===================================================================
// 4. Resource limits
// ===================================================================

#[test]
fn wasm_example_resource_limits_builder() {
    let limits = ResourceLimits::default()
        .with_memory(8 * 1024 * 1024)
        .with_fuel(5_000_000)
        .with_table_elements(5_000)
        .with_max_instances(16);

    assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 5_000_000);
    assert_eq!(limits.max_table_elements, 5_000);
    assert_eq!(limits.max_instances, 16);
}

#[test]
fn wasm_example_conservative_and_generous_limits() {
    let conservative = ResourceLimits::conservative();
    let generous = ResourceLimits::generous();

    assert!(conservative.max_memory_bytes < generous.max_memory_bytes);
    assert!(conservative.max_fuel < generous.max_fuel);
    assert!(conservative.max_instances < generous.max_instances);
}

#[test]
fn wasm_example_resource_limits_validation() {
    let valid = ResourceLimits::default();
    assert!(valid.validate().is_ok());

    let too_small_mem = ResourceLimits::default().with_memory(1024);
    assert!(too_small_mem.validate().is_err());

    let too_small_fuel = ResourceLimits::default().with_fuel(100);
    assert!(too_small_fuel.validate().is_err());

    let zero_instances = ResourceLimits::default().with_max_instances(0);
    assert!(zero_instances.validate().is_err());
}
