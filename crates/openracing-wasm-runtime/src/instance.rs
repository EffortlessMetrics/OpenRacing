//! WASM plugin instance management.
//!
//! This module provides the [`WasmPluginInstance`] struct that represents
//! a loaded WASM plugin with its own isolated state.

use std::time::Instant;

use wasmtime::{Instance, Store, TypedFunc};

use crate::state::WasmPluginState;

/// Plugin identifier type alias for clarity.
pub type PluginId = uuid::Uuid;

/// Information about why a plugin was disabled.
#[derive(Debug, Clone)]
pub struct PluginDisabledInfo {
    /// Reason the plugin was disabled
    pub reason: String,
    /// When the plugin was disabled
    pub disabled_at: Instant,
    /// Optional trap location information
    pub trap_location: Option<String>,
}

/// WASM plugin instance with sandboxing.
///
/// Each instance contains its own store (isolated state), the instantiated
/// module, and a typed function reference for the main processing function.
///
/// # Safety Guarantees
///
/// - Memory is isolated from other plugins and the host
/// - Execution is bounded by fuel limits
/// - Traps are caught and the plugin is disabled
pub struct WasmPluginInstance {
    /// Wasmtime store containing the plugin's isolated state
    pub(crate) store: Store<WasmPluginState>,
    /// The instantiated WASM module
    pub(crate) instance: Instance,
    /// Typed function for DSP processing: (input: f32, dt: f32) -> f32
    pub(crate) process_fn: Option<TypedFunc<(f32, f32), f32>>,
    /// Whether the plugin is disabled (e.g., due to a trap/panic)
    disabled: Option<PluginDisabledInfo>,
}

impl WasmPluginInstance {
    /// Create a new WASM plugin instance.
    pub(crate) fn new(
        store: Store<WasmPluginState>,
        instance: Instance,
        process_fn: Option<TypedFunc<(f32, f32), f32>>,
    ) -> Self {
        Self {
            store,
            instance,
            process_fn,
            disabled: None,
        }
    }

    /// Get a mutable reference to the store.
    pub fn store_mut(&mut self) -> &mut Store<WasmPluginState> {
        &mut self.store
    }

    /// Get a reference to the store.
    pub fn store(&self) -> &Store<WasmPluginState> {
        &self.store
    }

    /// Get a reference to the instance.
    pub fn instance(&self) -> &Instance {
        &self.instance
    }

    /// Check if the instance has a process function.
    #[must_use]
    pub fn has_process_fn(&self) -> bool {
        self.process_fn.is_some()
    }

    /// Check if the plugin is disabled.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled.is_some()
    }

    /// Get the disabled info if the plugin is disabled.
    #[must_use]
    pub fn disabled_info(&self) -> Option<&PluginDisabledInfo> {
        self.disabled.as_ref()
    }

    /// Mark the plugin as disabled due to a trap/panic.
    pub(crate) fn mark_disabled(&mut self, reason: String, trap_location: Option<String>) {
        self.disabled = Some(PluginDisabledInfo {
            reason,
            disabled_at: Instant::now(),
            trap_location,
        });
    }

    /// Re-enable a disabled plugin.
    pub(crate) fn re_enable(&mut self) {
        self.disabled = None;
    }

    /// Get the ABI state.
    pub fn abi_state(&self) -> &crate::state::WasmPluginAbiState {
        &self.store.data().abi_state
    }

    /// Get a mutable reference to the ABI state.
    pub fn abi_state_mut(&mut self) -> &mut crate::state::WasmPluginAbiState {
        &mut self.store.data_mut().abi_state
    }

    /// Check if the plugin is initialized.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.store.data().abi_state.is_initialized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_disabled_info_creation() {
        let info = PluginDisabledInfo {
            reason: "Test trap".to_string(),
            disabled_at: Instant::now(),
            trap_location: Some("func[0]".to_string()),
        };

        assert_eq!(info.reason, "Test trap");
        assert!(info.trap_location.is_some());
        assert_eq!(info.trap_location.as_deref(), Some("func[0]"));
    }
}
