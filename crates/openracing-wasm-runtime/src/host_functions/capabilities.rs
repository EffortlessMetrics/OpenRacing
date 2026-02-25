//! Capability checking host functions for WASM plugins.

use wasmtime::{Caller, Linker};

use openracing_plugin_abi::{HOST_MODULE, capability_str, host_function, return_code, wasm_export};

use crate::WasmResult;
use crate::state::WasmPluginState;

/// Register capability checking host functions with the linker.
///
/// # Functions Registered
///
/// - `check_capability(cap_ptr: i32, cap_len: i32) -> i32` - Check if capability is granted
pub fn register_capability_functions(linker: &mut Linker<WasmPluginState>) -> WasmResult<()> {
    linker.func_wrap(
        HOST_MODULE,
        host_function::CHECK_CAPABILITY,
        |mut caller: Caller<'_, WasmPluginState>,
         capability_ptr: i32,
         capability_len: i32|
         -> i32 { check_capability_impl(&mut caller, capability_ptr, capability_len) },
    )?;

    Ok(())
}

/// Helper function to check capability from WASM plugin.
fn check_capability_impl(
    caller: &mut Caller<'_, WasmPluginState>,
    capability_ptr: i32,
    capability_len: i32,
) -> i32 {
    let memory = match caller.get_export(wasm_export::MEMORY) {
        Some(wasmtime::Extern::Memory(mem)) => mem,
        _ => return return_code::ERROR,
    };

    if capability_ptr < 0 || capability_len < 0 {
        return return_code::INVALID_ARG;
    }

    let start = capability_ptr as usize;
    let end = match start.checked_add(capability_len as usize) {
        Some(e) => e,
        None => return return_code::INVALID_ARG,
    };

    let capability_str = {
        let data = match memory.data(&*caller).get(start..end) {
            Some(data) => data,
            None => return return_code::INVALID_ARG,
        };

        match std::str::from_utf8(data) {
            Ok(s) => s.to_string(),
            Err(_) => return return_code::INVALID_ARG,
        }
    };

    let result = match capability_str.as_str() {
        capability_str::READ_TELEMETRY => caller.data().capability_checker.check_telemetry_read(),
        capability_str::MODIFY_TELEMETRY => {
            caller.data().capability_checker.check_telemetry_modify()
        }
        capability_str::CONTROL_LEDS => caller.data().capability_checker.check_led_control(),
        capability_str::PROCESS_DSP => caller.data().capability_checker.check_dsp_processing(),
        _ => return return_code::INVALID_ARG,
    };

    if result.is_ok() {
        1
    } else {
        return_code::PERMISSION_DENIED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_capability_strings() {
        assert_eq!(capability_str::READ_TELEMETRY, "read_telemetry");
        assert_eq!(capability_str::MODIFY_TELEMETRY, "modify_telemetry");
        assert_eq!(capability_str::CONTROL_LEDS, "control_leds");
        assert_eq!(capability_str::PROCESS_DSP, "process_dsp");
    }
}
