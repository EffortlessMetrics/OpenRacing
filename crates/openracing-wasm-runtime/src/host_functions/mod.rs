//! Host functions for WASM plugins.
//!
//! This module provides the host functions that WASM plugins can import
//! and call during execution.

pub mod capabilities;
pub mod logging;
pub mod telemetry;
pub mod timestamp;

pub use capabilities::register_capability_functions;
pub use logging::register_logging_functions;
pub use telemetry::register_telemetry_functions;
pub use timestamp::register_timestamp_functions;

use wasmtime::Linker;

use crate::WasmResult;
use crate::state::WasmPluginState;

/// Register all host functions with the linker.
///
/// This function registers all host functions that WASM plugins can import:
/// - Logging functions (log_debug, log_info, log_warn, log_error, plugin_log)
/// - Capability checking (check_capability)
/// - Telemetry access (get_telemetry)
/// - Timestamp access (get_timestamp_us)
///
/// # Errors
///
/// Returns an error if any host function fails to register.
pub fn register_all_host_functions(linker: &mut Linker<WasmPluginState>) -> WasmResult<()> {
    register_logging_functions(linker)?;
    register_capability_functions(linker)?;
    register_telemetry_functions(linker)?;
    register_timestamp_functions(linker)?;
    Ok(())
}
