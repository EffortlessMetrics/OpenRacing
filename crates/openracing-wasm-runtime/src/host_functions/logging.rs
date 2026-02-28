//! Logging host functions for WASM plugins.

use wasmtime::{Caller, Linker};

use openracing_plugin_abi::{HOST_MODULE, host_function, log_level, wasm_export};

use crate::WasmResult;
use crate::state::WasmPluginState;

/// Register logging host functions with the linker.
///
/// # Functions Registered
///
/// - `log_debug(msg_ptr: i32, msg_len: i32)` - Log at debug level
/// - `log_info(msg_ptr: i32, msg_len: i32)` - Log at info level
/// - `log_warn(msg_ptr: i32, msg_len: i32)` - Log at warning level
/// - `log_error(msg_ptr: i32, msg_len: i32)` - Log at error level
/// - `plugin_log(level: i32, msg_ptr: i32, msg_len: i32)` - Log at specified level
pub fn register_logging_functions(linker: &mut Linker<WasmPluginState>) -> WasmResult<()> {
    linker.func_wrap(
        HOST_MODULE,
        host_function::LOG_DEBUG,
        |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
            log_message(&mut caller, log_level::DEBUG, msg_ptr, msg_len);
        },
    )?;

    linker.func_wrap(
        HOST_MODULE,
        host_function::LOG_INFO,
        |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
            log_message(&mut caller, log_level::INFO, msg_ptr, msg_len);
        },
    )?;

    linker.func_wrap(
        HOST_MODULE,
        host_function::LOG_WARN,
        |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
            log_message(&mut caller, log_level::WARN, msg_ptr, msg_len);
        },
    )?;

    linker.func_wrap(
        HOST_MODULE,
        host_function::LOG_ERROR,
        |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
            log_message(&mut caller, log_level::ERROR, msg_ptr, msg_len);
        },
    )?;

    linker.func_wrap(
        HOST_MODULE,
        host_function::PLUGIN_LOG,
        |mut caller: Caller<'_, WasmPluginState>, level: i32, msg_ptr: i32, msg_len: i32| {
            log_message(&mut caller, level, msg_ptr, msg_len);
        },
    )?;

    Ok(())
}

/// Helper function to log a message from WASM plugin memory.
fn log_message(caller: &mut Caller<'_, WasmPluginState>, level: i32, msg_ptr: i32, msg_len: i32) {
    let memory = match caller.get_export(wasm_export::MEMORY) {
        Some(wasmtime::Extern::Memory(mem)) => mem,
        _ => return,
    };

    if msg_ptr < 0 || msg_len < 0 {
        return;
    }

    let start = msg_ptr as usize;
    let end = match start.checked_add(msg_len as usize) {
        Some(e) => e,
        None => return,
    };

    let data = match memory.data(caller).get(start..end) {
        Some(data) => data,
        None => return,
    };

    let message = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    match level {
        l if l <= log_level::ERROR => tracing::error!("Plugin: {}", message),
        l if l == log_level::WARN => tracing::warn!("Plugin: {}", message),
        l if l == log_level::INFO => tracing::info!("Plugin: {}", message),
        l if l == log_level::DEBUG => tracing::debug!("Plugin: {}", message),
        _ => tracing::trace!("Plugin: {}", message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_constants() {
        const _: () = assert!(log_level::ERROR < log_level::WARN);
        const _: () = assert!(log_level::WARN < log_level::INFO);
        const _: () = assert!(log_level::INFO < log_level::DEBUG);
        const _: () = assert!(log_level::DEBUG < log_level::TRACE);
    }
}
