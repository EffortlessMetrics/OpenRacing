//! Telemetry access host functions for WASM plugins.

use wasmtime::{Caller, Linker};

use openracing_plugin_abi::{HOST_MODULE, host_function, return_code, wasm_export};

use crate::WasmResult;
use crate::state::WasmPluginState;

/// Telemetry frame size in bytes.
const TELEMETRY_SIZE: usize = 32;

/// Register telemetry access host functions with the linker.
///
/// # Functions Registered
///
/// - `get_telemetry(out_ptr: i32, out_len: i32) -> i32` - Get current telemetry data
pub fn register_telemetry_functions(linker: &mut Linker<WasmPluginState>) -> WasmResult<()> {
    linker.func_wrap(
        HOST_MODULE,
        host_function::GET_TELEMETRY,
        |mut caller: Caller<'_, WasmPluginState>, out_ptr: i32, out_len: i32| -> i32 {
            get_telemetry_impl(&mut caller, out_ptr, out_len)
        },
    )?;

    Ok(())
}

/// Helper function to get telemetry data for WASM plugin.
fn get_telemetry_impl(caller: &mut Caller<'_, WasmPluginState>, out_ptr: i32, out_len: i32) -> i32 {
    if caller
        .data()
        .capability_checker
        .check_telemetry_read()
        .is_err()
    {
        return return_code::PERMISSION_DENIED;
    }

    let memory = match caller.get_export(wasm_export::MEMORY) {
        Some(wasmtime::Extern::Memory(mem)) => mem,
        _ => return return_code::ERROR,
    };

    if out_ptr < 0 || out_len < 0 {
        return return_code::INVALID_ARG;
    }

    if (out_len as usize) < TELEMETRY_SIZE {
        return return_code::BUFFER_TOO_SMALL;
    }

    let telemetry_bytes = caller.data().abi_state.current_telemetry.to_bytes();

    let start = out_ptr as usize;
    let end = start + TELEMETRY_SIZE;

    let mem_data = memory.data_mut(caller);
    if let Some(dest) = mem_data.get_mut(start..end) {
        dest.copy_from_slice(&telemetry_bytes);
        return_code::SUCCESS
    } else {
        return_code::INVALID_ARG
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_size() {
        assert_eq!(TELEMETRY_SIZE, 32);
    }
}
