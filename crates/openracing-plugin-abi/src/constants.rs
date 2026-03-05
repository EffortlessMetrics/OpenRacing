//! ABI version constants and magic numbers.
//!
//! These constants define the ABI contract between host and plugins.
//! All integers use little-endian byte order.

/// Plugin ABI version constant for handshake.
///
/// Format: major version (16 bits) << 16 | minor version (16 bits)
/// Version 1.0 = 0x0001_0000
///
/// # ABI Stability Guarantee
///
/// The major version component indicates breaking changes. Plugins built
/// for a different major version are incompatible. The minor version
/// indicates backward-compatible additions.
pub const PLUG_ABI_VERSION: u32 = 0x0001_0000;

/// Plugin ABI magic number for handshake validation.
///
/// 'WWL1' in little-endian: 0x57574C31
///
/// This magic number is used to validate that a plugin header is valid
/// and was produced by the OpenRacing toolchain.
pub const PLUG_ABI_MAGIC: u32 = 0x57574C31;

/// Extract the major version from a packed ABI version.
///
/// The major version occupies the upper 16 bits.
#[must_use]
pub const fn abi_version_major(version: u32) -> u16 {
    (version >> 16) as u16
}

/// Extract the minor version from a packed ABI version.
///
/// The minor version occupies the lower 16 bits.
#[must_use]
pub const fn abi_version_minor(version: u32) -> u16 {
    version as u16
}

/// Pack major and minor into a single ABI version constant.
#[must_use]
pub const fn abi_version_pack(major: u16, minor: u16) -> u32 {
    ((major as u32) << 16) | (minor as u32)
}

/// Check whether a plugin ABI version is forward-compatible with the host.
///
/// A plugin is compatible when:
/// - Its major version equals the host major version (breaking changes bump major).
/// - Its minor version is less than or equal to the host minor version
///   (the host knows about all features the plugin may use).
///
/// This allows a newer host to load older plugins built against the same
/// major version.
#[must_use]
pub const fn is_abi_compatible(host_version: u32, plugin_version: u32) -> bool {
    let host_major = abi_version_major(host_version);
    let plugin_major = abi_version_major(plugin_version);
    let host_minor = abi_version_minor(host_version);
    let plugin_minor = abi_version_minor(plugin_version);

    host_major == plugin_major && plugin_minor <= host_minor
}

/// WASM plugin ABI version.
///
/// This version is separate from the native plugin ABI version to allow
/// independent evolution of the two ABIs.
pub const WASM_ABI_VERSION: u32 = 1;

/// Host module name for WASM imports.
pub const HOST_MODULE: &str = "env";

/// Log level constants for WASM plugins.
///
/// These match the tracing crate's log levels.
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

/// Return codes for WASM plugin functions.
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

/// Capability string constants for WASM plugins.
///
/// These are the strings that plugins pass to check_capability().
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

/// Names of required WASM exports.
pub mod wasm_export {
    /// Main processing function: process(input: f32, dt: f32) -> f32
    pub const PROCESS: &str = "process";
    /// Linear memory export for host function communication
    pub const MEMORY: &str = "memory";
}

/// Names of optional WASM exports.
pub mod wasm_optional_export {
    /// Initialization function: init() -> i32
    pub const INIT: &str = "init";
    /// Shutdown function: shutdown()
    pub const SHUTDOWN: &str = "shutdown";
    /// Get plugin info function: get_info(out_ptr: i32, out_len: i32) -> i32
    pub const GET_INFO: &str = "get_info";
}

/// Names of host functions provided to WASM plugins.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_constants() {
        assert_eq!(PLUG_ABI_VERSION, 0x0001_0000);
        assert_eq!(PLUG_ABI_MAGIC, 0x57574C31);
        assert_eq!(WASM_ABI_VERSION, 1);
    }

    #[test]
    fn test_abi_version_major_minor_extraction() {
        assert_eq!(abi_version_major(0x0001_0000), 1);
        assert_eq!(abi_version_minor(0x0001_0000), 0);

        assert_eq!(abi_version_major(0x0003_0005), 3);
        assert_eq!(abi_version_minor(0x0003_0005), 5);
    }

    #[test]
    fn test_abi_version_pack_roundtrip() {
        let packed = abi_version_pack(1, 0);
        assert_eq!(packed, PLUG_ABI_VERSION);

        let packed2 = abi_version_pack(3, 5);
        assert_eq!(abi_version_major(packed2), 3);
        assert_eq!(abi_version_minor(packed2), 5);
    }

    #[test]
    fn test_is_abi_compatible_same_version() {
        assert!(is_abi_compatible(PLUG_ABI_VERSION, PLUG_ABI_VERSION));
    }

    #[test]
    fn test_is_abi_compatible_older_minor() {
        // Host v1.2 can load plugin v1.0
        let host = abi_version_pack(1, 2);
        let plugin = abi_version_pack(1, 0);
        assert!(is_abi_compatible(host, plugin));
    }

    #[test]
    fn test_is_abi_compatible_rejects_newer_minor() {
        // Host v1.0 cannot load plugin v1.2 (plugin uses features host doesn't know)
        let host = abi_version_pack(1, 0);
        let plugin = abi_version_pack(1, 2);
        assert!(!is_abi_compatible(host, plugin));
    }

    #[test]
    fn test_is_abi_compatible_rejects_different_major() {
        let host = abi_version_pack(1, 0);
        let plugin = abi_version_pack(2, 0);
        assert!(!is_abi_compatible(host, plugin));

        let host2 = abi_version_pack(2, 5);
        let plugin2 = abi_version_pack(1, 5);
        assert!(!is_abi_compatible(host2, plugin2));
    }

    #[test]
    fn test_log_level_ordering() {
        const _: () = assert!(log_level::ERROR < log_level::WARN);
        const _: () = assert!(log_level::WARN < log_level::INFO);
        const _: () = assert!(log_level::INFO < log_level::DEBUG);
        const _: () = assert!(log_level::DEBUG < log_level::TRACE);
    }

    #[test]
    fn test_return_code_values() {
        assert_eq!(return_code::SUCCESS, 0);
        const _: () = assert!(return_code::ERROR < 0);
        const _: () = assert!(return_code::INVALID_ARG < 0);
        const _: () = assert!(return_code::PERMISSION_DENIED < 0);
        const _: () = assert!(return_code::BUFFER_TOO_SMALL < 0);
        const _: () = assert!(return_code::NOT_INITIALIZED < 0);
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
