//! Convenience re-exports for common WASM runtime types.
//!
//! This module provides a simple way to import the most commonly used
//! types and traits from the WASM runtime crate.
//!
//! # Example
//!
//! ```
//! use openracing_wasm_runtime::prelude::*;
//!
//! let runtime = WasmRuntime::new()?;
//! let limits = ResourceLimits::default();
//! # Ok::<(), openracing_wasm_runtime::WasmError>(())
//! ```

pub use crate::error::{WasmError, WasmResult};
pub use crate::hot_reload::{HotReloader, PreservedPluginState};
pub use crate::instance::{PluginDisabledInfo, PluginId, WasmPluginInstance};
pub use crate::resource_limits::ResourceLimits;
pub use crate::runtime::WasmRuntime;
pub use crate::state::{CapabilityChecker, WasmPluginAbiState, WasmPluginState};

pub use openracing_plugin_abi::prelude::*;
