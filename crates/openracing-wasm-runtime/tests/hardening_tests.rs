//! Hardening tests for the WASM plugin runtime.
//!
//! These tests cover:
//! 1. WASM module compilation and instantiation
//! 2. Resource limits enforcement (memory, CPU, fuel)
//! 3. Capability-based permissions
//! 4. Crash recovery and isolation
//! 5. Plugin API surface validation

use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT module helpers
// ---------------------------------------------------------------------------

fn parse_wat(wat: &str) -> Result<Vec<u8>, WasmError> {
    wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))
}

fn minimal_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )"#,
    )
}

fn adding_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add
            )
        )"#,
    )
}

fn trap_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable
            )
        )"#,
    )
}

fn full_lifecycle_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (global $initialized (mut i32) (i32.const 0))
            (func (export "init") (result i32)
                i32.const 1
                global.set $initialized
                i32.const 0
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.mul
            )
            (func (export "shutdown")
                i32.const 0
                global.set $initialized
            )
        )"#,
    )
}

/// A plugin whose init() returns an error code.
fn init_fail_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const -1
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )"#,
    )
}

/// A plugin whose init() traps.
fn init_trap_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                unreachable
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )"#,
    )
}

/// A busy-loop plugin that burns lots of fuel.
fn fuel_heavy_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (local $i i32)
                (local.set $i (i32.const 0))
                (block $break
                    (loop $loop
                        (br_if $break (i32.ge_u (local.get $i) (i32.const 1000000)))
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $loop)
                    )
                )
                local.get 0
            )
        )"#,
    )
}

/// A plugin that tries to grow memory.
fn memory_grow_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; Try to grow memory by 256 pages = 16MB
                (drop (memory.grow (i32.const 256)))
                local.get 0
            )
        )"#,
    )
}

/// A plugin that does integer division by zero (trap).
fn div_zero_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (drop (i32.div_s (i32.const 1) (i32.const 0)))
                local.get 0
            )
        )"#,
    )
}

/// A plugin with a stack overflow via infinite recursion.
fn stack_overflow_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func $recurse (result f32)
                call $recurse
            )
            (func (export "process") (param f32 f32) (result f32)
                call $recurse
            )
        )"#,
    )
}

/// A plugin exporting only memory (no process).
fn memory_only_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
        )"#,
    )
}

/// A plugin exporting only process (no memory).
fn process_only_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )"#,
    )
}

/// A plugin with wrong process signature.
fn wrong_signature_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param i32 i32) (result i32)
                local.get 0
            )
        )"#,
    )
}

/// A plugin that returns constant zero.
fn zero_plugin() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                f32.const 0.0
            )
        )"#,
    )
}

// ===========================================================================
// 1. WASM module compilation and instantiation
// ===========================================================================

#[test]
fn compile_valid_minimal_module() -> Result<(), WasmError> {
    let wasm = minimal_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 1);
    Ok(())
}

#[test]
fn compile_module_with_all_exports() -> Result<(), WasmError> {
    let wasm = full_lifecycle_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);
    Ok(())
}

#[test]
fn compile_invalid_bytes_fails() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, b"not-wasm", vec![]);
    assert!(result.is_err());
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn compile_empty_bytes_fails() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &[], vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn compile_missing_memory_export_fails() -> Result<(), WasmError> {
    let wasm = process_only_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::MissingExport(_))));
    Ok(())
}

#[test]
fn compile_missing_process_export_fails() -> Result<(), WasmError> {
    let wasm = memory_only_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::MissingExport(_))));
    Ok(())
}

#[test]
fn compile_wrong_process_signature_fails() -> Result<(), WasmError> {
    let wasm = wrong_signature_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Wrong signature means the typed-func lookup for (f32,f32)->f32 fails,
    // so the module should be rejected as missing the `process` export.
    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn instantiate_same_id_twice_replaces() -> Result<(), WasmError> {
    let wasm1 = minimal_plugin()?;
    let wasm2 = adding_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm1, vec![])?;
    let r1 = runtime.process(&id, 5.0, 3.0)?;
    assert!((r1 - 5.0).abs() < f32::EPSILON); // minimal returns first arg

    // Loading again with same ID replaces via reload
    runtime.reload_plugin(&id, &wasm2, vec![])?;
    let r2 = runtime.process(&id, 5.0, 3.0)?;
    assert!((r2 - 8.0).abs() < f32::EPSILON); // adding returns sum
    assert_eq!(runtime.instance_count(), 1);
    Ok(())
}

#[test]
fn init_returning_error_rejects_plugin() -> Result<(), WasmError> {
    let wasm = init_fail_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::LoadingFailed(_))));
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn init_trap_rejects_plugin() -> Result<(), WasmError> {
    let wasm = init_trap_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::LoadingFailed(_))));
    Ok(())
}

// ===========================================================================
// 2. Resource limits enforcement (memory, CPU, fuel)
// ===========================================================================

#[test]
fn fuel_exhaustion_stops_execution() -> Result<(), WasmError> {
    // Use very low fuel so the busy loop exhausts it
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = fuel_heavy_plugin()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    // Plugin should be disabled after fuel exhaustion
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn sufficient_fuel_allows_completion() -> Result<(), WasmError> {
    // Use generous fuel
    let limits = ResourceLimits::default().with_fuel(50_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = minimal_plugin()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 42.0, 0.001)?;
    assert!((result - 42.0).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn memory_growth_limited_by_resource_limits() -> Result<(), WasmError> {
    // 1MB limit — the plugin tries to grow by 256 pages (16MB)
    let limits = ResourceLimits::default().with_memory(1024 * 1024);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = memory_grow_plugin()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // The memory.grow should return -1 (failure) but not trap,
    // so process still completes returning the input.
    let result = runtime.process(&id, 7.0, 0.001)?;
    assert!((result - 7.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn max_instances_enforced() -> Result<(), WasmError> {
    let limits = ResourceLimits::default().with_max_instances(3);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = minimal_plugin()?;

    for _ in 0..3 {
        runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;
    }

    let result = runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::MaxInstancesReached(3))));
    assert_eq!(runtime.instance_count(), 3);
    Ok(())
}

#[test]
fn unload_frees_instance_slot() -> Result<(), WasmError> {
    let limits = ResourceLimits::default().with_max_instances(1);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let wasm = minimal_plugin()?;

    let id1 = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;

    // Cannot add another
    let result = runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(result.is_err());

    // Unload frees the slot
    runtime.unload_plugin(&id1)?;

    let id2 = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id2));
    Ok(())
}

#[test]
fn conservative_limits_are_valid() {
    let limits = ResourceLimits::conservative();
    assert!(limits.validate().is_ok());
    assert_eq!(limits.max_memory_bytes, 4 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 1_000_000);
    assert_eq!(limits.max_instances, 8);
    assert!(limits.max_execution_time.is_some());
}

#[test]
fn generous_limits_are_valid() {
    let limits = ResourceLimits::generous();
    assert!(limits.validate().is_ok());
    assert_eq!(limits.max_memory_bytes, 64 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 50_000_000);
    assert_eq!(limits.max_instances, 128);
}

#[test]
fn validation_rejects_memory_too_small() {
    let limits = ResourceLimits::default().with_memory(100);
    assert!(limits.validate().is_err());
}

#[test]
fn validation_rejects_fuel_too_small() {
    let limits = ResourceLimits::default().with_fuel(10);
    assert!(limits.validate().is_err());
}

#[test]
fn validation_rejects_zero_instances() {
    let limits = ResourceLimits::default().with_max_instances(0);
    assert!(limits.validate().is_err());
}

#[test]
fn validation_rejects_too_many_instances() {
    let limits = ResourceLimits::default().with_max_instances(5000);
    assert!(limits.validate().is_err());
}

// ===========================================================================
// 3. Capability-based permissions
// ===========================================================================

#[test]
fn plugin_loaded_with_capabilities() -> Result<(), WasmError> {
    let wasm = minimal_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let caps = vec!["read_telemetry".to_string(), "control_leds".to_string()];
    runtime.load_plugin_from_bytes(id, &wasm, caps)?;
    assert!(runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn plugin_loaded_with_no_capabilities() -> Result<(), WasmError> {
    let wasm = minimal_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&id));
    // Still processes fine — capabilities gate host function access, not
    // the ability to call the process() export.
    let result = runtime.process(&id, 1.0, 0.001)?;
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn capability_checker_grants_correct_perms() {
    use openracing_wasm_runtime::state::CapabilityChecker;

    let checker = CapabilityChecker::new(vec![
        "read_telemetry".to_string(),
        "process_dsp".to_string(),
    ]);

    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_dsp_processing().is_ok());

    // Not granted
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
}

#[test]
fn capability_checker_denies_all_when_empty() {
    use openracing_wasm_runtime::state::CapabilityChecker;

    let checker = CapabilityChecker::default();

    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
}

#[test]
fn capability_violation_error_classification() {
    let err = WasmError::CapabilityViolation {
        capability: "read_telemetry".to_string(),
    };
    assert!(err.is_capability_violation());
    assert!(!err.is_crash());
    assert!(!err.is_timeout());
    assert!(!err.is_budget_violation());
}

#[test]
fn capability_checker_has_capability_query() {
    use openracing_wasm_runtime::state::CapabilityChecker;

    let checker = CapabilityChecker::new(vec!["control_leds".to_string()]);
    assert!(checker.has_capability("control_leds"));
    assert!(!checker.has_capability("read_telemetry"));
    assert!(!checker.has_capability("nonexistent"));
}

// ===========================================================================
// 4. Crash recovery and isolation
// ===========================================================================

#[test]
fn unreachable_trap_disables_plugin() -> Result<(), WasmError> {
    let wasm = trap_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(!runtime.is_plugin_disabled(&id)?);

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn div_zero_trap_disables_plugin() -> Result<(), WasmError> {
    let wasm = div_zero_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn stack_overflow_trap_disables_plugin() -> Result<(), WasmError> {
    let wasm = stack_overflow_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn disabled_plugin_returns_error_on_process() -> Result<(), WasmError> {
    let wasm = trap_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // First call traps
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Second call returns PluginDisabled
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::PluginDisabled { .. })));
    Ok(())
}

#[test]
fn disabled_plugin_has_info() -> Result<(), WasmError> {
    let wasm = trap_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);

    let info = runtime.get_plugin_disabled_info(&id)?;
    assert!(info.is_some());
    let info = info.ok_or(WasmError::PluginNotInitialized)?;
    assert!(!info.reason.is_empty());
    Ok(())
}

#[test]
fn re_enable_recovers_disabled_plugin() -> Result<(), WasmError> {
    let wasm = trap_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);

    // Re-enabling again is a no-op
    let was_disabled_again = runtime.re_enable_plugin(&id)?;
    assert!(!was_disabled_again);
    Ok(())
}

#[test]
fn crash_in_one_plugin_does_not_affect_another() -> Result<(), WasmError> {
    let wasm_good = minimal_plugin()?;
    let wasm_bad = trap_plugin()?;
    let mut runtime = WasmRuntime::new()?;

    let good_id = Uuid::new_v4();
    let bad_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(good_id, &wasm_good, vec![])?;
    runtime.load_plugin_from_bytes(bad_id, &wasm_bad, vec![])?;

    // Crash the bad plugin
    let _ = runtime.process(&bad_id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&bad_id)?);

    // Good plugin still works
    let result = runtime.process(&good_id, 99.0, 0.001)?;
    assert!((result - 99.0).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&good_id)?);
    Ok(())
}

#[test]
fn hot_reload_recovers_crashed_plugin() -> Result<(), WasmError> {
    let wasm_bad = trap_plugin()?;
    let wasm_good = adding_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm_bad, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Reload with a working plugin
    runtime.reload_plugin(&id, &wasm_good, vec![])?;
    assert!(!runtime.is_plugin_disabled(&id)?);

    let result = runtime.process(&id, 2.0, 3.0)?;
    assert!((result - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn hot_reload_failure_preserves_old_plugin() -> Result<(), WasmError> {
    let wasm_good = minimal_plugin()?;
    let wasm_no_process = memory_only_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm_good, vec![])?;
    let before = runtime.process(&id, 10.0, 0.001)?;
    assert!((before - 10.0).abs() < f32::EPSILON);

    // Reload with invalid module (missing process) — should fail
    let result = runtime.reload_plugin(&id, &wasm_no_process, vec![]);
    assert!(result.is_err());

    // Old plugin still works
    let after = runtime.process(&id, 10.0, 0.001)?;
    assert!((after - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn hot_reload_preserves_statistics() -> Result<(), WasmError> {
    let wasm = minimal_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..5 {
        runtime.process(&id, 1.0, 0.001)?;
    }

    let (count_before, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_before, 5);

    // Reload
    runtime.reload_plugin(&id, &wasm, vec![])?;

    let (count_after, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count_after, 5); // preserved
    Ok(())
}

// ===========================================================================
// 5. Plugin API surface validation
// ===========================================================================

#[test]
fn process_nonexistent_plugin_returns_not_found() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(matches!(result, Err(WasmError::PluginNotFound(_))));
    Ok(())
}

#[test]
fn unload_nonexistent_plugin_returns_not_found() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.unload_plugin(&id);
    assert!(matches!(result, Err(WasmError::PluginNotFound(_))));
    Ok(())
}

#[test]
fn stats_nonexistent_plugin_returns_not_found() -> Result<(), WasmError> {
    let runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.get_plugin_stats(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn is_initialized_nonexistent_plugin_errors() -> Result<(), WasmError> {
    let runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.is_plugin_initialized(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn is_disabled_nonexistent_plugin_errors() -> Result<(), WasmError> {
    let runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.is_plugin_disabled(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn re_enable_nonexistent_plugin_errors() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.re_enable_plugin(&id);
    assert!(matches!(result, Err(WasmError::PluginNotFound(_))));
    Ok(())
}

#[test]
fn runtime_new_starts_empty() -> Result<(), WasmError> {
    let runtime = WasmRuntime::new()?;
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn runtime_resource_limits_accessible() -> Result<(), WasmError> {
    let limits = ResourceLimits::new(2 * 1024 * 1024, 500_000, 5_000, 4);
    let runtime = WasmRuntime::with_limits(limits)?;

    let rl = runtime.resource_limits();
    assert_eq!(rl.max_memory_bytes, 2 * 1024 * 1024);
    assert_eq!(rl.max_fuel, 500_000);
    assert_eq!(rl.max_table_elements, 5_000);
    assert_eq!(rl.max_instances, 4);
    Ok(())
}

#[test]
fn process_correct_output_for_identity() -> Result<(), WasmError> {
    let wasm = minimal_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for input in [0.0_f32, 1.0, -1.0, 0.5, 100.0, -100.0, f32::MIN, f32::MAX] {
        let result = runtime.process(&id, input, 0.001)?;
        assert!(
            (result - input).abs() < f32::EPSILON,
            "Identity failed for input={input}: got {result}"
        );
    }
    Ok(())
}

#[test]
fn process_correct_output_for_add() -> Result<(), WasmError> {
    let wasm = adding_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let test_cases: &[(f32, f32, f32)] = &[
        (1.0, 2.0, 3.0),
        (0.0, 0.0, 0.0),
        (-1.0, 1.0, 0.0),
        (0.5, 0.25, 0.75),
    ];

    for &(a, b, expected) in test_cases {
        let result = runtime.process(&id, a, b)?;
        assert!(
            (result - expected).abs() < 0.001,
            "Add({a}, {b}) = {result}, expected {expected}"
        );
    }
    Ok(())
}

#[test]
fn process_correct_output_for_multiply() -> Result<(), WasmError> {
    let wasm = full_lifecycle_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 3.0, 4.0)?;
    assert!((result - 12.0).abs() < 0.001);
    Ok(())
}

#[test]
fn process_constant_zero_plugin() -> Result<(), WasmError> {
    let wasm = zero_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 999.0, 0.001)?;
    assert!((result - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn plugin_stats_track_calls() -> Result<(), WasmError> {
    let wasm = minimal_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let (count, avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 0);
    assert_eq!(avg, 0.0);

    for i in 0..20 {
        runtime.process(&id, i as f32, 0.001)?;
    }

    let (count, avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 20);
    assert!(avg >= 0.0);
    Ok(())
}

#[test]
fn unload_calls_shutdown_without_error() -> Result<(), WasmError> {
    let wasm = full_lifecycle_plugin()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    runtime.process(&id, 1.0, 0.001)?;
    runtime.unload_plugin(&id)?;

    assert!(!runtime.has_plugin(&id));
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn error_type_classification() {
    let crash = WasmError::crashed("boom");
    assert!(crash.is_crash());
    assert!(!crash.is_timeout());

    let timeout = WasmError::ExecutionTimeout {
        duration: std::time::Duration::from_secs(1),
    };
    assert!(timeout.is_timeout());
    assert!(!timeout.is_crash());

    let budget = WasmError::BudgetViolation {
        used_us: 100,
        budget_us: 50,
    };
    assert!(budget.is_budget_violation());
    assert!(!budget.is_capability_violation());

    let cap = WasmError::CapabilityViolation {
        capability: "foo".to_string(),
    };
    assert!(cap.is_capability_violation());
    assert!(!cap.is_budget_violation());
}

#[test]
fn error_display_messages() {
    let err = WasmError::loading_failed("bad module");
    let msg = format!("{err}");
    assert!(msg.contains("bad module"));

    let err = WasmError::plugin_not_found("abc-123");
    let msg = format!("{err}");
    assert!(msg.contains("abc-123"));

    let err = WasmError::MaxInstancesReached(32);
    let msg = format!("{err}");
    assert!(msg.contains("32"));
}

#[test]
fn multiple_plugins_process_independently() -> Result<(), WasmError> {
    let wasm_identity = minimal_plugin()?;
    let wasm_add = adding_plugin()?;
    let wasm_mul = full_lifecycle_plugin()?;

    let mut runtime = WasmRuntime::new()?;
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm_identity, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm_add, vec![])?;
    runtime.load_plugin_from_bytes(id3, &wasm_mul, vec![])?;

    assert_eq!(runtime.instance_count(), 3);

    let r1 = runtime.process(&id1, 5.0, 3.0)?;
    let r2 = runtime.process(&id2, 5.0, 3.0)?;
    let r3 = runtime.process(&id3, 5.0, 3.0)?;

    assert!((r1 - 5.0).abs() < f32::EPSILON); // identity
    assert!((r2 - 8.0).abs() < f32::EPSILON); // add
    assert!((r3 - 15.0).abs() < f32::EPSILON); // multiply
    Ok(())
}

// ===========================================================================
// Additional edge cases
// ===========================================================================

#[test]
fn abi_state_lifecycle() {
    use openracing_wasm_runtime::state::WasmPluginAbiState;

    let mut state = WasmPluginAbiState::new();
    assert!(!state.is_initialized());
    assert_eq!(state.process_count, 0);
    assert_eq!(state.average_process_time_us(), 0.0);

    state.mark_initialized();
    assert!(state.is_initialized());

    state.record_process_call(100);
    state.record_process_call(200);
    assert_eq!(state.process_count, 2);
    assert!((state.average_process_time_us() - 150.0).abs() < f64::EPSILON);

    state.store_data("key".to_string(), vec![1, 2, 3]);
    assert_eq!(state.get_data("key"), Some(&vec![1, 2, 3]));
    assert_eq!(state.get_data("missing"), None);

    state.remove_data("key");
    assert_eq!(state.get_data("key"), None);

    state.reset_stats();
    assert_eq!(state.process_count, 0);
    assert_eq!(state.total_process_time_us, 0);

    state.mark_failed("oops".to_string());
    assert!(!state.is_initialized());
    assert_eq!(state.last_error, Some("oops".to_string()));

    state.mark_shutdown();
    assert!(!state.is_initialized());
}

#[test]
fn preserved_state_helpers() {
    use openracing_wasm_runtime::PreservedPluginState;

    let empty = PreservedPluginState::new();
    assert!(empty.is_empty());
    assert_eq!(empty.average_process_time_us(), 0.0);

    let mut data = std::collections::HashMap::new();
    data.insert("k".to_string(), vec![42]);

    let state = PreservedPluginState {
        plugin_data: data,
        process_count: 10,
        total_process_time_us: 1000,
    };
    assert!(!state.is_empty());
    assert!((state.average_process_time_us() - 100.0).abs() < f64::EPSILON);
}

#[test]
fn hot_reloader_stats() {
    use openracing_wasm_runtime::HotReloader;

    let mut reloader = HotReloader::new();
    assert_eq!(reloader.reload_count(), 0);
    assert_eq!(reloader.failed_reload_count(), 0);
    assert_eq!(reloader.total_attempts(), 0);
    assert!((reloader.success_rate() - 100.0).abs() < f64::EPSILON);

    reloader.record_success();
    reloader.record_success();
    reloader.record_failure();

    assert_eq!(reloader.reload_count(), 2);
    assert_eq!(reloader.failed_reload_count(), 1);
    assert_eq!(reloader.total_attempts(), 3);

    let expected_rate = 2.0 / 3.0 * 100.0;
    assert!((reloader.success_rate() - expected_rate).abs() < 0.01);

    reloader.reset_stats();
    assert_eq!(reloader.total_attempts(), 0);
}
