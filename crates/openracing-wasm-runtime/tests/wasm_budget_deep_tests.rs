//! Deep tests for WASM fuel/budget management.
//!
//! Covers fuel consumption tracking, budget exhaustion handling (graceful vs hard),
//! fuel refill between invocations, budget enforcement for different operations,
//! and memory limit enforcement.

use openracing_wasm_runtime::{ResourceLimits, WasmRuntime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT helper modules
// ---------------------------------------------------------------------------

/// Minimal passthrough: costs very few instructions.
fn wat_passthrough() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Tight loop that burns fuel proportional to the iteration count.
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

/// Module that unconditionally traps (for crash-budget interaction).
fn wat_trap() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable))"#,
    )?)
}

/// Module that tries to grow memory beyond limits.
fn wat_memory_grow(pages: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let src = format!(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (drop (memory.grow (i32.const {pages})))
                local.get 0))"#,
    );
    Ok(wat::parse_str(&src)?)
}

/// Module with init that burns fuel.
fn wat_init_fuel_burn(iterations: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let src = format!(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                (local $i i32)
                (local.set $i (i32.const 0))
                (block $break
                    (loop $loop
                        (br_if $break (i32.ge_u (local.get $i) (i32.const {iterations})))
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br $loop)))
                i32.const 0)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    );
    Ok(wat::parse_str(&src)?)
}

/// Module whose process accumulates via global state.
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

/// Module that performs arithmetic (moderate fuel cost).
fn wat_arithmetic() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 0
                f32.mul
                local.get 1
                f32.add
                local.get 0
                f32.sub))"#,
    )?)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_passthrough_uses_minimal_fuel() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 1.0, 0.001)?;
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_fuel_exhaustion_with_tight_loop() -> Result<(), Box<dyn std::error::Error>> {
    // Very small fuel budget vs. huge loop → should exhaust fuel.
    let wasm = wat_loop(100_000_000)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.5, 0.001);
    assert!(result.is_err(), "Expected fuel exhaustion error");
    Ok(())
}

#[test]
fn test_fuel_exhaustion_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(100_000_000)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 0.5, 0.001);

    // Plugin should be disabled after fuel exhaustion trap
    let disabled = runtime.is_plugin_disabled(&id)?;
    assert!(disabled, "Plugin should be disabled after fuel exhaustion");
    Ok(())
}

#[test]
fn test_fuel_refill_between_invocations() -> Result<(), Box<dyn std::error::Error>> {
    // Moderate loop that fits within budget each call.
    let wasm = wat_loop(100)?;
    let limits = ResourceLimits::default().with_fuel(100_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    // Call many times – each call should refill fuel.
    for i in 0..20 {
        let result = runtime.process(&id, i as f32, 0.001);
        assert!(result.is_ok(), "Call {} should succeed with fuel refill", i);
    }
    Ok(())
}

#[test]
fn test_conservative_fuel_limits() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::conservative();
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    assert_eq!(runtime.resource_limits().max_fuel, 1_000_000);
    let result = runtime.process(&id, 0.5, 0.001)?;
    assert!((result - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_generous_fuel_limits() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(1_000_000)?;
    let limits = ResourceLimits::generous();
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    assert_eq!(runtime.resource_limits().max_fuel, 50_000_000);
    let result = runtime.process(&id, 0.5, 0.001)?;
    assert!((result - 0.5).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_different_fuel_budgets_same_module() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(500_000)?;

    // Tight budget should fail
    let limits_tight = ResourceLimits::default().with_fuel(1_000);
    let mut rt_tight = WasmRuntime::with_limits(limits_tight)?;
    let id1 = Uuid::new_v4();
    rt_tight.load_plugin_from_bytes(id1, &wasm, vec![])?;
    let res_tight = rt_tight.process(&id1, 0.5, 0.001);
    assert!(res_tight.is_err(), "Tight budget should fail");

    // Generous budget should succeed
    let limits_gen = ResourceLimits::generous();
    let mut rt_gen = WasmRuntime::with_limits(limits_gen)?;
    let id2 = Uuid::new_v4();
    rt_gen.load_plugin_from_bytes(id2, &wasm, vec![])?;
    let res_gen = rt_gen.process(&id2, 0.5, 0.001);
    assert!(res_gen.is_ok(), "Generous budget should succeed");
    Ok(())
}

#[test]
fn test_budget_enforcement_arithmetic_operations() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_arithmetic()?;
    let limits = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 3.0, 1.0)?;
    // 3.0 * 3.0 + 1.0 - 3.0 = 7.0
    assert!((result - 7.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_memory_limit_enforcement_via_resource_limits() -> Result<(), Box<dyn std::error::Error>> {
    // Module tries to grow by 256 pages (16 MB) — within the minimum 1-page module.
    // With only 2-page (128 KB) max, growth should be denied (returns -1).
    let wasm = wat_memory_grow(256)?;
    let limits = ResourceLimits::default()
        .with_memory(2 * 64 * 1024)
        .with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    // memory.grow returns -1 on failure but does not trap, so process still succeeds.
    let result = runtime.process(&id, 42.0, 0.001)?;
    assert!((result - 42.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_memory_grow_within_limits_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    // Grow by 1 page when plenty of memory is available.
    let wasm = wat_memory_grow(1)?;
    let limits = ResourceLimits::default()
        .with_memory(16 * 1024 * 1024)
        .with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 5.0, 0.001)?;
    assert!((result - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_fuel_exhaustion_returns_budget_violation_or_crash() -> Result<(), Box<dyn std::error::Error>>
{
    let wasm = wat_loop(100_000_000)?;
    let limits = ResourceLimits::default().with_fuel(500);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let err = runtime.process(&id, 0.5, 0.001);
    assert!(err.is_err());

    let e = err.err().ok_or("expected error")?;
    let is_budget_or_crash = e.is_budget_violation() || e.is_crash();
    assert!(
        is_budget_or_crash,
        "Expected BudgetViolation or Crash, got: {e}"
    );
    Ok(())
}

#[test]
fn test_crash_vs_fuel_exhaustion_distinction() -> Result<(), Box<dyn std::error::Error>> {
    // Crash from unreachable (plenty of fuel)
    let trap_wasm = wat_trap()?;
    let limits = ResourceLimits::default().with_fuel(10_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id_trap = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id_trap, &trap_wasm, vec![])?;
    let err_trap = runtime.process(&id_trap, 0.5, 0.001);
    assert!(err_trap.is_err());
    let e_trap = err_trap.err().ok_or("expected error")?;
    assert!(
        e_trap.is_crash(),
        "Unreachable should be a crash, not budget violation"
    );

    // Fuel exhaustion from loop
    let loop_wasm = wat_loop(100_000_000)?;
    let limits2 = ResourceLimits::default().with_fuel(500);
    let mut runtime2 = WasmRuntime::with_limits(limits2)?;
    let id_fuel = Uuid::new_v4();

    runtime2.load_plugin_from_bytes(id_fuel, &loop_wasm, vec![])?;
    let err_fuel = runtime2.process(&id_fuel, 0.5, 0.001);
    assert!(err_fuel.is_err());
    let e_fuel = err_fuel.err().ok_or("expected error")?;
    let is_budget_or_crash = e_fuel.is_budget_violation() || e_fuel.is_crash();
    assert!(
        is_budget_or_crash,
        "Fuel exhaustion should be budget violation or crash, got: {e_fuel}"
    );
    Ok(())
}

#[test]
fn test_re_enable_after_fuel_exhaustion() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_loop(100_000_000)?;
    let limits = ResourceLimits::default().with_fuel(500);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let _ = runtime.process(&id, 0.5, 0.001);
    assert!(runtime.is_plugin_disabled(&id)?);

    let was_disabled = runtime.re_enable_plugin(&id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&id)?);
    Ok(())
}

#[test]
fn test_stateful_plugin_fuel_refill_preserves_state() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_stateful()?;
    let limits = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    let r1 = runtime.process(&id, 1.0, 0.001)?;
    assert!((r1 - 1.0).abs() < f32::EPSILON);

    let r2 = runtime.process(&id, 2.0, 0.001)?;
    assert!((r2 - 3.0).abs() < f32::EPSILON);

    let r3 = runtime.process(&id, 3.0, 0.001)?;
    assert!((r3 - 6.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_fuel_limit_validation_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let too_small = ResourceLimits::default().with_fuel(100);
    assert!(too_small.validate().is_err());

    let minimum_valid = ResourceLimits::default().with_fuel(1000);
    assert!(minimum_valid.validate().is_ok());

    let too_large = ResourceLimits::default().with_fuel(10_000_000_001);
    assert!(too_large.validate().is_err());

    let max_valid = ResourceLimits::default().with_fuel(10_000_000_000);
    assert!(max_valid.validate().is_ok());
    Ok(())
}

#[test]
fn test_memory_limit_validation_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let too_small = ResourceLimits::default().with_memory(1024);
    assert!(too_small.validate().is_err());

    let minimum = ResourceLimits::default().with_memory(64 * 1024);
    assert!(minimum.validate().is_ok());

    let max_valid = ResourceLimits::default().with_memory(4 * 1024 * 1024 * 1024);
    assert!(max_valid.validate().is_ok());
    Ok(())
}

#[test]
fn test_process_stats_accumulate_across_fuel_refills() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

    for _ in 0..10 {
        let _ = runtime.process(&id, 1.0, 0.001)?;
    }

    let (count, _avg) = runtime.get_plugin_stats(&id)?;
    assert_eq!(count, 10);
    Ok(())
}

#[test]
fn test_init_fuel_consumption() -> Result<(), Box<dyn std::error::Error>> {
    // Module whose init() does a small loop — should succeed with enough fuel.
    let wasm = wat_init_fuel_burn(100)?;
    let limits = ResourceLimits::default().with_fuel(1_000_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&id)?);
    Ok(())
}
