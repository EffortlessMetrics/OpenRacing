//! Unit tests for the WASM runtime.

use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use uuid::Uuid;

#[test]
fn test_runtime_creation() -> Result<(), WasmError> {
    let runtime = WasmRuntime::new()?;
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn test_runtime_with_limits() -> Result<(), WasmError> {
    let limits = ResourceLimits::conservative();
    let runtime = WasmRuntime::with_limits(limits)?;

    assert_eq!(runtime.resource_limits().max_memory_bytes, 4 * 1024 * 1024);
    assert_eq!(runtime.resource_limits().max_fuel, 1_000_000);
    assert_eq!(runtime.resource_limits().max_instances, 8);
    Ok(())
}

#[test]
fn test_resource_limits_validation() {
    let limits = ResourceLimits::default();
    assert!(limits.validate().is_ok());

    let invalid_limits = ResourceLimits::default().with_memory(1024);
    assert!(invalid_limits.validate().is_err());
}

#[test]
fn test_plugin_not_found() -> Result<(), WasmError> {
    let runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    assert!(!runtime.has_plugin(&plugin_id));
    assert!(runtime.is_plugin_initialized(&plugin_id).is_err());
    assert!(runtime.is_plugin_disabled(&plugin_id).is_err());
    assert!(runtime.get_plugin_stats(&plugin_id).is_err());
    Ok(())
}

#[test]
fn test_unload_nonexistent_plugin() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let result = runtime.unload_plugin(&plugin_id);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::PluginNotFound(_))));
    Ok(())
}

#[test]
fn test_process_nonexistent_plugin() -> Result<(), WasmError> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let result = runtime.process(&plugin_id, 0.5, 0.001);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::PluginNotFound(_))));
    Ok(())
}

#[test]
fn test_load_simple_plugin() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    assert!(runtime.has_plugin(&plugin_id));
    assert!(runtime.is_plugin_initialized(&plugin_id)?);
    assert!(!runtime.is_plugin_disabled(&plugin_id)?);
    assert_eq!(runtime.instance_count(), 1);

    runtime.unload_plugin(&plugin_id)?;
    assert!(!runtime.has_plugin(&plugin_id));
    assert_eq!(runtime.instance_count(), 0);
    Ok(())
}

#[test]
fn test_load_plugin_missing_memory() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::MissingExport(_))));
    Ok(())
}

#[test]
fn test_load_plugin_missing_process() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::MissingExport(_))));
    Ok(())
}

#[test]
fn test_max_instances_limit() -> Result<(), WasmError> {
    let limits = ResourceLimits::default().with_max_instances(2);
    let mut runtime = WasmRuntime::with_limits(limits)?;

    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
    "#;
    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;
    runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![])?;

    let result = runtime.load_plugin_from_bytes(Uuid::new_v4(), &wasm, vec![]);
    assert!(result.is_err());
    assert!(matches!(result, Err(WasmError::MaxInstancesReached(_))));
    Ok(())
}

#[test]
fn test_plugin_process_add() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    let result = runtime.process(&plugin_id, 1.0, 2.0)?;
    assert!((result - 3.0).abs() < 0.001);

    let result = runtime.process(&plugin_id, 0.5, 0.25)?;
    assert!((result - 0.75).abs() < 0.001);
    Ok(())
}

#[test]
fn test_plugin_process_multiply() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.mul
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    let result = runtime.process(&plugin_id, 3.0, 4.0)?;
    assert!((result - 12.0).abs() < 0.001);
    Ok(())
}

#[test]
fn test_plugin_stats() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count, 0);

    runtime.process(&plugin_id, 0.0, 0.0)?;
    runtime.process(&plugin_id, 0.0, 0.0)?;
    runtime.process(&plugin_id, 0.0, 0.0)?;

    let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count, 3);
    Ok(())
}

#[test]
fn test_plugin_with_init() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const 0
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;
    assert!(runtime.is_plugin_initialized(&plugin_id)?);
    Ok(())
}

#[test]
fn test_plugin_init_failure() -> Result<(), WasmError> {
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const -1
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )
    "#;

    let wasm = wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![]);
    assert!(result.is_err());
    Ok(())
}
