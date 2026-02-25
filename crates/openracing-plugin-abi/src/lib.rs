//! Plugin ABI definitions with versioning and endianness.
//!
//! This crate defines the stable ABI for both native and WASM plugins, including:
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
//!
//! # ABI Stability Guarantees
//!
//! All structures in this crate are marked with `#[repr(C)]` and have fixed
//! sizes and alignments. The byte layout is guaranteed to be stable across
//! versions. When the ABI changes, the version constants will be updated.
//!
//! # Endianness
//!
//! All integers in the ABI are stored in little-endian format. The `to_bytes()`
//! and `from_bytes()` methods handle conversion automatically.

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

pub mod constants;
pub mod host_functions;
pub mod prelude;
pub mod telemetry_frame;
pub mod types;

pub use constants::{
    HOST_MODULE, PLUG_ABI_MAGIC, PLUG_ABI_VERSION, WASM_ABI_VERSION, capability_str, host_function,
    log_level, return_code, wasm_export, wasm_optional_export,
};
pub use host_functions::names as host_function_names;
pub use telemetry_frame::TelemetryFrame;
pub use types::{
    PluginCapabilities, PluginHeader, PluginInitStatus, WasmExportValidation, WasmPluginInfo,
};

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_re_exports() {
        let _ = PLUG_ABI_VERSION;
        let _ = PLUG_ABI_MAGIC;
        let _ = WASM_ABI_VERSION;
        let _ = HOST_MODULE;
        let _ = host_function::LOG_DEBUG;
        let _ = log_level::ERROR;
        let _ = return_code::SUCCESS;
        let _ = capability_str::READ_TELEMETRY;
        let _ = wasm_export::PROCESS;
        let _ = wasm_optional_export::INIT;
    }
}
