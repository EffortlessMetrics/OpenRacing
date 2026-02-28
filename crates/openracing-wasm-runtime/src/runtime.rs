//! WASM runtime management.
//!
//! This module provides the main [`WasmRuntime`] struct that manages the
//! wasmtime engine, linker, and all plugin instances.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use wasmtime::{Config, Engine, Instance, Linker, Module, Store};

use openracing_plugin_abi::{
    TelemetryFrame, WasmExportValidation, return_code, wasm_export, wasm_optional_export,
};

use crate::host_functions;
use crate::hot_reload::PreservedPluginState;
use crate::instance::{PluginDisabledInfo, PluginId, WasmPluginInstance};
use crate::resource_limits::ResourceLimits;
use crate::state::WasmPluginState;
use crate::{WasmError, WasmResult};

/// WASM plugin runtime using wasmtime.
///
/// The runtime manages the wasmtime engine, linker, and all plugin instances.
/// It enforces resource limits and provides methods for loading, reloading,
/// and executing plugins.
///
/// # Thread Safety
///
/// The runtime is not thread-safe. Use external synchronization if needed.
/// For async operations, use `tokio::sync::Mutex` or similar.
///
/// # RT-Safety Warning
///
/// **This struct is NOT suitable for real-time code paths!**
/// Plugin loading, unloading, and hot-reloading all involve allocation
/// and potentially I/O. The `process` method may be RT-safe for
/// already-compiled plugins with appropriate fuel limits.
pub struct WasmRuntime {
    /// Wasmtime engine with configured resource limits
    engine: Engine,
    /// Linker with host functions registered
    linker: Linker<WasmPluginState>,
    /// Map of plugin ID to plugin instance
    instances: HashMap<PluginId, WasmPluginInstance>,
    /// Resource limits applied to all plugins
    resource_limits: ResourceLimits,
}

impl WasmRuntime {
    /// Create a new WASM runtime with default resource limits.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine or linker fails to initialize.
    pub fn new() -> WasmResult<Self> {
        Self::with_limits(ResourceLimits::default())
    }

    /// Create a new WASM runtime with custom resource limits.
    ///
    /// # Arguments
    ///
    /// * `resource_limits` - Custom resource limits to apply.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine or linker fails to initialize.
    pub fn with_limits(resource_limits: ResourceLimits) -> WasmResult<Self> {
        let mut config = Config::new();

        // Disable potentially dangerous features for security
        config.wasm_bulk_memory(false);
        config.wasm_multi_value(false);
        config.wasm_threads(false);

        // Enable resource limiting features
        config.consume_fuel(true);
        config.epoch_interruption(resource_limits.epoch_interruption);

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);
        Self::register_host_functions(&mut linker)?;

        Ok(Self {
            engine,
            linker,
            instances: HashMap::new(),
            resource_limits,
        })
    }

    /// Register host functions in the linker.
    fn register_host_functions(linker: &mut Linker<WasmPluginState>) -> WasmResult<()> {
        // Add WASI support
        wasmtime_wasi::p1::add_to_linker_sync(linker, |s: &mut WasmPluginState| &mut s.wasi)?;

        // Add custom host functions
        host_functions::register_all_host_functions(linker)?;

        Ok(())
    }

    /// Get the current resource limits.
    #[must_use]
    pub fn resource_limits(&self) -> &ResourceLimits {
        &self.resource_limits
    }

    /// Get the number of loaded plugin instances.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Check if a plugin is loaded.
    #[must_use]
    pub fn has_plugin(&self, id: &PluginId) -> bool {
        self.instances.contains_key(id)
    }

    /// Get the engine reference.
    #[must_use]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Validate WASM module exports.
    pub fn validate_exports(
        store: &mut Store<WasmPluginState>,
        instance: &Instance,
    ) -> WasmExportValidation {
        let has_process = instance
            .get_typed_func::<(f32, f32), f32>(&mut *store, wasm_export::PROCESS)
            .is_ok();
        let has_memory = instance
            .get_memory(&mut *store, wasm_export::MEMORY)
            .is_some();
        let has_init = instance
            .get_typed_func::<(), i32>(&mut *store, wasm_optional_export::INIT)
            .is_ok();
        let has_shutdown = instance
            .get_typed_func::<(), ()>(&mut *store, wasm_optional_export::SHUTDOWN)
            .is_ok();
        let has_get_info = instance
            .get_typed_func::<(i32, i32), i32>(&mut *store, wasm_optional_export::GET_INFO)
            .is_ok();

        WasmExportValidation {
            has_process,
            has_memory,
            has_init,
            has_shutdown,
            has_get_info,
        }
    }

    /// Load a WASM plugin from bytes.
    ///
    /// This method compiles the WASM module, creates a new store with the
    /// configured resource limits, and instantiates the module.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the plugin.
    /// * `wasm_bytes` - WASM module bytes.
    /// * `capabilities` - Capabilities to grant to the plugin.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Maximum instances limit is reached
    /// - Module compilation fails
    /// - Module is missing required exports
    /// - Plugin initialization fails
    pub fn load_plugin_from_bytes(
        &mut self,
        id: PluginId,
        wasm_bytes: &[u8],
        capabilities: Vec<String>,
    ) -> WasmResult<()> {
        if self.instances.len() >= self.resource_limits.max_instances {
            return Err(WasmError::MaxInstancesReached(
                self.resource_limits.max_instances,
            ));
        }

        let module = Module::new(&self.engine, wasm_bytes)?;

        let wasi = wasmtime_wasi::WasiCtxBuilder::new().build_p1();

        let state = WasmPluginState::with_capabilities(wasi, capabilities);

        let mut store = Store::new(&self.engine, state);
        store.set_fuel(self.resource_limits.max_fuel)?;
        store.set_epoch_deadline(1);

        let instance = self.linker.instantiate(&mut store, &module)?;

        let validation = Self::validate_exports(&mut store, &instance);
        if !validation.is_valid() {
            let missing = validation.missing_required();
            return Err(WasmError::MissingExport(missing.join(", ")));
        }

        let process_fn = instance
            .get_typed_func::<(f32, f32), f32>(&mut store, wasm_export::PROCESS)
            .ok();

        if validation.has_init {
            if let Ok(init_fn) =
                instance.get_typed_func::<(), i32>(&mut store, wasm_optional_export::INIT)
            {
                store.data_mut().abi_state.init_status =
                    openracing_plugin_abi::PluginInitStatus::Initializing;
                match init_fn.call(&mut store, ()) {
                    Ok(result) if result == return_code::SUCCESS => {
                        store.data_mut().abi_state.mark_initialized();
                    }
                    Ok(result) => {
                        store
                            .data_mut()
                            .abi_state
                            .mark_failed(format!("init() returned error code: {}", result));
                        return Err(WasmError::LoadingFailed(format!(
                            "Plugin init() returned error code: {}",
                            result
                        )));
                    }
                    Err(e) => {
                        store
                            .data_mut()
                            .abi_state
                            .mark_failed(format!("init() failed: {}", e));
                        return Err(WasmError::LoadingFailed(format!(
                            "Plugin init() failed: {}",
                            e
                        )));
                    }
                }
            }
        } else {
            store.data_mut().abi_state.mark_initialized();
        }

        let plugin_instance = WasmPluginInstance::new(store, instance, process_fn);
        self.instances.insert(id, plugin_instance);

        tracing::info!("Loaded WASM plugin: {}", id);
        Ok(())
    }

    /// Load a WASM plugin from a file path.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the plugin.
    /// * `path` - Path to the WASM module file.
    /// * `capabilities` - Capabilities to grant to the plugin.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or loading fails.
    pub async fn load_plugin(
        &mut self,
        id: PluginId,
        path: &Path,
        capabilities: Vec<String>,
    ) -> WasmResult<()> {
        let wasm_bytes = tokio::fs::read(path).await?;
        self.load_plugin_from_bytes(id, &wasm_bytes, capabilities)
    }

    /// Unload a plugin.
    ///
    /// This method calls the plugin's shutdown function (if present) and
    /// removes the plugin instance from the runtime.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID to unload.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn unload_plugin(&mut self, id: &PluginId) -> WasmResult<()> {
        if let Some(mut instance) = self.instances.remove(id) {
            if let Ok(shutdown_fn) = instance
                .instance
                .get_typed_func::<(), ()>(&mut instance.store, wasm_optional_export::SHUTDOWN)
            {
                let _ = shutdown_fn.call(&mut instance.store, ());
            }
            instance.store.data_mut().abi_state.mark_shutdown();
            tracing::info!("Unloaded WASM plugin: {}", id);
            Ok(())
        } else {
            Err(WasmError::plugin_not_found(id))
        }
    }

    /// Process FFB through a plugin.
    ///
    /// This method calls the plugin's process function with the given input
    /// and delta time, returning the processed output.
    ///
    /// If the plugin traps (WASM equivalent of panic), the trap is caught,
    /// the plugin is disabled, and an error is returned.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID to process.
    /// * `input` - Input value (typically FFB magnitude).
    /// * `dt` - Delta time in seconds.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Plugin is not found
    /// - Plugin is disabled
    /// - Plugin is not initialized
    /// - Plugin traps during execution
    /// - Fuel is exhausted
    pub fn process(&mut self, id: &PluginId, input: f32, dt: f32) -> WasmResult<f32> {
        let start_time = Instant::now();

        // Get the instance and check state
        {
            let instance = self
                .instances
                .get_mut(id)
                .ok_or_else(|| WasmError::plugin_not_found(id))?;

            if let Some(disabled_info) = instance.disabled_info() {
                return Err(WasmError::PluginDisabled {
                    reason: format!(
                        "Plugin is disabled: {} (disabled at {:?})",
                        disabled_info.reason, disabled_info.disabled_at
                    ),
                });
            }

            if !instance.store.data().abi_state.is_initialized() {
                return Err(WasmError::PluginNotInitialized);
            }

            instance.store.set_fuel(self.resource_limits.max_fuel)?;
            instance.store.set_epoch_deadline(100);
        }

        self.engine.increment_epoch();

        // Get the process function and call it
        let call_result = {
            let instance = self
                .instances
                .get_mut(id)
                .ok_or_else(|| WasmError::plugin_not_found(id))?;

            // Get the process function from the instance
            let instance_ref = &instance.instance;
            let store = &mut instance.store;

            instance_ref
                .get_typed_func::<(f32, f32), f32>(&mut *store, wasm_export::PROCESS)?
                .call(&mut *store, (input, dt))
        };

        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        match call_result {
            Ok(result) => {
                let duration_us = start_time.elapsed().as_micros() as u64;
                instance
                    .store
                    .data_mut()
                    .abi_state
                    .record_process_call(duration_us);
                Ok(result)
            }
            Err(trap) => {
                let trap_reason = trap.to_string();
                let trap_location = Self::extract_trap_location(&trap);

                tracing::error!(
                    plugin_id = %id,
                    trap_reason = %trap_reason,
                    trap_location = ?trap_location,
                    "WASM plugin trapped during execution, disabling plugin"
                );

                instance.mark_disabled(trap_reason.clone(), trap_location.clone());

                if instance.store.get_fuel().unwrap_or(0) == 0 {
                    Err(WasmError::BudgetViolation {
                        used_us: 0,
                        budget_us: 0,
                    })
                } else {
                    Err(WasmError::crashed(format!(
                        "Plugin trapped: {}{}",
                        trap_reason,
                        trap_location
                            .map(|loc| format!(" at {}", loc))
                            .unwrap_or_default()
                    )))
                }
            }
        }
    }

    /// Extract trap location information from a wasmtime error.
    fn extract_trap_location(error: &wasmtime::Error) -> Option<String> {
        if let Some(trap) = error.downcast_ref::<wasmtime::Trap>() {
            return Some(format!("trap: {:?}", trap));
        }

        let error_string = error.to_string();
        if error_string.contains("wasm backtrace") || error_string.contains("at ") {
            for line in error_string.lines() {
                if line.contains("at ") || line.contains("func[") {
                    return Some(line.trim().to_string());
                }
            }
        }

        None
    }

    /// Update telemetry data for a plugin.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID to update.
    /// * `telemetry` - New telemetry frame.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn update_plugin_telemetry(
        &mut self,
        id: &PluginId,
        telemetry: TelemetryFrame,
    ) -> WasmResult<()> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        instance
            .store
            .data_mut()
            .abi_state
            .update_telemetry(telemetry);
        Ok(())
    }

    /// Get plugin statistics.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID to get stats for.
    ///
    /// # Returns
    ///
    /// Returns a tuple of (process_count, average_process_time_us).
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn get_plugin_stats(&self, id: &PluginId) -> WasmResult<(u64, f64)> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        let state = &instance.store.data().abi_state;
        Ok((state.process_count, state.average_process_time_us()))
    }

    /// Check if a plugin is initialized.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn is_plugin_initialized(&self, id: &PluginId) -> WasmResult<bool> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        Ok(instance.store.data().abi_state.is_initialized())
    }

    /// Check if a plugin is disabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn is_plugin_disabled(&self, id: &PluginId) -> WasmResult<bool> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        Ok(instance.is_disabled())
    }

    /// Get information about why a plugin was disabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn get_plugin_disabled_info(
        &self,
        id: &PluginId,
    ) -> WasmResult<Option<PluginDisabledInfo>> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        Ok(instance.disabled_info().cloned())
    }

    /// Re-enable a disabled plugin.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID to re-enable.
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the plugin was re-enabled, `Ok(false)` if it wasn't disabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin is not found.
    pub fn re_enable_plugin(&mut self, id: &PluginId) -> WasmResult<bool> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| WasmError::plugin_not_found(id))?;

        if instance.is_disabled() {
            tracing::info!(plugin_id = %id, "Re-enabling previously disabled WASM plugin");
            instance.re_enable();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Extract state to preserve from a plugin instance.
    fn extract_preserved_state(instance: &WasmPluginInstance) -> PreservedPluginState {
        let abi_state = &instance.store.data().abi_state;
        PreservedPluginState {
            plugin_data: abi_state.plugin_data.clone(),
            process_count: abi_state.process_count,
            total_process_time_us: abi_state.total_process_time_us,
        }
    }

    /// Restore preserved state to a plugin instance.
    fn restore_preserved_state(instance: &mut WasmPluginInstance, state: &PreservedPluginState) {
        let abi_state = &mut instance.store.data_mut().abi_state;
        abi_state.plugin_data = state.plugin_data.clone();
        abi_state.process_count = state.process_count;
        abi_state.total_process_time_us = state.total_process_time_us;
    }

    /// Hot-reload a plugin from bytes with state preservation.
    ///
    /// This method reloads a plugin while preserving:
    /// - Custom plugin data (plugin_data HashMap)
    /// - Statistics (process_count, total_process_time_us)
    ///
    /// If the reload fails, the old plugin remains active and an error is returned.
    ///
    /// # Arguments
    ///
    /// * `id` - The plugin ID to reload.
    /// * `wasm_bytes` - The new WASM module bytes.
    /// * `capabilities` - Capabilities for the new plugin instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the new module fails to compile or instantiate.
    pub fn reload_plugin(
        &mut self,
        id: &PluginId,
        wasm_bytes: &[u8],
        capabilities: Vec<String>,
    ) -> WasmResult<()> {
        let preserved_state = self.instances.get(id).map(Self::extract_preserved_state);

        let module = match Module::new(&self.engine, wasm_bytes) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    plugin_id = %id,
                    error = %e,
                    "Failed to compile new WASM module during reload, keeping old plugin"
                );
                return Err(WasmError::from(e));
            }
        };

        let wasi = wasmtime_wasi::WasiCtxBuilder::new().build_p1();
        let state = WasmPluginState::with_capabilities(wasi, capabilities);

        let mut store = Store::new(&self.engine, state);
        if let Err(e) = store.set_fuel(self.resource_limits.max_fuel) {
            tracing::warn!(
                plugin_id = %id,
                error = %e,
                "Failed to set fuel during reload, keeping old plugin"
            );
            return Err(WasmError::from(e));
        }
        store.set_epoch_deadline(1);

        let instance = match self.linker.instantiate(&mut store, &module) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!(
                    plugin_id = %id,
                    error = %e,
                    "Failed to instantiate new WASM module during reload, keeping old plugin"
                );
                return Err(WasmError::from(e));
            }
        };

        let validation = Self::validate_exports(&mut store, &instance);
        if !validation.is_valid() {
            let missing = validation.missing_required();
            tracing::warn!(
                plugin_id = %id,
                missing_exports = ?missing,
                "New plugin missing required exports during reload, keeping old plugin"
            );
            return Err(WasmError::MissingExport(missing.join(", ")));
        }

        let process_fn = instance
            .get_typed_func::<(f32, f32), f32>(&mut store, wasm_export::PROCESS)
            .ok();

        if validation.has_init {
            if let Ok(init_fn) =
                instance.get_typed_func::<(), i32>(&mut store, wasm_optional_export::INIT)
            {
                store.data_mut().abi_state.init_status =
                    openracing_plugin_abi::PluginInitStatus::Initializing;
                match init_fn.call(&mut store, ()) {
                    Ok(result) if result == return_code::SUCCESS => {
                        store.data_mut().abi_state.mark_initialized();
                    }
                    Ok(result) => {
                        store
                            .data_mut()
                            .abi_state
                            .mark_failed(format!("init() returned error code: {}", result));
                        tracing::warn!(
                            plugin_id = %id,
                            error_code = result,
                            "New plugin init() failed during reload, keeping old plugin"
                        );
                        return Err(WasmError::LoadingFailed(format!(
                            "Plugin init() returned error code: {}",
                            result
                        )));
                    }
                    Err(e) => {
                        store
                            .data_mut()
                            .abi_state
                            .mark_failed(format!("init() failed: {}", e));
                        tracing::warn!(
                            plugin_id = %id,
                            error = %e,
                            "New plugin init() trapped during reload, keeping old plugin"
                        );
                        return Err(WasmError::LoadingFailed(format!(
                            "Plugin init() failed: {}",
                            e
                        )));
                    }
                }
            }
        } else {
            store.data_mut().abi_state.mark_initialized();
        }

        let mut plugin_instance = WasmPluginInstance::new(store, instance, process_fn);

        if let Some(ref state) = preserved_state {
            Self::restore_preserved_state(&mut plugin_instance, state);
            tracing::debug!(
                plugin_id = %id,
                process_count = state.process_count,
                data_keys = state.plugin_data.len(),
                "Restored preserved state during hot-reload"
            );
        }

        if let Some(mut old_instance) = self.instances.remove(id) {
            if let Ok(shutdown_fn) = old_instance
                .instance
                .get_typed_func::<(), ()>(&mut old_instance.store, wasm_optional_export::SHUTDOWN)
            {
                let _ = shutdown_fn.call(&mut old_instance.store, ());
            }
            old_instance.store.data_mut().abi_state.mark_shutdown();
        }

        self.instances.insert(*id, plugin_instance);

        tracing::info!(
            plugin_id = %id,
            state_preserved = preserved_state.is_some(),
            "Hot-reloaded WASM plugin successfully"
        );
        Ok(())
    }

    /// Hot-reload a plugin from a file path with state preservation.
    ///
    /// # Arguments
    ///
    /// * `id` - The plugin ID to reload.
    /// * `path` - Path to the new WASM module file.
    /// * `capabilities` - Capabilities for the new plugin instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the reload fails.
    pub async fn reload_plugin_from_path(
        &mut self,
        id: &PluginId,
        path: &Path,
        capabilities: Vec<String>,
    ) -> WasmResult<()> {
        let wasm_bytes = tokio::fs::read(path).await.map_err(|e| {
            tracing::warn!(
                plugin_id = %id,
                path = %path.display(),
                error = %e,
                "Failed to read WASM file during reload, keeping old plugin"
            );
            WasmError::Io(e)
        })?;

        self.reload_plugin(id, &wasm_bytes, capabilities)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_runtime_creation() -> WasmResult<()> {
        let runtime = WasmRuntime::new()?;
        assert_eq!(runtime.instance_count(), 0);
        Ok(())
    }

    #[test]
    fn test_runtime_with_custom_limits() -> WasmResult<()> {
        let limits = ResourceLimits::default()
            .with_memory(8 * 1024 * 1024)
            .with_max_instances(16);

        let runtime = WasmRuntime::with_limits(limits)?;

        assert_eq!(runtime.resource_limits().max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(runtime.resource_limits().max_instances, 16);

        Ok(())
    }

    #[test]
    fn test_has_plugin() -> WasmResult<()> {
        let runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        assert!(!runtime.has_plugin(&plugin_id));

        Ok(())
    }

    #[test]
    fn test_unload_nonexistent_plugin() -> WasmResult<()> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        let result = runtime.unload_plugin(&plugin_id);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_process_nonexistent_plugin() -> WasmResult<()> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_load_and_process_plugin() -> WasmResult<()> {
        let working_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                    local.get 1
                    f32.add
                )
            )
        "#;

        let working_wasm =
            wat::parse_str(working_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        runtime.load_plugin_from_bytes(plugin_id, &working_wasm, vec![])?;

        let result = runtime.process(&plugin_id, 0.5, 0.001)?;
        assert!((result - 0.501).abs() < 0.001);

        runtime.unload_plugin(&plugin_id)?;

        Ok(())
    }

    #[test]
    fn test_plugin_trap_disables_plugin() -> WasmResult<()> {
        let trap_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    unreachable
                )
            )
        "#;

        let trap_wasm =
            wat::parse_str(trap_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        runtime.load_plugin_from_bytes(plugin_id, &trap_wasm, vec![])?;

        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_re_enable_plugin() -> WasmResult<()> {
        let trap_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    unreachable
                )
            )
        "#;

        let trap_wasm =
            wat::parse_str(trap_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        runtime.load_plugin_from_bytes(plugin_id, &trap_wasm, vec![])?;
        let _ = runtime.process(&plugin_id, 0.5, 0.001);

        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        let was_disabled = runtime.re_enable_plugin(&plugin_id)?;
        assert!(was_disabled);

        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        Ok(())
    }

    #[test]
    fn test_hot_reload_preserves_data() -> WasmResult<()> {
        let simple_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;

        let simple_wasm =
            wat::parse_str(simple_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        runtime.load_plugin_from_bytes(plugin_id, &simple_wasm, vec![])?;

        {
            let instance = runtime
                .instances
                .get_mut(&plugin_id)
                .ok_or_else(|| WasmError::plugin_not_found(plugin_id))?;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("test_key".to_string(), vec![1, 2, 3, 4, 5]);
        }

        let new_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                    local.get 1
                    f32.add
                )
            )
        "#;
        let new_wasm =
            wat::parse_str(new_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        runtime.reload_plugin(&plugin_id, &new_wasm, vec![])?;

        {
            let instance = runtime
                .instances
                .get(&plugin_id)
                .ok_or_else(|| WasmError::plugin_not_found(plugin_id))?;
            let data = instance
                .store
                .data()
                .abi_state
                .get_data("test_key")
                .ok_or_else(|| WasmError::LoadingFailed("Data not found".to_string()))?;
            assert_eq!(data, &vec![1, 2, 3, 4, 5]);
        }

        let result = runtime.process(&plugin_id, 1.0, 2.0)?;
        assert!((result - 3.0).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_hot_reload_failure_keeps_old_plugin() -> WasmResult<()> {
        let valid_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                    f32.const 42.0
                    f32.add
                )
            )
        "#;

        let valid_wasm =
            wat::parse_str(valid_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = Uuid::new_v4();

        runtime.load_plugin_from_bytes(plugin_id, &valid_wasm, vec![])?;

        let result_before = runtime.process(&plugin_id, 1.0, 0.001)?;
        assert!((result_before - 43.0).abs() < 0.001);

        let invalid_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "not_process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;
        let invalid_wasm =
            wat::parse_str(invalid_wat).map_err(|e| WasmError::InvalidModule(e.to_string()))?;

        let reload_result = runtime.reload_plugin(&plugin_id, &invalid_wasm, vec![]);
        assert!(reload_result.is_err());

        let result_after = runtime.process(&plugin_id, 1.0, 0.001)?;
        assert!((result_after - 43.0).abs() < 0.001);

        Ok(())
    }
}
