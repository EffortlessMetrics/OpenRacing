#![allow(clippy::redundant_closure)]
//! Comprehensive property-based tests for the WASM plugin runtime.
//!
//! Tests cover:
//! - Resource limits property invariants
//! - Capability-based permission model
//! - Plugin lifecycle (load, init, run, teardown)
//! - Edge cases: invalid WASM, fuel exhaustion, trapping plugins

use openracing_wasm_runtime::{ResourceLimits, WasmError, WasmRuntime};
use proptest::prelude::*;
use uuid::Uuid;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// WAT module helpers
// ---------------------------------------------------------------------------

fn parse_wat(wat: &str) -> Result<Vec<u8>, WasmError> {
    wat::parse_str(wat).map_err(|e| WasmError::InvalidModule(e.to_string()))
}

fn minimal_plugin_wasm() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )"#,
    )
}

fn init_plugin_wasm() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const 0
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
                local.get 1
                f32.add
            )
        )"#,
    )
}

fn trap_plugin_wasm() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "process") (param f32 f32) (result f32)
                unreachable
            )
        )"#,
    )
}

fn shutdown_plugin_wasm() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (global $state (mut i32) (i32.const 0))
            (func (export "init") (result i32)
                i32.const 1
                global.set $state
                i32.const 0
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
            (func (export "shutdown")
                i32.const 0
                global.set $state
            )
        )"#,
    )
}

fn init_fail_plugin_wasm() -> Result<Vec<u8>, WasmError> {
    parse_wat(
        r#"(module
            (memory (export "memory") 1)
            (func (export "init") (result i32)
                i32.const -1
            )
            (func (export "process") (param f32 f32) (result f32)
                local.get 0
            )
        )"#,
    )
}

// ---------------------------------------------------------------------------
// Resource limits proptest
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn prop_valid_memory_limits_pass_validation(
        memory in 64 * 1024usize..=4 * 1024 * 1024 * 1024usize,
    ) {
        let limits = ResourceLimits::default().with_memory(memory);
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn prop_valid_fuel_limits_pass_validation(
        fuel in 1000u64..=10_000_000_000u64,
    ) {
        let limits = ResourceLimits::default().with_fuel(fuel);
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn prop_valid_instance_limits_pass_validation(
        instances in 1usize..=1000usize,
    ) {
        let limits = ResourceLimits::default().with_max_instances(instances);
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn prop_too_small_memory_fails_validation(
        memory in 0usize..64 * 1024usize,
    ) {
        let limits = ResourceLimits::default().with_memory(memory);
        prop_assert!(limits.validate().is_err());
    }

    #[test]
    fn prop_too_small_fuel_fails_validation(
        fuel in 0u64..1000u64,
    ) {
        let limits = ResourceLimits::default().with_fuel(fuel);
        prop_assert!(limits.validate().is_err());
    }

    #[test]
    fn prop_builder_preserves_values(
        memory in 1024 * 1024usize..=64 * 1024 * 1024usize,
        fuel in 1_000_000u64..=50_000_000u64,
        instances in 1usize..=128usize,
        table_elements in 1u32..=50_000u32,
    ) {
        let limits = ResourceLimits::default()
            .with_memory(memory)
            .with_fuel(fuel)
            .with_max_instances(instances)
            .with_table_elements(table_elements);

        prop_assert_eq!(limits.max_memory_bytes, memory);
        prop_assert_eq!(limits.max_fuel, fuel);
        prop_assert_eq!(limits.max_instances, instances);
        prop_assert_eq!(limits.max_table_elements, table_elements);
    }

    #[test]
    fn prop_conservative_limits_always_valid(_seed in any::<u64>()) {
        let limits = ResourceLimits::conservative();
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn prop_generous_limits_always_valid(_seed in any::<u64>()) {
        let limits = ResourceLimits::generous();
        prop_assert!(limits.validate().is_ok());
    }
}

// ---------------------------------------------------------------------------
// Capability-based permission model
// ---------------------------------------------------------------------------

mod capability_tests {
    use openracing_wasm_runtime::state::CapabilityChecker;

    #[test]
    fn test_empty_capabilities_deny_everything() {
        let checker = CapabilityChecker::new(vec![]);
        assert!(checker.check_telemetry_read().is_err());
        assert!(checker.check_telemetry_modify().is_err());
        assert!(checker.check_led_control().is_err());
        assert!(checker.check_dsp_processing().is_err());
    }

    #[test]
    fn test_telemetry_read_only() {
        let checker = CapabilityChecker::new(vec!["read_telemetry".to_string()]);
        assert!(checker.check_telemetry_read().is_ok());
        assert!(checker.check_telemetry_modify().is_err());
        assert!(checker.check_led_control().is_err());
        assert!(checker.check_dsp_processing().is_err());
    }

    #[test]
    fn test_multiple_capabilities() {
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
    fn test_has_capability_check() {
        let checker = CapabilityChecker::new(vec!["process_dsp".to_string()]);
        assert!(checker.has_capability("process_dsp"));
        assert!(!checker.has_capability("read_telemetry"));
        assert!(!checker.has_capability("nonexistent"));
    }

    #[test]
    fn test_capability_violation_error_type() {
        let checker = CapabilityChecker::new(vec![]);
        let err = checker.check_telemetry_read().err();
        assert!(err.is_some());
        let err = err.as_ref().map(|e| e.is_capability_violation());
        assert_eq!(err, Some(true));
    }
}

// ---------------------------------------------------------------------------
// Plugin lifecycle tests
// ---------------------------------------------------------------------------

mod lifecycle_tests {
    use super::*;

    #[test]
    fn test_load_minimal_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = minimal_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        assert!(runtime.has_plugin(&id));
        assert_eq!(runtime.instance_count(), 1);
        Ok(())
    }

    #[test]
    fn test_load_plugin_with_init() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = init_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        assert!(runtime.is_plugin_initialized(&id)?);
        Ok(())
    }

    #[test]
    fn test_process_passthrough() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = minimal_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        let result = runtime.process(&id, 0.5, 0.001)?;
        assert!((result - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn test_process_adding_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = init_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        let result = runtime.process(&id, 1.0, 0.5)?;
        assert!((result - 1.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn test_unload_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = minimal_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        assert!(runtime.has_plugin(&id));

        runtime.unload_plugin(&id)?;
        assert!(!runtime.has_plugin(&id));
        assert_eq!(runtime.instance_count(), 0);
        Ok(())
    }

    #[test]
    fn test_full_lifecycle_with_shutdown() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = shutdown_plugin_wasm()?;

        // Load (with init)
        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        assert!(runtime.is_plugin_initialized(&id)?);

        // Run
        let result = runtime.process(&id, 1.0, 0.001)?;
        assert!((result - 1.0).abs() < f32::EPSILON);

        // Unload (calls shutdown)
        runtime.unload_plugin(&id)?;
        assert!(!runtime.has_plugin(&id));
        Ok(())
    }

    #[test]
    fn test_init_failure_rejects_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = init_fail_plugin_wasm()?;

        let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
        assert!(result.is_err());
        assert!(!runtime.has_plugin(&id));
        Ok(())
    }

    #[test]
    fn test_plugin_stats_increment() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = minimal_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;

        let (count_before, _) = runtime.get_plugin_stats(&id)?;
        assert_eq!(count_before, 0);

        runtime.process(&id, 1.0, 0.001)?;
        runtime.process(&id, 2.0, 0.001)?;

        let (count_after, _) = runtime.get_plugin_stats(&id)?;
        assert_eq!(count_after, 2);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge case tests
// ---------------------------------------------------------------------------

mod edge_cases {
    use super::*;

    #[test]
    fn test_invalid_wasm_bytes_rejected() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();

        let result = runtime.load_plugin_from_bytes(id, b"not wasm at all", vec![]);
        assert!(result.is_err());
        assert!(!runtime.has_plugin(&id));
        Ok(())
    }

    #[test]
    fn test_empty_wasm_bytes_rejected() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();

        let result = runtime.load_plugin_from_bytes(id, &[], vec![]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_wasm_missing_process_export_rejected() -> TestResult {
        let wasm = parse_wat(
            r#"(module
                (memory (export "memory") 1)
                (func (export "not_process") (param f32 f32) (result f32)
                    local.get 0
                )
            )"#,
        )?;

        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();

        let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_wasm_missing_memory_export_rejected() -> TestResult {
        let wasm = parse_wat(
            r#"(module
                (memory 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
            )"#,
        )?;

        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();

        let result = runtime.load_plugin_from_bytes(id, &wasm, vec![]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_trap_disables_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = trap_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        let result = runtime.process(&id, 1.0, 0.001);
        assert!(result.is_err());

        assert!(runtime.is_plugin_disabled(&id)?);

        // Calling again also fails
        let result2 = runtime.process(&id, 1.0, 0.001);
        assert!(result2.is_err());
        Ok(())
    }

    #[test]
    fn test_re_enable_disabled_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();
        let wasm = trap_plugin_wasm()?;

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        let _ = runtime.process(&id, 1.0, 0.001);
        assert!(runtime.is_plugin_disabled(&id)?);

        let was_disabled = runtime.re_enable_plugin(&id)?;
        assert!(was_disabled);
        assert!(!runtime.is_plugin_disabled(&id)?);
        Ok(())
    }

    #[test]
    fn test_process_nonexistent_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();

        let result = runtime.process(&id, 1.0, 0.001);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_unload_nonexistent_plugin() -> TestResult {
        let mut runtime = WasmRuntime::new()?;
        let id = Uuid::new_v4();

        let result = runtime.unload_plugin(&id);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_max_instances_enforced() -> TestResult {
        let limits = ResourceLimits::default().with_max_instances(2);
        let mut runtime = WasmRuntime::with_limits(limits)?;
        let wasm = minimal_plugin_wasm()?;

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        runtime.load_plugin_from_bytes(id1, &wasm, vec![])?;
        runtime.load_plugin_from_bytes(id2, &wasm, vec![])?;

        let result = runtime.load_plugin_from_bytes(id3, &wasm, vec![]);
        assert!(result.is_err());
        assert_eq!(runtime.instance_count(), 2);
        Ok(())
    }

    #[test]
    fn test_fuel_exhaustion_detected() -> TestResult {
        // Module with a tight loop that will exhaust fuel
        let wasm = parse_wat(
            r#"(module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    (local $i i32)
                    (loop $loop
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br_if $loop (i32.lt_u (local.get $i) (i32.const 999999999)))
                    )
                    local.get 0
                )
            )"#,
        )?;

        let limits = ResourceLimits::default().with_fuel(1000);
        let mut runtime = WasmRuntime::with_limits(limits)?;
        let id = Uuid::new_v4();

        runtime.load_plugin_from_bytes(id, &wasm, vec![])?;
        let result = runtime.process(&id, 1.0, 0.001);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_runtime_creation_default() -> TestResult {
        let runtime = WasmRuntime::new()?;
        assert_eq!(runtime.instance_count(), 0);
        assert_eq!(runtime.resource_limits().max_memory_bytes, 16 * 1024 * 1024);
        Ok(())
    }

    #[test]
    fn test_runtime_creation_custom_limits() -> TestResult {
        let limits = ResourceLimits::conservative();
        let runtime = WasmRuntime::with_limits(limits)?;
        assert_eq!(runtime.resource_limits().max_memory_bytes, 4 * 1024 * 1024);
        assert_eq!(runtime.resource_limits().max_fuel, 1_000_000);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Hot reload tests
// ---------------------------------------------------------------------------

mod hot_reload_tests {
    use openracing_wasm_runtime::hot_reload::{HotReloader, PreservedPluginState};
    use std::collections::HashMap;

    #[test]
    fn test_preserved_state_empty() {
        let state = PreservedPluginState::new();
        assert!(state.is_empty());
        assert_eq!(state.process_count, 0);
        assert!((state.average_process_time_us() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_preserved_state_with_data() {
        let mut data = HashMap::new();
        data.insert("key".to_string(), vec![1, 2, 3]);

        let state = PreservedPluginState {
            plugin_data: data,
            process_count: 100,
            total_process_time_us: 5000,
        };

        assert!(!state.is_empty());
        assert!((state.average_process_time_us() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hot_reloader_stats() {
        let mut reloader = HotReloader::new();
        assert_eq!(reloader.total_attempts(), 0);
        assert!((reloader.success_rate() - 100.0).abs() < f64::EPSILON);

        reloader.record_success();
        reloader.record_success();
        reloader.record_failure();

        assert_eq!(reloader.reload_count(), 2);
        assert_eq!(reloader.failed_reload_count(), 1);
        assert_eq!(reloader.total_attempts(), 3);

        reloader.reset_stats();
        assert_eq!(reloader.total_attempts(), 0);
    }
}

// ---------------------------------------------------------------------------
// ABI state tests
// ---------------------------------------------------------------------------

mod abi_state_tests {
    use openracing_wasm_runtime::state::WasmPluginAbiState;

    #[test]
    fn test_abi_state_lifecycle() {
        let mut state = WasmPluginAbiState::new();
        assert!(!state.is_initialized());

        state.mark_initialized();
        assert!(state.is_initialized());

        state.mark_shutdown();
        assert!(!state.is_initialized());
    }

    #[test]
    fn test_abi_state_failure() {
        let mut state = WasmPluginAbiState::new();
        state.mark_failed("test error".to_string());
        assert!(!state.is_initialized());
        assert_eq!(state.last_error, Some("test error".to_string()));
    }

    #[test]
    fn test_abi_state_data_storage() {
        let mut state = WasmPluginAbiState::new();
        state.store_data("key1".to_string(), vec![1, 2, 3]);
        assert_eq!(state.get_data("key1"), Some(&vec![1, 2, 3]));
        assert_eq!(state.get_data("missing"), None);

        state.remove_data("key1");
        assert_eq!(state.get_data("key1"), None);

        state.store_data("a".to_string(), vec![10]);
        state.store_data("b".to_string(), vec![20]);
        state.clear_data();
        assert_eq!(state.get_data("a"), None);
    }

    #[test]
    fn test_abi_state_stats() {
        let mut state = WasmPluginAbiState::new();
        assert!((state.average_process_time_us() - 0.0).abs() < f64::EPSILON);

        state.record_process_call(100);
        state.record_process_call(200);
        assert_eq!(state.process_count, 2);
        assert!((state.average_process_time_us() - 150.0).abs() < f64::EPSILON);

        state.reset_stats();
        assert_eq!(state.process_count, 0);
    }
}

// ---------------------------------------------------------------------------
// Error type tests
// ---------------------------------------------------------------------------

mod error_tests {
    use openracing_wasm_runtime::WasmError;
    use std::time::Duration;

    #[test]
    fn test_error_classification() {
        let crash = WasmError::crashed("test");
        assert!(crash.is_crash());
        assert!(!crash.is_timeout());
        assert!(!crash.is_budget_violation());
        assert!(!crash.is_capability_violation());

        let timeout = WasmError::ExecutionTimeout {
            duration: Duration::from_secs(1),
        };
        assert!(timeout.is_timeout());
        assert!(!timeout.is_crash());

        let budget = WasmError::BudgetViolation {
            used_us: 100,
            budget_us: 50,
        };
        assert!(budget.is_budget_violation());

        let cap = WasmError::CapabilityViolation {
            capability: "test".to_string(),
        };
        assert!(cap.is_capability_violation());
    }

    #[test]
    fn test_error_display() {
        let err = WasmError::plugin_not_found("abc-123");
        let msg = format!("{err}");
        assert!(msg.contains("abc-123"));

        let err = WasmError::loading_failed("bad module");
        let msg = format!("{err}");
        assert!(msg.contains("bad module"));
    }
}
