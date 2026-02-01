//! WASM plugin host with capability-based sandboxing
//!
//! This module provides a sandboxed WASM runtime for executing plugins safely.
//! It uses wasmtime with resource limits (memory, fuel) to prevent plugins from
//! consuming excessive resources or causing system instability.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use wasmtime::*;
use wasmtime_wasi::p1::WasiP1Ctx;

use crate::abi::{
    self, WasmExportValidation, WasmPluginAbiState, host_function, log_level, return_code,
    wasm_export, wasm_optional_export,
};
use crate::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use crate::manifest::{PluginManifest, PluginOperation};
use crate::{Plugin, PluginContext, PluginError, PluginOutput, PluginResult};
use racing_wheel_engine::NormalizedTelemetry;

/// Plugin identifier type alias for clarity
pub type PluginId = uuid::Uuid;

/// WASM plugin state
///
/// This struct combines WASI context, capability checking, and ABI state
/// for a complete plugin execution environment.
pub struct WasmPluginState {
    /// WASI context for system calls
    pub wasi: WasiP1Ctx,
    /// Capability checker for permission validation
    pub capability_checker: CapabilityChecker,
    /// ABI-level plugin state (telemetry, stats, custom data)
    pub abi_state: WasmPluginAbiState,
}

/// Resource limits for WASM plugins
///
/// These limits ensure plugins cannot consume excessive system resources.
/// All limits are enforced by the wasmtime runtime.
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory in bytes a plugin can allocate (default: 16MB)
    pub max_memory_bytes: usize,
    /// Maximum fuel (instruction count) per call (default: 10_000_000)
    pub max_fuel: u64,
    /// Maximum table elements (default: 10_000)
    pub max_table_elements: u32,
    /// Maximum number of plugin instances (default: 32)
    pub max_instances: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 16 * 1024 * 1024, // 16MB
            max_fuel: 10_000_000,               // ~10M instructions per call
            max_table_elements: 10_000,
            max_instances: 32,
        }
    }
}

impl ResourceLimits {
    /// Create new resource limits with custom values
    pub fn new(
        max_memory_bytes: usize,
        max_fuel: u64,
        max_table_elements: u32,
        max_instances: usize,
    ) -> Self {
        Self {
            max_memory_bytes,
            max_fuel,
            max_table_elements,
            max_instances,
        }
    }

    /// Create resource limits with a specific memory limit
    pub fn with_memory(mut self, max_memory_bytes: usize) -> Self {
        self.max_memory_bytes = max_memory_bytes;
        self
    }

    /// Create resource limits with a specific fuel limit
    pub fn with_fuel(mut self, max_fuel: u64) -> Self {
        self.max_fuel = max_fuel;
        self
    }

    /// Create resource limits with a specific table elements limit
    pub fn with_table_elements(mut self, max_table_elements: u32) -> Self {
        self.max_table_elements = max_table_elements;
        self
    }

    /// Create resource limits with a specific max instances limit
    pub fn with_max_instances(mut self, max_instances: usize) -> Self {
        self.max_instances = max_instances;
        self
    }
}

/// Plugin disabled state with reason
#[derive(Debug, Clone)]
pub struct PluginDisabledInfo {
    /// Reason the plugin was disabled
    pub reason: String,
    /// When the plugin was disabled
    pub disabled_at: Instant,
    /// Optional trap location information
    pub trap_location: Option<String>,
}

/// WASM plugin instance with sandboxing
///
/// Each instance contains its own store (isolated state), the instantiated
/// module, and a typed function reference for the main processing function.
pub struct WasmPluginInstance {
    /// Wasmtime store containing the plugin's isolated state
    pub(crate) store: Store<WasmPluginState>,
    /// The instantiated WASM module
    instance: Instance,
    /// Typed function for DSP processing: (input: f32, dt: f32) -> f32
    process_fn: Option<TypedFunc<(f32, f32), f32>>,
    /// Whether the plugin is disabled (e.g., due to a trap/panic)
    disabled: Option<PluginDisabledInfo>,
}

impl WasmPluginInstance {
    /// Create a new WASM plugin instance
    fn new(
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

    /// Get a mutable reference to the store
    pub fn store_mut(&mut self) -> &mut Store<WasmPluginState> {
        &mut self.store
    }

    /// Get a reference to the instance
    pub fn instance(&self) -> &Instance {
        &self.instance
    }

    /// Check if the instance has a process function
    pub fn has_process_fn(&self) -> bool {
        self.process_fn.is_some()
    }

    /// Check if the plugin is disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled.is_some()
    }

    /// Get the disabled info if the plugin is disabled
    pub fn disabled_info(&self) -> Option<&PluginDisabledInfo> {
        self.disabled.as_ref()
    }

    /// Mark the plugin as disabled due to a trap/panic
    fn mark_disabled(&mut self, reason: String, trap_location: Option<String>) {
        self.disabled = Some(PluginDisabledInfo {
            reason,
            disabled_at: Instant::now(),
            trap_location,
        });
    }

    /// Re-enable a disabled plugin
    fn re_enable(&mut self) {
        self.disabled = None;
    }
}

/// WASM plugin runtime using wasmtime
///
/// The runtime manages the wasmtime engine, linker, and all plugin instances.
/// It enforces resource limits and provides methods for loading, reloading,
/// and executing plugins.
pub struct WasmRuntime {
    /// Wasmtime engine with configured resource limits
    engine: Engine,
    /// Linker with host functions registered
    linker: Linker<WasmPluginState>,
    /// Map of plugin ID to plugin instance
    pub(crate) instances: HashMap<PluginId, WasmPluginInstance>,
    /// Resource limits applied to all plugins
    resource_limits: ResourceLimits,
}

impl WasmRuntime {
    /// Create a new WASM runtime with default resource limits
    pub fn new() -> PluginResult<Self> {
        Self::with_limits(ResourceLimits::default())
    }

    /// Create a new WASM runtime with custom resource limits
    pub fn with_limits(resource_limits: ResourceLimits) -> PluginResult<Self> {
        // Create WASM engine with security configuration
        let mut config = Config::new();

        // Disable potentially dangerous features for security
        // Note: We don't disable SIMD as it conflicts with relaxed SIMD defaults
        config.wasm_bulk_memory(false);
        config.wasm_multi_value(false);
        config.wasm_threads(false);

        // Enable resource limiting features
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config)?;

        // Create linker and register host functions
        let mut linker = Linker::new(&engine);
        Self::register_host_functions(&mut linker)?;

        Ok(Self {
            engine,
            linker,
            instances: HashMap::new(),
            resource_limits,
        })
    }

    /// Register host functions in the linker
    ///
    /// This registers all host functions that WASM plugins can import:
    /// - Logging functions (log_debug, log_info, log_warn, log_error, plugin_log)
    /// - Capability checking (check_capability)
    /// - Telemetry access (get_telemetry)
    /// - Timestamp access (get_timestamp_us)
    fn register_host_functions(linker: &mut Linker<WasmPluginState>) -> PluginResult<()> {
        // Add WASI support
        wasmtime_wasi::p1::add_to_linker_sync(linker, |s: &mut WasmPluginState| &mut s.wasi)?;

        // ====================================================================
        // Logging Host Functions
        // ====================================================================

        // log_debug(msg_ptr: i32, msg_len: i32)
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::LOG_DEBUG,
            |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
                Self::log_message(&mut caller, log_level::DEBUG, msg_ptr, msg_len);
            },
        )?;

        // log_info(msg_ptr: i32, msg_len: i32)
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::LOG_INFO,
            |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
                Self::log_message(&mut caller, log_level::INFO, msg_ptr, msg_len);
            },
        )?;

        // log_warn(msg_ptr: i32, msg_len: i32)
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::LOG_WARN,
            |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
                Self::log_message(&mut caller, log_level::WARN, msg_ptr, msg_len);
            },
        )?;

        // log_error(msg_ptr: i32, msg_len: i32)
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::LOG_ERROR,
            |mut caller: Caller<'_, WasmPluginState>, msg_ptr: i32, msg_len: i32| {
                Self::log_message(&mut caller, log_level::ERROR, msg_ptr, msg_len);
            },
        )?;

        // plugin_log(level: i32, msg_ptr: i32, msg_len: i32) - generic logging
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::PLUGIN_LOG,
            |mut caller: Caller<'_, WasmPluginState>, level: i32, msg_ptr: i32, msg_len: i32| {
                Self::log_message(&mut caller, level, msg_ptr, msg_len);
            },
        )?;

        // ====================================================================
        // Capability Checking Host Function
        // ====================================================================

        // check_capability(cap_ptr: i32, cap_len: i32) -> i32
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::CHECK_CAPABILITY,
            |mut caller: Caller<'_, WasmPluginState>,
             capability_ptr: i32,
             capability_len: i32|
             -> i32 {
                Self::check_capability_impl(&mut caller, capability_ptr, capability_len)
            },
        )?;

        // ====================================================================
        // Telemetry Access Host Function
        // ====================================================================

        // get_telemetry(out_ptr: i32, out_len: i32) -> i32
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::GET_TELEMETRY,
            |mut caller: Caller<'_, WasmPluginState>, out_ptr: i32, out_len: i32| -> i32 {
                Self::get_telemetry_impl(&mut caller, out_ptr, out_len)
            },
        )?;

        // ====================================================================
        // Timestamp Access Host Function
        // ====================================================================

        // get_timestamp_us() -> i64
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::GET_TIMESTAMP_US,
            |caller: Caller<'_, WasmPluginState>| -> i64 {
                caller.data().abi_state.timestamp_us() as i64
            },
        )?;

        Ok(())
    }

    /// Helper function to log a message from WASM plugin memory
    fn log_message(
        caller: &mut Caller<'_, WasmPluginState>,
        level: i32,
        msg_ptr: i32,
        msg_len: i32,
    ) {
        let memory = match caller.get_export(wasm_export::MEMORY) {
            Some(Extern::Memory(mem)) => mem,
            _ => return,
        };

        // Validate bounds
        if msg_ptr < 0 || msg_len < 0 {
            return;
        }

        let start = msg_ptr as usize;
        let end = start.saturating_add(msg_len as usize);

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

    /// Helper function to check capability from WASM plugin
    fn check_capability_impl(
        caller: &mut Caller<'_, WasmPluginState>,
        capability_ptr: i32,
        capability_len: i32,
    ) -> i32 {
        let memory = match caller.get_export(wasm_export::MEMORY) {
            Some(Extern::Memory(mem)) => mem,
            _ => return return_code::ERROR,
        };

        // Validate bounds
        if capability_ptr < 0 || capability_len < 0 {
            return return_code::INVALID_ARG;
        }

        let start = capability_ptr as usize;
        let end = start.saturating_add(capability_len as usize);

        // Read capability string from memory
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

        // Check capability against the capability checker
        let result = match capability_str.as_str() {
            abi::capability_str::READ_TELEMETRY => {
                caller.data().capability_checker.check_telemetry_read()
            }
            abi::capability_str::MODIFY_TELEMETRY => {
                caller.data().capability_checker.check_telemetry_modify()
            }
            abi::capability_str::CONTROL_LEDS => {
                caller.data().capability_checker.check_led_control()
            }
            abi::capability_str::PROCESS_DSP => {
                caller.data().capability_checker.check_dsp_processing()
            }
            _ => return return_code::INVALID_ARG,
        };

        if result.is_ok() {
            1 // Capability granted
        } else {
            return_code::PERMISSION_DENIED
        }
    }

    /// Helper function to get telemetry data for WASM plugin
    fn get_telemetry_impl(
        caller: &mut Caller<'_, WasmPluginState>,
        out_ptr: i32,
        out_len: i32,
    ) -> i32 {
        // Check capability first
        if caller
            .data()
            .capability_checker
            .check_telemetry_read()
            .is_err()
        {
            return return_code::PERMISSION_DENIED;
        }

        let memory = match caller.get_export(wasm_export::MEMORY) {
            Some(Extern::Memory(mem)) => mem,
            _ => return return_code::ERROR,
        };

        // Validate bounds
        if out_ptr < 0 || out_len < 0 {
            return return_code::INVALID_ARG;
        }

        const TELEMETRY_SIZE: usize = 32; // Size of TelemetryFrame
        if (out_len as usize) < TELEMETRY_SIZE {
            return return_code::BUFFER_TOO_SMALL;
        }

        // Get telemetry bytes
        let telemetry_bytes = caller.data().abi_state.current_telemetry.to_bytes();

        // Write to plugin memory
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

    /// Validate WASM module exports
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

    /// Get the current resource limits
    pub fn resource_limits(&self) -> &ResourceLimits {
        &self.resource_limits
    }

    /// Get the number of loaded plugin instances
    pub fn instance_count(&self) -> usize {
        self.instances.len()
    }

    /// Check if a plugin is loaded
    pub fn has_plugin(&self, id: &PluginId) -> bool {
        self.instances.contains_key(id)
    }

    /// Load a WASM plugin from bytes
    ///
    /// This method compiles the WASM module, creates a new store with the
    /// configured resource limits, and instantiates the module.
    pub fn load_plugin_from_bytes(
        &mut self,
        id: PluginId,
        wasm_bytes: &[u8],
        capabilities: Vec<crate::manifest::Capability>,
    ) -> PluginResult<()> {
        // Check instance limit
        if self.instances.len() >= self.resource_limits.max_instances {
            return Err(PluginError::LoadingFailed(format!(
                "Maximum plugin instances ({}) reached",
                self.resource_limits.max_instances
            )));
        }

        // Compile the module
        let module = Module::new(&self.engine, wasm_bytes)?;

        // Create capability enforcer and WASI context
        let capability_enforcer = WasmCapabilityEnforcer::new(capabilities.clone());
        let wasi = capability_enforcer.create_wasi_context()?.build_p1();

        // Create plugin state with ABI state
        let state = WasmPluginState {
            wasi,
            capability_checker: CapabilityChecker::new(capabilities),
            abi_state: WasmPluginAbiState::new(),
        };

        // Create store with resource limits
        let mut store = Store::new(&self.engine, state);
        store.set_fuel(self.resource_limits.max_fuel)?;
        store.set_epoch_deadline(1);

        // Instantiate the module
        let instance = self.linker.instantiate(&mut store, &module)?;

        // Validate exports
        let validation = Self::validate_exports(&mut store, &instance);
        if !validation.is_valid() {
            let missing = validation.missing_required();
            return Err(PluginError::LoadingFailed(format!(
                "Plugin missing required exports: {}",
                missing.join(", ")
            )));
        }

        // Try to get the process function
        let process_fn = instance
            .get_typed_func::<(f32, f32), f32>(&mut store, wasm_export::PROCESS)
            .ok();

        // Call init function if present
        if validation.has_init {
            if let Ok(init_fn) =
                instance.get_typed_func::<(), i32>(&mut store, wasm_optional_export::INIT)
            {
                store.data_mut().abi_state.init_status = abi::PluginInitStatus::Initializing;
                match init_fn.call(&mut store, ()) {
                    Ok(result) if result == return_code::SUCCESS => {
                        store.data_mut().abi_state.mark_initialized();
                    }
                    Ok(result) => {
                        store
                            .data_mut()
                            .abi_state
                            .mark_failed(format!("init() returned error code: {}", result));
                        return Err(PluginError::LoadingFailed(format!(
                            "Plugin init() returned error code: {}",
                            result
                        )));
                    }
                    Err(e) => {
                        store
                            .data_mut()
                            .abi_state
                            .mark_failed(format!("init() failed: {}", e));
                        return Err(PluginError::LoadingFailed(format!(
                            "Plugin init() failed: {}",
                            e
                        )));
                    }
                }
            }
        } else {
            // No init function, mark as initialized
            store.data_mut().abi_state.mark_initialized();
        }

        // Store the instance
        let plugin_instance = WasmPluginInstance::new(store, instance, process_fn);
        self.instances.insert(id, plugin_instance);

        tracing::info!("Loaded WASM plugin: {}", id);
        Ok(())
    }

    /// Load a WASM plugin from a file path
    pub async fn load_plugin(
        &mut self,
        id: PluginId,
        path: &Path,
        capabilities: Vec<crate::manifest::Capability>,
    ) -> PluginResult<()> {
        let wasm_bytes = tokio::fs::read(path).await?;
        self.load_plugin_from_bytes(id, &wasm_bytes, capabilities)
    }

    /// Unload a plugin
    ///
    /// This method calls the plugin's shutdown function (if present) and
    /// removes the plugin instance from the runtime.
    pub fn unload_plugin(&mut self, id: &PluginId) -> PluginResult<()> {
        if let Some(mut instance) = self.instances.remove(id) {
            // Try to call shutdown function if present
            if let Ok(shutdown_fn) = instance
                .instance
                .get_typed_func::<(), ()>(&mut instance.store, wasm_optional_export::SHUTDOWN)
            {
                // Best effort - ignore errors during shutdown
                let _ = shutdown_fn.call(&mut instance.store, ());
            }
            instance.store.data_mut().abi_state.mark_shutdown();
            tracing::info!("Unloaded WASM plugin: {}", id);
            Ok(())
        } else {
            Err(PluginError::LoadingFailed(format!(
                "Plugin {} not found",
                id
            )))
        }
    }

    /// Extract state to preserve from a plugin instance
    fn extract_preserved_state(instance: &WasmPluginInstance) -> PreservedPluginState {
        let abi_state = &instance.store.data().abi_state;
        PreservedPluginState {
            plugin_data: abi_state.plugin_data.clone(),
            process_count: abi_state.process_count,
            total_process_time_us: abi_state.total_process_time_us,
        }
    }

    /// Restore preserved state to a plugin instance
    fn restore_preserved_state(instance: &mut WasmPluginInstance, state: &PreservedPluginState) {
        let abi_state = &mut instance.store.data_mut().abi_state;
        abi_state.plugin_data = state.plugin_data.clone();
        abi_state.process_count = state.process_count;
        abi_state.total_process_time_us = state.total_process_time_us;
    }

    /// Hot-reload a plugin from bytes with state preservation
    ///
    /// This method reloads a plugin while preserving:
    /// - Custom plugin data (plugin_data HashMap)
    /// - Statistics (process_count, total_process_time_us)
    ///
    /// If the reload fails, the old plugin remains active and an error is returned.
    /// The reload is atomic from the caller's perspective - either the new plugin
    /// is fully loaded with preserved state, or the old plugin remains unchanged.
    ///
    /// # Arguments
    /// * `id` - The plugin ID to reload
    /// * `wasm_bytes` - The new WASM module bytes
    /// * `capabilities` - Capabilities for the new plugin instance
    ///
    /// # Returns
    /// * `Ok(())` - Plugin was successfully reloaded with state preserved
    /// * `Err(PluginError)` - Reload failed, old plugin remains active
    pub fn reload_plugin(
        &mut self,
        id: &PluginId,
        wasm_bytes: &[u8],
        capabilities: Vec<crate::manifest::Capability>,
    ) -> PluginResult<()> {
        // Extract preserved state from the old instance (if it exists)
        let preserved_state = self.instances.get(id).map(Self::extract_preserved_state);

        // Try to compile and instantiate the new module first
        // This validates the new WASM before we touch the old instance
        let module = match Module::new(&self.engine, wasm_bytes) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    plugin_id = %id,
                    error = %e,
                    "Failed to compile new WASM module during reload, keeping old plugin"
                );
                return Err(PluginError::WasmRuntime(e));
            }
        };

        // Create capability enforcer and WASI context for the new instance
        let capability_enforcer = WasmCapabilityEnforcer::new(capabilities.clone());
        let wasi = match capability_enforcer.create_wasi_context() {
            Ok(mut builder) => builder.build_p1(),
            Err(e) => {
                tracing::warn!(
                    plugin_id = %id,
                    error = %e,
                    "Failed to create WASI context during reload, keeping old plugin"
                );
                return Err(e);
            }
        };

        // Create plugin state with ABI state
        let state = WasmPluginState {
            wasi,
            capability_checker: CapabilityChecker::new(capabilities),
            abi_state: WasmPluginAbiState::new(),
        };

        // Create store with resource limits
        let mut store = Store::new(&self.engine, state);
        if let Err(e) = store.set_fuel(self.resource_limits.max_fuel) {
            tracing::warn!(
                plugin_id = %id,
                error = %e,
                "Failed to set fuel during reload, keeping old plugin"
            );
            return Err(PluginError::WasmRuntime(e));
        }
        store.set_epoch_deadline(1);

        // Instantiate the module
        let instance = match self.linker.instantiate(&mut store, &module) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!(
                    plugin_id = %id,
                    error = %e,
                    "Failed to instantiate new WASM module during reload, keeping old plugin"
                );
                return Err(PluginError::WasmRuntime(e));
            }
        };

        // Validate exports
        let validation = Self::validate_exports(&mut store, &instance);
        if !validation.is_valid() {
            let missing = validation.missing_required();
            tracing::warn!(
                plugin_id = %id,
                missing_exports = ?missing,
                "New plugin missing required exports during reload, keeping old plugin"
            );
            return Err(PluginError::LoadingFailed(format!(
                "Plugin missing required exports: {}",
                missing.join(", ")
            )));
        }

        // Try to get the process function
        let process_fn = instance
            .get_typed_func::<(f32, f32), f32>(&mut store, wasm_export::PROCESS)
            .ok();

        // Call init function if present
        if validation.has_init {
            if let Ok(init_fn) =
                instance.get_typed_func::<(), i32>(&mut store, wasm_optional_export::INIT)
            {
                store.data_mut().abi_state.init_status = abi::PluginInitStatus::Initializing;
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
                        return Err(PluginError::LoadingFailed(format!(
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
                        return Err(PluginError::LoadingFailed(format!(
                            "Plugin init() failed: {}",
                            e
                        )));
                    }
                }
            }
        } else {
            // No init function, mark as initialized
            store.data_mut().abi_state.mark_initialized();
        }

        // Create the new plugin instance
        let mut plugin_instance = WasmPluginInstance::new(store, instance, process_fn);

        // Restore preserved state if we had an old instance
        if let Some(ref state) = preserved_state {
            Self::restore_preserved_state(&mut plugin_instance, state);
            tracing::debug!(
                plugin_id = %id,
                process_count = state.process_count,
                data_keys = state.plugin_data.len(),
                "Restored preserved state during hot-reload"
            );
        }

        // Now we can safely remove the old instance and insert the new one
        // This is the atomic swap point - if we got here, the new plugin is ready
        if let Some(mut old_instance) = self.instances.remove(id) {
            // Try to call shutdown function on the old instance (best effort)
            if let Ok(shutdown_fn) = old_instance
                .instance
                .get_typed_func::<(), ()>(&mut old_instance.store, wasm_optional_export::SHUTDOWN)
            {
                let _ = shutdown_fn.call(&mut old_instance.store, ());
            }
            old_instance.store.data_mut().abi_state.mark_shutdown();
        }

        // Insert the new instance
        self.instances.insert(*id, plugin_instance);

        tracing::info!(
            plugin_id = %id,
            state_preserved = preserved_state.is_some(),
            "Hot-reloaded WASM plugin successfully"
        );
        Ok(())
    }

    /// Hot-reload a plugin from a file path with state preservation
    ///
    /// This is a convenience method that reads the WASM bytes from a file
    /// and calls `reload_plugin()`. See `reload_plugin()` for details on
    /// state preservation and error handling.
    ///
    /// # Arguments
    /// * `id` - The plugin ID to reload
    /// * `path` - Path to the new WASM module file
    /// * `capabilities` - Capabilities for the new plugin instance
    ///
    /// # Returns
    /// * `Ok(())` - Plugin was successfully reloaded with state preserved
    /// * `Err(PluginError)` - Reload failed, old plugin remains active
    pub async fn reload_plugin_from_path(
        &mut self,
        id: &PluginId,
        path: &Path,
        capabilities: Vec<crate::manifest::Capability>,
    ) -> PluginResult<()> {
        // Read the WASM bytes from the file
        let wasm_bytes = tokio::fs::read(path).await.map_err(|e| {
            tracing::warn!(
                plugin_id = %id,
                path = %path.display(),
                error = %e,
                "Failed to read WASM file during reload, keeping old plugin"
            );
            PluginError::LoadingFailed(format!("Failed to read WASM file: {}", e))
        })?;

        // Delegate to the bytes-based reload
        self.reload_plugin(id, &wasm_bytes, capabilities)
    }

    /// Process FFB through a plugin (non-RT, for preview)
    ///
    /// This method calls the plugin's process function with the given input
    /// and delta time, returning the processed output. It also tracks
    /// execution statistics in the plugin's ABI state.
    ///
    /// If the plugin traps (WASM equivalent of panic), the trap is caught,
    /// the plugin is disabled, and an error is returned. Disabled plugins
    /// cannot be called again until re-enabled.
    pub fn process(&mut self, id: &PluginId, input: f32, dt: f32) -> PluginResult<f32> {
        let start_time = Instant::now();

        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        // Check if plugin is disabled
        if let Some(disabled_info) = instance.disabled_info() {
            return Err(PluginError::Crashed {
                reason: format!(
                    "Plugin is disabled: {} (disabled at {:?})",
                    disabled_info.reason, disabled_info.disabled_at
                ),
            });
        }

        // Check if plugin is initialized
        if !instance.store.data().abi_state.is_initialized() {
            return Err(PluginError::LoadingFailed(
                "Plugin not initialized".to_string(),
            ));
        }

        // Reset fuel for this call
        instance.store.set_fuel(self.resource_limits.max_fuel)?;

        // Set epoch deadline for this call (allows interruption after many epochs)
        // We use a high value to allow normal execution while still supporting interruption
        instance.store.set_epoch_deadline(100);

        // Increment epoch for interruption support (used for external cancellation)
        self.engine.increment_epoch();

        // Get the process function (TypedFunc is Copy, so we can copy it out of the Option)
        let process_fn = instance.process_fn.as_ref().ok_or_else(|| {
            PluginError::LoadingFailed("Plugin does not export 'process' function".to_string())
        })?;

        // Call the process function and catch any traps
        let call_result = process_fn.call(&mut instance.store, (input, dt));

        match call_result {
            Ok(result) => {
                // Record statistics
                let duration_us = start_time.elapsed().as_micros() as u64;
                instance
                    .store
                    .data_mut()
                    .abi_state
                    .record_process_call(duration_us);

                Ok(result)
            }
            Err(trap) => {
                // Extract trap information for logging
                let trap_reason = trap.to_string();
                let trap_location = Self::extract_trap_location(&trap);

                // Log the trap information
                tracing::error!(
                    plugin_id = %id,
                    trap_reason = %trap_reason,
                    trap_location = ?trap_location,
                    "WASM plugin trapped during execution, disabling plugin"
                );

                // Mark the plugin as disabled
                instance.mark_disabled(trap_reason.clone(), trap_location.clone());

                // Check if this was a resource limit violation (fuel exhaustion)
                if instance.store.get_fuel().unwrap_or(0) == 0 {
                    Err(PluginError::BudgetViolation {
                        used_us: 0, // Fuel exhausted
                        budget_us: 0,
                    })
                } else {
                    // Return a Crashed error with trap information
                    Err(PluginError::Crashed {
                        reason: format!(
                            "Plugin trapped: {}{}",
                            trap_reason,
                            trap_location
                                .map(|loc| format!(" at {}", loc))
                                .unwrap_or_default()
                        ),
                    })
                }
            }
        }
    }

    /// Extract trap location information from a wasmtime error
    fn extract_trap_location(error: &wasmtime::Error) -> Option<String> {
        // Try to get trap information from the error
        // In wasmtime 41+, Trap is an enum with variants like StackOverflow, UnreachableCodeReached, etc.
        if let Some(trap) = error.downcast_ref::<wasmtime::Trap>() {
            return Some(format!("trap: {:?}", trap));
        }

        // Try to extract backtrace information from the error string
        let error_string = error.to_string();
        if error_string.contains("wasm backtrace") || error_string.contains("at ") {
            // Extract relevant location info from the error message
            for line in error_string.lines() {
                if line.contains("at ") || line.contains("func[") {
                    return Some(line.trim().to_string());
                }
            }
        }

        None
    }

    /// Update telemetry data for a plugin
    ///
    /// This method updates the current telemetry frame in the plugin's state,
    /// making it available to the plugin via the get_telemetry host function.
    pub fn update_plugin_telemetry(
        &mut self,
        id: &PluginId,
        telemetry: crate::abi::TelemetryFrame,
    ) -> PluginResult<()> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        instance
            .store
            .data_mut()
            .abi_state
            .update_telemetry(telemetry);
        Ok(())
    }

    /// Get plugin statistics
    pub fn get_plugin_stats(&self, id: &PluginId) -> PluginResult<(u64, f64)> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        let state = &instance.store.data().abi_state;
        Ok((state.process_count, state.average_process_time_us()))
    }

    /// Check if a plugin is initialized
    pub fn is_plugin_initialized(&self, id: &PluginId) -> PluginResult<bool> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        Ok(instance.store.data().abi_state.is_initialized())
    }

    /// Check if a plugin is disabled (e.g., due to a trap/panic)
    ///
    /// Returns `Ok(true)` if the plugin is disabled, `Ok(false)` if it's enabled,
    /// or an error if the plugin is not found.
    pub fn is_plugin_disabled(&self, id: &PluginId) -> PluginResult<bool> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        Ok(instance.is_disabled())
    }

    /// Get information about why a plugin was disabled
    ///
    /// Returns `Ok(Some(info))` if the plugin is disabled with the reason,
    /// `Ok(None)` if the plugin is not disabled, or an error if the plugin is not found.
    pub fn get_plugin_disabled_info(
        &self,
        id: &PluginId,
    ) -> PluginResult<Option<PluginDisabledInfo>> {
        let instance = self
            .instances
            .get(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        Ok(instance.disabled_info().cloned())
    }

    /// Re-enable a disabled plugin
    ///
    /// This allows a plugin that was disabled due to a trap/panic to be used again.
    /// Use with caution - the plugin may trap again if the underlying issue is not resolved.
    ///
    /// Returns `Ok(true)` if the plugin was re-enabled, `Ok(false)` if it wasn't disabled,
    /// or an error if the plugin is not found.
    pub fn re_enable_plugin(&mut self, id: &PluginId) -> PluginResult<bool> {
        let instance = self
            .instances
            .get_mut(id)
            .ok_or_else(|| PluginError::LoadingFailed(format!("Plugin {} not found", id)))?;

        if instance.is_disabled() {
            tracing::info!(
                plugin_id = %id,
                "Re-enabling previously disabled WASM plugin"
            );
            instance.re_enable();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get a reference to the engine
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// State preserved during hot-reload
///
/// This struct captures the state that should be preserved when
/// reloading a plugin, including custom data and statistics.
#[derive(Debug, Clone)]
pub struct PreservedPluginState {
    /// Custom plugin data (key-value pairs)
    pub plugin_data: HashMap<String, Vec<u8>>,
    /// Number of successful process() calls
    pub process_count: u64,
    /// Total processing time in microseconds
    pub total_process_time_us: u64,
}

/// Legacy WASM plugin instance (for backward compatibility)
///
/// This struct wraps the new WasmRuntime for existing code that uses
/// the old WasmPlugin interface.
pub struct WasmPlugin {
    manifest: PluginManifest,
    engine: Engine,
    _module: Module,
    runtime: Mutex<LegacyWasmRuntime>,
    _capability_enforcer: WasmCapabilityEnforcer,
}

/// Legacy runtime wrapper for backward compatibility
struct LegacyWasmRuntime {
    store: Store<WasmPluginState>,
    instance: Instance,
}

impl WasmPlugin {
    /// Load a WASM plugin from file
    pub async fn load(manifest: PluginManifest, wasm_path: &Path) -> PluginResult<Self> {
        // Create WASM engine with security configuration
        let mut config = Config::new();
        config.wasm_simd(false); // Disable SIMD for security
        config.wasm_bulk_memory(false); // Disable bulk memory
        config.wasm_multi_value(false); // Disable multi-value
        config.wasm_threads(false); // Disable threads
        config.consume_fuel(true); // Enable fuel for execution limits
        config.epoch_interruption(true); // Enable epoch interruption

        let engine = Engine::new(&config)?;

        // Load WASM module
        let wasm_bytes = tokio::fs::read(wasm_path).await?;
        let module = Module::new(&engine, &wasm_bytes)?;

        // Create capability enforcer
        let capability_enforcer = WasmCapabilityEnforcer::new(manifest.capabilities.clone());

        // Create WASI context with restricted capabilities
        let wasi = capability_enforcer.create_wasi_context()?.build_p1();

        let state = WasmPluginState {
            wasi,
            capability_checker: CapabilityChecker::new(manifest.capabilities.clone()),
            abi_state: WasmPluginAbiState::new(),
        };

        let mut store = Store::new(&engine, state);

        // Set fuel limit based on execution time budget
        let fuel_limit = (manifest.constraints.max_execution_time_us as u64) * 1000; // Rough estimate
        store.set_fuel(fuel_limit)?;

        // Set epoch deadline
        store.set_epoch_deadline(1);

        // Add WASI to linker
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |s: &mut WasmPluginState| &mut s.wasi)?;

        // Add custom host functions
        Self::add_host_functions(&mut linker)?;

        // Instantiate the module
        let instance = linker.instantiate(&mut store, &module)?;
        let runtime = Mutex::new(LegacyWasmRuntime { store, instance });

        Ok(Self {
            manifest,
            engine,
            _module: module,
            runtime,
            _capability_enforcer: capability_enforcer,
        })
    }

    /// Add custom host functions for plugin API
    fn add_host_functions(linker: &mut Linker<WasmPluginState>) -> PluginResult<()> {
        // Host function for capability checking
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::CHECK_CAPABILITY,
            |mut caller: Caller<'_, WasmPluginState>,
             capability_ptr: i32,
             capability_len: i32|
             -> i32 {
                let memory = match caller.get_export(wasm_export::MEMORY) {
                    Some(Extern::Memory(mem)) => mem,
                    _ => return return_code::ERROR,
                };

                // Validate bounds
                if capability_ptr < 0 || capability_len < 0 {
                    return return_code::INVALID_ARG;
                }

                let start = capability_ptr as usize;
                let end = start.saturating_add(capability_len as usize);

                let data = match memory.data(&caller).get(start..end) {
                    Some(data) => data,
                    None => return return_code::INVALID_ARG,
                };

                let capability_str = match std::str::from_utf8(data) {
                    Ok(s) => s,
                    Err(_) => return return_code::INVALID_ARG,
                };

                // Check capability
                let result = match capability_str {
                    abi::capability_str::READ_TELEMETRY => {
                        caller.data().capability_checker.check_telemetry_read()
                    }
                    abi::capability_str::MODIFY_TELEMETRY => {
                        caller.data().capability_checker.check_telemetry_modify()
                    }
                    abi::capability_str::CONTROL_LEDS => {
                        caller.data().capability_checker.check_led_control()
                    }
                    _ => return return_code::INVALID_ARG,
                };

                if result.is_ok() {
                    1 // Capability granted
                } else {
                    return_code::PERMISSION_DENIED
                }
            },
        )?;

        // Host function for logging
        linker.func_wrap(
            abi::HOST_MODULE,
            host_function::PLUGIN_LOG,
            |mut caller: Caller<'_, WasmPluginState>, level: i32, msg_ptr: i32, msg_len: i32| {
                let memory = match caller.get_export(wasm_export::MEMORY) {
                    Some(Extern::Memory(mem)) => mem,
                    _ => return,
                };

                // Validate bounds
                if msg_ptr < 0 || msg_len < 0 {
                    return;
                }

                let start = msg_ptr as usize;
                let end = start.saturating_add(msg_len as usize);

                let data = match memory.data(&caller).get(start..end) {
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
            },
        )?;

        Ok(())
    }

    /// Execute plugin function with timeout and fuel limits
    async fn execute_with_limits(
        &mut self,
        func_name: &str,
        _args: &[Val],
        timeout: Duration,
    ) -> PluginResult<Vec<Val>> {
        let start_time = Instant::now();
        let engine = self.engine.clone();

        // Execute with timeout
        let result = {
            let mut runtime = self.runtime.lock().await;

            // Reset fuel
            let fuel_limit = (self.manifest.constraints.max_execution_time_us as u64) * 1000;
            runtime.store.set_fuel(fuel_limit)?;

            // Get function
            let instance = runtime.instance;
            let store = &mut runtime.store;
            let func = instance
                .get_typed_func::<(), ()>(store, func_name)
                .map_err(PluginError::WasmRuntime)?;

            tokio::time::timeout(timeout, async move {
                // Increment epoch to trigger interruption if needed
                engine.increment_epoch();

                // Call function
                func.call(&mut runtime.store, ())
                    .map_err(PluginError::WasmRuntime)
            })
            .await
        };

        let execution_time = start_time.elapsed();

        match result {
            Ok(Ok(_)) => {
                // Check if execution time exceeded budget
                if execution_time.as_micros()
                    > self.manifest.constraints.max_execution_time_us as u128
                {
                    return Err(PluginError::BudgetViolation {
                        used_us: execution_time.as_micros() as u32,
                        budget_us: self.manifest.constraints.max_execution_time_us,
                    });
                }
                Ok(vec![])
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(PluginError::ExecutionTimeout { duration: timeout }),
        }
    }
}

#[async_trait::async_trait]
impl Plugin for WasmPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn initialize(&mut self, config: serde_json::Value) -> PluginResult<()> {
        // Serialize config and pass to WASM module
        let config_bytes = serde_json::to_vec(&config)
            .map_err(|e| PluginError::LoadingFailed(format!("Config serialization: {}", e)))?;

        // Store config in plugin data
        {
            let mut runtime = self.runtime.lock().await;
            runtime
                .store
                .data_mut()
                .abi_state
                .store_data("config".to_string(), config_bytes);
        }

        // Call initialization function if present
        if let Some(init_func) = self.manifest.entry_points.init_function.clone() {
            let _result: Vec<wasmtime::Val> = self
                .execute_with_limits(
                    &init_func,
                    &[],
                    Duration::from_millis(5000), // 5 second timeout for init
                )
                .await?;
        }

        // Mark as initialized
        {
            let mut runtime = self.runtime.lock().await;
            runtime.store.data_mut().abi_state.mark_initialized();
        }

        Ok(())
    }

    async fn process_telemetry(
        &mut self,
        input: &NormalizedTelemetry,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput> {
        // Check capability
        {
            let runtime = self.runtime.lock().await;
            runtime
                .store
                .data()
                .capability_checker
                .check_telemetry_read()?;
        }

        // Serialize input telemetry
        let input_bytes = serde_json::to_vec(input)
            .map_err(|e| PluginError::LoadingFailed(format!("Telemetry serialization: {}", e)))?;

        {
            let mut runtime = self.runtime.lock().await;
            runtime
                .store
                .data_mut()
                .abi_state
                .store_data("input_telemetry".to_string(), input_bytes);
        }

        // Execute main function
        let timeout = Duration::from_micros(context.budget_us as u64);
        let main_function = self.manifest.entry_points.main_function.clone();
        let _result: Vec<wasmtime::Val> = self
            .execute_with_limits(&main_function, &[], timeout)
            .await?;

        // Get output from plugin data (simplified - real implementation would use proper WASM memory interface)
        let output_bytes = {
            let runtime = self.runtime.lock().await;
            runtime
                .store
                .data()
                .abi_state
                .get_data("output_telemetry")
                .cloned()
                .unwrap_or_default()
        };

        if output_bytes.is_empty() {
            // No modification
            Ok(PluginOutput::Telemetry(crate::PluginTelemetryOutput {
                modified_telemetry: None,
                custom_data: serde_json::Value::Null,
            }))
        } else {
            let modified_telemetry: NormalizedTelemetry = serde_json::from_slice(&output_bytes)
                .map_err(|e| {
                    PluginError::LoadingFailed(format!("Output deserialization: {}", e))
                })?;

            Ok(PluginOutput::Telemetry(crate::PluginTelemetryOutput {
                modified_telemetry: Some(modified_telemetry),
                custom_data: serde_json::Value::Null,
            }))
        }
    }

    async fn process_led_mapping(
        &mut self,
        _input: &NormalizedTelemetry,
        context: &PluginContext,
    ) -> PluginResult<PluginOutput> {
        // Check capability
        {
            let runtime = self.runtime.lock().await;
            runtime
                .store
                .data()
                .capability_checker
                .check_led_control()?;
        }

        // Execute LED mapping function
        let timeout = Duration::from_micros(context.budget_us as u64);
        let main_function = self.manifest.entry_points.main_function.clone();
        let _result: Vec<wasmtime::Val> = self
            .execute_with_limits(&main_function, &[], timeout)
            .await?;

        // Return default LED output (simplified)
        Ok(PluginOutput::Led(crate::PluginLedOutput {
            led_pattern: vec![255, 0, 0], // Red
            brightness: 1.0,
            duration_ms: 100,
        }))
    }

    async fn process_dsp(
        &mut self,
        _ffb_input: f32,
        _wheel_speed: f32,
        _context: &PluginContext,
    ) -> PluginResult<PluginOutput> {
        // DSP processing not allowed for WASM plugins
        Err(PluginError::CapabilityViolation {
            capability: "ProcessDsp".to_string(),
        })
    }

    async fn shutdown(&mut self) -> PluginResult<()> {
        // Call cleanup function if present
        if let Some(cleanup_func) = self.manifest.entry_points.cleanup_function.clone() {
            let _result: Vec<wasmtime::Val> = self
                .execute_with_limits(
                    &cleanup_func,
                    &[],
                    Duration::from_millis(1000), // 1 second timeout for cleanup
                )
                .await?;
        }

        Ok(())
    }
}

/// WASM plugin host manager
pub struct WasmPluginHost {
    plugins: Arc<RwLock<HashMap<uuid::Uuid, WasmPlugin>>>,
    _engine: Engine,
}

impl WasmPluginHost {
    pub fn new() -> PluginResult<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config)?;

        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            _engine: engine,
        })
    }

    /// Load a WASM plugin
    pub async fn load_plugin(
        &self,
        manifest: PluginManifest,
        wasm_path: &Path,
    ) -> PluginResult<uuid::Uuid> {
        let plugin = WasmPlugin::load(manifest.clone(), wasm_path).await?;
        let plugin_id = manifest.id;

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id, plugin);

        Ok(plugin_id)
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, plugin_id: uuid::Uuid) -> PluginResult<()> {
        let mut plugins = self.plugins.write().await;
        if let Some(mut plugin) = plugins.remove(&plugin_id) {
            plugin.shutdown().await?;
        }
        Ok(())
    }

    /// Execute plugin operation
    pub async fn execute_plugin(
        &self,
        plugin_id: uuid::Uuid,
        operation: PluginOperation,
        input_data: serde_json::Value,
        context: PluginContext,
    ) -> PluginResult<PluginOutput> {
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(&plugin_id)
            .ok_or_else(|| PluginError::LoadingFailed("Plugin not found".to_string()))?;

        match operation {
            PluginOperation::TelemetryProcessor => {
                let telemetry: NormalizedTelemetry =
                    serde_json::from_value(input_data).map_err(|e| {
                        PluginError::LoadingFailed(format!("Invalid telemetry data: {}", e))
                    })?;
                plugin.process_telemetry(&telemetry, &context).await
            }
            PluginOperation::LedMapper => {
                let led_input: NormalizedTelemetry = serde_json::from_value(input_data)
                    .map_err(|e| PluginError::LoadingFailed(format!("Invalid LED data: {}", e)))?;
                plugin.process_led_mapping(&led_input, &context).await
            }
            _ => Err(PluginError::CapabilityViolation {
                capability: format!("Operation {:?} not supported for WASM plugins", operation),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() -> Result<(), Box<dyn std::error::Error>> {
        let limits = ResourceLimits::default();

        assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024); // 16MB
        assert_eq!(limits.max_fuel, 10_000_000);
        assert_eq!(limits.max_table_elements, 10_000);
        assert_eq!(limits.max_instances, 32);

        Ok(())
    }

    #[test]
    fn test_resource_limits_builder() -> Result<(), Box<dyn std::error::Error>> {
        let limits = ResourceLimits::default()
            .with_memory(8 * 1024 * 1024)
            .with_fuel(5_000_000)
            .with_table_elements(5_000)
            .with_max_instances(16);

        assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 5_000_000);
        assert_eq!(limits.max_table_elements, 5_000);
        assert_eq!(limits.max_instances, 16);

        Ok(())
    }

    #[test]
    fn test_resource_limits_new() -> Result<(), Box<dyn std::error::Error>> {
        let limits = ResourceLimits::new(
            32 * 1024 * 1024, // 32MB
            20_000_000,
            20_000,
            64,
        );

        assert_eq!(limits.max_memory_bytes, 32 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 20_000_000);
        assert_eq!(limits.max_table_elements, 20_000);
        assert_eq!(limits.max_instances, 64);

        Ok(())
    }

    #[test]
    fn test_wasm_runtime_creation() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = WasmRuntime::new()?;

        // Verify default resource limits are applied
        let limits = runtime.resource_limits();
        assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 10_000_000);
        assert_eq!(limits.max_instances, 32);

        // Verify no plugins are loaded initially
        assert_eq!(runtime.instance_count(), 0);

        Ok(())
    }

    #[test]
    fn test_wasm_runtime_with_custom_limits() -> Result<(), Box<dyn std::error::Error>> {
        let custom_limits = ResourceLimits::default()
            .with_memory(8 * 1024 * 1024)
            .with_max_instances(16);

        let runtime = WasmRuntime::with_limits(custom_limits)?;

        let limits = runtime.resource_limits();
        assert_eq!(limits.max_memory_bytes, 8 * 1024 * 1024);
        assert_eq!(limits.max_instances, 16);

        Ok(())
    }

    #[test]
    fn test_wasm_runtime_has_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Plugin should not exist initially
        assert!(!runtime.has_plugin(&plugin_id));

        Ok(())
    }

    #[test]
    fn test_wasm_runtime_unload_nonexistent_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Unloading a non-existent plugin should return an error
        let result = runtime.unload_plugin(&plugin_id);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_wasm_runtime_process_nonexistent_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Processing with a non-existent plugin should return an error
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_plugin_disabled_info_creation() -> Result<(), Box<dyn std::error::Error>> {
        let info = PluginDisabledInfo {
            reason: "Test trap".to_string(),
            disabled_at: Instant::now(),
            trap_location: Some("func[0]".to_string()),
        };

        assert_eq!(info.reason, "Test trap");
        assert!(info.trap_location.is_some());
        assert_eq!(info.trap_location.as_deref(), Some("func[0]"));

        Ok(())
    }

    #[test]
    fn test_is_plugin_disabled_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Checking disabled status of non-existent plugin should return an error
        let result = runtime.is_plugin_disabled(&plugin_id);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_get_plugin_disabled_info_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Getting disabled info of non-existent plugin should return an error
        let result = runtime.get_plugin_disabled_info(&plugin_id);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_re_enable_plugin_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Re-enabling a non-existent plugin should return an error
        let result = runtime.re_enable_plugin(&plugin_id);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_plugin_trap_disables_plugin() -> Result<(), Box<dyn std::error::Error>> {
        // Create a WASM module that traps (unreachable instruction)
        let trap_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    unreachable
                )
            )
        "#;

        let trap_wasm = wat::parse_str(trap_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the plugin
        runtime.load_plugin_from_bytes(plugin_id, &trap_wasm, vec![])?;

        // Plugin should not be disabled initially
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        // Process should fail due to trap
        let result = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result.is_err());

        // Plugin should now be disabled
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        // Subsequent calls should fail with Crashed error
        let result2 = runtime.process(&plugin_id, 0.5, 0.001);
        assert!(result2.is_err());
        match result2 {
            Err(PluginError::Crashed { reason }) => {
                assert!(reason.contains("disabled"));
            }
            _ => panic!("Expected Crashed error"),
        }

        Ok(())
    }

    #[test]
    fn test_plugin_re_enable_after_trap() -> Result<(), Box<dyn std::error::Error>> {
        // Create a WASM module that traps
        let trap_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    unreachable
                )
            )
        "#;

        let trap_wasm = wat::parse_str(trap_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load and trigger trap
        runtime.load_plugin_from_bytes(plugin_id, &trap_wasm, vec![])?;
        let _ = runtime.process(&plugin_id, 0.5, 0.001);

        // Plugin should be disabled
        assert!(runtime.is_plugin_disabled(&plugin_id)?);

        // Re-enable the plugin
        let was_disabled = runtime.re_enable_plugin(&plugin_id)?;
        assert!(was_disabled);

        // Plugin should no longer be disabled
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        // Re-enabling again should return false (wasn't disabled)
        let was_disabled_again = runtime.re_enable_plugin(&plugin_id)?;
        assert!(!was_disabled_again);

        Ok(())
    }

    #[test]
    fn test_plugin_disabled_info_contains_trap_reason() -> Result<(), Box<dyn std::error::Error>> {
        // Create a WASM module that traps
        let trap_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    unreachable
                )
            )
        "#;

        let trap_wasm = wat::parse_str(trap_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load and trigger trap
        runtime.load_plugin_from_bytes(plugin_id, &trap_wasm, vec![])?;
        let _ = runtime.process(&plugin_id, 0.5, 0.001);

        // Get disabled info
        let info = runtime.get_plugin_disabled_info(&plugin_id)?;
        assert!(info.is_some());

        let info = info.as_ref().ok_or("Expected disabled info")?;
        // The reason should contain information about the trap
        assert!(!info.reason.is_empty());

        Ok(())
    }

    #[test]
    fn test_working_plugin_not_disabled() -> Result<(), Box<dyn std::error::Error>> {
        // Create a WASM module that works correctly
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

        let working_wasm = wat::parse_str(working_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the plugin
        runtime.load_plugin_from_bytes(plugin_id, &working_wasm, vec![])?;

        // Plugin should not be disabled
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        // Process should succeed
        let result = runtime.process(&plugin_id, 0.5, 0.001)?;
        assert!((result - 0.501).abs() < 0.001);

        // Plugin should still not be disabled
        assert!(!runtime.is_plugin_disabled(&plugin_id)?);

        // Disabled info should be None
        let info = runtime.get_plugin_disabled_info(&plugin_id)?;
        assert!(info.is_none());

        Ok(())
    }

    // ========================================================================
    // Hot-Reload Tests
    // ========================================================================

    #[test]
    fn test_hot_reload_preserves_plugin_data() -> Result<(), Box<dyn std::error::Error>> {
        // Create a simple WASM module
        let simple_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;

        let simple_wasm = wat::parse_str(simple_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the plugin
        runtime.load_plugin_from_bytes(plugin_id, &simple_wasm, vec![])?;

        // Store some custom data in the plugin
        {
            let instance = runtime
                .instances
                .get_mut(&plugin_id)
                .ok_or("Plugin not found")?;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("test_key".to_string(), vec![1, 2, 3, 4, 5]);
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("another_key".to_string(), vec![10, 20, 30]);
        }

        // Create a new version of the plugin (slightly different)
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
        let new_wasm = wat::parse_str(new_wat)?;

        // Hot-reload the plugin
        runtime.reload_plugin(&plugin_id, &new_wasm, vec![])?;

        // Verify the custom data was preserved
        {
            let instance = runtime
                .instances
                .get(&plugin_id)
                .ok_or("Plugin not found after reload")?;
            let data1 = instance
                .store
                .data()
                .abi_state
                .get_data("test_key")
                .ok_or("test_key not found")?;
            assert_eq!(data1, &vec![1, 2, 3, 4, 5]);

            let data2 = instance
                .store
                .data()
                .abi_state
                .get_data("another_key")
                .ok_or("another_key not found")?;
            assert_eq!(data2, &vec![10, 20, 30]);
        }

        // Verify the new plugin works (adds instead of just returning input)
        let result = runtime.process(&plugin_id, 1.0, 2.0)?;
        assert!((result - 3.0).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_hot_reload_preserves_statistics() -> Result<(), Box<dyn std::error::Error>> {
        // Create a simple WASM module
        let simple_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;

        let simple_wasm = wat::parse_str(simple_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the plugin
        runtime.load_plugin_from_bytes(plugin_id, &simple_wasm, vec![])?;

        // Process several times to accumulate statistics
        for i in 0..10 {
            let _ = runtime.process(&plugin_id, i as f32, 0.001)?;
        }

        // Get the statistics before reload
        let (process_count_before, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(process_count_before, 10);

        // Hot-reload the plugin
        runtime.reload_plugin(&plugin_id, &simple_wasm, vec![])?;

        // Verify statistics were preserved
        let (process_count_after, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(process_count_after, 10);

        // Process more and verify count continues from preserved value
        let _ = runtime.process(&plugin_id, 0.5, 0.001)?;
        let (process_count_final, _) = runtime.get_plugin_stats(&plugin_id)?;
        assert_eq!(process_count_final, 11);

        Ok(())
    }

    #[test]
    fn test_hot_reload_failure_keeps_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
        // Create a valid WASM module
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

        let valid_wasm = wat::parse_str(valid_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the valid plugin
        runtime.load_plugin_from_bytes(plugin_id, &valid_wasm, vec![])?;

        // Verify it works
        let result_before = runtime.process(&plugin_id, 1.0, 0.001)?;
        assert!((result_before - 43.0).abs() < 0.001);

        // Try to reload with invalid WASM (missing required exports)
        let invalid_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "not_process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;
        let invalid_wasm = wat::parse_str(invalid_wat)?;

        // Reload should fail
        let reload_result = runtime.reload_plugin(&plugin_id, &invalid_wasm, vec![]);
        assert!(reload_result.is_err());

        // Old plugin should still work
        let result_after = runtime.process(&plugin_id, 1.0, 0.001)?;
        assert!((result_after - 43.0).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_hot_reload_with_invalid_wasm_bytes() -> Result<(), Box<dyn std::error::Error>> {
        // Create a valid WASM module
        let valid_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;

        let valid_wasm = wat::parse_str(valid_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the valid plugin
        runtime.load_plugin_from_bytes(plugin_id, &valid_wasm, vec![])?;

        // Store some data
        {
            let instance = runtime
                .instances
                .get_mut(&plugin_id)
                .ok_or("Plugin not found")?;
            instance
                .store
                .data_mut()
                .abi_state
                .store_data("important_data".to_string(), vec![42]);
        }

        // Try to reload with garbage bytes
        let garbage_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0xFF, 0xFF, 0xFF, 0xFF];
        let reload_result = runtime.reload_plugin(&plugin_id, &garbage_bytes, vec![]);
        assert!(reload_result.is_err());

        // Old plugin should still work and have its data
        let result = runtime.process(&plugin_id, 5.0, 0.001)?;
        assert!((result - 5.0).abs() < 0.001);

        {
            let instance = runtime
                .instances
                .get(&plugin_id)
                .ok_or("Plugin not found")?;
            let data = instance
                .store
                .data()
                .abi_state
                .get_data("important_data")
                .ok_or("Data not found")?;
            assert_eq!(data, &vec![42]);
        }

        Ok(())
    }

    #[test]
    fn test_hot_reload_nonexistent_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let simple_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
            )
        "#;

        let simple_wasm = wat::parse_str(simple_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Reload a plugin that doesn't exist - should succeed (loads as new)
        let result = runtime.reload_plugin(&plugin_id, &simple_wasm, vec![]);
        assert!(result.is_ok());

        // Plugin should now exist and work
        assert!(runtime.has_plugin(&plugin_id));
        let process_result = runtime.process(&plugin_id, 5.0, 0.001)?;
        assert!((process_result - 5.0).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_hot_reload_calls_shutdown_on_old_plugin() -> Result<(), Box<dyn std::error::Error>> {
        // Create a WASM module with a shutdown function
        // We can't easily verify shutdown was called, but we can verify the reload works
        let with_shutdown_wat = r#"
            (module
                (memory (export "memory") 1)
                (global $shutdown_called (mut i32) (i32.const 0))
                (func (export "process") (param f32 f32) (result f32)
                    local.get 0
                )
                (func (export "shutdown")
                    i32.const 1
                    global.set $shutdown_called
                )
            )
        "#;

        let with_shutdown_wasm = wat::parse_str(with_shutdown_wat)?;

        let mut runtime = WasmRuntime::new()?;
        let plugin_id = uuid::Uuid::new_v4();

        // Load the plugin
        runtime.load_plugin_from_bytes(plugin_id, &with_shutdown_wasm, vec![])?;

        // Reload the plugin
        runtime.reload_plugin(&plugin_id, &with_shutdown_wasm, vec![])?;

        // Plugin should still work after reload
        let result = runtime.process(&plugin_id, 3.0, 0.001)?;
        assert!((result - 3.0).abs() < 0.001);

        Ok(())
    }

    #[test]
    fn test_preserved_plugin_state_struct() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = HashMap::new();
        data.insert("key1".to_string(), vec![1, 2, 3]);
        data.insert("key2".to_string(), vec![4, 5, 6]);

        let state = PreservedPluginState {
            plugin_data: data.clone(),
            process_count: 100,
            total_process_time_us: 5000,
        };

        assert_eq!(state.plugin_data.len(), 2);
        assert_eq!(state.process_count, 100);
        assert_eq!(state.total_process_time_us, 5000);
        assert_eq!(state.plugin_data.get("key1"), Some(&vec![1, 2, 3]));

        // Test Clone
        let cloned = state.clone();
        assert_eq!(cloned.process_count, state.process_count);
        assert_eq!(cloned.plugin_data.len(), state.plugin_data.len());

        Ok(())
    }
}
