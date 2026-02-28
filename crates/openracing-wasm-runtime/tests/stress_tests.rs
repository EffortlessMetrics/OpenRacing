//! Stress tests for WASM runtime.

use openracing_wasm_runtime::{ResourceLimits, WasmRuntime};
use uuid::Uuid;

fn create_process_wasm() -> Result<Vec<u8>, wat::Error> {
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
}

#[test]
fn test_long_running_plugin_stability() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_process_wasm()?;

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // Process 10,000 times
    for i in 0..10_000 {
        let input = i as f32 * 0.001;
        let result = runtime.process(&plugin_id, input, 0.001)?;
        let expected = input + 0.001;
        assert!(
            (result - expected).abs() < 0.01,
            "Mismatch at iteration {}: expected {}, got {}",
            i,
            expected,
            result
        );
    }

    // Check statistics
    let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
    assert_eq!(count, 10_000);

    runtime.unload_plugin(&plugin_id)?;

    Ok(())
}

#[test]
fn test_concurrent_plugin_operations() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let wasm = create_process_wasm()?;

    // Load multiple plugins
    let plugin_ids: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();

    for id in &plugin_ids {
        runtime.load_plugin_from_bytes(*id, &wasm, vec![])?;
    }

    // Process on each plugin
    for (i, id) in plugin_ids.iter().enumerate() {
        let result = runtime.process(id, i as f32, 1.0)?;
        let expected = i as f32 + 1.0;
        assert!((result - expected).abs() < f32::EPSILON);
    }

    // Unload all
    for id in &plugin_ids {
        runtime.unload_plugin(id)?;
    }

    assert_eq!(runtime.instance_count(), 0);

    Ok(())
}

#[test]
fn test_plugin_reload_stress() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_process_wasm()?;

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // Reload multiple times
    for i in 0..100 {
        runtime.reload_plugin(&plugin_id, &wasm, vec![])?;

        // Verify it still works
        let result = runtime.process(&plugin_id, i as f32, 0.0)?;
        assert!((result - i as f32).abs() < f32::EPSILON);
    }

    runtime.unload_plugin(&plugin_id)?;

    Ok(())
}

#[test]
fn test_memory_stability() -> Result<(), Box<dyn std::error::Error>> {
    let limits = ResourceLimits::default().with_memory(1024 * 1024);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let plugin_id = Uuid::new_v4();
    let wasm = create_process_wasm()?;

    runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

    // Many iterations
    for _ in 0..1_000 {
        let _ = runtime.process(&plugin_id, 1.0, 1.0);
    }

    runtime.unload_plugin(&plugin_id)?;

    Ok(())
}

#[test]
fn test_plugin_lifecycle_stress() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let wasm = create_process_wasm()?;

    // Load/unload cycle many times
    for i in 0..50 {
        let plugin_id = Uuid::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm, vec![])?;

        // Do some processing
        for j in 0..10 {
            let _ = runtime.process(&plugin_id, i as f32, j as f32);
        }

        runtime.unload_plugin(&plugin_id)?;
    }

    assert_eq!(runtime.instance_count(), 0);

    Ok(())
}
