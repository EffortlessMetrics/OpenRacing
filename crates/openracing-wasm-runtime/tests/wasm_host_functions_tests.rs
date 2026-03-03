//! Tests for WASM host function bindings.
//!
//! Covers each host function binding (logging, capability checking, telemetry,
//! timestamps), error handling for invalid arguments, and versioning / ABI
//! consistency.

use openracing_plugin_abi::{
    HOST_MODULE, TelemetryFrame, capability_str, host_function, log_level, return_code,
    wasm_export, wasm_optional_export,
};
use openracing_wasm_runtime::WasmRuntime;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// WAT helper modules
// ---------------------------------------------------------------------------

/// Module that calls get_timestamp_us and returns the value as f32.
fn wat_timestamp() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_timestamp_us" (func $ts (result i64)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (f32.convert_i64_s (call $ts))))"#,
    )?)
}

/// Module that calls log_info with a message stored in memory.
fn wat_log_info() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_info" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "test message")
            (func (export "process") (param f32 f32) (result f32)
                (call $log (i32.const 0) (i32.const 12))
                local.get 0))"#,
    )?)
}

/// Module that calls log_debug.
fn wat_log_debug() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_debug" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "debug msg")
            (func (export "process") (param f32 f32) (result f32)
                (call $log (i32.const 0) (i32.const 9))
                local.get 0))"#,
    )?)
}

/// Module that calls log_warn.
fn wat_log_warn() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_warn" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "warn msg")
            (func (export "process") (param f32 f32) (result f32)
                (call $log (i32.const 0) (i32.const 8))
                local.get 0))"#,
    )?)
}

/// Module that calls log_error.
fn wat_log_error() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_error" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "error msg")
            (func (export "process") (param f32 f32) (result f32)
                (call $log (i32.const 0) (i32.const 9))
                local.get 0))"#,
    )?)
}

/// Module that calls plugin_log with a specific level.
fn wat_plugin_log() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "plugin_log" (func $log (param i32 i32 i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "plugin log msg")
            (func (export "process") (param f32 f32) (result f32)
                ;; log at INFO level (2)
                (call $log (i32.const 2) (i32.const 0) (i32.const 14))
                local.get 0))"#,
    )?)
}

/// Module that calls log_info with invalid (negative) pointer.
fn wat_log_invalid_ptr() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_info" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (call $log (i32.const -1) (i32.const 5))
                local.get 0))"#,
    )?)
}

/// Module that calls log_info with out-of-bounds length.
fn wat_log_oob_len() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "log_info" (func $log (param i32 i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; pointer at end of memory with large length
                (call $log (i32.const 65530) (i32.const 100))
                local.get 0))"#,
    )?)
}

/// Module that calls get_telemetry and reads back a specific field.
fn wat_telemetry_read() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_telemetry" (func $gt (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                ;; write telemetry at offset 256
                (call $gt (i32.const 256) (i32.const 32))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls get_telemetry with invalid arguments.
fn wat_telemetry_invalid() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(wat::parse_str(
        r#"(module
            (import "env" "get_telemetry" (func $gt (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                (call $gt (i32.const -1) (i32.const 32))
                f32.convert_i32_s))"#,
    )?)
}

/// Module that calls check_capability for read_telemetry.
fn wat_check_capability() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_get_timestamp_returns_positive_value() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_timestamp()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // Timestamp should be >= 0 (microseconds since plugin start)
    assert!(
        result >= 0.0,
        "Timestamp should be non-negative, got {result}"
    );
    Ok(())
}

#[test]
fn test_timestamp_increases_between_calls() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_timestamp()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let t1 = runtime.process(&id, 0.0, 0.001)?;
    let t2 = runtime.process(&id, 0.0, 0.001)?;
    assert!(
        t2 >= t1,
        "Second timestamp ({t2}) should be >= first ({t1})"
    );
    Ok(())
}

#[test]
fn test_log_info_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_log_info()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 7.0, 0.001)?;
    // log functions are fire-and-forget; process should return the passthrough value
    assert!((result - 7.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_log_debug_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_log_debug()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 3.0, 0.001)?;
    assert!((result - 3.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_log_warn_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_log_warn()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 5.0, 0.001)?;
    assert!((result - 5.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_log_error_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_log_error()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 9.0, 0.001)?;
    assert!((result - 9.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_plugin_log_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_plugin_log()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 11.0, 0.001)?;
    assert!((result - 11.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_log_with_invalid_pointer_does_not_crash() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_log_invalid_ptr()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    // Logging with invalid pointer should be silently ignored, not crash
    let result = runtime.process(&id, 4.0, 0.001)?;
    assert!((result - 4.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_log_with_oob_length_does_not_crash() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_log_oob_len()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 6.0, 0.001)?;
    assert!((result - 6.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_get_telemetry_returns_success_with_capability() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_telemetry_read()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // SUCCESS = 0
    assert!((result - 0.0_f32).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_get_telemetry_with_invalid_ptr_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_telemetry_invalid()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // INVALID_ARG = -2
    assert!(
        result < 0.0,
        "Invalid telemetry pointer should return error code"
    );
    Ok(())
}

#[test]
fn test_host_function_names_are_consistent() {
    // Verify the ABI constants match expected values for versioning stability
    assert_eq!(host_function::LOG_DEBUG, "log_debug");
    assert_eq!(host_function::LOG_INFO, "log_info");
    assert_eq!(host_function::LOG_WARN, "log_warn");
    assert_eq!(host_function::LOG_ERROR, "log_error");
    assert_eq!(host_function::PLUGIN_LOG, "plugin_log");
    assert_eq!(host_function::CHECK_CAPABILITY, "check_capability");
    assert_eq!(host_function::GET_TELEMETRY, "get_telemetry");
    assert_eq!(host_function::GET_TIMESTAMP_US, "get_timestamp_us");
}

#[test]
fn test_return_code_values_are_stable() {
    assert_eq!(return_code::SUCCESS, 0);
    assert_eq!(return_code::ERROR, -1);
    assert_eq!(return_code::INVALID_ARG, -2);
    assert_eq!(return_code::PERMISSION_DENIED, -3);
    assert_eq!(return_code::BUFFER_TOO_SMALL, -4);
    assert_eq!(return_code::NOT_INITIALIZED, -5);
}

#[test]
fn test_capability_string_values_are_stable() {
    assert_eq!(capability_str::READ_TELEMETRY, "read_telemetry");
    assert_eq!(capability_str::MODIFY_TELEMETRY, "modify_telemetry");
    assert_eq!(capability_str::CONTROL_LEDS, "control_leds");
    assert_eq!(capability_str::PROCESS_DSP, "process_dsp");
}

#[test]
fn test_host_module_constant() {
    assert_eq!(HOST_MODULE, "env");
}

#[test]
fn test_wasm_export_names_are_stable() {
    assert_eq!(wasm_export::PROCESS, "process");
    assert_eq!(wasm_export::MEMORY, "memory");
    assert_eq!(wasm_optional_export::INIT, "init");
    assert_eq!(wasm_optional_export::SHUTDOWN, "shutdown");
    assert_eq!(wasm_optional_export::GET_INFO, "get_info");
}

#[test]
fn test_log_level_ordering() {
    const _: () = assert!(log_level::ERROR < log_level::WARN);
    const _: () = assert!(log_level::WARN < log_level::INFO);
    const _: () = assert!(log_level::INFO < log_level::DEBUG);
    const _: () = assert!(log_level::DEBUG < log_level::TRACE);
}

#[test]
fn test_telemetry_frame_default_roundtrip() {
    let frame = TelemetryFrame::default();
    let bytes = frame.to_bytes();
    assert_eq!(bytes.len(), 32);
    // Default timestamp should be 0
    let ts = u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    assert_eq!(ts, 0);
}

#[test]
fn test_check_capability_host_function_granted() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_capability()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec!["read_telemetry".to_string()])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // Returns 1 when granted
    assert!((result - 1.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn test_check_capability_host_function_denied() -> Result<(), Box<dyn std::error::Error>> {
    let wasm = wat_check_capability()?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
    let result = runtime.process(&id, 0.0, 0.001)?;
    // PERMISSION_DENIED = -3
    assert!((result - (-3.0_f32)).abs() < f32::EPSILON);
    Ok(())
}
