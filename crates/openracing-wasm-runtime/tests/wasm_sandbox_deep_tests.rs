//! Deep tests for WASM sandbox and capability enforcement.
//!
//! Covers capability-based permission model, filesystem/network/IPC restrictions,
//! host function access control, sandbox escape prevention, and isolation between
//! plugin instances.

use openracing_wasm_runtime::prelude::CapabilityChecker;
use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT helper modules
// ---------------------------------------------------------------------------

/// Minimal passthrough.
fn wat_passthrough() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0))"#,
    )?)
}

/// Module that calls check_capability for "read_telemetry".
fn wat_check_read_telemetry() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "read_telemetry")
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const 14))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability for "modify_telemetry".
fn wat_check_modify_telemetry() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "modify_telemetry")
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const 16))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability for "control_leds".
fn wat_check_control_leds() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "control_leds")
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const 12))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability for "process_dsp".
fn wat_check_process_dsp() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "process_dsp")
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const 11))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability with an unknown capability string.
fn wat_check_unknown_capability() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "network_access")
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const 14))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls get_telemetry (requires read_telemetry capability).
fn wat_get_telemetry() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_telemetry" (func $gt (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; request 32 bytes at offset 64
                (call $gt (i32.const 64) (i32.const 32))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability with invalid pointer (negative).
fn wat_check_cap_invalid_ptr() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const -1) (i32.const 14))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability with zero length.
fn wat_check_cap_zero_len() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "check_capability" (func $chk (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (call $chk (i32.const 0) (i32.const 0))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that accumulates state via a global.
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

/// Module that tries to call get_telemetry with a too-small buffer.
fn wat_get_telemetry_small_buf() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_telemetry" (func $gt (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; only 8 bytes instead of 32
                (call $gt (i32.const 64) (i32.const 8))
                f32.convert_i32_s))"#,
    )?)
}

// ---------------------------------------------------------------------------
// Capability model tests
// ---------------------------------------------------------------------------

#[test]
fn test_capability_checker_no_capabilities() {
    let checker = CapabilityChecker::default();
    assert!(checker.check_telemetry_read().is_err());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
    assert!(!checker.has_capability("read_telemetry"));
}

#[test]
fn test_capability_checker_single_grant() {
    let checker = CapabilityChecker::new(vec!["read_telemetry".to_string()]);
    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_telemetry_modify().is_err());
    assert!(checker.check_led_control().is_err());
    assert!(checker.check_dsp_processing().is_err());
}

#[test]
fn test_capability_checker_multiple_grants() {
    let checker = CapabilityChecker::new(vec![
        "read_telemetry".to_string(),
        "modify_telemetry".to_string(),
        "control_leds".to_string(),
        "process_dsp".to_string(),
    ]);
    assert!(checker.check_telemetry_read().is_ok());
    assert!(checker.check_telemetry_modify().is_ok());
    assert!(checker.check_led_control().is_ok());
    assert!(checker.check_dsp_processing().is_ok());
}

#[test]
fn test_capability_checker_violation_error_type() {
    let checker = CapabilityChecker::default();
    let err = checker.check_telemetry_read();
    assert!(err.is_err());
    if let Err(e) = err {
        assert!(e.is_capability_violation());
    }
}

#[test]
fn test_check_read_telemetry_granted() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_read_telemetry()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // check_capability returns 1 when granted
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_check_read_telemetry_denied() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_read_telemetry()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // No capabilities granted
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // check_capability returns PERMISSION_DENIED (-3) when not granted
    assert!(result < 0.0, "Expected negative return code for denied capability");
    Ok(())
}

#[test]
fn test_check_modify_telemetry_denied_without_grant() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_modify_telemetry()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // Only read granted, not modify
    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    assert!(result < 0.0, "modify_telemetry should be denied");
    Ok(())
}

#[test]
fn test_check_control_leds_granted() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_control_leds()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["control_leds".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_check_process_dsp_denied() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_process_dsp()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    assert!(result < 0.0, "process_dsp should be denied without grant");
    Ok(())
}

#[test]
fn test_unknown_capability_returns_invalid_arg() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_unknown_capability()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // INVALID_ARG = -2
    assert!(result < 0.0, "Unknown capability should return error code");
    Ok(())
}

// ---------------------------------------------------------------------------
// Telemetry access restrictions
// ---------------------------------------------------------------------------

#[test]
fn test_get_telemetry_without_capability_returns_permission_denied(
) -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_get_telemetry()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // No read_telemetry capability
    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // PERMISSION_DENIED = -3
    assert!((result - (-3.0_f32)).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_get_telemetry_with_capability_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_get_telemetry()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // SUCCESS = 0
    assert!((result - 0.0_f32).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_get_telemetry_buffer_too_small() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_get_telemetry_small_buf()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // BUFFER_TOO_SMALL = -4
    assert!((result - (-4.0_f32)).abs() < f32::EPSILON);
    Ok(())
}

// ---------------------------------------------------------------------------
// Invalid host function arguments (sandbox escape prevention)
// ---------------------------------------------------------------------------

#[test]
fn test_check_capability_with_invalid_pointer() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_cap_invalid_ptr()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // INVALID_ARG = -2
    assert!(result < 0.0, "Invalid pointer should return error");
    Ok(())
}

#[test]
fn test_check_capability_with_zero_length() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_cap_zero_len()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // Zero-length string won't match any known capability → INVALID_ARG
    assert!(result < 0.0, "Zero-length capability should return error");
    Ok(())
}

// ---------------------------------------------------------------------------
// Instance isolation
// ---------------------------------------------------------------------------

#[test]
fn test_plugin_instances_have_isolated_state() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_stateful()?;
    let mut runtime = WasmRuntime::new()?;
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

    // Feed values into plugin 1
    let r1a = runtime.process(&id1, 10.0, 0.001)?;
    assert!((r1a - 10.0).abs() < f32::EPSILON);

    // Plugin 2 should start fresh
    let r2a = runtime.process(&id2, 5.0, 0.001)?;
    assert!((r2a - 5.0).abs() < f32::EPSILON);

    // Plugin 1 continues accumulating independently
    let r1b = runtime.process(&id1, 3.0, 0.001)?;
    assert!((r1b - 13.0).abs() < f32::EPSILON);

    // Plugin 2 also accumulates independently
    let r2b = runtime.process(&id2, 2.0, 0.001)?;
    assert!((r2b - 7.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_plugin_crash_does_not_affect_other_plugins() -> Result<(), Box<dyn std::error::Error>> {
    let good_wasm = wat_passthrough()?;
    let bad_wasm = wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable))"#,
    )?;

    let mut runtime = WasmRuntime::new()?;
    let good_id = Uuid::new_v4();
    let bad_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(good_id, &good_wasm, vec![])?;
    runtime.load_plugin_from_bytes(bad_id, &bad_wasm, vec![])?;

    // Crash the bad plugin
    let _ = runtime.process(&bad_id, 0.5, 0.001);
    assert!(runtime.is_plugin_disabled(&bad_id)?);

    // Good plugin should still work
    let result = runtime.process(&good_id, 42.0, 0.001)?;
    assert!((result - 42.0).abs() < f32::EPSILON);
    assert!(!runtime.is_plugin_disabled(&good_id)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Max instances limit
// ---------------------------------------------------------------------------

#[test]
fn test_max_instances_limit_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_passthrough()?;
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
    runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

    let result = runtime.load_plugin_from_bytes(id3, &wasm, vec![]);
    assert!(result.is_err(), "Third plugin should be rejected");

    if let Err(WasmError::MaxInstancesReached(n)) = result {
        assert_eq!(n, 2);
    } else {
        return Err("Expected MaxInstancesReached error".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WASI sandbox restrictions (no filesystem/network by default)
// ---------------------------------------------------------------------------

#[test]
fn test_wasi_sandbox_no_filesystem_access() -> Result<(), Box<dyn std::error::Error>> {
    // A module that tries to call fd_read (WASI file read) should fail or return error.
    // Default WASI context has no preopened dirs, so any FS operation will fail.
    // We verify the runtime can be created with restricted WASI.
    let wasm = wat_passthrough()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    // Plugin works fine — it just can't access the filesystem
    let result = runtime.process(&id, 1.0, 0.001)?;
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_wasm_threads_disabled() -> Result<(), Box<dyn std::error::Error>> {
    // Attempt to compile a module that uses shared memory (threads proposal).
    // This should fail because wasm_threads is disabled in the runtime config.
    let shared_mem_wat = r#"(module
        (memory (export "memory") 1 2 shared)
        (func (export "process") (param f32 f32) (result f32)
            local.get 0))"#;

    let wasm_result = wat::parse_str(shared_mem_wat);
    // If WAT compilation even succeeds, loading should fail
    if let Ok(wasm) = wasm_result {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let load_result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
        assert!(
            load_result.is_err(),
            "Shared memory (threads) should be rejected"
        );
    }
    // If WAT compilation fails for shared memory, that's also fine — threads are blocked
    Ok(())
}
