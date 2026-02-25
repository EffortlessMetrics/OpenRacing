//! Convenience re-exports for common ABI types.
//!
//! This module provides a simple way to import the most commonly used
//! types and constants from the plugin ABI crate.
//!
//! # Example
//!
//! ```
//! use openracing_plugin_abi::prelude::*;
//!
//! let header = PluginHeader::new(PluginCapabilities::TELEMETRY);
//! assert!(header.is_valid());
//! ```

pub use crate::constants::{
    HOST_MODULE, PLUG_ABI_MAGIC, PLUG_ABI_VERSION, WASM_ABI_VERSION, capability_str, host_function,
    log_level, return_code, wasm_export, wasm_optional_export,
};

pub use crate::host_functions::{names as host_names, signatures as host_signatures};

pub use crate::telemetry_frame::TelemetryFrame;

pub use crate::types::{
    PluginCapabilities, PluginHeader, PluginInitStatus, WasmExportValidation, WasmPluginInfo,
};
