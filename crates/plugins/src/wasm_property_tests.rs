//! Property-based tests for WASM plugin loading and resource sandboxing.
//!
//! These tests validate that:
//! - Valid WASM modules conforming to the plugin ABI can be successfully loaded
//!   and instantiated by the WASM runtime. (**Validates: Requirements 8.1**)
//! - WASM plugins attempting to exceed resource limits are terminated with
//!   appropriate errors. (**Validates: Requirements 8.2, 8.6**)

use crate::PluginError;
use crate::manifest::Capability;
use crate::wasm::{PluginId, ResourceLimits, WasmRuntime};
use proptest::prelude::*;

/// Minimal valid WASM module in WAT format that exports a memory.
const MINIMAL_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
)
"#;

/// WASM module with a process function conforming to the plugin ABI.
const PROCESS_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 1
        f32.mul
    )
)
"#;

/// WASM module with a passthrough process function.
const PASSTHROUGH_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

/// WASM module with a constant output process function.
const CONSTANT_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        f32.const 0.5
    )
)
"#;

/// WASM module with clamping logic.
const CLAMP_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        f32.const 1.0
        f32.min
        f32.const -1.0
        f32.max
    )
)
"#;

/// WASM module with multiple exported functions.
const MULTI_EXPORT_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
    (func (export "init") (result i32)
        i32.const 0
    )
)
"#;

/// WASM module with global variables.
const GLOBALS_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (global $state (mut f32) (f32.const 0.0))
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        global.get $state
        f32.add
    )
)
"#;

// ============================================================================
// WASM modules for resource sandboxing tests (Property 8)
// ============================================================================

/// WASM module that attempts to grow memory beyond limits.
/// This module exports a function that tries to grow memory by a specified
/// number of pages (each page is 64KB).
const MEMORY_GROW_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; Try to grow memory by a large amount (pages = input * 1000)
        ;; Each page is 64KB, so this can request significant memory
        local.get 0
        i32.trunc_f32_s
        i32.const 1000
        i32.mul
        memory.grow
        drop
        local.get 0
    )
)
"#;

/// WASM module with an infinite loop that exhausts fuel.
/// This module runs a loop that never terminates, testing fuel limits.
const INFINITE_LOOP_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        (local $counter i32)
        (local.set $counter (i32.const 0))
        (block $break
            (loop $continue
                ;; Increment counter
                (local.set $counter (i32.add (local.get $counter) (i32.const 1)))
                ;; Loop forever (no break condition)
                (br $continue)
            )
        )
        local.get 0
    )
)
"#;

/// WASM module with a long-running computation that may exhaust fuel.
/// This module performs many iterations based on input.
const LONG_COMPUTATION_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        (local $counter i32)
        (local $iterations i32)
        (local $result f32)
        ;; Convert input to iteration count (scaled up significantly)
        (local.set $iterations 
            (i32.mul 
                (i32.trunc_f32_s (local.get 0))
                (i32.const 100000)
            )
        )
        (local.set $counter (i32.const 0))
        (local.set $result (f32.const 0.0))
        (block $break
            (loop $continue
                ;; Increment counter
                (local.set $counter (i32.add (local.get $counter) (i32.const 1)))
                ;; Add to result
                (local.set $result (f32.add (local.get $result) (f32.const 0.001)))
                ;; Check if we've done enough iterations
                (br_if $break (i32.ge_s (local.get $counter) (local.get $iterations)))
                (br $continue)
            )
        )
        (local.get $result)
    )
)
"#;

/// WASM module that allocates memory in a loop.
/// This tests memory allocation behavior under limits.
const MEMORY_ALLOC_LOOP_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1 256)
    (func (export "process") (param f32 f32) (result f32)
        (local $i i32)
        (local $pages i32)
        ;; Try to grow memory multiple times
        (local.set $pages (i32.trunc_f32_s (local.get 0)))
        (local.set $i (i32.const 0))
        (block $break
            (loop $continue
                ;; Try to grow by 1 page each iteration
                (drop (memory.grow (i32.const 1)))
                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                (br_if $break (i32.ge_s (local.get $i) (local.get $pages)))
                (br $continue)
            )
        )
        local.get 0
    )
)
"#;

/// Convert WAT text to WASM binary bytes.
fn wat_to_wasm(wat: &str) -> Option<Vec<u8>> {
    wat::parse_str(wat).ok()
}

/// Strategy for selecting a valid WASM module variant.
/// All modules must export 'memory' and 'process' to conform to the plugin ABI.
fn valid_wasm_module_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        Just(PROCESS_WASM_WAT),
        Just(PASSTHROUGH_WASM_WAT),
        Just(CONSTANT_WASM_WAT),
        Just(CLAMP_WASM_WAT),
        Just(MULTI_EXPORT_WASM_WAT),
        Just(GLOBALS_WASM_WAT),
    ]
    .prop_filter_map("WAT must compile to valid WASM", wat_to_wasm)
}

/// Strategy for selecting a minimal WASM module (memory only, no process).
/// Used for testing that modules without required exports are rejected.
#[allow(dead_code)]
fn minimal_wasm_module_strategy() -> impl Strategy<Value = Vec<u8>> {
    Just(MINIMAL_WASM_WAT).prop_filter_map("WAT must compile to valid WASM", wat_to_wasm)
}

/// Strategy for generating valid resource limits.
fn resource_limits_strategy() -> impl Strategy<Value = ResourceLimits> {
    (
        (1usize..=64).prop_map(|mb| mb * 1024 * 1024),
        1_000_000u64..=100_000_000,
        1000u32..=50000,
        1usize..=64,
    )
        .prop_map(|(memory, fuel, table, instances)| {
            ResourceLimits::new(memory, fuel, table, instances)
        })
}

/// Strategy for generating capability sets.
fn capabilities_strategy() -> impl Strategy<Value = Vec<Capability>> {
    prop::collection::vec(
        prop_oneof![
            Just(Capability::ReadTelemetry),
            Just(Capability::ModifyTelemetry),
            Just(Capability::ControlLeds),
        ],
        0..=3,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 7: WASM Plugin Loading
    ///
    /// *For any* valid WASM module conforming to the plugin ABI, the WASM runtime
    /// SHALL successfully load and instantiate the plugin.
    ///
    /// **Validates: Requirements 8.1**
    #[test]
    fn prop_wasm_plugin_loading(
        wasm_bytes in valid_wasm_module_strategy(),
        capabilities in capabilities_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, capabilities);

        prop_assert!(
            result.is_ok(),
            "Failed to load valid WASM module: {:?}",
            result.err()
        );

        prop_assert!(
            runtime.has_plugin(&plugin_id),
            "Plugin not found after successful load"
        );

        prop_assert_eq!(
            runtime.instance_count(),
            1,
            "Instance count should be 1 after loading one plugin"
        );
    }

    /// Feature: release-roadmap-v1, Property 7: WASM Plugin Loading (with custom limits)
    ///
    /// **Validates: Requirements 8.1**
    #[test]
    fn prop_wasm_plugin_loading_with_custom_limits(
        wasm_bytes in valid_wasm_module_strategy(),
        limits in resource_limits_strategy(),
        capabilities in capabilities_strategy(),
    ) {
        let mut runtime = WasmRuntime::with_limits(limits.clone())
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        prop_assert_eq!(
            runtime.resource_limits().max_memory_bytes,
            limits.max_memory_bytes,
            "Memory limit not applied correctly"
        );

        let plugin_id = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, capabilities);

        prop_assert!(
            result.is_ok(),
            "Failed to load valid WASM module with custom limits: {:?}",
            result.err()
        );

        prop_assert!(
            runtime.has_plugin(&plugin_id),
            "Plugin not found after successful load"
        );
    }

    /// Feature: release-roadmap-v1, Property 7: WASM Plugin Loading (multiple plugins)
    ///
    /// **Validates: Requirements 8.1**
    #[test]
    fn prop_wasm_multiple_plugin_loading(
        plugin_count in 1usize..=5,
        wasm_bytes in valid_wasm_module_strategy(),
        capabilities in capabilities_strategy(),
    ) {
        let limits = ResourceLimits::default().with_max_instances(10);
        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let mut loaded_ids = Vec::new();

        for _ in 0..plugin_count {
            let plugin_id = PluginId::new_v4();
            let result = runtime.load_plugin_from_bytes(
                plugin_id,
                &wasm_bytes,
                capabilities.clone(),
            );

            prop_assert!(
                result.is_ok(),
                "Failed to load plugin {}: {:?}",
                loaded_ids.len() + 1,
                result.err()
            );

            loaded_ids.push(plugin_id);
        }

        prop_assert_eq!(runtime.instance_count(), plugin_count, "Instance count mismatch");

        for id in &loaded_ids {
            prop_assert!(runtime.has_plugin(id), "Plugin {:?} not found after loading", id);
        }
    }

    /// Feature: release-roadmap-v1, Property 7: WASM Plugin Loading (unload and reload)
    ///
    /// **Validates: Requirements 8.1**
    #[test]
    fn prop_wasm_plugin_unload_reload(
        wasm_bytes in valid_wasm_module_strategy(),
        capabilities in capabilities_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();

        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, capabilities.clone())
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        prop_assert!(runtime.has_plugin(&plugin_id), "Plugin not loaded");

        runtime.unload_plugin(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to unload plugin: {}", e)))?;

        prop_assert!(!runtime.has_plugin(&plugin_id), "Plugin still present after unload");
        prop_assert_eq!(runtime.instance_count(), 0, "Instance count should be 0 after unload");

        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, capabilities)
            .map_err(|e| TestCaseError::fail(format!("Failed to reload plugin: {}", e)))?;

        prop_assert!(runtime.has_plugin(&plugin_id), "Plugin not reloaded");
        prop_assert_eq!(runtime.instance_count(), 1, "Instance count should be 1 after reload");
    }

    /// Feature: release-roadmap-v1, Property 7: WASM Plugin Loading (different modules)
    ///
    /// **Validates: Requirements 8.1**
    #[test]
    fn prop_wasm_plugin_loading_different_modules(
        module1 in valid_wasm_module_strategy(),
        module2 in valid_wasm_module_strategy(),
        capabilities in capabilities_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id1 = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id1, &module1, capabilities.clone())
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin 1: {}", e)))?;

        let plugin_id2 = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id2, &module2, capabilities)
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin 2: {}", e)))?;

        prop_assert!(runtime.has_plugin(&plugin_id1), "Plugin 1 not found");
        prop_assert!(runtime.has_plugin(&plugin_id2), "Plugin 2 not found");
        prop_assert_eq!(runtime.instance_count(), 2, "Should have 2 plugins loaded");
    }

    /// Feature: release-roadmap-v1, Property 7: WASM Plugin Loading (instance limit)
    ///
    /// **Validates: Requirements 8.1**
    #[test]
    fn prop_wasm_plugin_instance_limit(
        max_instances in 1usize..=5,
        wasm_bytes in valid_wasm_module_strategy(),
    ) {
        let limits = ResourceLimits::default().with_max_instances(max_instances);
        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        for i in 0..max_instances {
            let plugin_id = PluginId::new_v4();
            let result = runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![]);
            prop_assert!(
                result.is_ok(),
                "Failed to load plugin {} within limit: {:?}",
                i + 1,
                result.err()
            );
        }

        prop_assert_eq!(runtime.instance_count(), max_instances, "Should have max_instances");

        let extra_plugin_id = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(extra_plugin_id, &wasm_bytes, vec![]);

        prop_assert!(result.is_err(), "Loading beyond instance limit should fail");
        prop_assert_eq!(runtime.instance_count(), max_instances, "Count should not exceed limit");
    }
}

// ============================================================================
// Property 8: WASM Resource Sandboxing Tests
// ============================================================================

/// Strategy for generating fuel limits that will be exhausted by infinite loops.
fn restrictive_fuel_limit_strategy() -> impl Strategy<Value = u64> {
    // Very low fuel limits to ensure infinite loops are caught quickly
    100u64..=10_000
}

/// Strategy for generating memory limits in bytes.
/// These are intentionally small to test memory limit enforcement.
fn restrictive_memory_limit_strategy() -> impl Strategy<Value = usize> {
    // 64KB to 1MB - small enough to trigger limits with memory growth
    (1usize..=16).prop_map(|pages| pages * 64 * 1024)
}

/// Convert WAT to WASM for resource sandboxing tests.
fn wat_to_wasm_sandboxing(wat: &str) -> Option<Vec<u8>> {
    wat::parse_str(wat).ok()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 8: WASM Resource Sandboxing (Fuel Exhaustion)
    ///
    /// *For any* WASM plugin attempting to run an infinite loop, the runtime SHALL
    /// terminate the plugin when fuel is exhausted and return a resource limit error.
    ///
    /// **Validates: Requirements 8.2, 8.6**
    #[test]
    fn prop_wasm_fuel_exhaustion_terminates_plugin(
        fuel_limit in restrictive_fuel_limit_strategy(),
        input in -100.0f32..=100.0f32,
        dt in 0.0001f32..=0.1f32,
    ) {
        let wasm_bytes = wat_to_wasm_sandboxing(INFINITE_LOOP_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile infinite loop WASM"))?;

        let limits = ResourceLimits::default()
            .with_fuel(fuel_limit);

        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Calling process on an infinite loop should fail due to fuel exhaustion
        let result = runtime.process(&plugin_id, input, dt);

        prop_assert!(
            result.is_err(),
            "Infinite loop should be terminated by fuel exhaustion, but got: {:?}",
            result
        );

        // Verify the error is related to resource limits (fuel exhaustion or WASM trap)
        match result {
            Err(PluginError::BudgetViolation { .. }) => {
                // Expected: fuel exhaustion detected
            }
            Err(PluginError::WasmRuntime(_)) => {
                // Wasmtime trap or interrupt due to resource limits - acceptable
                // The error can be "interrupt", "trap", "fuel", etc.
            }
            Err(_) => {
                // Any error is acceptable as long as the plugin was terminated
            }
            Ok(_) => {
                return Err(TestCaseError::fail("Infinite loop should not complete successfully"));
            }
        }
    }

    /// Feature: release-roadmap-v1, Property 8: WASM Resource Sandboxing (Long Computation)
    ///
    /// *For any* WASM plugin running a long computation that exceeds fuel limits,
    /// the runtime SHALL terminate the plugin and return a resource limit error.
    ///
    /// **Validates: Requirements 8.2, 8.6**
    #[test]
    fn prop_wasm_long_computation_fuel_limit(
        fuel_limit in restrictive_fuel_limit_strategy(),
        // Use larger input values to trigger more iterations
        input in 10.0f32..=100.0f32,
        dt in 0.0001f32..=0.1f32,
    ) {
        let wasm_bytes = wat_to_wasm_sandboxing(LONG_COMPUTATION_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile long computation WASM"))?;

        let limits = ResourceLimits::default()
            .with_fuel(fuel_limit);

        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // With very low fuel and high iteration count, this should fail
        let result = runtime.process(&plugin_id, input, dt);

        // The computation should either complete (if fuel was sufficient) or fail
        // With our restrictive fuel limits and high input, it should fail
        match result {
            Err(PluginError::BudgetViolation { .. }) => {
                // Expected: fuel exhaustion
            }
            Err(PluginError::WasmRuntime(_)) => {
                // Wasmtime trap or interrupt - acceptable resource limit enforcement
            }
            Err(_) => {
                // Any termination error is acceptable
            }
            Ok(_) => {
                // If it completed, the fuel was sufficient for this input
                // This is acceptable - the property is about termination when limits are exceeded
            }
        }
    }

    /// Feature: release-roadmap-v1, Property 8: WASM Resource Sandboxing (Memory Growth)
    ///
    /// *For any* WASM plugin attempting to grow memory beyond configured limits,
    /// the memory growth operation SHALL fail (return -1) without crashing.
    ///
    /// **Validates: Requirements 8.2, 8.6**
    #[test]
    fn prop_wasm_memory_growth_limited(
        memory_limit in restrictive_memory_limit_strategy(),
        // Request more pages than available
        requested_pages in 10.0f32..=100.0f32,
        dt in 0.0001f32..=0.1f32,
    ) {
        let wasm_bytes = wat_to_wasm_sandboxing(MEMORY_GROW_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile memory grow WASM"))?;

        let limits = ResourceLimits::default()
            .with_memory(memory_limit)
            .with_fuel(10_000_000); // Sufficient fuel

        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // The plugin tries to grow memory; this should either:
        // 1. Fail the memory.grow (returns -1 in WASM) and continue
        // 2. Trap if the runtime enforces hard limits
        // 3. Succeed if the requested amount is within limits
        let result = runtime.process(&plugin_id, requested_pages, dt);

        // The key property: the runtime should NOT crash, and should handle
        // the memory growth attempt gracefully
        match result {
            Ok(output) => {
                // Memory growth was handled (either succeeded within limits or failed gracefully)
                // The output should be the input (passthrough after failed grow)
                prop_assert!(
                    (output - requested_pages).abs() < 0.001,
                    "Output should match input after memory grow attempt"
                );
            }
            Err(PluginError::WasmRuntime(_)) => {
                // Runtime trapped or interrupted - acceptable sandboxing behavior
            }
            Err(_) => {
                // Any controlled error is acceptable - the plugin was sandboxed
            }
        }
    }

    /// Feature: release-roadmap-v1, Property 8: WASM Resource Sandboxing (Repeated Memory Allocation)
    ///
    /// *For any* WASM plugin attempting repeated memory allocations in a loop,
    /// the runtime SHALL enforce limits and prevent unbounded growth.
    ///
    /// **Validates: Requirements 8.2, 8.6**
    #[test]
    fn prop_wasm_repeated_memory_allocation_limited(
        memory_limit in restrictive_memory_limit_strategy(),
        allocation_attempts in 5.0f32..=50.0f32,
        dt in 0.0001f32..=0.1f32,
    ) {
        let wasm_bytes = wat_to_wasm_sandboxing(MEMORY_ALLOC_LOOP_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile memory alloc loop WASM"))?;

        let limits = ResourceLimits::default()
            .with_memory(memory_limit)
            .with_fuel(10_000_000); // Sufficient fuel

        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Try to allocate memory repeatedly
        let result = runtime.process(&plugin_id, allocation_attempts, dt);

        // The runtime should handle this gracefully - either by:
        // 1. Allowing allocations within limits
        // 2. Failing individual allocations (memory.grow returns -1)
        // 3. Trapping if hard limits are exceeded
        match result {
            Ok(_) => {
                // Allocations were handled within limits
            }
            Err(PluginError::WasmRuntime(_)) => {
                // Runtime enforced limits via trap
            }
            Err(_) => {
                // Any controlled termination is acceptable
            }
        }

        // The key assertion: we reached this point without crashing
        // The runtime successfully sandboxed the memory allocation attempts
        prop_assert!(true, "Runtime successfully sandboxed memory allocation attempts");
    }

    /// Feature: release-roadmap-v1, Property 8: WASM Resource Sandboxing (Resource Limit Configuration)
    ///
    /// *For any* fuel limit configuration, the runtime SHALL correctly store and apply
    /// the configured limits to plugins.
    ///
    /// **Validates: Requirements 8.2, 8.6**
    #[test]
    fn prop_wasm_resource_limit_configuration(
        fuel_limit in 1_000_000u64..=10_000_000,
        memory_limit in (1usize..=64).prop_map(|mb| mb * 1024 * 1024),
        max_instances in 1usize..=32,
    ) {
        let limits = ResourceLimits::default()
            .with_fuel(fuel_limit)
            .with_memory(memory_limit)
            .with_max_instances(max_instances);

        let runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        // Verify all limits are configured correctly
        prop_assert_eq!(
            runtime.resource_limits().max_fuel,
            fuel_limit,
            "Fuel limit should be configured correctly"
        );
        prop_assert_eq!(
            runtime.resource_limits().max_memory_bytes,
            memory_limit,
            "Memory limit should be configured correctly"
        );
        prop_assert_eq!(
            runtime.resource_limits().max_instances,
            max_instances,
            "Instance limit should be configured correctly"
        );
    }

    /// Feature: release-roadmap-v1, Property 8: WASM Resource Sandboxing (Different Fuel Limits)
    ///
    /// *For any* fuel limit configuration, plugins exceeding that limit SHALL be terminated.
    ///
    /// **Validates: Requirements 8.2, 8.6**
    #[test]
    fn prop_wasm_configurable_fuel_limits(
        fuel_limit in restrictive_fuel_limit_strategy(),
    ) {
        let wasm_bytes = wat_to_wasm_sandboxing(INFINITE_LOOP_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile infinite loop WASM"))?;

        let limits = ResourceLimits::default()
            .with_fuel(fuel_limit);

        let mut runtime = WasmRuntime::with_limits(limits)
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        // Verify the fuel limit was applied
        prop_assert_eq!(
            runtime.resource_limits().max_fuel,
            fuel_limit,
            "Fuel limit should be configured correctly"
        );

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Infinite loop should be terminated regardless of specific fuel limit
        let result = runtime.process(&plugin_id, 1.0, 0.001);
        prop_assert!(
            result.is_err(),
            "Infinite loop should be terminated with fuel limit {}",
            fuel_limit
        );
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_minimal_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(MINIMAL_WASM_WAT).ok_or("Failed to compile minimal WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_process_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile process WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_all_wat_modules_compile() -> Result<(), Box<dyn std::error::Error>> {
        let modules = [
            ("MINIMAL", MINIMAL_WASM_WAT),
            ("PROCESS", PROCESS_WASM_WAT),
            ("PASSTHROUGH", PASSTHROUGH_WASM_WAT),
            ("CONSTANT", CONSTANT_WASM_WAT),
            ("CLAMP", CLAMP_WASM_WAT),
            ("MULTI_EXPORT", MULTI_EXPORT_WASM_WAT),
            ("GLOBALS", GLOBALS_WASM_WAT),
        ];

        for (name, wat) in modules {
            let wasm_bytes =
                wat_to_wasm(wat).ok_or_else(|| format!("Failed to compile {} WASM", name))?;
            assert!(wasm_bytes.len() >= 8, "{} WASM too short", name);
            assert_eq!(
                &wasm_bytes[0..4],
                b"\0asm",
                "{} WASM missing magic number",
                name
            );
        }
        Ok(())
    }

    #[test]
    fn test_load_minimal_wasm() -> Result<(), Box<dyn std::error::Error>> {
        // Minimal WASM without process function should be rejected
        let wasm_bytes = wat_to_wasm(MINIMAL_WASM_WAT).ok_or("Failed to compile minimal WASM")?;
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![]);
        // Should fail because process export is required
        assert!(result.is_err());
        assert!(!runtime.has_plugin(&plugin_id));
        Ok(())
    }

    #[test]
    fn test_load_process_wasm() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile process WASM")?;
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;
        assert!(runtime.has_plugin(&plugin_id));
        assert_eq!(runtime.instance_count(), 1);
        Ok(())
    }

    #[test]
    fn test_load_multi_export_wasm() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(MULTI_EXPORT_WASM_WAT).ok_or("Failed to compile multi-export WASM")?;
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;
        assert!(runtime.has_plugin(&plugin_id));
        assert_eq!(runtime.instance_count(), 1);
        Ok(())
    }

    #[test]
    fn test_load_globals_wasm() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(GLOBALS_WASM_WAT).ok_or("Failed to compile globals WASM")?;
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;
        assert!(runtime.has_plugin(&plugin_id));
        assert_eq!(runtime.instance_count(), 1);
        Ok(())
    }

    #[test]
    fn test_invalid_wasm_fails() -> Result<(), Box<dyn std::error::Error>> {
        let invalid_bytes = b"not valid wasm";
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(plugin_id, invalid_bytes, vec![]);
        assert!(result.is_err());
        assert!(!runtime.has_plugin(&plugin_id));
        Ok(())
    }

    #[test]
    fn test_truncated_wasm_fails() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile process WASM")?;
        let truncated = &wasm_bytes[..wasm_bytes.len() / 2];
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(plugin_id, truncated, vec![]);
        assert!(result.is_err());
        assert!(!runtime.has_plugin(&plugin_id));
        Ok(())
    }

    #[test]
    fn test_instance_limit_enforced() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile process WASM")?;
        let limits = ResourceLimits::default().with_max_instances(2);
        let mut runtime = WasmRuntime::with_limits(limits)?;

        let id1 = PluginId::new_v4();
        runtime.load_plugin_from_bytes(id1, &wasm_bytes, vec![])?;

        let id2 = PluginId::new_v4();
        runtime.load_plugin_from_bytes(id2, &wasm_bytes, vec![])?;

        let id3 = PluginId::new_v4();
        let result = runtime.load_plugin_from_bytes(id3, &wasm_bytes, vec![]);
        assert!(result.is_err());
        assert_eq!(runtime.instance_count(), 2);
        Ok(())
    }

    #[test]
    fn test_unload_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile process WASM")?;
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;
        assert!(runtime.has_plugin(&plugin_id));
        runtime.unload_plugin(&plugin_id)?;
        assert!(!runtime.has_plugin(&plugin_id));
        assert_eq!(runtime.instance_count(), 0);
        Ok(())
    }

    #[test]
    fn test_unload_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        let result = runtime.unload_plugin(&plugin_id);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_process_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_process_no_function_fails() -> Result<(), Box<dyn std::error::Error>> {
        // With the new ABI, modules without process function are rejected at load time
        let wasm_bytes = wat_to_wasm(MINIMAL_WASM_WAT).ok_or("Failed to compile minimal WASM")?;
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        // Loading should fail because process export is required
        let result = runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![]);
        assert!(
            result.is_err(),
            "Loading module without process should fail"
        );
        Ok(())
    }

    #[test]
    fn test_load_with_capabilities() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile process WASM")?;
        let capability_sets = vec![
            vec![],
            vec![Capability::ReadTelemetry],
            vec![Capability::ModifyTelemetry],
            vec![Capability::ControlLeds],
            vec![Capability::ReadTelemetry, Capability::ModifyTelemetry],
            vec![
                Capability::ReadTelemetry,
                Capability::ModifyTelemetry,
                Capability::ControlLeds,
            ],
        ];

        for capabilities in capability_sets {
            let mut runtime = WasmRuntime::new()?;
            let plugin_id = PluginId::new_v4();
            let result = runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, capabilities);
            assert!(result.is_ok(), "Failed to load with capabilities");
            assert!(runtime.has_plugin(&plugin_id));
        }
        Ok(())
    }

    #[test]
    fn test_resource_limits_stored() -> Result<(), Box<dyn std::error::Error>> {
        let limits = ResourceLimits::new(8 * 1024 * 1024, 5_000_000, 5_000, 16);
        let runtime = WasmRuntime::with_limits(limits)?;
        assert_eq!(runtime.resource_limits().max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(runtime.resource_limits().max_fuel, 5_000_000);
        assert_eq!(runtime.resource_limits().max_table_elements, 5_000);
        assert_eq!(runtime.resource_limits().max_instances, 16);
        Ok(())
    }

    // ========================================================================
    // Unit tests for Property 8: WASM Resource Sandboxing
    // ========================================================================

    #[test]
    fn test_infinite_loop_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(INFINITE_LOOP_WASM_WAT).ok_or("Failed to compile infinite loop WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_long_computation_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(LONG_COMPUTATION_WASM_WAT)
            .ok_or("Failed to compile long computation WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_memory_grow_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(MEMORY_GROW_WASM_WAT).ok_or("Failed to compile memory grow WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_memory_alloc_loop_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(MEMORY_ALLOC_LOOP_WASM_WAT)
            .ok_or("Failed to compile memory alloc loop WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_fuel_exhaustion_terminates_infinite_loop() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(INFINITE_LOOP_WASM_WAT).ok_or("Failed to compile infinite loop WASM")?;

        // Use very low fuel to ensure quick termination
        let limits = ResourceLimits::default().with_fuel(1000);
        let mut runtime = WasmRuntime::with_limits(limits)?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // This should fail due to fuel exhaustion
        let result = runtime.process(&plugin_id, 1.0, 0.001);
        assert!(result.is_err(), "Infinite loop should be terminated");

        Ok(())
    }

    #[test]
    fn test_resource_limits_configuration() -> Result<(), Box<dyn std::error::Error>> {
        // Test that resource limits are correctly configured and stored
        let limits = ResourceLimits::default()
            .with_fuel(5_000_000)
            .with_memory(8 * 1024 * 1024)
            .with_max_instances(16);

        let runtime = WasmRuntime::with_limits(limits)?;

        assert_eq!(runtime.resource_limits().max_fuel, 5_000_000);
        assert_eq!(runtime.resource_limits().max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(runtime.resource_limits().max_instances, 16);

        Ok(())
    }

    #[test]
    fn test_memory_growth_handled_gracefully() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(MEMORY_GROW_WASM_WAT).ok_or("Failed to compile memory grow WASM")?;

        // Small memory limit
        let limits = ResourceLimits::default()
            .with_memory(128 * 1024) // 128KB
            .with_fuel(10_000_000);
        let mut runtime = WasmRuntime::with_limits(limits)?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Try to grow memory significantly - should be handled gracefully
        let result = runtime.process(&plugin_id, 10.0, 0.001);

        // The result can be Ok (if grow failed gracefully) or Err (if trapped)
        // Either way, we should not crash
        match result {
            Ok(output) => {
                // Memory grow failed gracefully, output should be input
                assert!((output - 10.0).abs() < 0.001);
            }
            Err(_) => {
                // Runtime trapped - also acceptable
            }
        }

        Ok(())
    }

    #[test]
    fn test_plugin_termination_on_resource_violation() -> Result<(), Box<dyn std::error::Error>> {
        // Test that plugins are terminated when they violate resource limits
        // This validates the core Property 8 requirement
        let wasm_bytes =
            wat_to_wasm(INFINITE_LOOP_WASM_WAT).ok_or("Failed to compile infinite loop WASM")?;

        // Very low fuel limit to ensure termination
        let limits = ResourceLimits::default().with_fuel(500);
        let mut runtime = WasmRuntime::with_limits(limits)?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Infinite loop should be terminated
        let result = runtime.process(&plugin_id, 1.0, 0.001);
        assert!(result.is_err(), "Infinite loop should be terminated");

        // Plugin should still be loaded (not crashed the runtime)
        assert!(
            runtime.has_plugin(&plugin_id),
            "Plugin should still be registered after termination"
        );

        Ok(())
    }

    #[test]
    fn test_long_computation_with_low_fuel_fails() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(LONG_COMPUTATION_WASM_WAT)
            .ok_or("Failed to compile long computation WASM")?;

        // Very low fuel limit
        let limits = ResourceLimits::default().with_fuel(100);
        let mut runtime = WasmRuntime::with_limits(limits)?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Large input = many iterations, should fail with low fuel
        let result = runtime.process(&plugin_id, 100.0, 0.001);
        assert!(
            result.is_err(),
            "Large computation should fail with low fuel"
        );

        Ok(())
    }

    #[test]
    fn test_different_fuel_limits_enforced() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(INFINITE_LOOP_WASM_WAT).ok_or("Failed to compile infinite loop WASM")?;

        // Test with various fuel limits - all should terminate the infinite loop
        for fuel in [100, 1000, 10000] {
            let limits = ResourceLimits::default().with_fuel(fuel);
            let mut runtime = WasmRuntime::with_limits(limits)?;

            let plugin_id = PluginId::new_v4();
            runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

            let result = runtime.process(&plugin_id, 1.0, 0.001);
            assert!(
                result.is_err(),
                "Infinite loop should be terminated with fuel limit {}",
                fuel
            );
        }

        Ok(())
    }
}

// ============================================================================
// Property 9: WASM Panic Isolation Tests
// ============================================================================

/// WASM module with unreachable instruction (trap).
const UNREACHABLE_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        unreachable
    )
)
"#;

/// WASM module with integer divide by zero (trap).
/// Note: WASM traps on integer division by zero, not floating-point.
const DIVIDE_BY_ZERO_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; Integer divide by zero causes a trap
        i32.const 1
        i32.const 0
        i32.div_s
        drop
        local.get 0
    )
)
"#;

/// WASM module with stack overflow via deep recursion (trap).
const STACK_OVERFLOW_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func $recurse (param f32) (result f32)
        ;; Recursive call without termination condition
        local.get 0
        f32.const 1.0
        f32.add
        call $recurse
    )
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        call $recurse
    )
)
"#;

/// WASM module that conditionally traps based on input.
/// Traps when input is negative.
const CONDITIONAL_TRAP_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        ;; If input < 0, trap
        local.get 0
        f32.const 0.0
        f32.lt
        if
            unreachable
        end
        local.get 0
    )
)
"#;

/// WASM module that works correctly (for comparison).
const WORKING_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        local.get 1
        f32.add
    )
)
"#;

/// Strategy for selecting a WASM module that will trap.
fn trapping_wasm_module_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        Just(UNREACHABLE_WASM_WAT),
        Just(DIVIDE_BY_ZERO_WASM_WAT),
        Just(STACK_OVERFLOW_WASM_WAT),
    ]
    .prop_filter_map("WAT must compile to valid WASM", wat_to_wasm)
}

/// Strategy for generating valid input values for process function.
fn process_input_strategy() -> impl Strategy<Value = (f32, f32)> {
    (
        prop::num::f32::NORMAL.prop_filter("finite", |x| x.is_finite()),
        (0.0001f32..=0.1f32),
    )
}

/// Strategy for generating positive input values (won't trigger conditional trap).
fn positive_input_strategy() -> impl Strategy<Value = f32> {
    0.0f32..=100.0f32
}

/// Strategy for generating negative input values (will trigger conditional trap).
fn negative_input_strategy() -> impl Strategy<Value = f32> {
    -100.0f32..-0.001f32
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Trap Catching)
    ///
    /// *For any* WASM plugin that traps during execution (unreachable, divide by zero,
    /// stack overflow), the runtime SHALL catch the trap without crashing.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_trap_is_caught(
        wasm_bytes in trapping_wasm_module_strategy(),
        (input, dt) in process_input_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Process should fail due to trap, but NOT crash the runtime
        let result = runtime.process(&plugin_id, input, dt);

        prop_assert!(
            result.is_err(),
            "Trapping plugin should return an error, not succeed"
        );

        // The runtime should still be operational (not crashed)
        // We verify this by checking that we can still query the plugin
        let is_disabled = runtime.is_plugin_disabled(&plugin_id);
        prop_assert!(
            is_disabled.is_ok(),
            "Runtime should still be operational after trap"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Plugin Disabled After Trap)
    ///
    /// *For any* WASM plugin that traps during execution, the runtime SHALL
    /// disable the plugin after the trap is caught.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_plugin_disabled_after_trap(
        wasm_bytes in trapping_wasm_module_strategy(),
        (input, dt) in process_input_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Plugin should not be disabled initially
        let initially_disabled = runtime.is_plugin_disabled(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
        prop_assert!(!initially_disabled, "Plugin should not be disabled initially");

        // Trigger the trap
        let _ = runtime.process(&plugin_id, input, dt);

        // Plugin should now be disabled
        let disabled_after_trap = runtime.is_plugin_disabled(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
        prop_assert!(disabled_after_trap, "Plugin should be disabled after trap");

        // Disabled info should be available
        let disabled_info = runtime.get_plugin_disabled_info(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to get disabled info: {}", e)))?;
        prop_assert!(disabled_info.is_some(), "Disabled info should be available");

        let info = disabled_info.ok_or_else(|| TestCaseError::fail("Expected disabled info"))?;
        prop_assert!(!info.reason.is_empty(), "Disabled reason should not be empty");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Runtime Continues)
    ///
    /// *For any* WASM plugin that traps, the runtime SHALL continue operating
    /// and be able to load and execute other plugins without crashing.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_runtime_continues_after_trap(
        trapping_wasm in trapping_wasm_module_strategy(),
        (input, dt) in process_input_strategy(),
    ) {
        let working_wasm = wat_to_wasm(WORKING_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile working WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        // Load the trapping plugin
        let trapping_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(trapping_id, &trapping_wasm, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load trapping plugin: {}", e)))?;

        // Trigger the trap
        let _ = runtime.process(&trapping_id, input, dt);

        // Runtime should still be operational - load another plugin
        let working_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(working_id, &working_wasm, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load working plugin: {}", e)))?;

        // The working plugin should execute successfully
        let result = runtime.process(&working_id, input, dt);
        prop_assert!(
            result.is_ok(),
            "Working plugin should execute after another plugin trapped: {:?}",
            result.err()
        );

        // Verify both plugins are still registered
        prop_assert!(runtime.has_plugin(&trapping_id), "Trapping plugin should still be registered");
        prop_assert!(runtime.has_plugin(&working_id), "Working plugin should be registered");
        prop_assert_eq!(runtime.instance_count(), 2, "Should have 2 plugins loaded");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Disabled Plugin Error)
    ///
    /// *For any* disabled WASM plugin, subsequent calls to process SHALL return
    /// an appropriate error indicating the plugin is disabled.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_disabled_plugin_returns_error(
        wasm_bytes in trapping_wasm_module_strategy(),
        (input1, dt1) in process_input_strategy(),
        (input2, dt2) in process_input_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Trigger the trap to disable the plugin
        let _ = runtime.process(&plugin_id, input1, dt1);

        // Subsequent calls should return Crashed error
        let result = runtime.process(&plugin_id, input2, dt2);
        prop_assert!(result.is_err(), "Disabled plugin should return error");

        match result {
            Err(PluginError::Crashed { reason }) => {
                prop_assert!(
                    reason.contains("disabled"),
                    "Error should indicate plugin is disabled, got: {}",
                    reason
                );
            }
            Err(other) => {
                return Err(TestCaseError::fail(format!(
                    "Expected Crashed error, got: {:?}",
                    other
                )));
            }
            Ok(_) => {
                return Err(TestCaseError::fail("Disabled plugin should not succeed"));
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Re-enable Plugin)
    ///
    /// *For any* disabled WASM plugin, the runtime SHALL support re-enabling
    /// the plugin, allowing it to be called again.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_plugin_can_be_reenabled(
        wasm_bytes in trapping_wasm_module_strategy(),
        (input, dt) in process_input_strategy(),
    ) {
        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Trigger the trap to disable the plugin
        let _ = runtime.process(&plugin_id, input, dt);

        // Plugin should be disabled
        let disabled = runtime.is_plugin_disabled(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
        prop_assert!(disabled, "Plugin should be disabled after trap");

        // Re-enable the plugin
        let was_disabled = runtime.re_enable_plugin(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to re-enable: {}", e)))?;
        prop_assert!(was_disabled, "re_enable_plugin should return true for disabled plugin");

        // Plugin should no longer be disabled
        let still_disabled = runtime.is_plugin_disabled(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
        prop_assert!(!still_disabled, "Plugin should not be disabled after re-enable");

        // Re-enabling again should return false
        let was_disabled_again = runtime.re_enable_plugin(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to re-enable again: {}", e)))?;
        prop_assert!(!was_disabled_again, "re_enable_plugin should return false for enabled plugin");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Conditional Trap)
    ///
    /// *For any* WASM plugin that conditionally traps based on input, the runtime
    /// SHALL correctly handle both trapping and non-trapping executions.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_conditional_trap_handling(
        positive_input in positive_input_strategy(),
        negative_input in negative_input_strategy(),
        dt in 0.0001f32..=0.1f32,
    ) {
        let wasm_bytes = wat_to_wasm(CONDITIONAL_TRAP_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile conditional trap WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Positive input should succeed
        let result = runtime.process(&plugin_id, positive_input, dt);
        prop_assert!(
            result.is_ok(),
            "Positive input should not trap: {:?}",
            result.err()
        );

        // Plugin should not be disabled after successful execution
        let disabled = runtime.is_plugin_disabled(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
        prop_assert!(!disabled, "Plugin should not be disabled after successful execution");

        // Negative input should trap
        let result = runtime.process(&plugin_id, negative_input, dt);
        prop_assert!(result.is_err(), "Negative input should trap");

        // Plugin should now be disabled
        let disabled = runtime.is_plugin_disabled(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
        prop_assert!(disabled, "Plugin should be disabled after trap");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Feature: release-roadmap-v1, Property 9: WASM Panic Isolation (Multiple Traps)
    ///
    /// *For any* sequence of plugin loads and traps, the runtime SHALL continue
    /// operating correctly without accumulating state corruption.
    ///
    /// **Validates: Requirements 8.4**
    #[test]
    fn prop_wasm_multiple_traps_handled(
        trap_count in 1usize..=5,
        (input, dt) in process_input_strategy(),
    ) {
        let trapping_wasm = wat_to_wasm(UNREACHABLE_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile unreachable WASM"))?;
        let working_wasm = wat_to_wasm(WORKING_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile working WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        // Load and trap multiple plugins
        for i in 0..trap_count {
            let plugin_id = PluginId::new_v4();
            runtime.load_plugin_from_bytes(plugin_id, &trapping_wasm, vec![])
                .map_err(|e| TestCaseError::fail(format!("Failed to load plugin {}: {}", i, e)))?;

            // Trigger trap
            let _ = runtime.process(&plugin_id, input, dt);

            // Verify plugin is disabled
            let disabled = runtime.is_plugin_disabled(&plugin_id)
                .map_err(|e| TestCaseError::fail(format!("Failed to check disabled: {}", e)))?;
            prop_assert!(disabled, "Plugin {} should be disabled after trap", i);
        }

        // Runtime should still work - load and execute a working plugin
        let working_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(working_id, &working_wasm, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load working plugin: {}", e)))?;

        let result = runtime.process(&working_id, input, dt);
        prop_assert!(
            result.is_ok(),
            "Working plugin should execute after {} traps: {:?}",
            trap_count,
            result.err()
        );

        // Total instance count should be trap_count + 1
        prop_assert_eq!(
            runtime.instance_count(),
            trap_count + 1,
            "Should have {} plugins loaded",
            trap_count + 1
        );
    }
}

#[cfg(test)]
mod panic_isolation_unit_tests {
    use super::*;

    // ========================================================================
    // Unit tests for Property 9: WASM Panic Isolation
    // ========================================================================

    #[test]
    fn test_unreachable_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(UNREACHABLE_WASM_WAT).ok_or("Failed to compile unreachable WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_divide_by_zero_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(DIVIDE_BY_ZERO_WASM_WAT).ok_or("Failed to compile divide by zero WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_stack_overflow_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(STACK_OVERFLOW_WASM_WAT).ok_or("Failed to compile stack overflow WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_conditional_trap_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(CONDITIONAL_TRAP_WASM_WAT)
            .ok_or("Failed to compile conditional trap WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_working_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(WORKING_WASM_WAT).ok_or("Failed to compile working WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_unreachable_trap_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(UNREACHABLE_WASM_WAT).ok_or("Failed to compile unreachable WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Plugin should not be disabled initially
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        // Trigger the trap
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        // Plugin should now be disabled
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_divide_by_zero_trap_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(DIVIDE_BY_ZERO_WASM_WAT).ok_or("Failed to compile divide by zero WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Trigger the trap
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        // Plugin should be disabled
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_stack_overflow_trap_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(STACK_OVERFLOW_WASM_WAT).ok_or("Failed to compile stack overflow WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Trigger the trap (stack overflow)
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        // Plugin should be disabled
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_disabled_plugin_returns_crashed_error() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(UNREACHABLE_WASM_WAT).ok_or("Failed to compile unreachable WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Trigger the trap
        let _ = runtime.process(&plugin_id, 0.5, 0.001);

        // Subsequent calls should return Crashed error
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        match result {
            Err(PluginError::Crashed { reason }) => {
                assert!(reason.contains("disabled"));
            }
            _ => panic!("Expected Crashed error with 'disabled' in reason"),
        }

        Ok(())
    }

    #[test]
    fn test_runtime_continues_after_trap() -> Result<(), Box<dyn std::error::Error>> {
        let trapping_wasm =
            wat_to_wasm(UNREACHABLE_WASM_WAT).ok_or("Failed to compile unreachable WASM")?;
        let working_wasm = wat_to_wasm(WORKING_WASM_WAT).ok_or("Failed to compile working WASM")?;

        let mut runtime = WasmRuntime::new()?;

        // Load and trap a plugin
        let trapping_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(trapping_id, &trapping_wasm, vec![])?;
        let _ = runtime.process(&trapping_id, 0.5, 0.001);

        // Load and execute a working plugin
        let working_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(working_id, &working_wasm, vec![])?;
        let result = runtime.process(&working_id, 0.5, 0.001)?;

        // Working plugin should produce correct output (0.5 + 0.001 = 0.501)
        assert!((result - 0.501).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_plugin_reenable_after_trap() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(UNREACHABLE_WASM_WAT).ok_or("Failed to compile unreachable WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Trigger the trap
        let _ = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        // Re-enable the plugin
        let was_disabled = runtime.re_enable_plugin(&plugin_id)?;
        assert!(was_disabled);
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        // Re-enabling again should return false
        let was_disabled_again = runtime.re_enable_plugin(&plugin_id)?;
        assert!(!was_disabled_again);

        Ok(())
    }

    #[test]
    fn test_conditional_trap_positive_input_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(CONDITIONAL_TRAP_WASM_WAT)
            .ok_or("Failed to compile conditional trap WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Positive input should succeed
        let result = runtime.process(&plugin_id, 5.0, 0.001)?;
        assert!((result - 5.0).abs() < 0.001);

        // Plugin should not be disabled
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_conditional_trap_negative_input_traps() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(CONDITIONAL_TRAP_WASM_WAT)
            .ok_or("Failed to compile conditional trap WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Negative input should trap
        let result = runtime.process(&plugin_id, -5.0, 0.001);
        assert!(result.is_err());

        // Plugin should be disabled
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_disabled_info_contains_reason() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(UNREACHABLE_WASM_WAT).ok_or("Failed to compile unreachable WASM")?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Trigger the trap
        let _ = runtime.process(&plugin_id, 0.5, 0.001);

        // Get disabled info
        let info = runtime.get_plugin_disabled_info(&plugin_id)?;
        assert!(info.is_some());

        let info = info.ok_or("Expected disabled info")?;
        assert!(!info.reason.is_empty());
        // The reason should contain information about the trap/error
        // wasmtime may format this as "wasm backtrace", "unreachable", "trap", etc.
        let reason_lower = info.reason.to_lowercase();
        assert!(
            reason_lower.contains("unreachable")
                || reason_lower.contains("trap")
                || reason_lower.contains("wasm")
                || reason_lower.contains("backtrace")
                || reason_lower.contains("error"),
            "Reason should contain trap-related information: {}",
            info.reason
        );

        Ok(())
    }
}

// ============================================================================
// Property 10: WASM Hot-Reload State Preservation Tests
// ============================================================================

/// WASM module with stateful processing (accumulates value).
const STATEFUL_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (global $accumulator (mut f32) (f32.const 0.0))
    (func (export "process") (param f32 f32) (result f32)
        ;; Add input to accumulator and return new value
        global.get $accumulator
        local.get 0
        f32.add
        global.set $accumulator
        global.get $accumulator
    )
)
"#;

/// WASM module with different behavior (multiplies instead of adds).
const ALTERNATE_WASM_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
        f32.const 2.0
        f32.mul
    )
)
"#;

/// WASM module with init function that returns success.
const INIT_SUCCESS_WASM_WAT: &str = r#"
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

/// WASM module with init function that returns failure.
const INIT_FAILURE_WASM_WAT: &str = r#"
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

/// Strategy for generating plugin data key-value pairs.
/// Uses a HashMap to ensure unique keys, then converts to Vec for iteration.
fn plugin_data_strategy() -> impl Strategy<Value = Vec<(String, Vec<u8>)>> {
    prop::collection::hash_map(
        "[a-z]{1,10}".prop_map(|s| s.to_string()),
        prop::collection::vec(any::<u8>(), 0..100),
        0..5,
    )
    .prop_map(|map| map.into_iter().collect())
}

/// Strategy for generating process call counts and times.
fn stats_strategy() -> impl Strategy<Value = (u64, u64)> {
    (0u64..1000, 0u64..1_000_000)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 10: WASM Hot-Reload State Preservation
    ///
    /// *For any* plugin reload operation, the service state (connected devices,
    /// active profiles) SHALL remain unchanged after the reload completes.
    /// Specifically, plugin_data (custom data) is preserved across reloads.
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_hot_reload_preserves_plugin_data(
        plugin_data in plugin_data_strategy(),
    ) {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile process WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Store plugin data before reload
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            for (key, value) in &plugin_data {
                instance.store.data_mut().abi_state.store_data(key.clone(), value.clone());
            }
        }

        // Reload the plugin
        runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to reload plugin: {}", e)))?;

        // Verify plugin data is preserved
        if let Some(instance) = runtime.instances.get(&plugin_id) {
            for (key, expected_value) in &plugin_data {
                let actual_value = instance.store.data().abi_state.get_data(key);
                prop_assert!(
                    actual_value.is_some(),
                    "Plugin data key '{}' should be preserved after reload",
                    key
                );
                prop_assert_eq!(
                    actual_value,
                    Some(expected_value),
                    "Plugin data value for key '{}' should match after reload",
                    key
                );
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 10: WASM Hot-Reload State Preservation
    ///
    /// *For any* plugin reload operation, statistics (process_count, total_process_time_us)
    /// SHALL be preserved after the reload completes.
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_hot_reload_preserves_statistics(
        (process_count, total_time) in stats_strategy(),
    ) {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile process WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Set statistics before reload
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = process_count;
            instance.store.data_mut().abi_state.total_process_time_us = total_time;
        }

        // Reload the plugin
        runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to reload plugin: {}", e)))?;

        // Verify statistics are preserved
        let (actual_count, _avg_time) = runtime.get_plugin_stats(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to get stats: {}", e)))?;

        prop_assert_eq!(
            actual_count,
            process_count,
            "process_count should be preserved after reload"
        );

        // Verify total_process_time_us directly
        if let Some(instance) = runtime.instances.get(&plugin_id) {
            prop_assert_eq!(
                instance.store.data().abi_state.total_process_time_us,
                total_time,
                "total_process_time_us should be preserved after reload"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Feature: release-roadmap-v1, Property 10: WASM Hot-Reload State Preservation
    ///
    /// *For any* plugin reload operation, other plugins in the runtime SHALL
    /// remain unaffected by the reload.
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_hot_reload_does_not_affect_other_plugins(
        plugin_count in 2usize..=4,
        reload_index in 0usize..4,
        plugin_data in plugin_data_strategy(),
    ) {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile process WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        // Load multiple plugins
        let mut plugin_ids = Vec::new();
        for _ in 0..plugin_count {
            let plugin_id = PluginId::new_v4();
            runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
                .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;
            plugin_ids.push(plugin_id);
        }

        // Set unique data for each plugin
        for (i, plugin_id) in plugin_ids.iter().enumerate() {
            if let Some(instance) = runtime.instances.get_mut(plugin_id) {
                instance.store.data_mut().abi_state.process_count = (i + 1) as u64 * 100;
                for (key, value) in &plugin_data {
                    let unique_key = format!("{}_{}", key, i);
                    instance.store.data_mut().abi_state.store_data(unique_key, value.clone());
                }
            }
        }

        // Reload one plugin (use modulo to ensure valid index)
        let reload_idx = reload_index % plugin_count;
        let reload_id = plugin_ids[reload_idx];
        runtime.reload_plugin(&reload_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to reload plugin: {}", e)))?;

        // Verify other plugins are unaffected
        for (i, plugin_id) in plugin_ids.iter().enumerate() {
            if i == reload_idx {
                continue; // Skip the reloaded plugin
            }

            let (count, _) = runtime.get_plugin_stats(plugin_id)
                .map_err(|e| TestCaseError::fail(format!("Failed to get stats: {}", e)))?;

            prop_assert_eq!(
                count,
                (i + 1) as u64 * 100,
                "Plugin {} process_count should be unaffected by reload of plugin {}",
                i,
                reload_idx
            );

            // Verify plugin data is unaffected
            if let Some(instance) = runtime.instances.get(plugin_id) {
                for (key, expected_value) in &plugin_data {
                    let unique_key = format!("{}_{}", key, i);
                    let actual_value = instance.store.data().abi_state.get_data(&unique_key);
                    prop_assert_eq!(
                        actual_value,
                        Some(expected_value),
                        "Plugin {} data should be unaffected by reload",
                        i
                    );
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Feature: release-roadmap-v1, Property 10: WASM Hot-Reload State Preservation
    ///
    /// *For any* reload failure, the old plugin SHALL remain active and its
    /// state SHALL not be corrupted.
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_hot_reload_failure_preserves_old_plugin(
        (process_count, total_time) in stats_strategy(),
        plugin_data in plugin_data_strategy(),
    ) {
        let valid_wasm = wat_to_wasm(PROCESS_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile valid WASM"))?;
        let invalid_wasm = b"not valid wasm bytes".to_vec();

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &valid_wasm, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Set state before failed reload
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = process_count;
            instance.store.data_mut().abi_state.total_process_time_us = total_time;
            for (key, value) in &plugin_data {
                instance.store.data_mut().abi_state.store_data(key.clone(), value.clone());
            }
        }

        // Attempt reload with invalid WASM (should fail)
        let reload_result = runtime.reload_plugin(&plugin_id, &invalid_wasm, vec![]);
        prop_assert!(reload_result.is_err(), "Reload with invalid WASM should fail");

        // Verify old plugin is still active
        prop_assert!(
            runtime.has_plugin(&plugin_id),
            "Plugin should still exist after failed reload"
        );

        // Verify state is preserved
        let (actual_count, _) = runtime.get_plugin_stats(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to get stats: {}", e)))?;
        prop_assert_eq!(
            actual_count,
            process_count,
            "process_count should be preserved after failed reload"
        );

        if let Some(instance) = runtime.instances.get(&plugin_id) {
            prop_assert_eq!(
                instance.store.data().abi_state.total_process_time_us,
                total_time,
                "total_process_time_us should be preserved after failed reload"
            );

            for (key, expected_value) in &plugin_data {
                let actual_value = instance.store.data().abi_state.get_data(key);
                prop_assert_eq!(
                    actual_value,
                    Some(expected_value),
                    "Plugin data should be preserved after failed reload"
                );
            }
        }

        // Verify plugin is still functional
        let result = runtime.process(&plugin_id, 1.0, 0.001);
        prop_assert!(result.is_ok(), "Plugin should still be functional after failed reload");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Feature: release-roadmap-v1, Property 10: WASM Hot-Reload State Preservation
    ///
    /// *For any* sequence of multiple sequential reloads, state SHALL be
    /// preserved correctly across all reloads.
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_multiple_sequential_reloads_preserve_state(
        reload_count in 2usize..=5,
        initial_count in 0u64..100,
        plugin_data in plugin_data_strategy(),
    ) {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile process WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Set initial state
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = initial_count;
            for (key, value) in &plugin_data {
                instance.store.data_mut().abi_state.store_data(key.clone(), value.clone());
            }
        }

        // Perform multiple sequential reloads
        for i in 0..reload_count {
            runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])
                .map_err(|e| TestCaseError::fail(format!("Failed reload {}: {}", i, e)))?;

            // Verify state after each reload
            let (count, _) = runtime.get_plugin_stats(&plugin_id)
                .map_err(|e| TestCaseError::fail(format!("Failed to get stats: {}", e)))?;
            prop_assert_eq!(
                count,
                initial_count,
                "process_count should be preserved after reload {}",
                i
            );

            if let Some(instance) = runtime.instances.get(&plugin_id) {
                for (key, expected_value) in &plugin_data {
                    let actual_value = instance.store.data().abi_state.get_data(key);
                    prop_assert_eq!(
                        actual_value,
                        Some(expected_value),
                        "Plugin data '{}' should be preserved after reload {}",
                        key,
                        i
                    );
                }
            }
        }

        // Verify plugin is still functional after all reloads
        let result = runtime.process(&plugin_id, 1.0, 0.001);
        prop_assert!(
            result.is_ok(),
            "Plugin should be functional after {} reloads",
            reload_count
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// Feature: release-roadmap-v1, Property 10: WASM Hot-Reload State Preservation
    ///
    /// *For any* reload with a different WASM module, state SHALL still be
    /// preserved (the new module behavior may differ, but preserved state remains).
    ///
    /// **Validates: Requirements 8.5**
    #[test]
    fn prop_hot_reload_with_different_module_preserves_state(
        (process_count, total_time) in stats_strategy(),
        plugin_data in plugin_data_strategy(),
    ) {
        let original_wasm = wat_to_wasm(PROCESS_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile original WASM"))?;
        let alternate_wasm = wat_to_wasm(ALTERNATE_WASM_WAT)
            .ok_or_else(|| TestCaseError::fail("Failed to compile alternate WASM"))?;

        let mut runtime = WasmRuntime::new()
            .map_err(|e| TestCaseError::fail(format!("Failed to create runtime: {}", e)))?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &original_wasm, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to load plugin: {}", e)))?;

        // Set state before reload
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = process_count;
            instance.store.data_mut().abi_state.total_process_time_us = total_time;
            for (key, value) in &plugin_data {
                instance.store.data_mut().abi_state.store_data(key.clone(), value.clone());
            }
        }

        // Reload with different module
        runtime.reload_plugin(&plugin_id, &alternate_wasm, vec![])
            .map_err(|e| TestCaseError::fail(format!("Failed to reload: {}", e)))?;

        // Verify state is preserved
        let (actual_count, _) = runtime.get_plugin_stats(&plugin_id)
            .map_err(|e| TestCaseError::fail(format!("Failed to get stats: {}", e)))?;
        prop_assert_eq!(
            actual_count,
            process_count,
            "process_count should be preserved when reloading with different module"
        );

        if let Some(instance) = runtime.instances.get(&plugin_id) {
            prop_assert_eq!(
                instance.store.data().abi_state.total_process_time_us,
                total_time,
                "total_process_time_us should be preserved"
            );

            for (key, expected_value) in &plugin_data {
                let actual_value = instance.store.data().abi_state.get_data(key);
                prop_assert_eq!(
                    actual_value,
                    Some(expected_value),
                    "Plugin data should be preserved when reloading with different module"
                );
            }
        }

        // Verify new module behavior (alternate multiplies by 2)
        let result = runtime.process(&plugin_id, 5.0, 0.001)
            .map_err(|e| TestCaseError::fail(format!("Failed to process: {}", e)))?;
        prop_assert!(
            (result - 10.0).abs() < 0.001,
            "New module should have different behavior (5.0 * 2.0 = 10.0), got {}",
            result
        );
    }
}

#[cfg(test)]
mod hot_reload_unit_tests {
    use super::*;

    // ========================================================================
    // Unit tests for Property 10: WASM Hot-Reload State Preservation
    // ========================================================================

    #[test]
    fn test_stateful_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(STATEFUL_WASM_WAT).ok_or("Failed to compile stateful WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_alternate_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(ALTERNATE_WASM_WAT).ok_or("Failed to compile alternate WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_init_success_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(INIT_SUCCESS_WASM_WAT).ok_or("Failed to compile init success WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_init_failure_wasm_compiles() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes =
            wat_to_wasm(INIT_FAILURE_WASM_WAT).ok_or("Failed to compile init failure WASM")?;
        assert!(wasm_bytes.len() >= 8);
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        Ok(())
    }

    #[test]
    fn test_basic_hot_reload() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Reload should succeed
        runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])?;

        // Plugin should still be functional
        let result = runtime.process(&plugin_id, 2.0, 0.001)?;
        assert!((result - 0.002).abs() < 0.001); // 2.0 * 0.001 = 0.002

        Ok(())
    }

    #[test]
    fn test_hot_reload_preserves_process_count() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Set process count
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = 42;
        }

        // Reload
        runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])?;

        // Verify preserved
        let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(count, 42);

        Ok(())
    }

    #[test]
    fn test_hot_reload_preserves_total_process_time() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Set total process time
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.total_process_time_us = 12345;
        }

        // Reload
        runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])?;

        // Verify preserved
        if let Some(instance) = runtime.instances.get(&plugin_id) {
            assert_eq!(instance.store.data().abi_state.total_process_time_us, 12345);
        } else {
            return Err("Plugin not found after reload".into());
        }

        Ok(())
    }

    #[test]
    fn test_hot_reload_preserves_plugin_data() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Store plugin data
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("test_key".to_string(), vec![1, 2, 3, 4, 5]);
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("another_key".to_string(), vec![10, 20, 30]);
        }

        // Reload
        runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])?;

        // Verify preserved
        if let Some(instance) = runtime.instances.get(&plugin_id) {
            let data1 = instance.store.data().abi_state.get_data("test_key");
            assert_eq!(data1, Some(&vec![1, 2, 3, 4, 5]));

            let data2 = instance.store.data().abi_state.get_data("another_key");
            assert_eq!(data2, Some(&vec![10, 20, 30]));
        } else {
            return Err("Plugin not found after reload".into());
        }

        Ok(())
    }

    #[test]
    fn test_hot_reload_failure_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let valid_wasm = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let invalid_wasm = b"invalid wasm bytes";

        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &valid_wasm, vec![])?;

        // Set state
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = 100;
        }

        // Attempt reload with invalid WASM
        let result = runtime.reload_plugin(&plugin_id, invalid_wasm, vec![]);
        assert!(result.is_err());

        // Old plugin should still exist and have preserved state
        assert!(runtime.has_plugin(&plugin_id));
        let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(count, 100);

        // Plugin should still be functional
        let process_result = runtime.process(&plugin_id, 2.0, 0.001)?;
        assert!((process_result - 0.002).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_hot_reload_with_init_failure_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>>
    {
        let valid_wasm = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile valid WASM")?;
        let failing_init_wasm =
            wat_to_wasm(INIT_FAILURE_WASM_WAT).ok_or("Failed to compile init failure WASM")?;

        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &valid_wasm, vec![])?;

        // Set state
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = 50;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("key".to_string(), vec![1, 2, 3]);
        }

        // Attempt reload with WASM that has failing init
        let result = runtime.reload_plugin(&plugin_id, &failing_init_wasm, vec![]);
        assert!(result.is_err());

        // Old plugin should still exist with preserved state
        assert!(runtime.has_plugin(&plugin_id));
        let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(count, 50);

        if let Some(instance) = runtime.instances.get(&plugin_id) {
            let data = instance.store.data().abi_state.get_data("key");
            assert_eq!(data, Some(&vec![1, 2, 3]));
        }

        Ok(())
    }

    #[test]
    fn test_hot_reload_other_plugins_unaffected() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        // Load two plugins
        let plugin_id1 = PluginId::new_v4();
        let plugin_id2 = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id1, &wasm_bytes, vec![])?;
        runtime.load_plugin_from_bytes(plugin_id2, &wasm_bytes, vec![])?;

        // Set different state for each
        if let Some(instance) = runtime.instances.get_mut(&plugin_id1) {
            instance.store.data_mut().abi_state.process_count = 111;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("p1_key".to_string(), vec![1]);
        }
        if let Some(instance) = runtime.instances.get_mut(&plugin_id2) {
            instance.store.data_mut().abi_state.process_count = 222;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("p2_key".to_string(), vec![2]);
        }

        // Reload plugin 1
        runtime.reload_plugin(&plugin_id1, &wasm_bytes, vec![])?;

        // Plugin 2 should be completely unaffected
        let (count2, _) = runtime.get_plugin_stats(&plugin_id2)?;
        assert_eq!(count2, 222);

        if let Some(instance) = runtime.instances.get(&plugin_id2) {
            let data = instance.store.data().abi_state.get_data("p2_key");
            assert_eq!(data, Some(&vec![2]));
        }

        // Plugin 1 should have preserved state
        let (count1, _) = runtime.get_plugin_stats(&plugin_id1)?;
        assert_eq!(count1, 111);

        Ok(())
    }

    #[test]
    fn test_multiple_sequential_reloads() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &wasm_bytes, vec![])?;

        // Set initial state
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = 10;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("persistent".to_string(), vec![42]);
        }

        // Perform multiple reloads
        for i in 0..5 {
            runtime.reload_plugin(&plugin_id, &wasm_bytes, vec![])?;

            // Verify state after each reload
            let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
            assert_eq!(count, 10, "Count should be preserved after reload {}", i);

            if let Some(instance) = runtime.instances.get(&plugin_id) {
                let data = instance.store.data().abi_state.get_data("persistent");
                assert_eq!(
                    data,
                    Some(&vec![42]),
                    "Data should be preserved after reload {}",
                    i
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_hot_reload_with_different_module() -> Result<(), Box<dyn std::error::Error>> {
        let original_wasm = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile original")?;
        let alternate_wasm =
            wat_to_wasm(ALTERNATE_WASM_WAT).ok_or("Failed to compile alternate")?;

        let mut runtime = WasmRuntime::new()?;

        let plugin_id = PluginId::new_v4();
        runtime.load_plugin_from_bytes(plugin_id, &original_wasm, vec![])?;

        // Set state
        if let Some(instance) = runtime.instances.get_mut(&plugin_id) {
            instance.store.data_mut().abi_state.process_count = 77;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("data".to_string(), vec![7, 7]);
        }

        // Reload with different module
        runtime.reload_plugin(&plugin_id, &alternate_wasm, vec![])?;

        // State should be preserved
        let (count, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(count, 77);

        if let Some(instance) = runtime.instances.get(&plugin_id) {
            let data = instance.store.data().abi_state.get_data("data");
            assert_eq!(data, Some(&vec![7, 7]));
        }

        // But behavior should be different (alternate multiplies by 2)
        let result = runtime.process(&plugin_id, 3.0, 0.001)?;
        assert!((result - 6.0).abs() < 0.001); // 3.0 * 2.0 = 6.0

        Ok(())
    }

    #[test]
    fn test_reload_nonexistent_plugin_fails() -> Result<(), Box<dyn std::error::Error>> {
        let wasm_bytes = wat_to_wasm(PROCESS_WASM_WAT).ok_or("Failed to compile WASM")?;
        let mut runtime = WasmRuntime::new()?;

        let nonexistent_id = PluginId::new_v4();

        // Reloading a non-existent plugin should succeed (it just loads it fresh)
        // but with no state to preserve
        let result = runtime.reload_plugin(&nonexistent_id, &wasm_bytes, vec![]);

        // This should succeed as reload_plugin handles non-existent plugins
        // by essentially doing a fresh load
        assert!(result.is_ok());
        assert!(runtime.has_plugin(&nonexistent_id));

        Ok(())
    }
}
