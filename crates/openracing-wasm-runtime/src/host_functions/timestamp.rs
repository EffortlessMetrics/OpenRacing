//! Timestamp access host functions for WASM plugins.

use wasmtime::Linker;

use openracing_plugin_abi::{HOST_MODULE, host_function};

use crate::WasmResult;
use crate::state::WasmPluginState;

/// Register timestamp access host functions with the linker.
///
/// # Functions Registered
///
/// - `get_timestamp_us() -> i64` - Get current timestamp in microseconds
pub fn register_timestamp_functions(linker: &mut Linker<WasmPluginState>) -> WasmResult<()> {
    linker.func_wrap(
        HOST_MODULE,
        host_function::GET_TIMESTAMP_US,
        |caller: wasmtime::Caller<'_, WasmPluginState>| -> i64 {
            caller.data().abi_state.timestamp_us() as i64
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
