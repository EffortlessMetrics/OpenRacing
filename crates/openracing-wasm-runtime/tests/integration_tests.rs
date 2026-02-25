//! Integration tests for WASM runtime.

use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

fn create_minimal_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
        "#,
    )
    .expect("Failed to parse minimal WAT")
}

fn create_adding_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add
            )
        )
        "#,
    )
    .expect("Failed to parse adding WAT")
}

fn create_trap_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable
            )
        )
        "#,
    )
    .expect("Failed to parse trap WAT")
}

fn create_init_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (memory (export "memory") 1)
            (global $initialized (mut i32) (i32.const 0))
            (func (export "init") (result i32)
                i32.const 1
                global.set $initialized
                i32.const 0
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
            (func (export "shutdown")
                i32.const 0
                global.set $initialized
            )
        )
        "#,
    )
    .expect("Failed to parse init WAT")
}

#[test]
fn test_plugin_lifecycle() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_minimal_wasm();

    // Load plugin
    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;
    assert!(runtime.has_plugin(&plugin_id));
    assert!(runtime.is_plugin_initialized(&plugin_id)?);

    // Process
    let result = runtime.process(&plugin_id, 0.5, 0.001)?;
    assert!((result - 0.5).abs() < f32::EPSILON);

    // Unload
    runtime.unload_plugin(&plugin_id)?;
    assert!(!runtime.has_plugin(&plugin_id));

    Ok(())
}

#[test]
fn test_multiple_plugins() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;

    let plugin1_id = Uuid::new_v4();
    let plugin2_id = Uuid::new_v4();

    let wasm1 = create_minimal_wasm();
    let wasm2 = create_adding_wasm();

    runtime.load_plugin_from_bytes(plugin1_id, &wasm1, vec![])?;
    runtime.load_plugin_from_bytes(plugin2_id, &wasm2, vec![])?;

    assert_eq!(runtime.instance_count(), 2);

    let result1 = runtime.process(&plugin1_id, 1.0, 0.0)?;
    assert!((result1 - 1.0).abs() < f32::EPSILON);

    let result2 = runtime.process(&plugin2_id, 1.0, 2.0)?;
    assert!((result2 - 3.0).abs() < f32::EPSILON);

    runtime.unload_plugin(&plugin1_id)?;
    assert_eq!(runtime.instance_count(), 1);

    runtime.unload_plugin(&plugin2_id)?;
    assert_eq!(runtime.instance_count(), 0);

    Ok(())
}

#[test]
fn test_plugin_trap_handling() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_trap_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // First call should trap
    let result = runtime.process(&plugin_id, 0.5, 0.001);
    assert!(result.is_err());
    assert!(result.unwrap_err().is_crash());

    // Plugin should be disabled
    assert!(runtime.is_plugin_disabled(&plugin_id)?);

    // Subsequent calls should return disabled error
    let result2 = runtime.process(&plugin_id, 0.5, 0.001);
    assert!(result2.is_err());

    Ok(())
}

#[test]
fn test_plugin_re_enable() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_trap_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // Trigger trap
    let _ = runtime.process(&plugin_id, 0.5, 0.001);
    assert!(runtime.is_plugin_disabled(&plugin_id)?);

    // Re-enable
    let was_disabled = runtime.re_enable_plugin(&plugin_id)?;
    assert!(was_disabled);
    assert!(!runtime.is_plugin_disabled(&plugin_id)?);

    // Re-enabling again returns false
    let was_disabled_again = runtime.re_enable_plugin(&plugin_id)?;
    assert!(!was_disabled_again);

    Ok(())
}

#[test]
fn test_plugin_statistics() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_minimal_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    let (count, avg_time) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count, 0);
    assert_eq!(avg_time, 0.0);

    for i in 0..10 {
        runtime.process(&plugin_id, i as f32, 0.001)?;
    }

    let (count, avg_time) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count, 10);
    assert!(avg_time > 0.0);

    Ok(())
}

#[test]
fn test_plugin_with_init() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_init_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&plugin_id)?);

    let result = runtime.process(&plugin_id, 5.0, 0.0)?;
    assert!((result - 5.0).abs() < f32::EPSILON);

    runtime.unload_plugin(&plugin_id)?;

    Ok(())
}

#[test]
fn test_max_instances_limit() -> Result<(), WasmError> {
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let wasm = create_minimal_wasm();

    // Load first two plugins
    runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;
    runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;

    // Third should fail
    let result = runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn test_hot_reload() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let wasm1 = create_minimal_wasm();
    let wasm2 = create_adding_wasm();

    // Load initial plugin
    runtime.load_plugin_from_bytes(plugin_id, &wasm1, vec![])?;

    let result1 = runtime.process(&plugin_id, 5.0, 0.0)?;
    assert!((result1 - 5.0).abs() < f32::EPSILON);

    // Hot reload
    runtime.reload_plugin(&plugin_id, &wasm2, vec![])?;

    // New behavior
    let result2 = runtime.process(&plugin_id, 5.0, 3.0)?;
    assert!((result2 - 8.0).abs() < f32::EPSILON);

    Ok(())
}

#[test]
fn test_hot_reload_preserves_statistics() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let wasm = create_minimal_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // Process several times
    for _ in 0..10 {
        runtime.process(&plugin_id, 1.0, 0.001)?;
    }

    let (count_before, _) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count_before, 10);

    // Hot reload
    runtime.reload_plugin(&plugin_id, &wasm, vec![])?;

    // Statistics should be preserved
    let (count_after, _) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count_after, 10);

    Ok(())
}

#[test]
fn test_capabilities() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_minimal_wasm();

    runtime.load_plugin_from_bytes(
        plugin_id,
        &wasm,
        vec!["read_telemetry".to_string(), "control_leds".to_string()],
    )?;

    assert!(runtime.has_plugin(&plugin_id));

    Ok(())
}

#[test]
fn test_resource_limits_validation() {
    let limits = ResourceLimits::conservative();
    assert!(limits.validate().is_ok());

    let invalid_limits = ResourceLimits::default().with_memory(1024);
    assert!(invalid_limits.validate().is_err());
}

#[test]
fn test_process_returns_identity() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_minimal_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // Test various inputs
    for input in [0.0, 0.5, 1.0, -1.0, 100.0, -100.0] {
        let result = runtime.process(&plugin_id, input, 0.001)?;
        assert!((result - input).abs() < f32::EPSILON);
    }

    Ok(())
}

#[test]
fn test_process_adds_inputs() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_adding_wasm();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    let result = runtime.process(&plugin_id, 1.5, 2.5)?;
    assert!((result - 4.0).abs() < f32::EPSILON);

    Ok(())
}
