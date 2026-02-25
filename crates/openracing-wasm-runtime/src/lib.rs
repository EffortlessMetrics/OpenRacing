//! WASM plugin runtime with capability-based sandboxing for OpenRacing.
//!
//! This crate provides a sandboxed WASM runtime for executing plugins safely.
//! It uses wasmtime with resource limits (memory, fuel) to prevent plugins from
//! consuming excessive resources or causing system instability.
//!
//! # Architecture
//!
//! The runtime consists of several key components:
//!
//! - [`WasmRuntime`]: The main runtime that manages engine, linker, and plugin instances
//! - [`WasmPluginInstance`]: An individual plugin instance with its own store and state
//! - [`ResourceLimits`]: Configuration for memory, fuel, and instance limits
//! - [`HotReloader`]: Hot-reload support with state preservation
//!
//! # Safety Guarantees
//!
//! - **Memory Isolation**: Each plugin has isolated memory with configurable limits
//! - **Execution Limits**: Fuel-based execution counting prevents infinite loops
//! - **Capability Enforcement**: WASI capabilities are restricted per plugin
//! - **Trap Handling**: Plugin crashes are caught and the plugin is disabled
//!
//! # RT-Safety Considerations
//!
//! **WARNING**: This crate is NOT suitable for real-time (RT) code paths!
//!
//! The following operations are NOT RT-safe:
//! - Loading/unloading plugins (involves allocation, I/O)
//! - Hot-reloading (involves allocation, module compilation)
//! - Creating the runtime (involves allocation)
//!
//! The following operations MAY be RT-safe with careful use:
//! - [`WasmRuntime::process()`] - Only if fuel limits are set appropriately
//!   and the plugin itself is RT-safe. Note that WASM execution still has
//!   non-deterministic timing due to JIT compilation on first execution.
//!
//! For RT-safe plugins, use the native plugin system instead.
//!
//! # Example
//!
//! ```no_run
//! use openracing_wasm_runtime::{WasmRuntime, ResourceLimits};
//! use uuid::Uuid;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a runtime with custom limits
//! let limits = ResourceLimits::default()
//!     .with_memory(8 * 1024 * 1024)  // 8MB
//!     .with_fuel(5_000_000);          // 5M instructions
//!
//! let mut runtime = WasmRuntime::with_limits(limits)?;
//!
//! // Load a plugin
//! let plugin_id = Uuid::new_v4();
//! let wasm_bytes: &[u8] = &[];
//! runtime.load_plugin_from_bytes(plugin_id, wasm_bytes, vec![])?;
//!
//! // Process data
//! let result = runtime.process(&plugin_id, 0.5, 0.001)?;
//!
//! // Cleanup
//! runtime.unload_plugin(&plugin_id)?;
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod host_functions;
pub mod hot_reload;
pub mod instance;
pub mod resource_limits;
pub mod runtime;
pub mod state;

pub mod prelude;

pub use error::{WasmError, WasmResult};
pub use hot_reload::{HotReloader, PreservedPluginState};
pub use instance::{PluginDisabledInfo, WasmPluginInstance};
pub use resource_limits::ResourceLimits;
pub use runtime::WasmRuntime;
pub use state::WasmPluginState;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() -> WasmResult<()> {
        let runtime = WasmRuntime::new()?;
        assert_eq!(runtime.instance_count(), 0);
        Ok(())
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 10_000_000);
        assert_eq!(limits.max_table_elements, 10_000);
        assert_eq!(limits.max_instances, 32);
    }

    #[test]
    fn test_resource_limits_builder() {
        let limits = ResourceLimits::default()
            .with_memory(8 * 1024 * 1024)
            .with_fuel(5_000_000)
            .with_table_elements(5_000)
            .with_max_instances(16);

        assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 5_000_000);
        assert_eq!(limits.max_table_elements, 5_000);
        assert_eq!(limits.max_instances, 16);
    }
}
