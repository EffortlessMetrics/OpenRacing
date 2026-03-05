//! WASM sandbox hardening tests.
//!
//! Covers sandbox isolation, memory limit enforcement, CPU time budget enforcement,
//! capability permission checking, guest-host communication boundary tests,
//! and panic/trap recovery in guest code.

use openracing_wasm_runtime::prelude::CapabilityChecker;
use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT helper modules
// ---------------------------------------------------------------------------

fn wat_passthrough() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

fn wat_trap() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable))"#,
    )?)
}

fn wat_infinite_loop() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (loop $lp br $lp)
                local.get 0))"#,
    )?)
}

fn wat_with_init(return_code: i32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(format!(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const {return_code})
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#
    ))?)
}

fn wat_large_memory(pages: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(format!(
        r#"(module
            (memory (export "memory") {pages})
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#
    ))?)
}

fn wat_check_capability(cap_name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let len = cap_name.len();
    Ok(wat::parse_str(format!(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "{cap_name}")
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const {len}))
                f32.convert_i32_s))"#
    ))?)
}

fn wat_doubler() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 0
                f32.add))"#,
    )?)
}

fn wat_with_shutdown() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const 0)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0)
            (func (export "shutdown")
                nop))"#,
    )?)
}

fn wat_oob_memory_access() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; Access memory well beyond the 1 page (64KiB) boundary
                (i32.load (i32.const 1048576))
                drop
                local.get 0))"#,
    )?)
}

fn wat_stack_overflow() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func $recurse (result f32)
                call $recurse)
            (func (export "process") (param f32 f32) (result f32)
                call $recurse))"#,
    )?)
}

fn wat_get_timestamp() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_timestamp_us" (func $ts (result i64)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (call $ts)
                drop
                local.get 0))"#,
    )?)
}

// ===========================================================================
// 1. Sandbox isolation
// ===========================================================================

#[test]
fn sandbox_plugins_have_isolated_memory() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_a = wat_passthrough()?;
    let wasm_b = wat_doubler()?;
    let mut runtime = WasmRuntime::new()?;

    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id_a, &wasm_a, vec![])?;
    runtime.load_plugin_from_bytes(id_b, &wasm_b, vec![])?;

    let out_a = runtime.process(&id_a, 5.0, 0.001)?;
    let out_b = runtime.process(&id_b, 5.0, 0.001)?;

    assert!((out_a - 5.0).abs() < f32::EPSILON);
    assert!((out_b - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn sandbox_crash_in_one_plugin_does_not_corrupt_another() -> Result<(), Box<dyn std::error::Error>>
{
    let good = wat_passthrough()?;
    let bad = wat_trap()?;
    let mut runtime = WasmRuntime::new()?;

    let good_id = Uuid::new_v4();
    let bad_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(good_id, &good, vec![])?;
    runtime.load_plugin_from_bytes(bad_id, &bad, vec![])?;

    let _ = runtime.process(&bad_id, 1.0, 0.001);

    let output = runtime.process(&good_id, 42.0, 0.001)?;
    assert!((output - 42.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn sandbox_instance_count_accurate() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let mut runtime = WasmRuntime::new()?;
    assert_eq!(runtime.instance_count(), 0);

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    assert_eq!(runtime.instance_count(), 1);

    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;
    assert_eq!(runtime.instance_count(), 2);

    runtime.unload_plugin(&id1)?;
    assert_eq!(runtime.instance_count(), 1);
    Ok(())
}

#[test]
fn sandbox_oob_memory_access_traps() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_oob_memory_access()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "out-of-bounds memory access must trap");
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn sandbox_stack_overflow_traps() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_stack_overflow()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "stack overflow must trap");
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

// ===========================================================================
// 2. Memory limit enforcement
// ===========================================================================

#[test]
fn memory_default_limit_allows_small_module() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_large_memory(4)?; // 256 KiB
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let output = runtime.process(&id, 1.0, 0.001)?;
    assert!((output - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn memory_conservative_limits_validate() {
    let limits = ResourceLimits::conservative();
    assert!(limits.validate().is_ok());
    assert_eq!(limits.max_memory_bytes, 4 * 1024 * 1024);
}

#[test]
fn memory_generous_limits_validate() {
    let limits = ResourceLimits::generous();
    assert!(limits.validate().is_ok());
    assert_eq!(limits.max_memory_bytes, 64 * 1024 * 1024);
}

#[test]
fn memory_below_minimum_rejected() {
    let limits = ResourceLimits::default().with_memory(1024); // way below 64KiB min
    assert!(limits.validate().is_err());
}

#[test]
fn memory_exactly_at_minimum_accepted() {
    let limits = ResourceLimits::default().with_memory(64 * 1024);
    assert!(limits.validate().is_ok());
}

// ===========================================================================
// 3. CPU time budget enforcement
// ===========================================================================

#[test]
fn fuel_exhaustion_terminates_infinite_loop() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(500);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());

    let err = result.err().unwrap_or_else(|| unreachable!());
    assert!(
        err.is_fuel_exhausted() || err.is_budget_violation(),
        "expected fuel exhaustion, got: {err}"
    );
    Ok(())
}

#[test]
fn fuel_replenished_each_call() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_fuel(100_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..10 {
        let output = runtime.process(&id, 1.0, 0.001)?;
        assert!((output - 1.0).abs() < f32::EPSILON);
    }
    Ok(())
}

#[test]
fn fuel_disabled_plugin_stays_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(500);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);

    assert!(runtime.is_plugin_disabled(&id)?);

    let result = runtime.process(&id, 2.0, 0.001);
    assert!(result.is_err(), "disabled plugin must reject calls");
    Ok(())
}

#[test]
fn fuel_minimum_below_threshold_rejected() {
    let limits = ResourceLimits::default().with_fuel(100);
    assert!(limits.validate().is_err());
}

// ===========================================================================
// 4. Capability permission checking
// ===========================================================================

#[test]
fn cap_read_telemetry_granted() {
    let checker = CapabilityChecker::new(vec!["read_telemetry".to_string()]);
    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.has_capability("read_telemetry"));
}

#[test]
fn cap_read_telemetry_denied_when_not_granted() {
    let checker = CapabilityChecker::new(vec![]);
    assert!(checker.check_telemetry_read().is_err());
}

#[test]
fn cap_modify_telemetry_requires_explicit_grant() {
    let read_only = CapabilityChecker::new(vec!["read_telemetry".to_string()]);
    assert!(read_only.check_telemetry_modify().is_err());

    let with_modify = CapabilityChecker::new(vec![
        "read_telemetry".to_string(),
        "modify_telemetry".to_string(),
    ]);
    assert!(with_modify.check_telemetry_modify().is_ok());
}

#[test]
fn cap_led_control_requires_grant() {
    let without = CapabilityChecker::new(vec!["read_telemetry".to_string()]);
    assert!(without.check_led_control().is_err());

    let with = CapabilityChecker::new(vec!["control_leds".to_string()]);
    assert!(with.check_led_control().is_ok());
}

#[test]
fn cap_dsp_processing_requires_grant() {
    let without = CapabilityChecker::new(vec![]);
    assert!(without.check_dsp_processing().is_err());

    let with = CapabilityChecker::new(vec!["process_dsp".to_string()]);
    assert!(with.check_dsp_processing().is_ok());
}

#[test]
fn cap_empty_denies_all() {
    let checker = CapabilityChecker::default();
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
}

#[test]
fn cap_wasm_host_check_capability_granted() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_capability("read_telemetry")?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;

    let result = runtime.process(&id, 0.0, 0.001)?;
    // check_capability returns 1 for success (granted)
    assert!(
        (result - 1.0).abs() < f32::EPSILON,
        "check_capability should return 1 (granted) when capability is allowed, got {result}"
    );
    Ok(())
}

#[test]
fn cap_wasm_host_check_capability_denied() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_capability("modify_telemetry")?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load without modify_telemetry capability
    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;

    let result = runtime.process(&id, 0.0, 0.001)?;
    // check_capability returns negative value for denied
    assert!(
        result < 0.0,
        "check_capability should return negative (denied), got {result}"
    );
    Ok(())
}

// ===========================================================================
// 5. Guest-host communication boundary tests
// ===========================================================================

#[test]
fn host_fn_get_timestamp_returns_value() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_get_timestamp()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 7.5, 0.001)?;
    assert!(
        (result - 7.5).abs() < f32::EPSILON,
        "plugin should return input after calling get_timestamp: got {result}"
    );
    Ok(())
}

#[test]
fn host_fn_init_success_initializes_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_with_init(0)?; // return code 0 = success
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);
    Ok(())
}

#[test]
fn host_fn_init_failure_rejects_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_with_init(-1)?; // return code -1 = error
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(
        result.is_err(),
        "plugin with failing init must not be loaded"
    );
    Ok(())
}

#[test]
fn host_fn_shutdown_called_on_unload() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_with_shutdown()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);

    runtime.unload_plugin(&id)?;
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn host_fn_process_stats_tracked() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..5 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }

    let (count, avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 5);
    assert!(avg >= 0.0);
    Ok(())
}

// ===========================================================================
// 6. Panic/trap recovery in guest code
// ===========================================================================

#[test]
fn trap_unreachable_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_trap()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn trap_disabled_info_available() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_trap()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 1.0, 0.001);

    let info = runtime.get_plugin_disabled_info(&id)?;
    assert!(info.is_some());
    let info = info.unwrap_or_else(|| unreachable!());
    assert!(!info.reason.is_empty());
    Ok(())
}

#[test]
fn trap_re_enable_allows_retry() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_trap()?;
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
fn trap_re_enable_nondisabled_returns_false() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(!was_disabled);
    Ok(())
}

#[test]
fn trap_invalid_wasm_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, b"not valid wasm", vec![]);
    assert!(result.is_err());
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn trap_missing_process_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(
        r#"(module
            (memory (export "memory") 1))"#,
    )?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn trap_missing_memory_export_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(
        r#"(module
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn trap_process_nonexistent_plugin_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn trap_unload_nonexistent_plugin_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.unload_plugin(&id);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn trap_max_instances_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

    let result = runtime.load_plugin_from_bytes(id3, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}

// ===========================================================================
// 7. Resource limits validation
// ===========================================================================

#[test]
fn resource_limits_zero_instances_rejected() {
    let limits = ResourceLimits::default().with_max_instances(0);
    assert!(limits.validate().is_err());
}

#[test]
fn resource_limits_too_many_instances_rejected() {
    let limits = ResourceLimits::default().with_max_instances(5000);
    assert!(limits.validate().is_err());
}

#[test]
fn resource_limits_builder_chaining() {
    let limits = ResourceLimits::default()
        .with_memory(8 * 1024 * 1024)
        .with_fuel(5_000_000)
        .with_table_elements(5_000)
        .with_max_instances(16)
        .with_epoch_interruption(false);

    assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.max_fuel, 5_000_000);
    assert_eq!(limits.max_table_elements, 5_000);
    assert_eq!(limits.max_instances, 16);
    assert!(!limits.epoch_interruption);
}

#[test]
fn resource_limits_compilation_timeout() {
    let limits =
        ResourceLimits::default().with_compilation_timeout(std::time::Duration::from_secs(5));
    assert_eq!(
        limits.compilation_timeout,
        Some(std::time::Duration::from_secs(5))
    );
}

// ===========================================================================
// 8. WasmError classification
// ===========================================================================

#[test]
fn wasm_error_is_crash() {
    let err = WasmError::crashed("test");
    assert!(err.is_crash());
    assert!(!err.is_timeout());
    assert!(!err.is_budget_violation());
}

#[test]
fn wasm_error_is_timeout() {
    let err = WasmError::ExecutionTimeout {
        duration: std::time::Duration::from_millis(100),
    };
    assert!(err.is_timeout());
    assert!(!err.is_crash());
}

#[test]
fn wasm_error_is_fuel_exhausted() {
    let err = WasmError::FuelExhausted { fuel_limit: 1000 };
    assert!(err.is_fuel_exhausted());
    assert!(err.is_budget_violation());
}

#[test]
fn wasm_error_is_capability_violation() {
    let err = WasmError::CapabilityViolation {
        capability: "read_telemetry".to_string(),
    };
    assert!(err.is_capability_violation());
    assert!(!err.is_crash());
}

#[test]
fn wasm_error_is_compilation_timeout() {
    let err = WasmError::CompilationTimeout {
        duration: std::time::Duration::from_secs(5),
    };
    assert!(err.is_compilation_timeout());
    assert!(!err.is_crash());
}
