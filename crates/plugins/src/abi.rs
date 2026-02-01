//! Plugin ABI definitions with versioning and endianness
//!
//! This module defines the stable ABI for both native and WASM plugins, including:
//! - Version handshake protocol
//! - Capability bitflags
//! - C-compatible data structures
//! - Endianness documentation (little-endian for all integers)
//! - WASM plugin ABI contract (host functions, exports, state management)
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

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Plugin ABI version constant for handshake
/// Format: major version (16 bits) << 16 | minor version (16 bits)
/// Version 1.0 = 0x0001_0000
pub const PLUG_ABI_VERSION: u32 = 0x0001_0000;

/// Plugin ABI magic number for handshake validation
/// 'WWL1' in little-endian: 0x57574C31
pub const PLUG_ABI_MAGIC: u32 = 0x57574C31;

bitflags! {
    /// Plugin capability flags
    ///
    /// These flags indicate what operations a plugin can perform.
    /// All unused bits are reserved for future capabilities.
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PluginCapabilities: u32 {
        /// Plugin can read telemetry data
        const TELEMETRY    = 0b0000_0001;

        /// Plugin can control LED patterns
        const LEDS         = 0b0000_0010;

        /// Plugin can process haptic feedback
        const HAPTICS      = 0b0000_0100;

        /// Reserved bits for future capabilities
        /// Plugins should not set these bits
        const RESERVED     = 0xFFFF_FFF8;
    }
}

/// Plugin header for ABI handshake and capability declaration
///
/// All integers are stored in little-endian format.
/// This structure is used for initial handshake between host and plugin.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginHeader {
    /// Magic number for validation (little-endian)
    /// Must be PLUG_ABI_MAGIC (0x57574C31)
    pub magic: u32,

    /// ABI version (little-endian)
    /// Must match PLUG_ABI_VERSION for compatibility
    pub abi_version: u32,

    /// Plugin capabilities bitfield (little-endian)
    /// See PluginCapabilities for valid flags
    pub capabilities: u32,

    /// Reserved field for future use
    /// Must be set to 0
    pub reserved: u32,
}

/// Telemetry frame for real-time plugin communication
///
/// All integers are stored in little-endian format.
/// Field names updated to match new schema conventions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TelemetryFrame {
    /// Timestamp in microseconds (little-endian)
    pub timestamp_us: u64,

    /// Wheel angle in degrees (not millidegrees)
    /// Range: -1800.0 to +1800.0 degrees for 5-turn wheels
    pub wheel_angle_deg: f32,

    /// Wheel speed in radians per second (not mrad/s)
    /// Positive values indicate clockwise rotation
    pub wheel_speed_rad_s: f32,

    /// Temperature in degrees Celsius (not temp_c)
    /// Typical range: 20-80°C for normal operation
    pub temperature_c: f32,

    /// Fault flags bitfield (not faults)
    /// Each bit represents a specific fault condition
    pub fault_flags: u32,

    /// Padding to ensure 8-byte alignment
    pub _pad: u32,
}

impl Default for PluginHeader {
    fn default() -> Self {
        Self {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: 0,
            reserved: 0,
        }
    }
}

impl Default for TelemetryFrame {
    fn default() -> Self {
        Self {
            timestamp_us: 0,
            wheel_angle_deg: 0.0,
            wheel_speed_rad_s: 0.0,
            temperature_c: 20.0, // Room temperature default
            fault_flags: 0,
            _pad: 0,
        }
    }
}

impl PluginHeader {
    /// Create a new plugin header with specified capabilities
    pub fn new(capabilities: PluginCapabilities) -> Self {
        Self {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: capabilities.bits(),
            reserved: 0,
        }
    }

    /// Validate the header magic and version
    pub fn is_valid(&self) -> bool {
        self.magic == PLUG_ABI_MAGIC && self.abi_version == PLUG_ABI_VERSION
    }

    /// Get the capabilities as a bitflags struct
    pub fn get_capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::from_bits_truncate(self.capabilities)
    }

    /// Convert header to byte array (little-endian)
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.magic.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.abi_version.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.capabilities.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }

    /// Create header from byte array (little-endian)
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            magic: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            abi_version: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            capabilities: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            reserved: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        }
    }
}

impl TelemetryFrame {
    /// Create a new telemetry frame with timestamp
    pub fn new(timestamp_us: u64) -> Self {
        Self {
            timestamp_us,
            ..Default::default()
        }
    }

    /// Convert frame to byte array for IPC
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&self.timestamp_us.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.wheel_angle_deg.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.wheel_speed_rad_s.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.temperature_c.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.fault_flags.to_le_bytes());
        bytes[24..28].copy_from_slice(&self._pad.to_le_bytes());
        // bytes[28..32] remain zero (additional padding)
        bytes
    }

    /// Create frame from byte array
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            timestamp_us: u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            wheel_angle_deg: f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            wheel_speed_rad_s: f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            temperature_c: f32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            fault_flags: u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
            _pad: u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
        }
    }
}

// ============================================================================
// WASM Plugin ABI Definitions
// ============================================================================

/// WASM plugin ABI version
/// This version is separate from the native plugin ABI version to allow
/// independent evolution of the two ABIs.
pub const WASM_ABI_VERSION: u32 = 1;

/// Log level constants for WASM plugins
/// These match the tracing crate's log levels
pub mod log_level {
    /// Error level - critical issues that need immediate attention
    pub const ERROR: i32 = 0;
    /// Warning level - potential issues that should be investigated
    pub const WARN: i32 = 1;
    /// Info level - general operational information
    pub const INFO: i32 = 2;
    /// Debug level - detailed debugging information
    pub const DEBUG: i32 = 3;
    /// Trace level - very detailed tracing information
    pub const TRACE: i32 = 4;
}

/// Return codes for WASM plugin functions
pub mod return_code {
    /// Success - operation completed successfully
    pub const SUCCESS: i32 = 0;
    /// Generic error - operation failed
    pub const ERROR: i32 = -1;
    /// Invalid argument - one or more arguments were invalid
    pub const INVALID_ARG: i32 = -2;
    /// Permission denied - capability not granted
    pub const PERMISSION_DENIED: i32 = -3;
    /// Buffer too small - output buffer is too small
    pub const BUFFER_TOO_SMALL: i32 = -4;
    /// Not initialized - plugin not yet initialized
    pub const NOT_INITIALIZED: i32 = -5;
}

/// Capability string constants for WASM plugins
/// These are the strings that plugins pass to check_capability()
pub mod capability_str {
    /// Read telemetry data capability
    pub const READ_TELEMETRY: &str = "read_telemetry";
    /// Modify telemetry data capability
    pub const MODIFY_TELEMETRY: &str = "modify_telemetry";
    /// Control LED patterns capability
    pub const CONTROL_LEDS: &str = "control_leds";
    /// Process DSP filters capability
    pub const PROCESS_DSP: &str = "process_dsp";
}

/// Names of required WASM exports
pub mod wasm_export {
    /// Main processing function: process(input: f32, dt: f32) -> f32
    pub const PROCESS: &str = "process";
    /// Linear memory export for host function communication
    pub const MEMORY: &str = "memory";
}

/// Names of optional WASM exports
pub mod wasm_optional_export {
    /// Initialization function: init() -> i32
    pub const INIT: &str = "init";
    /// Shutdown function: shutdown()
    pub const SHUTDOWN: &str = "shutdown";
    /// Get plugin info function: get_info(out_ptr: i32, out_len: i32) -> i32
    pub const GET_INFO: &str = "get_info";
}

/// Names of host functions provided to WASM plugins
pub mod host_function {
    /// Debug level logging: log_debug(msg_ptr: i32, msg_len: i32)
    pub const LOG_DEBUG: &str = "log_debug";
    /// Info level logging: log_info(msg_ptr: i32, msg_len: i32)
    pub const LOG_INFO: &str = "log_info";
    /// Warning level logging: log_warn(msg_ptr: i32, msg_len: i32)
    pub const LOG_WARN: &str = "log_warn";
    /// Error level logging: log_error(msg_ptr: i32, msg_len: i32)
    pub const LOG_ERROR: &str = "log_error";
    /// Generic logging: plugin_log(level: i32, msg_ptr: i32, msg_len: i32)
    pub const PLUGIN_LOG: &str = "plugin_log";
    /// Check capability: check_capability(cap_ptr: i32, cap_len: i32) -> i32
    pub const CHECK_CAPABILITY: &str = "check_capability";
    /// Get telemetry: get_telemetry(out_ptr: i32, out_len: i32) -> i32
    pub const GET_TELEMETRY: &str = "get_telemetry";
    /// Get timestamp: get_timestamp_us() -> i64
    pub const GET_TIMESTAMP_US: &str = "get_timestamp_us";
}

/// Host module name for WASM imports
pub const HOST_MODULE: &str = "env";

/// Plugin initialization status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PluginInitStatus {
    /// Plugin has not been initialized yet
    #[default]
    Uninitialized,
    /// Plugin is currently initializing
    Initializing,
    /// Plugin initialized successfully
    Initialized,
    /// Plugin initialization failed
    Failed,
    /// Plugin has been shut down
    ShutDown,
}

/// Per-plugin state for WASM plugins (ABI-level state)
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
    /// Create a new plugin state
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

    /// Check if the plugin is initialized
    pub fn is_initialized(&self) -> bool {
        self.init_status == PluginInitStatus::Initialized
    }

    /// Mark the plugin as initialized
    pub fn mark_initialized(&mut self) {
        self.init_status = PluginInitStatus::Initialized;
    }

    /// Mark the plugin as failed with an error message
    pub fn mark_failed(&mut self, error: String) {
        self.init_status = PluginInitStatus::Failed;
        self.last_error = Some(error);
    }

    /// Mark the plugin as shut down
    pub fn mark_shutdown(&mut self) {
        self.init_status = PluginInitStatus::ShutDown;
    }

    /// Get the current timestamp in microseconds since plugin start
    pub fn timestamp_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    /// Update telemetry data
    pub fn update_telemetry(&mut self, telemetry: TelemetryFrame) {
        self.current_telemetry = telemetry;
    }

    /// Record a successful process call
    pub fn record_process_call(&mut self, duration_us: u64) {
        self.process_count += 1;
        self.total_process_time_us += duration_us;
    }

    /// Get average processing time in microseconds
    pub fn average_process_time_us(&self) -> f64 {
        if self.process_count == 0 {
            0.0
        } else {
            self.total_process_time_us as f64 / self.process_count as f64
        }
    }

    /// Store custom plugin data
    pub fn store_data(&mut self, key: String, data: Vec<u8>) {
        self.plugin_data.insert(key, data);
    }

    /// Retrieve custom plugin data
    pub fn get_data(&self, key: &str) -> Option<&Vec<u8>> {
        self.plugin_data.get(key)
    }

    /// Remove custom plugin data
    pub fn remove_data(&mut self, key: &str) -> Option<Vec<u8>> {
        self.plugin_data.remove(key)
    }

    /// Clear all custom plugin data
    pub fn clear_data(&mut self) {
        self.plugin_data.clear();
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.process_count = 0;
        self.total_process_time_us = 0;
    }
}

/// WASM plugin export validation result
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WasmExportValidation {
    /// Whether the required 'process' function is exported
    pub has_process: bool,
    /// Whether the required 'memory' export is present
    pub has_memory: bool,
    /// Whether the optional 'init' function is exported
    pub has_init: bool,
    /// Whether the optional 'shutdown' function is exported
    pub has_shutdown: bool,
    /// Whether the optional 'get_info' function is exported
    pub has_get_info: bool,
}

impl WasmExportValidation {
    /// Check if all required exports are present
    pub fn is_valid(&self) -> bool {
        self.has_process && self.has_memory
    }

    /// Get a list of missing required exports
    pub fn missing_required(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.has_process {
            missing.push(wasm_export::PROCESS);
        }
        if !self.has_memory {
            missing.push(wasm_export::MEMORY);
        }
        missing
    }
}

/// Plugin info structure returned by get_info()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmPluginInfo {
    /// Plugin name
    pub name: String,
    /// Plugin version string
    pub version: String,
    /// Plugin author
    pub author: String,
    /// Plugin description
    pub description: String,
    /// ABI version the plugin was built for
    pub abi_version: u32,
}

impl Default for WasmPluginInfo {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            author: String::new(),
            description: String::new(),
            abi_version: WASM_ABI_VERSION,
        }
    }
}

// Compile-time size and alignment assertions
// These ensure ABI stability across different platforms and compilers
static_assertions::const_assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
static_assertions::const_assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
static_assertions::const_assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
static_assertions::const_assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);

// Ensure bitflags has correct size
static_assertions::const_assert_eq!(std::mem::size_of::<PluginCapabilities>(), 4);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_header_size_and_alignment() {
        // Verify size and alignment requirements
        assert_eq!(std::mem::size_of::<PluginHeader>(), 16);
        assert_eq!(std::mem::align_of::<PluginHeader>(), 4);
    }

    #[test]
    fn test_telemetry_frame_size_and_alignment() {
        // Verify size and alignment requirements
        assert_eq!(std::mem::size_of::<TelemetryFrame>(), 32);
        assert_eq!(std::mem::align_of::<TelemetryFrame>(), 8);
    }

    #[test]
    fn test_plugin_capabilities_bitflags() {
        let caps = PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS;
        assert_eq!(caps.bits(), 0b0000_0011);

        let caps_with_haptics = caps | PluginCapabilities::HAPTICS;
        assert_eq!(caps_with_haptics.bits(), 0b0000_0111);

        // Test reserved bits are not set in valid capabilities
        let valid_caps =
            PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS | PluginCapabilities::HAPTICS;
        assert_eq!(valid_caps.bits() & PluginCapabilities::RESERVED.bits(), 0);
    }

    #[test]
    fn test_plugin_header_byte_exact_serialization() {
        // Create header with known values
        let header = PluginHeader {
            magic: PLUG_ABI_MAGIC,
            abi_version: PLUG_ABI_VERSION,
            capabilities: (PluginCapabilities::TELEMETRY | PluginCapabilities::LEDS).bits(),
            reserved: 0,
        };

        // Convert to bytes
        let bytes = header.to_bytes();

        // Verify exact byte representation (little-endian)
        let expected_bytes = [
            // magic: 0x57574C31 in LE
            0x31, 0x4C, 0x57, 0x57, // abi_version: 0x00010000 in LE
            0x00, 0x00, 0x01, 0x00, // capabilities: 0x00000003 in LE
            0x03, 0x00, 0x00, 0x00, // reserved: 0x00000000 in LE
            0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(bytes, expected_bytes);

        // Test round-trip conversion
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
            abi_version: 0x00020000, // Version 2.0
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

        // Test round-trip conversion
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
        // This test documents the little-endian requirement
        let test_value: u32 = 0x12345678;
        let le_bytes = test_value.to_le_bytes();

        // On little-endian systems, this should be [0x78, 0x56, 0x34, 0x12]
        // On big-endian systems, to_le_bytes() will swap to little-endian
        let expected_le = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(le_bytes, expected_le);

        // Verify round-trip
        let restored = u32::from_le_bytes(le_bytes);
        assert_eq!(restored, test_value);
    }

    #[test]
    fn test_abi_constants() {
        // Verify ABI constants have expected values
        assert_eq!(PLUG_ABI_VERSION, 0x0001_0000); // Version 1.0
        assert_eq!(PLUG_ABI_MAGIC, 0x57574C31); // 'WWL1' in LE
    }

    #[test]
    fn test_capability_flags_reserved_bits() {
        // Ensure reserved bits are properly defined
        let reserved_mask = PluginCapabilities::RESERVED.bits();
        let valid_mask = (PluginCapabilities::TELEMETRY
            | PluginCapabilities::LEDS
            | PluginCapabilities::HAPTICS)
            .bits();

        // Reserved and valid bits should not overlap
        assert_eq!(reserved_mask & valid_mask, 0);

        // All bits should be accounted for
        assert_eq!(reserved_mask | valid_mask, 0xFFFF_FFFF);
    }

    // ========================================================================
    // WASM Plugin ABI Tests
    // ========================================================================

    #[test]
    fn test_wasm_abi_version() {
        assert_eq!(WASM_ABI_VERSION, 1);
    }

    #[test]
    fn test_log_level_constants() {
        // Verify log levels are in correct order (lower = more severe)
        // Use const blocks to satisfy clippy::assertions_on_constants
        const _: () = assert!(log_level::ERROR < log_level::WARN);
        const _: () = assert!(log_level::WARN < log_level::INFO);
        const _: () = assert!(log_level::INFO < log_level::DEBUG);
        const _: () = assert!(log_level::DEBUG < log_level::TRACE);
    }

    #[test]
    fn test_return_code_constants() {
        // Success should be 0
        assert_eq!(return_code::SUCCESS, 0);
        // All error codes should be negative
        // Use const blocks to satisfy clippy::assertions_on_constants
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

        // Initially not initialized
        assert!(!state.is_initialized());

        // Mark as initialized
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

        // Store data
        state.store_data("key1".to_string(), vec![1, 2, 3]);
        state.store_data("key2".to_string(), vec![4, 5, 6]);

        // Retrieve data
        assert_eq!(state.get_data("key1"), Some(&vec![1, 2, 3]));
        assert_eq!(state.get_data("key2"), Some(&vec![4, 5, 6]));
        assert_eq!(state.get_data("nonexistent"), None);

        // Remove data
        let removed = state.remove_data("key1");
        assert_eq!(removed, Some(vec![1, 2, 3]));
        assert_eq!(state.get_data("key1"), None);

        // Clear all data
        state.clear_data();
        assert!(state.plugin_data.is_empty());
    }

    #[test]
    fn test_wasm_plugin_state_process_stats() {
        let mut state = WasmPluginAbiState::new();

        // Initially zero
        assert_eq!(state.process_count, 0);
        assert_eq!(state.average_process_time_us(), 0.0);

        // Record some calls
        state.record_process_call(100);
        state.record_process_call(200);
        state.record_process_call(300);

        assert_eq!(state.process_count, 3);
        assert_eq!(state.total_process_time_us, 600);
        assert!((state.average_process_time_us() - 200.0).abs() < f64::EPSILON);

        // Reset stats
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
        // Verify capability strings match expected values
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
