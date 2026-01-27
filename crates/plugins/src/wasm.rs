//! WASM plugin host with capability-based sandboxing

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use wasmtime::*;
use wasmtime_wasi::p1::WasiP1Ctx;

use crate::capability::{CapabilityChecker, WasmCapabilityEnforcer};
use crate::manifest::{PluginManifest, PluginOperation};
use crate::{Plugin, PluginContext, PluginError, PluginOutput, PluginResult};
use racing_wheel_engine::NormalizedTelemetry;

/// WASM plugin instance
pub struct WasmPlugin {
    manifest: PluginManifest,
    engine: Engine,
    _module: Module,
    runtime: Mutex<WasmRuntime>,
    _capability_enforcer: WasmCapabilityEnforcer,
}

struct WasmRuntime {
    store: Store<WasmPluginState>,
    instance: Instance,
}

/// WASM plugin state
struct WasmPluginState {
    wasi: WasiP1Ctx,
    capability_checker: CapabilityChecker,
    plugin_data: HashMap<String, Vec<u8>>,
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
            plugin_data: HashMap::new(),
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
        let runtime = Mutex::new(WasmRuntime { store, instance });

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
            "env",
            "check_capability",
            |mut caller: Caller<'_, WasmPluginState>,
             capability_ptr: i32,
             capability_len: i32|
             -> i32 {
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => return -1, // Error: no memory export
                };

                let data = match memory
                    .data(&caller)
                    .get(capability_ptr as usize..(capability_ptr + capability_len) as usize)
                {
                    Some(data) => data,
                    None => return -1, // Error: invalid memory access
                };

                let capability_str = match std::str::from_utf8(data) {
                    Ok(s) => s,
                    Err(_) => return -1, // Error: invalid UTF-8
                };

                // Check capability (simplified - in real implementation, parse capability string)
                match capability_str {
                    "read_telemetry" => {
                        if caller
                            .data()
                            .capability_checker
                            .check_telemetry_read()
                            .is_ok()
                        {
                            1 // Success
                        } else {
                            0 // Permission denied
                        }
                    }
                    "modify_telemetry" => {
                        if caller
                            .data()
                            .capability_checker
                            .check_telemetry_modify()
                            .is_ok()
                        {
                            1
                        } else {
                            0
                        }
                    }
                    "control_leds" => {
                        if caller.data().capability_checker.check_led_control().is_ok() {
                            1
                        } else {
                            0
                        }
                    }
                    _ => 0, // Unknown capability
                }
            },
        )?;

        // Host function for logging
        linker.func_wrap(
            "env",
            "plugin_log",
            |mut caller: Caller<'_, WasmPluginState>, level: i32, msg_ptr: i32, msg_len: i32| {
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => return,
                };

                let data = match memory
                    .data(&caller)
                    .get(msg_ptr as usize..(msg_ptr + msg_len) as usize)
                {
                    Some(data) => data,
                    None => return,
                };

                let message = match std::str::from_utf8(data) {
                    Ok(s) => s,
                    Err(_) => return,
                };

                match level {
                    0 => tracing::error!("Plugin: {}", message),
                    1 => tracing::warn!("Plugin: {}", message),
                    2 => tracing::info!("Plugin: {}", message),
                    3 => tracing::debug!("Plugin: {}", message),
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
                .plugin_data
                .insert("config".to_string(), config_bytes);
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
                .plugin_data
                .insert("input_telemetry".to_string(), input_bytes);
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
                .plugin_data
                .get("output_telemetry")
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
