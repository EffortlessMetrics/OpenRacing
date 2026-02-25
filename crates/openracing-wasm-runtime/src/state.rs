//! WASM plugin state management.
//!
//! This module provides the state structure that is passed to WASM plugins
//! during execution, including WASI context and ABI state.

use std::collections::HashMap;
use std::time::Instant;

use openracing_plugin_abi::{PluginInitStatus, TelemetryFrame};
use wasmtime_wasi::p1::WasiP1Ctx;

/// Capability checker for WASM plugins.
///
/// This struct is used to check if a plugin has specific capabilities
/// granted by the host.
#[derive(Debug, Clone)]
pub struct CapabilityChecker {
    capabilities: Vec<String>,
}

impl CapabilityChecker {
    /// Create a new capability checker with the given capabilities.
    #[must_use]
    pub fn new(capabilities: Vec<String>) -> Self {
        Self { capabilities }
    }

    /// Check if telemetry read access is allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not granted.
    pub fn check_telemetry_read(&self) -> crate::WasmResult<()> {
        if self.capabilities.contains(&"read_telemetry".to_string()) {
            Ok(())
        } else {
            Err(crate::WasmError::CapabilityViolation {
                capability: "read_telemetry".to_string(),
            })
        }
    }

    /// Check if telemetry modification is allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not granted.
    pub fn check_telemetry_modify(&self) -> crate::WasmResult<()> {
        if self.capabilities.contains(&"modify_telemetry".to_string()) {
            Ok(())
        } else {
            Err(crate::WasmError::CapabilityViolation {
                capability: "modify_telemetry".to_string(),
            })
        }
    }

    /// Check if LED control is allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not granted.
    pub fn check_led_control(&self) -> crate::WasmResult<()> {
        if self.capabilities.contains(&"control_leds".to_string()) {
            Ok(())
        } else {
            Err(crate::WasmError::CapabilityViolation {
                capability: "control_leds".to_string(),
            })
        }
    }

    /// Check if DSP processing is allowed.
    ///
    /// # Errors
    ///
    /// Returns an error if the capability is not granted.
    pub fn check_dsp_processing(&self) -> crate::WasmResult<()> {
        if self.capabilities.contains(&"process_dsp".to_string()) {
            Ok(())
        } else {
            Err(crate::WasmError::CapabilityViolation {
                capability: "process_dsp".to_string(),
            })
        }
    }

    /// Check if a specific capability is granted.
    #[must_use]
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.contains(&capability.to_string())
    }
}

impl Default for CapabilityChecker {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

/// Per-plugin ABI state for WASM plugins.
///
/// This struct maintains state that persists across plugin calls,
/// including initialization status, custom data storage, and statistics.
/// This is separate from WASI context.
#[derive(Debug)]
pub struct WasmPluginAbiState {
    /// Plugin initialization status
    pub init_status: PluginInitStatus,
    /// Custom plugin data storage (key-value pairs)
    pub plugin_data: HashMap<String, Vec<u8>>,
    /// Current telemetry frame (updated by host before process calls)
    pub current_telemetry: TelemetryFrame,
    /// Plugin start time for timestamp calculations
    pub start_time: Instant,
    /// Number of successful process() calls
    pub process_count: u64,
    /// Total processing time in microseconds
    pub total_process_time_us: u64,
    /// Last error message (if any)
    pub last_error: Option<String>,
}

impl Default for WasmPluginAbiState {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmPluginAbiState {
    /// Create a new plugin state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            init_status: PluginInitStatus::Uninitialized,
            plugin_data: HashMap::new(),
            current_telemetry: TelemetryFrame::default(),
            start_time: Instant::now(),
            process_count: 0,
            total_process_time_us: 0,
            last_error: None,
        }
    }

    /// Check if the plugin is initialized.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.init_status == PluginInitStatus::Initialized
    }

    /// Mark the plugin as initialized.
    pub fn mark_initialized(&mut self) {
        self.init_status = PluginInitStatus::Initialized;
    }

    /// Mark the plugin as failed with an error message.
    pub fn mark_failed(&mut self, error: String) {
        self.init_status = PluginInitStatus::Failed;
        self.last_error = Some(error);
    }

    /// Mark the plugin as shut down.
    pub fn mark_shutdown(&mut self) {
        self.init_status = PluginInitStatus::ShutDown;
    }

    /// Get the current timestamp in microseconds since plugin start.
    #[must_use]
    pub fn timestamp_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    /// Update telemetry data.
    pub fn update_telemetry(&mut self, telemetry: TelemetryFrame) {
        self.current_telemetry = telemetry;
    }

    /// Record a successful process call.
    pub fn record_process_call(&mut self, duration_us: u64) {
        self.process_count += 1;
        self.total_process_time_us += duration_us;
    }

    /// Get average processing time in microseconds.
    #[must_use]
    pub fn average_process_time_us(&self) -> f64 {
        if self.process_count == 0 {
            0.0
        } else {
            self.total_process_time_us as f64 / self.process_count as f64
        }
    }

    /// Store custom plugin data.
    pub fn store_data(&mut self, key: String, data: Vec<u8>) {
        self.plugin_data.insert(key, data);
    }

    /// Retrieve custom plugin data.
    #[must_use]
    pub fn get_data(&self, key: &str) -> Option<&Vec<u8>> {
        self.plugin_data.get(key)
    }

    /// Remove custom plugin data.
    pub fn remove_data(&mut self, key: &str) -> Option<Vec<u8>> {
        self.plugin_data.remove(key)
    }

    /// Clear all custom plugin data.
    pub fn clear_data(&mut self) {
        self.plugin_data.clear();
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.process_count = 0;
        self.total_process_time_us = 0;
    }
}

/// WASM plugin state.
///
/// This struct combines WASI context, capability checking, and ABI state
/// for a complete plugin execution environment.
pub struct WasmPluginState {
    /// WASI context for system calls
    pub wasi: WasiP1Ctx,
    /// Capability checker for permission validation
    pub capability_checker: CapabilityChecker,
    /// ABI-level plugin state (telemetry, stats, custom data)
    pub abi_state: WasmPluginAbiState,
}

impl WasmPluginState {
    /// Create a new plugin state with the given WASI context.
    #[must_use]
    pub fn new(wasi: WasiP1Ctx) -> Self {
        Self {
            wasi,
            capability_checker: CapabilityChecker::default(),
            abi_state: WasmPluginAbiState::new(),
        }
    }

    /// Create a new plugin state with WASI context and capabilities.
    #[must_use]
    pub fn with_capabilities(wasi: WasiP1Ctx, capabilities: Vec<String>) -> Self {
        Self {
            wasi,
            capability_checker: CapabilityChecker::new(capabilities),
            abi_state: WasmPluginAbiState::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_checker() {
        let checker = CapabilityChecker::new(vec!["read_telemetry".to_string()]);
        assert!(checker.check_telemetry_read().is_ok());
        assert!(checker.check_telemetry_modify().is_err());
        assert!(checker.has_capability("read_telemetry"));
        assert!(!checker.has_capability("modify_telemetry"));
    }

    #[test]
    fn test_wasm_plugin_abi_state() {
        let mut state = WasmPluginAbiState::new();
        assert!(!state.is_initialized());
        assert_eq!(state.process_count, 0);

        state.mark_initialized();
        assert!(state.is_initialized());

        state.store_data("key".to_string(), vec![1, 2, 3]);
        assert_eq!(state.get_data("key"), Some(&vec![1, 2, 3]));

        state.record_process_call(100);
        state.record_process_call(200);
        assert_eq!(state.process_count, 2);
        assert_eq!(state.total_process_time_us, 300);
        assert!((state.average_process_time_us() - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_wasm_plugin_abi_state_failure() {
        let mut state = WasmPluginAbiState::new();
        state.mark_failed("Test error".to_string());
        assert_eq!(state.init_status, PluginInitStatus::Failed);
        assert_eq!(state.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_wasm_plugin_abi_state_shutdown() {
        let mut state = WasmPluginAbiState::new();
        state.mark_initialized();
        state.mark_shutdown();
        assert_eq!(state.init_status, PluginInitStatus::ShutDown);
    }
}
