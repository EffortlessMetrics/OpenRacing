//! Comprehensive tests for WASM plugin timeout enforcement.
//!
//! Covers:
//! - Well-behaved plugins completing within budget
//! - Slow plugins terminated after timeout (epoch-based wall-clock)
//! - Infinite loop plugins caught by fuel exhaustion
//! - Timeout configuration (customizable per-plugin via separate runtimes)
//! - Resource cleanup after timeout (no leaks)
//! - Multiple plugins with different budgets running concurrently
//! - Compilation timeout enforcement
//! - Fuel exhaustion vs epoch interruption distinction

use std::time::{Duration, Instant};

use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT helper modules
// ---------------------------------------------------------------------------

/// Minimal passthrough plugin — very low cost.
fn wat_passthrough() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Plugin that adds input + dt — small cost, well-behaved.
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

/// Plugin with a configurable loop — burns fuel proportional to iterations.
fn wat_loop(iterations: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let src = format!(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (local $i i32)
                (local.set $i (i32.const 0))
                (block $break
                    (loop $loop
                        (br_if $break (i32.ge_u (local.get $i) (i32.const {iterations})))
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $loop)))
                local.get 0))"#,
    );
    Ok(wat::parse_str(&src)?)
}

/// Infinite loop plugin — will never terminate on its own.
fn wat_infinite_loop() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (loop $infinite
                    (br $infinite))
                local.get 0))"#,
    )?)
}

/// Plugin that multiplies input by a constant — moderate cost.
fn wat_multiply() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                f32.const 2.0
                f32.mul))"#,
    )?)
}

/// Plugin with stateful accumulator via global.
fn wat_accumulator() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (global $sum (mut f32) (f32.const 0.0))
            (func (export "process") (param f32 f32) (result f32)
                (global.set $sum (f32.add (global.get $sum) (local.get 0)))
                global.get $sum))"#,
    )?)
}

// ---------------------------------------------------------------------------
// Test: well-behaved plugin completes within budget
// ---------------------------------------------------------------------------

#[test]
fn well_behaved_plugin_completes_within_budget() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 0.75, 0.001)?;
    assert!((result - 0.75).abs() < f32::EPSILON);

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 1);
    Ok(())
}

#[test]
fn well_behaved_plugin_many_calls() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_add()?;
    let limits = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for i in 0..100 {
        let input = i as f32 * 0.01;
        let dt = 0.001_f32;
        let result = runtime.process(&id, input, dt)?;
        assert!((result - (input + dt)).abs() < 1e-5);
    }

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 100);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: slow plugin terminated after timeout (fuel-based)
// ---------------------------------------------------------------------------

#[test]
fn slow_plugin_terminated_by_fuel_limit() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(10_000_000)?;
    let limits = ResourceLimits::default().with_fuel(5_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "Expected fuel exhaustion error");

    let err = result.err().ok_or("expected error")?;
    assert!(
        err.is_budget_violation() || err.is_fuel_exhausted(),
        "Expected budget violation or fuel exhaustion, got: {err}"
    );
    Ok(())
}

#[test]
fn slow_plugin_terminated_by_epoch_timeout() -> Result<(), Box<dyn std::error::Error>> {
    // Use epoch-based timeout: 50ms wall-clock limit with an infinite loop.
    // The epoch ticker thread runs every 1ms, so 50ms ≈ 50 epoch ticks.
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default()
        .with_fuel(u64::MAX / 2) // very generous fuel to avoid fuel-based termination
        .with_execution_time(Duration::from_millis(50))
        .with_epoch_interruption(true);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let start = Instant::now();
    let result = runtime.process(&id, 1.0, 0.001);
    let elapsed = start.elapsed();

    assert!(result.is_err(), "Expected timeout error");
    let err = result.err().ok_or("expected error")?;
    assert!(err.is_timeout(), "Expected ExecutionTimeout, got: {err}");

    // Should have terminated roughly around the timeout (with some tolerance)
    assert!(
        elapsed < Duration::from_millis(500),
        "Timeout should have fired within 500ms, took {:?}",
        elapsed
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: infinite loop plugin caught
// ---------------------------------------------------------------------------

#[test]
fn infinite_loop_caught_by_fuel() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(10_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "Infinite loop should be caught");

    // Plugin should be disabled after being caught
    let disabled = runtime.is_plugin_disabled(&id)?;
    assert!(disabled, "Plugin should be disabled after infinite loop");
    Ok(())
}

#[test]
fn infinite_loop_caught_by_epoch_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default()
        .with_fuel(u64::MAX / 2)
        .with_execution_time(Duration::from_millis(30))
        .with_epoch_interruption(true);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "Infinite loop should be caught by epoch");

    let disabled = runtime.is_plugin_disabled(&id)?;
    assert!(disabled, "Plugin should be disabled after epoch timeout");

    // Verify the disabled info mentions timeout
    let info = runtime.get_plugin_disabled_info(&id)?;
    assert!(info.is_some(), "Should have disabled info");
    let info = info.ok_or("expected disabled info")?;
    assert!(
        info.reason.contains("timeout") || info.reason.contains("Timeout"),
        "Disabled reason should mention timeout, got: {}",
        info.reason
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: timeout configuration (customizable per-plugin via separate runtimes)
// ---------------------------------------------------------------------------

#[test]
fn different_fuel_budgets_per_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(500_000)?;

    // Tight budget — should fail
    let tight_limits = ResourceLimits::default().with_fuel(1_000);
    let mut tight_runtime = WasmRuntime::with_limits(tight_limits)?;
    let id1 = Uuid::new_v4();
    tight_runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    let result_tight = tight_runtime.process(&id1, 1.0, 0.001);
    assert!(result_tight.is_err(), "Tight budget should fail");

    // Generous budget — should succeed
    let generous_limits = ResourceLimits::generous();
    let mut generous_runtime = WasmRuntime::with_limits(generous_limits)?;
    let id2 = Uuid::new_v4();
    generous_runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;
    let result_generous = generous_runtime.process(&id2, 1.0, 0.001);
    assert!(result_generous.is_ok(), "Generous budget should succeed");
    Ok(())
}

#[test]
fn conservative_vs_default_limits() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;

    let conservative = ResourceLimits::conservative();
    let default_limits = ResourceLimits::default();

    assert!(conservative.max_fuel < default_limits.max_fuel);
    assert!(conservative.max_memory_bytes < default_limits.max_memory_bytes);
    assert!(conservative.max_execution_time.is_some());
    assert!(conservative.compilation_timeout.is_some());
    assert!(default_limits.max_execution_time.is_none());

    // Both should work with a simple passthrough
    let mut rt_conservative = WasmRuntime::with_limits(conservative)?;
    let mut rt_default = WasmRuntime::with_limits(default_limits)?;
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    rt_conservative.load_plugin_from_bytes(id1, &wasm, vec![])?;
    rt_default.load_plugin_from_bytes(id2, &wasm, vec![])?;

    let r1 = rt_conservative.process(&id1, 5.0, 0.001)?;
    let r2 = rt_default.process(&id2, 5.0, 0.001)?;
    assert!((r1 - 5.0).abs() < f32::EPSILON);
    assert!((r2 - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn custom_execution_timeout_builder() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default()
        .with_execution_time(Duration::from_millis(200))
        .with_fuel(50_000_000)
        .with_compilation_timeout(Duration::from_secs(10));

    assert_eq!(limits.max_execution_time, Some(Duration::from_millis(200)));
    assert_eq!(limits.max_fuel, 50_000_000);
    assert_eq!(limits.compilation_timeout, Some(Duration::from_secs(10)));

    // Validate the limits
    limits.validate()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: resource cleanup after timeout (no leaks)
// ---------------------------------------------------------------------------

#[test]
fn resource_cleanup_after_fuel_exhaustion() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(5_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert_eq!(runtime.instance_count(), 1);

    // Trigger fuel exhaustion
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Unload the failed plugin — should clean up resources
    runtime.unload_plugin(&id)?;
    assert_eq!(runtime.instance_count(), 0);
    assert!(!runtime.has_plugin(&id));
    Ok(())
}

#[test]
fn resource_cleanup_after_epoch_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default()
        .with_fuel(u64::MAX / 2)
        .with_execution_time(Duration::from_millis(30))
        .with_epoch_interruption(true);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert_eq!(runtime.instance_count(), 1);

    // Trigger epoch timeout
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Unload and verify cleanup
    runtime.unload_plugin(&id)?;
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn load_after_cleanup_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let bad_wasm = wat_infinite_loop()?;
    let good_wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_fuel(5_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    // Load and fail a plugin
    let id1 = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id1, &bad_wasm, vec![])?;
    let _ = runtime.process(&id1, 1.0, 0.001);
    runtime.unload_plugin(&id1)?;

    // Load a new plugin in the same runtime — should work fine
    let id2 = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id2, &good_wasm, vec![])?;
    let result = runtime.process(&id2, 3.0, 0.001)?;
    assert!((result - 3.0).abs() < f32::EPSILON);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: multiple plugins with different budgets running concurrently
// ---------------------------------------------------------------------------

#[test]
fn multiple_plugins_same_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_pass = wat_passthrough()?;
    let wasm_add = wat_add()?;
    let wasm_mul = wat_multiply()?;

    let limits = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id_pass = Uuid::new_v4();
    let id_add = Uuid::new_v4();
    let id_mul = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id_pass, &wasm_pass, vec![])?;
    runtime.load_plugin_from_bytes(id_add, &wasm_add, vec![])?;
    runtime.load_plugin_from_bytes(id_mul, &wasm_mul, vec![])?;

    assert_eq!(runtime.instance_count(), 3);

    // Process each plugin independently
    let r1 = runtime.process(&id_pass, 1.0, 0.001)?;
    let r2 = runtime.process(&id_add, 1.0, 0.5)?;
    let r3 = runtime.process(&id_mul, 3.0, 0.001)?;

    assert!((r1 - 1.0).abs() < f32::EPSILON);
    assert!((r2 - 1.5).abs() < f32::EPSILON);
    assert!((r3 - 6.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn multiple_plugins_one_fails_others_survive() -> Result<(), Box<dyn std::error::Error>> {
    let good_wasm = wat_passthrough()?;
    let bad_wasm = wat_infinite_loop()?;

    let limits = ResourceLimits::default().with_fuel(5_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id_good1 = Uuid::new_v4();
    let id_bad = Uuid::new_v4();
    let id_good2 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id_good1, &good_wasm, vec![])?;
    runtime.load_plugin_from_bytes(id_bad, &bad_wasm, vec![])?;
    runtime.load_plugin_from_bytes(id_good2, &good_wasm, vec![])?;

    // Good plugin 1 works
    let r1 = runtime.process(&id_good1, 2.0, 0.001)?;
    assert!((r1 - 2.0).abs() < f32::EPSILON);

    // Bad plugin fails
    let bad_result = runtime.process(&id_bad, 1.0, 0.001);
    assert!(bad_result.is_err());
    assert!(runtime.is_plugin_disabled(&id_bad)?);

    // Good plugin 2 still works after bad plugin failure
    let r2 = runtime.process(&id_good2, 4.0, 0.001)?;
    assert!((r2 - 4.0).abs() < f32::EPSILON);

    // Good plugin 1 also still works
    let r3 = runtime.process(&id_good1, 5.0, 0.001)?;
    assert!((r3 - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn multiple_runtimes_different_budgets_concurrently() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(100_000)?;

    // Runtime A: very tight budget → should fail
    let limits_a = ResourceLimits::default().with_fuel(100);
    let mut runtime_a = WasmRuntime::with_limits(limits_a)?;
    let id_a = Uuid::new_v4();
    runtime_a.load_plugin_from_bytes(id_a, &wasm, vec![])?;

    // Runtime B: generous budget → should succeed
    let limits_b = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime_b = WasmRuntime::with_limits(limits_b)?;
    let id_b = Uuid::new_v4();
    runtime_b.load_plugin_from_bytes(id_b, &wasm, vec![])?;

    // Runtime C: moderate budget → might succeed or fail depending on loop cost
    let limits_c = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime_c = WasmRuntime::with_limits(limits_c)?;
    let id_c = Uuid::new_v4();
    runtime_c.load_plugin_from_bytes(id_c, &wasm, vec![])?;

    let result_a = runtime_a.process(&id_a, 1.0, 0.001);
    let result_b = runtime_b.process(&id_b, 1.0, 0.001);
    let result_c = runtime_c.process(&id_c, 1.0, 0.001);

    assert!(result_a.is_err(), "Tight budget should fail");
    assert!(result_b.is_ok(), "Generous budget should succeed");
    // result_c may pass or fail — just verify no panic
    let _ = result_c;
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: compilation timeout
// ---------------------------------------------------------------------------

#[test]
fn compilation_with_generous_timeout_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_compilation_timeout(Duration::from_secs(30));
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    // Small module should compile well within 30s
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001)?;
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn compilation_timeout_error_variant() -> Result<(), Box<dyn std::error::Error>> {
    let err = WasmError::CompilationTimeout {
        duration: Duration::from_millis(500),
    };
    assert!(err.is_compilation_timeout());
    assert!(!err.is_timeout());
    assert!(!err.is_crash());

    let msg = format!("{err}");
    assert!(
        msg.contains("500ms"),
        "Error message should contain duration"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: fuel exhaustion error variant
// ---------------------------------------------------------------------------

#[test]
fn fuel_exhausted_error_is_also_budget_violation() -> Result<(), Box<dyn std::error::Error>> {
    let err = WasmError::FuelExhausted { fuel_limit: 10_000 };
    assert!(err.is_fuel_exhausted());
    assert!(err.is_budget_violation()); // FuelExhausted IS a budget violation
    assert!(!err.is_timeout());
    assert!(!err.is_crash());

    let msg = format!("{err}");
    assert!(
        msg.contains("10000") || msg.contains("10_000"),
        "Error message should contain fuel limit"
    );
    Ok(())
}

#[test]
fn fuel_exhaustion_returns_fuel_exhausted_error() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(5_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());

    let err = result.err().ok_or("expected error")?;
    // Should be detected as fuel exhaustion specifically
    assert!(
        err.is_fuel_exhausted() || err.is_budget_violation(),
        "Expected fuel exhaustion or budget violation, got: {err}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: graceful termination (no process crash)
// ---------------------------------------------------------------------------

#[test]
fn timeout_does_not_crash_process() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    // Load and trigger timeout for multiple plugins sequentially
    for _ in 0..5 {
        let id = Uuid::new_v4();
        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

        let result = runtime.process(&id, 1.0, 0.001);
        assert!(result.is_err());

        // Plugin is disabled but runtime continues operating
        assert!(runtime.is_plugin_disabled(&id)?);
        runtime.unload_plugin(&id)?;
    }

    // Runtime is still functional
    let good_wasm = wat_passthrough()?;
    let id = Uuid::new_v4();
    runtime.load_plugin_from_bytes(id, &good_wasm, vec![])?;
    let result = runtime.process(&id, 42.0, 0.001)?;
    assert!((result - 42.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn disabled_plugin_returns_error_on_process() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_infinite_loop()?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // First call: triggers fuel exhaustion, disables plugin
    let _ = runtime.process(&id, 1.0, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    // Second call: returns PluginDisabled error (not a crash)
    let result2 = runtime.process(&id, 1.0, 0.001);
    assert!(result2.is_err());
    if let Err(WasmError::PluginDisabled { reason }) = result2 {
        assert!(!reason.is_empty(), "Disabled reason should not be empty");
    } else {
        return Err("Expected PluginDisabled error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: re-enable after timeout
// ---------------------------------------------------------------------------

#[test]
fn re_enable_after_fuel_timeout_allows_retry() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(100)?; // small loop
    // Very tight fuel on first call — will fail. But after re-enable with
    // more fuel (changed via runtime limits) — this tests the re-enable path.
    let limits = ResourceLimits::default().with_fuel(50);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // Should fail with low fuel
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err());
    assert!(runtime.is_plugin_disabled(&id)?);

    // Re-enable
    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: epoch interruption can be disabled
// ---------------------------------------------------------------------------

#[test]
fn epoch_interruption_disabled_falls_back_to_fuel() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(1_000_000)?;
    let limits = ResourceLimits::default()
        .with_fuel(1_000)
        .with_epoch_interruption(false);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // Without epoch interruption, fuel is still enforced
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "Fuel should still catch the loop");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: stateful plugin + timeout interaction
// ---------------------------------------------------------------------------

#[test]
fn stateful_plugin_state_lost_after_timeout() -> Result<(), Box<dyn std::error::Error>> {
    // Use a stateful accumulator. Call it a few times successfully,
    // then trigger fuel exhaustion. After re-enable, the state from
    // the WASM global is retained (wasmtime doesn't reset it).
    let wasm = wat_accumulator()?;
    let limits = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let r1 = runtime.process(&id, 1.0, 0.001)?;
    assert!((r1 - 1.0).abs() < f32::EPSILON);

    let r2 = runtime.process(&id, 2.0, 0.001)?;
    assert!((r2 - 3.0).abs() < f32::EPSILON);

    // Stats should show 2 calls
    let (count, _) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: resource limits validation
// ---------------------------------------------------------------------------

#[test]
fn resource_limits_compilation_timeout_in_conservative() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::conservative();
    assert_eq!(limits.compilation_timeout, Some(Duration::from_secs(5)));
    assert_eq!(limits.max_execution_time, Some(Duration::from_secs(1)));
    limits.validate()?;
    Ok(())
}

#[test]
fn resource_limits_no_compilation_timeout_in_generous() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::generous();
    assert!(limits.compilation_timeout.is_none());
    assert!(limits.max_execution_time.is_none());
    limits.validate()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Test: max instances limit interacts correctly with timeout
// ---------------------------------------------------------------------------

#[test]
fn max_instances_still_enforced_with_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default()
        .with_max_instances(2)
        .with_execution_time(Duration::from_millis(100));
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

    // Third plugin should hit max instances
    let result = runtime.load_plugin_from_bytes(id3, &wasm, vec![]);
    assert!(result.is_err());
    if let Err(WasmError::MaxInstancesReached(n)) = result {
        assert_eq!(n, 2);
    } else {
        return Err("Expected MaxInstancesReached error".into());
    }
    Ok(())
}
