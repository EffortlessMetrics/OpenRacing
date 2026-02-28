//! Host function definitions for WASM plugins.
//!
//! This module defines the host functions that the OpenRacing runtime
//! provides to WASM plugins. All functions use C-compatible signatures
//! for FFI stability.

use crate::constants::host_function;

/// Names of host functions provided to WASM plugins.
///
/// These function names are imported by WASM plugins from the "env" module.
pub mod names {
    use super::host_function;

    /// Debug level logging: log_debug(msg_ptr: i32, msg_len: i32)
    pub const LOG_DEBUG: &str = host_function::LOG_DEBUG;
    /// Info level logging: log_info(msg_ptr: i32, msg_len: i32)
    pub const LOG_INFO: &str = host_function::LOG_INFO;
    /// Warning level logging: log_warn(msg_ptr: i32, msg_len: i32)
    pub const LOG_WARN: &str = host_function::LOG_WARN;
    /// Error level logging: log_error(msg_ptr: i32, msg_len: i32)
    pub const LOG_ERROR: &str = host_function::LOG_ERROR;
    /// Generic logging: plugin_log(level: i32, msg_ptr: i32, msg_len: i32)
    pub const PLUGIN_LOG: &str = host_function::PLUGIN_LOG;
    /// Check capability: check_capability(cap_ptr: i32, cap_len: i32) -> i32
    pub const CHECK_CAPABILITY: &str = host_function::CHECK_CAPABILITY;
    /// Get telemetry: get_telemetry(out_ptr: i32, out_len: i32) -> i32
    pub const GET_TELEMETRY: &str = host_function::GET_TELEMETRY;
    /// Get timestamp: get_timestamp_us() -> i64
    pub const GET_TIMESTAMP_US: &str = host_function::GET_TIMESTAMP_US;
}

/// Host function signatures for documentation and validation.
///
/// These types document the expected signatures of host functions.
/// The actual implementations are in the runtime.
pub mod signatures {
    /// Log function signature: (msg_ptr: i32, msg_len: i32) -> ()
    ///
    /// # Safety
    /// - `msg_ptr` must point to valid UTF-8 string in plugin memory
    /// - `msg_len` must be the exact length of the string
    pub type LogFn = extern "C" fn(i32, i32);

    /// Generic log function signature: (level: i32, msg_ptr: i32, msg_len: i32) -> ()
    ///
    /// # Safety
    /// - `level` must be a valid log level (0-4)
    /// - `msg_ptr` must point to valid UTF-8 string in plugin memory
    /// - `msg_len` must be the exact length of the string
    pub type PluginLogFn = extern "C" fn(i32, i32, i32);

    /// Check capability function signature: (cap_ptr: i32, cap_len: i32) -> i32
    ///
    /// # Safety
    /// - `cap_ptr` must point to valid UTF-8 string in plugin memory
    /// - `cap_len` must be the exact length of the string
    ///
    /// # Returns
    /// - 1 if capability is granted
    /// - 0 if capability is not granted
    /// - Negative error code on failure
    pub type CheckCapabilityFn = extern "C" fn(i32, i32) -> i32;

    /// Get telemetry function signature: (out_ptr: i32, out_len: i32) -> i32
    ///
    /// # Safety
    /// - `out_ptr` must point to writable memory in plugin
    /// - `out_len` must be at least 32 bytes for TelemetryFrame
    ///
    /// # Returns
    /// - 0 on success
    /// - Negative error code on failure
    pub type GetTelemetryFn = extern "C" fn(i32, i32) -> i32;

    /// Get timestamp function signature: () -> i64
    ///
    /// # Returns
    /// Current timestamp in microseconds since plugin start
    pub type GetTimestampFn = extern "C" fn() -> i64;
}

/// Host function parameter validation.
///
/// These functions help validate host function parameters.
#[cfg(feature = "std")]
pub mod validation {
    use crate::constants::return_code;

    /// Validate a string pointer and length.
    ///
    /// Returns `Ok(())` if the parameters are valid, or an error code.
    #[must_use]
    pub fn validate_string_params(ptr: i32, len: i32, max_len: usize) -> i32 {
        if ptr < 0 {
            return return_code::INVALID_ARG;
        }
        if len < 0 {
            return return_code::INVALID_ARG;
        }
        if len as usize > max_len {
            return return_code::BUFFER_TOO_SMALL;
        }
        return_code::SUCCESS
    }

    /// Validate an output buffer pointer and length.
    ///
    /// Returns `Ok(())` if the parameters are valid, or an error code.
    #[must_use]
    pub fn validate_output_buffer(ptr: i32, len: i32, required_len: usize) -> i32 {
        if ptr < 0 {
            return return_code::INVALID_ARG;
        }
        if len < 0 {
            return return_code::INVALID_ARG;
        }
        if (len as usize) < required_len {
            return return_code::BUFFER_TOO_SMALL;
        }
        return_code::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_function_names() {
        assert_eq!(names::LOG_DEBUG, "log_debug");
        assert_eq!(names::LOG_INFO, "log_info");
        assert_eq!(names::LOG_WARN, "log_warn");
        assert_eq!(names::LOG_ERROR, "log_error");
        assert_eq!(names::PLUGIN_LOG, "plugin_log");
        assert_eq!(names::CHECK_CAPABILITY, "check_capability");
        assert_eq!(names::GET_TELEMETRY, "get_telemetry");
        assert_eq!(names::GET_TIMESTAMP_US, "get_timestamp_us");
    }
}
