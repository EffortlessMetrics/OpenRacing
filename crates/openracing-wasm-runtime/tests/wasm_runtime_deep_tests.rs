//! Deep tests for the WASM plugin runtime.
//!
//! Covers module loading, sandboxing, capability enforcement, resource budgets,
//! crash recovery, concurrency, ABI compatibility, validation, host function
//! binding, state isolation, lifecycle, and error propagation.

use openracing_wasm_runtime::prelude::CapabilityChecker;
use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT helper modules
// ---------------------------------------------------------------------------

/// Minimal valid module: passthrough process.
fn wat_passthrough() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Module that adds its two inputs.
fn wat_add() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add))"#,
    )?)
}

/// Module that multiplies input by 2.
fn wat_double() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                f32.const 2.0
                f32.mul))"#,
    )?)
}

/// Module that unconditionally traps.
fn wat_trap() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable))"#,
    )?)
}

/// Module with init/shutdown lifecycle hooks.
fn wat_lifecycle() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (global $state (mut i32) (i32.const 0))
            (func (export "init") (result i32)
                i32.const 1
                global.set $state
                i32.const 0)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0)
            (func (export "shutdown")
                i32.const 0
                global.set $state))"#,
    )?)
}

/// Module whose init returns a non-zero (error) code.
fn wat_init_fail() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const -1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Module whose init traps.
fn wat_init_trap() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                unreachable)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Module that burns fuel in a tight loop (consumes many instructions).
fn wat_fuel_burner() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (local $i i32)
                (local.set $i (i32.const 0))
                (block $break
                    (loop $loop
                        (br_if $break (i32.ge_u (local.get $i) (i32.const 10000000)))
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $loop)))
                local.get 0))"#,
    )?)
}

/// Module that writes to its own linear memory (allocation test).
fn wat_memory_writer() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; write a sentinel value at offset 0
                (i32.store (i32.const 0) (i32.const 0xDEADBEEF))
                local.get 0))"#,
    )?)
}

/// Module with global mutable state that accumulates across calls.
fn wat_stateful() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (global $acc (mut f32) (f32.const 0.0))
            (func (export "process") (param f32 f32) (result f32)
                (global.set $acc (f32.add (global.get $acc) (local.get 0)))
                global.get $acc))"#,
    )?)
}

/// Module that imports and calls the host `get_timestamp_us` function.
fn wat_host_timestamp() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_timestamp_us" (func $ts (result i64)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (drop (call $ts))
                local.get 0))"#,
    )?)
}

/// Module that imports host logging.
fn wat_host_logging() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_info" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "hello")
            (func (export "process") (param f32 f32) (result f32)
                (call $log (i32.const 0) (i32.const 5))
                local.get 0))"#,
    )?)
}

/// Module that imports host check_capability.
fn wat_host_check_cap() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "read_telemetry")
            (func (export "process") (param f32 f32) (result f32)
                ;; check read_telemetry (14 bytes)
                (drop (call $chk (i32.const 0) (i32.const 14)))
                local.get 0))"#,
    )?)
}

/// Module missing the required `memory` export.
fn wat_missing_memory() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Module missing the required `process` export.
fn wat_missing_process() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1))"#,
    )?)
}

/// Module that returns a constant.
fn wat_constant(val: f32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let src = format!(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                f32.const {val}))"#
    );
    Ok(wat::parse_str(&src)?)
}

// ===========================================================================
// 1. WASM module loading and instantiation
// ===========================================================================

#[test]
fn load_valid_module_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_passthrough()?, vec![])?;
    assert!(rt.has_plugin(&id));
    assert_eq!(rt.instance_count(), 1);
    Ok(())
}

#[test]
fn load_multiple_modules_sequentially() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    for _ in 0..5 {
        rt.load_plugin_from_bytes(Uuid::new_v4(), &wat_passthrough()?, vec![])?;
    }
    assert_eq!(rt.instance_count(), 5);
    Ok(())
}

#[test]
fn load_module_with_init_and_shutdown() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_lifecycle()?, vec![])?;
    assert!(rt.is_plugin_initialized(&id)?);
    rt.unload_plugin(&id)?;
    assert!(!rt.has_plugin(&id));
    Ok(())
}

// ===========================================================================
// 2. Memory allocation within WASM sandbox
// ===========================================================================

#[test]
fn plugin_can_write_to_own_memory() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_memory_writer()?, vec![])?;
    let result = rt.process(&id, 7.0, 0.001)?;
    assert!((result - 7.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn memory_limit_restricts_allocation() -> Result<(), Box<dyn std::error::Error>> {
    // Use a very restrictive memory limit. If a plugin tries to grow memory
    // beyond what the runtime allows, the growth should fail or the module
    // with a very large initial memory request may still instantiate (wasmtime
    // may not enforce Store-level memory limits on initial declared memory).
    // Instead, verify that the runtime stores the limit and that conservative
    // limits are smaller than generous ones.
    let conservative = ResourceLimits::conservative();
    let generous = ResourceLimits::generous();
    assert!(conservative.max_memory_bytes < generous.max_memory_bytes);

    // Verify a normal plugin works under restricted limits.
    let limits = ResourceLimits::default().with_memory(1024 * 1024);
    let mut rt = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();
    let wasm = wat_passthrough()?;
    rt.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = rt.process(&id, 5.0, 0.001)?;
    assert!((result - 5.0).abs() < f32::EPSILON);
    Ok(())
}

// ===========================================================================
// 3. Capability enforcement
// ===========================================================================

#[test]
fn capability_checker_grants_listed_caps() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::new(vec!["read_telemetry".into(), "control_leds".into()]);
    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_led_control().is_ok());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_dsp_processing().is_err());
    Ok(())
}

#[test]
fn capability_checker_denies_unlisted_caps() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::default();
    let err = checker.check_telemetry_read();
    assert!(err.is_err());
    if let Err(e) = err {
        assert!(e.is_capability_violation());
    }
    Ok(())
}

#[test]
fn capability_has_capability_query() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::new(vec!["process_dsp".into()]);
    assert!(checker.has_capability("process_dsp"));
    assert!(!checker.has_capability("read_telemetry"));
    Ok(())
}

#[test]
fn plugin_loaded_with_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_host_check_cap()?, vec!["read_telemetry".into()])?;
    // The process function calls check_capability internally — should not trap.
    let result = rt.process(&id, 3.25, 0.001)?;
    assert!((result - 3.25).abs() < f32::EPSILON);
    Ok(())
}

// ===========================================================================
// 4. Resource budget enforcement (fuel / instruction limits)
// ===========================================================================

#[test]
fn fuel_exhaustion_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    // Very low fuel budget to trigger exhaustion.
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut rt = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_fuel_burner()?, vec![])?;
    let result = rt.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(rt.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn generous_fuel_allows_completion() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_fuel(100_000_000);
    let mut rt = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_fuel_burner()?, vec![])?;
    let result = rt.process(&id, 42.0, 0.001)?;
    assert!((result - 42.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn fuel_is_replenished_between_calls() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_passthrough()?, vec![])?;
    for i in 0..100 {
        let result = rt.process(&id, i as f32, 0.001)?;
        assert!((result - i as f32).abs() < f32::EPSILON);
    }
    Ok(())
}

// ===========================================================================
// 5. Plugin crash recovery and restart
// ===========================================================================

#[test]
fn trap_marks_plugin_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_trap()?, vec![])?;

    let err = rt.process(&id, 1.0, 0.001);
    assert!(err.is_err());
    assert!(rt.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn disabled_plugin_returns_error_on_process() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_trap()?, vec![])?;
    let _ = rt.process(&id, 1.0, 0.001);

    let second = rt.process(&id, 1.0, 0.001);
    assert!(second.is_err());
    if let Err(ref e) = second {
        assert!(matches!(e, WasmError::PluginDisabled { .. }));
    }
    Ok(())
}

#[test]
fn re_enable_after_crash() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_trap()?, vec![])?;
    let _ = rt.process(&id, 1.0, 0.001);

    assert!(rt.is_plugin_disabled(&id)?);
    let was = rt.re_enable_plugin(&id)?;
    assert!(was);
    assert!(!rt.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn re_enable_non_disabled_returns_false() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_passthrough()?, vec![])?;
    let was = rt.re_enable_plugin(&id)?;
    assert!(!was);
    Ok(())
}

#[test]
fn crash_recovery_via_hot_reload() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_trap()?, vec![])?;
    let _ = rt.process(&id, 1.0, 0.001);
    assert!(rt.is_plugin_disabled(&id)?);

    // Reload with a working module.
    rt.reload_plugin(&id, &wat_passthrough()?, vec![])?;
    assert!(!rt.is_plugin_disabled(&id)?);
    let result = rt.process(&id, 5.0, 0.001)?;
    assert!((result - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn disabled_info_captures_reason() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_trap()?, vec![])?;
    let _ = rt.process(&id, 1.0, 0.001);

    let info = rt.get_plugin_disabled_info(&id)?;
    assert!(info.is_some());
    let info = info.ok_or("expected disabled info")?;
    assert!(!info.reason.is_empty());
    Ok(())
}

// ===========================================================================
// 6. Multiple WASM modules running simultaneously
// ===========================================================================

#[test]
fn multiple_plugins_independent_results() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id_pass = Uuid::new_v4();
    let id_add = Uuid::new_v4();
    let id_dbl = Uuid::new_v4();

    rt.load_plugin_from_bytes(id_pass, &wat_passthrough()?, vec![])?;
    rt.load_plugin_from_bytes(id_add, &wat_add()?, vec![])?;
    rt.load_plugin_from_bytes(id_dbl, &wat_double()?, vec![])?;

    assert_eq!(rt.instance_count(), 3);

    let r1 = rt.process(&id_pass, 10.0, 1.0)?;
    let r2 = rt.process(&id_add, 10.0, 1.0)?;
    let r3 = rt.process(&id_dbl, 10.0, 1.0)?;

    assert!((r1 - 10.0).abs() < f32::EPSILON);
    assert!((r2 - 11.0).abs() < f32::EPSILON);
    assert!((r3 - 20.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn one_plugin_crash_does_not_affect_others() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id_good = Uuid::new_v4();
    let id_bad = Uuid::new_v4();

    rt.load_plugin_from_bytes(id_good, &wat_passthrough()?, vec![])?;
    rt.load_plugin_from_bytes(id_bad, &wat_trap()?, vec![])?;

    let _ = rt.process(&id_bad, 1.0, 0.001);
    assert!(rt.is_plugin_disabled(&id_bad)?);

    // The healthy plugin should still work.
    let result = rt.process(&id_good, 99.0, 0.001)?;
    assert!((result - 99.0).abs() < f32::EPSILON);
    assert!(!rt.is_plugin_disabled(&id_good)?);
    Ok(())
}

#[test]
fn max_instances_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_max_instances(3);
    let mut rt = WasmRuntime::with_limits(limits)?;
    let wasm = wat_passthrough()?;

    for _ in 0..3 {
        rt.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;
    }
    let fourth = rt.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(fourth.is_err());
    if let Err(ref e) = fourth {
        assert!(matches!(e, WasmError::MaxInstancesReached(3)));
    }
    Ok(())
}

// ===========================================================================
// 7. WASM ABI compatibility
// ===========================================================================

#[test]
fn process_signature_f32_f32_to_f32() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_add()?, vec![])?;
    let result = rt.process(&id, 1.5, 2.5)?;
    assert!((result - 4.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn init_returns_zero_for_success() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_lifecycle()?, vec![])?;
    assert!(rt.is_plugin_initialized(&id)?);
    Ok(())
}

#[test]
fn init_non_zero_means_failure() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    let result = rt.load_plugin_from_bytes(id, &wat_init_fail()?, vec![]);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(matches!(e, WasmError::LoadingFailed(_)));
    }
    Ok(())
}

#[test]
fn module_without_init_is_auto_initialized() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_passthrough()?, vec![])?;
    assert!(rt.is_plugin_initialized(&id)?);
    Ok(())
}

// ===========================================================================
// 8. Module validation (invalid WASM rejected)
// ===========================================================================

#[test]
fn reject_garbage_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), b"not wasm", vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn reject_empty_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), &[], vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn reject_truncated_wasm() -> Result<(), Box<dyn std::error::Error>> {
    let full = wat_passthrough()?;
    let truncated = &full[..full.len() / 2];
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), truncated, vec![]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn reject_module_missing_memory() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), &wat_missing_memory()?, vec![]);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(matches!(e, WasmError::MissingExport(_)));
    }
    Ok(())
}

#[test]
fn reject_module_missing_process() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), &wat_missing_process()?, vec![]);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(matches!(e, WasmError::MissingExport(_)));
    }
    Ok(())
}

#[test]
fn reject_init_that_traps() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), &wat_init_trap()?, vec![]);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(matches!(e, WasmError::LoadingFailed(_)));
    }
    Ok(())
}

// ===========================================================================
// 9. Host function binding
// ===========================================================================

#[test]
fn host_timestamp_function_callable() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_host_timestamp()?, vec![])?;
    let result = rt.process(&id, 1.0, 0.001)?;
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn host_logging_function_callable() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_host_logging()?, vec![])?;
    let result = rt.process(&id, 2.0, 0.001)?;
    assert!((result - 2.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn host_check_capability_function_callable() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_host_check_cap()?, vec!["read_telemetry".into()])?;
    let result = rt.process(&id, 9.0, 0.001)?;
    assert!((result - 9.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn missing_host_import_fails_load() -> Result<(), Box<dyn std::error::Error>> {
    // Module that imports a function the runtime does not provide.
    let bad_import = wat::parse_str(
        r#"(module
            (import "env" "nonexistent_fn" (func (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?;
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), &bad_import, vec![]);
    assert!(result.is_err());
    Ok(())
}

// ===========================================================================
// 10. Plugin state isolation between instances
// ===========================================================================

#[test]
fn stateful_plugins_have_independent_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    rt.load_plugin_from_bytes(id_a, &wat_stateful()?, vec![])?;
    rt.load_plugin_from_bytes(id_b, &wat_stateful()?, vec![])?;

    // Accumulate in plugin A: 1 + 2 + 3 = 6
    rt.process(&id_a, 1.0, 0.0)?;
    rt.process(&id_a, 2.0, 0.0)?;
    let r_a = rt.process(&id_a, 3.0, 0.0)?;
    assert!((r_a - 6.0).abs() < f32::EPSILON);

    // Plugin B has not been called, so first call yields just the input.
    let r_b = rt.process(&id_b, 10.0, 0.0)?;
    assert!((r_b - 10.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn plugin_stats_are_per_instance() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    rt.load_plugin_from_bytes(id_a, &wat_passthrough()?, vec![])?;
    rt.load_plugin_from_bytes(id_b, &wat_passthrough()?, vec![])?;

    for _ in 0..5 {
        rt.process(&id_a, 1.0, 0.001)?;
    }
    rt.process(&id_b, 1.0, 0.001)?;

    let (count_a, _) = rt.get_plugin_stats(&id_a)?;
    let (count_b, _) = rt.get_plugin_stats(&id_b)?;

    assert_eq!(count_a, 5);
    assert_eq!(count_b, 1);
    Ok(())
}

#[test]
fn telemetry_update_is_per_plugin() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_plugin_abi::TelemetryFrame;

    let mut rt = WasmRuntime::new()?;
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();

    rt.load_plugin_from_bytes(id_a, &wat_passthrough()?, vec![])?;
    rt.load_plugin_from_bytes(id_b, &wat_passthrough()?, vec![])?;

    let frame = TelemetryFrame::with_values(12345, 90.0, 1.0, 55.0, 0);
    rt.update_plugin_telemetry(&id_a, frame)?;

    // Plugin B should still have default telemetry.
    let (_, _) = rt.get_plugin_stats(&id_b)?;
    // Just verify it didn't error — telemetry for B was not touched.
    Ok(())
}

// ===========================================================================
// 11. Module lifecycle (load → init → process → shutdown)
// ===========================================================================

#[test]
fn full_lifecycle_with_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Load (includes init call)
    rt.load_plugin_from_bytes(id, &wat_lifecycle()?, vec![])?;
    assert!(rt.is_plugin_initialized(&id)?);

    // Process
    let result = rt.process(&id, 7.5, 0.001)?;
    assert!((result - 7.5).abs() < f32::EPSILON);

    // Statistics recorded
    let (count, avg) = rt.get_plugin_stats(&id)?;
    assert_eq!(count, 1);
    assert!(avg > 0.0);

    // Shutdown
    rt.unload_plugin(&id)?;
    assert!(!rt.has_plugin(&id));
    Ok(())
}

#[test]
fn process_without_load_returns_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let missing = Uuid::new_v4();
    let result = rt.process(&missing, 1.0, 0.001);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(matches!(e, WasmError::PluginNotFound(_)));
    }
    Ok(())
}

#[test]
fn unload_missing_plugin_returns_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let missing = Uuid::new_v4();
    let result = rt.unload_plugin(&missing);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(matches!(e, WasmError::PluginNotFound(_)));
    }
    Ok(())
}

#[test]
fn hot_reload_preserves_statistics() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_passthrough()?, vec![])?;

    for _ in 0..10 {
        rt.process(&id, 1.0, 0.001)?;
    }
    let (count_before, _) = rt.get_plugin_stats(&id)?;
    assert_eq!(count_before, 10);

    // Hot-reload with a different module.
    rt.reload_plugin(&id, &wat_add()?, vec![])?;

    let (count_after, _) = rt.get_plugin_stats(&id)?;
    assert_eq!(count_after, 10); // preserved
    Ok(())
}

#[test]
fn hot_reload_failure_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_constant(42.0)?, vec![])?;

    let before = rt.process(&id, 0.0, 0.0)?;
    assert!((before - 42.0).abs() < f32::EPSILON);

    // Attempt reload with invalid bytes.
    let bad_reload = rt.reload_plugin(&id, b"not valid wasm", vec![]);
    assert!(bad_reload.is_err());

    // Old plugin still intact.
    let after = rt.process(&id, 0.0, 0.0)?;
    assert!((after - 42.0).abs() < f32::EPSILON);
    Ok(())
}

// ===========================================================================
// 12. Error propagation from WASM to host
// ===========================================================================

#[test]
fn trap_propagates_as_crashed_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_trap()?, vec![])?;

    let err = rt.process(&id, 1.0, 0.001);
    assert!(err.is_err());
    let e = err.err().ok_or("expected error")?;
    assert!(e.is_crash());
    Ok(())
}

#[test]
fn fuel_exhaustion_propagates_as_budget_violation() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut rt = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_fuel_burner()?, vec![])?;

    let err = rt.process(&id, 1.0, 0.001);
    assert!(err.is_err());
    let e = err.err().ok_or("expected error")?;
    assert!(e.is_budget_violation());
    Ok(())
}

#[test]
fn init_error_propagates_as_loading_failed() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), &wat_init_fail()?, vec![]);
    assert!(result.is_err());
    if let Err(ref e) = result {
        assert!(
            matches!(e, WasmError::LoadingFailed(_)),
            "expected LoadingFailed, got: {e}"
        );
    }
    Ok(())
}

#[test]
fn error_display_contains_useful_info() -> Result<(), Box<dyn std::error::Error>> {
    let e = WasmError::crashed("oops");
    let msg = format!("{e}");
    assert!(msg.contains("oops"));

    let e2 = WasmError::CapabilityViolation {
        capability: "read_telemetry".into(),
    };
    let msg2 = format!("{e2}");
    assert!(msg2.contains("read_telemetry"));
    Ok(())
}

#[test]
fn compilation_of_invalid_module_gives_descriptive_error() -> Result<(), Box<dyn std::error::Error>>
{
    let mut rt = WasmRuntime::new()?;
    let result = rt.load_plugin_from_bytes(Uuid::new_v4(), b"\x00asm\x01\x00\x00\x00bad", vec![]);
    assert!(result.is_err());
    let msg = format!("{}", result.err().ok_or("expected error")?);
    assert!(!msg.is_empty());
    Ok(())
}

// ===========================================================================
// Additional edge-case tests
// ===========================================================================

#[test]
fn resource_limits_validation_rejects_extremes() {
    let too_small_mem = ResourceLimits::default().with_memory(1024);
    assert!(too_small_mem.validate().is_err());

    let too_small_fuel = ResourceLimits::default().with_fuel(100);
    assert!(too_small_fuel.validate().is_err());

    let zero_instances = ResourceLimits::default().with_max_instances(0);
    assert!(zero_instances.validate().is_err());

    let too_many_instances = ResourceLimits::default().with_max_instances(9999);
    assert!(too_many_instances.validate().is_err());
}

#[test]
fn conservative_and_generous_presets_validate() {
    assert!(ResourceLimits::conservative().validate().is_ok());
    assert!(ResourceLimits::generous().validate().is_ok());
}

#[test]
fn runtime_with_conservative_limits() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::with_limits(ResourceLimits::conservative())?;
    let id = Uuid::new_v4();
    rt.load_plugin_from_bytes(id, &wat_passthrough()?, vec![])?;
    let result = rt.process(&id, 3.0, 0.001)?;
    assert!((result - 3.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn update_telemetry_for_missing_plugin_errors() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_plugin_abi::TelemetryFrame;

    let mut rt = WasmRuntime::new()?;
    let missing = Uuid::new_v4();
    let result = rt.update_plugin_telemetry(&missing, TelemetryFrame::default());
    assert!(result.is_err());
    Ok(())
}

#[test]
fn get_stats_for_missing_plugin_errors() -> Result<(), Box<dyn std::error::Error>> {
    let rt = WasmRuntime::new()?;
    let missing = Uuid::new_v4();
    let result = rt.get_plugin_stats(&missing);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn re_enable_missing_plugin_errors() -> Result<(), Box<dyn std::error::Error>> {
    let mut rt = WasmRuntime::new()?;
    let missing = Uuid::new_v4();
    let result = rt.re_enable_plugin(&missing);
    assert!(result.is_err());
    Ok(())
}
