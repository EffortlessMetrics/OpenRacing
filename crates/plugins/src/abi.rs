//! Plugin ABI definitions with versioning and endianness.
//!
//! This module re-exports types from `openracing_plugin_abi` and adds
//! runtime-specific state management for WASM plugins.
//!
//! # WASM Plugin ABI
//!
//! WASM plugins must conform to the following ABI contract:
//!
//! ## Required Exports
//! - `process(input: f32, dt: f32) -> f32` - Main DSP processing function
//! - `memory` - Linear memory export for host function communication
//!
//! ## Optional Exports
//! - `init() -> i32` - Initialization function, returns 0 on success
//! - `shutdown()` - Cleanup function called before unloading
//!
//! ## Host Functions (imports from "env" module)
//! - `log_debug(msg_ptr: i32, msg_len: i32)` - Debug level logging
//! - `log_info(msg_ptr: i32, msg_len: i32)` - Info level logging
//! - `log_warn(msg_ptr: i32, msg_len: i32)` - Warning level logging
//! - `log_error(msg_ptr: i32, msg_len: i32)` - Error level logging
//! - `check_capability(cap_ptr: i32, cap_len: i32) -> i32` - Check if capability is granted
//! - `get_telemetry(out_ptr: i32, out_len: i32) -> i32` - Get current telemetry data
//! - `get_timestamp_us() -> i64` - Get current timestamp in microseconds

pub use openracing_plugin_abi::*;

use std::collections::HashMap;
use std::time::Instant;

/// Per-plugin state for WASM plugins (ABI-level state).
///
/// This struct maintains state that persists across plugin calls,
/// including initialization status, custom data storage, and statistics.
/// This is the ABI-level state that is separate from WASI context.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_header_size_and_alignment() {
        assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
        assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
    }

    #[test]
    fn test_telemetry_frame_size_and_alignment() {
        assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
    }

    #[test]
    fn test_plugin_capabilities_bitflags() {
        let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        assert_eq!(caps.bits(), 0b0000_0011);

        let caps_with_haptics = caps | PluginCapabilities::HAPTICS;
        assert_eq!(caps_with_haptics.bits(), 0b0000_0111);

        let valid_caps =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        assert_eq!(valid_caps.bits() & PluginCapabilities::RESERVED.bits(), 0);
    }

    #[test]
    fn test_plugin_header_byte_exact_serialization() {
        let header = PluginHeader {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: (PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS).bits(),
            reserved: 0,
        };

        let bytes = header.to_bytes();

        let expected_bytes = [
            0x31, 0x4C, 0x57, 0x57, 0x00, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];

        assert_eq!(bytes, expected_bytes);

        let restored_header = PluginHeader::from_bytes(&bytes);
        assert_eq!(header, restored_header);
    }

    #[test]
    fn test_plugin_header_validation() {
        let valid_header = PluginHeader::default();
        assert!(valid_header.is_valid());

        let invalid_magic = PluginHeader {
            magic: 0xDEADBEEF,
            ..Default::default()
        };
        assert!(!invalid_magic.is_valid());

        let invalid_version = PluginHeader {
            abi_version: 0x00020000,
            ..Default::default()
        };
        assert!(!invalid_version.is_valid());
    }

    #[test]
    fn test_telemetry_frame_byte_serialization() {
        let frame = TelemetryFrame {
            timestamp_us: 1234567890,
            wheel_angle_deg: 45.5,
            wheel_speed_rad_s: std::f32::consts::PI,
            temperature_c: 65.0,
            fault_flags: 0x12345678,
            _pad: 0,
        };

        let bytes = frame.to_bytes();
        let restored_frame = TelemetryFrame::from_bytes(&bytes);

        assert_eq!(frame.timestamp_us, restored_frame.timestamp_us);
        assert_eq!(frame.wheel_angle_deg, restored_frame.wheel_angle_deg);
        assert_eq!(frame.wheel_speed_rad_s, restored_frame.wheel_speed_rad_s);
        assert_eq!(frame.temperature_c, restored_frame.temperature_c);
        assert_eq!(frame.fault_flags, restored_frame.fault_flags);
        assert_eq!(frame._pad, restored_frame._pad);
    }

    #[test]
    fn test_endianness_documentation() {
        let test_value: u32 = 0x12345678;
        let le_bytes = test_value.to_le_bytes();

        let expected_le = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(le_bytes, expected_le);

        let restored = u32::from_le_bytes(le_bytes);
        assert_eq!(restored, test_value);
    }

    #[test]
    fn test_abi_constants() {
        assert_eq!(PLUG_ABI_VERSION, 0x0001_0000);
        assert_eq!(PLUG_ABI_MAGIC, 0x57574C31);
    }

    #[test]
    fn test_capability_flags_reserved_bits() {
        let reserved_mask = PluginCapabilities::RESERVED.bits();
        let valid_mask = (PluginCapabilities::TELEMETRY
            | PluginCapabilities::LEDS
            | PluginCapabilities::HAPTICS)
            .bits();

        assert_eq!(reserved_mask & valid_mask, 0);
        assert_eq!(reserved_mask | valid_mask, 0xFFFF_FFFF);
    }

    #[test]
    fn test_wasm_abi_version() {
        assert_eq!(WASM_ABI_VERSION, 1);
    }

    #[test]
    fn test_log_level_constants() {
        const _: () = assert!(log_level::ERROR < log_level::WARN);
        const _: () = assert!(log_level::WARN < log_level::INFO);
        const _: () = assert!(log_level::INFO < log_level::DEBUG);
        const _: () = assert!(log_level::DEBUG < log_level::TRACE);
    }

    #[test]
    fn test_return_code_constants() {
        assert_eq!(return_code::SUCCESS, 0);
        const _: () = assert!(return_code::ERROR < 0);
        const _: () = assert!(return_code::INVALID_ARG < 0);
        const _: () = assert!(return_code::PERMISSION_DENIED < 0);
        const _: () = assert!(return_code::BUFFER_TOO_SMALL < 0);
        const _: () = assert!(return_code::NOT_INITIALIZED < 0);
    }

    #[test]
    fn test_plugin_init_status_default() {
        let status = PluginInitStatus::default();
        assert_eq!(status, PluginInitStatus::Uninitialized);
    }

    #[test]
    fn test_wasm_plugin_state_new() {
        let state = WasmPluginAbiState::new();
        assert_eq!(state.init_status, PluginInitStatus::Uninitialized);
        assert!(!state.is_initialized());
        assert!(state.plugin_data.is_empty());
        assert_eq!(state.process_count, 0);
        assert_eq!(state.total_process_time_us, 0);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn test_wasm_plugin_state_initialization() {
        let mut state = WasmPluginAbiState::new();

        assert!(!state.is_initialized());

        state.mark_initialized();
        assert!(state.is_initialized());
        assert_eq!(state.init_status, PluginInitStatus::Initialized);
    }

    #[test]
    fn test_wasm_plugin_state_failure() {
        let mut state = WasmPluginAbiState::new();

        state.mark_failed("Test error".to_string());
        assert!(!state.is_initialized());
        assert_eq!(state.init_status, PluginInitStatus::Failed);
        assert_eq!(state.last_error, Some("Test error".to_string()));
    }

    #[test]
    fn test_wasm_plugin_state_shutdown() {
        let mut state = WasmPluginAbiState::new();
        state.mark_initialized();
        state.mark_shutdown();

        assert!(!state.is_initialized());
        assert_eq!(state.init_status, PluginInitStatus::ShutDown);
    }

    #[test]
    fn test_wasm_plugin_state_data_storage() {
        let mut state = WasmPluginAbiState::new();

        state.store_data("key1".to_string(), vec![1, 2, 3]);
        state.store_data("key2".to_string(), vec![4, 5, 6]);

        assert_eq!(state.get_data("key1"), Some(&vec![1, 2, 3]));
        assert_eq!(state.get_data("key2"), Some(&vec![4, 5, 6]));
        assert_eq!(state.get_data("nonexistent"), None);

        let removed = state.remove_data("key1");
        assert_eq!(removed, Some(vec![1, 2, 3]));
        assert_eq!(state.get_data("key1"), None);

        state.clear_data();
        assert!(state.plugin_data.is_empty());
    }

    #[test]
    fn test_wasm_plugin_state_process_stats() {
        let mut state = WasmPluginAbiState::new();

        assert_eq!(state.process_count, 0);
        assert_eq!(state.average_process_time_us(), 0.0);

        state.record_process_call(100);
        state.record_process_call(200);
        state.record_process_call(300);

        assert_eq!(state.process_count, 3);
        assert_eq!(state.total_process_time_us, 600);
        assert!((state.average_process_time_us() - 200.0).abs() < f64::EPSILON);

        state.reset_stats();
        assert_eq!(state.process_count, 0);
        assert_eq!(state.total_process_time_us, 0);
    }

    #[test]
    fn test_wasm_plugin_state_telemetry_update() {
        let mut state = WasmPluginAbiState::new();

        let telemetry = TelemetryFrame {
            timestamp_us: 12345,
            wheel_angle_deg: 90.0,
            wheel_speed_rad_s: 1.5,
            temperature_c: 45.0,
            fault_flags: 0,
            _pad: 0,
        };

        state.update_telemetry(telemetry);

        assert_eq!(state.current_telemetry.timestamp_us, 12345);
        assert_eq!(state.current_telemetry.wheel_angle_deg, 90.0);
        assert_eq!(state.current_telemetry.wheel_speed_rad_s, 1.5);
        assert_eq!(state.current_telemetry.temperature_c, 45.0);
    }

    #[test]
    fn test_wasm_export_validation_valid() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: true,
            has_init: false,
            has_shutdown: false,
            has_get_info: false,
        };

        assert!(validation.is_valid());
        assert!(validation.missing_required().is_empty());
    }

    #[test]
    fn test_wasm_export_validation_missing_process() {
        let validation = WasmExportValidation {
            has_process: false,
            has_memory: true,
            has_init: true,
            has_shutdown: true,
            has_get_info: true,
        };

        assert!(!validation.is_valid());
        let missing = validation.missing_required();
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&wasm_export::PROCESS));
    }

    #[test]
    fn test_wasm_export_validation_missing_memory() {
        let validation = WasmExportValidation {
            has_process: true,
            has_memory: false,
            has_init: true,
            has_shutdown: true,
            has_get_info: true,
        };

        assert!(!validation.is_valid());
        let missing = validation.missing_required();
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&wasm_export::MEMORY));
    }

    #[test]
    fn test_wasm_export_validation_missing_both() {
        let validation = WasmExportValidation::default();

        assert!(!validation.is_valid());
        let missing = validation.missing_required();
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&wasm_export::PROCESS));
        assert!(missing.contains(&wasm_export::MEMORY));
    }

    #[test]
    fn test_wasm_plugin_info_default() {
        let info = WasmPluginInfo::default();

        assert!(info.name.is_empty());
        assert!(info.version.is_empty());
        assert!(info.author.is_empty());
        assert!(info.description.is_empty());
        assert_eq!(info.abi_version, WASM_ABI_VERSION);
    }

    #[test]
    fn test_host_module_name() {
        assert_eq!(HOST_MODULE, "env");
    }

    #[test]
    fn test_capability_strings() {
        assert_eq!(capability_str::READ_TELEMETRY, "read_telemetry");
        assert_eq!(capability_str::MODIFY_TELEMETRY, "modify_telemetry");
        assert_eq!(capability_str::CONTROL_LEDS, "control_leds");
        assert_eq!(capability_str::PROCESS_DSP, "process_dsp");
    }

    #[test]
    fn test_wasm_export_names() {
        assert_eq!(wasm_export::PROCESS, "process");
        assert_eq!(wasm_export::MEMORY, "memory");
        assert_eq!(wasm_optional_export::INIT, "init");
        assert_eq!(wasm_optional_export::SHUTDOWN, "shutdown");
        assert_eq!(wasm_optional_export::GET_INFO, "get_info");
    }

    #[test]
    fn test_host_function_names() {
        assert_eq!(host_function::LOG_DEBUG, "log_debug");
        assert_eq!(host_function::LOG_INFO, "log_info");
        assert_eq!(host_function::LOG_WARN, "log_warn");
        assert_eq!(host_function::LOG_ERROR, "log_error");
        assert_eq!(host_function::PLUGIN_LOG, "plugin_log");
        assert_eq!(host_function::CHECK_CAPABILITY, "check_capability");
        assert_eq!(host_function::GET_TELEMETRY, "get_telemetry");
        assert_eq!(host_function::GET_TIMESTAMP_US, "get_timestamp_us");
    }
}
