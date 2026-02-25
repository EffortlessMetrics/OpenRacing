//! Plugin host system that manages both WASM and native plugins

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use openracing_crypto::trust_store::TrustStore;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::manifest::{PluginManifest, PluginOperation, load_manifest};
use crate::native::{NativePluginConfig, NativePluginHost};
use crate::quarantine::{QuarantineManager, QuarantinePolicy, ViolationType};
use crate::wasm::WasmPluginHost;
use crate::{PluginClass, PluginContext, PluginError, PluginOutput, PluginResult, PluginStats};

/// Plugin registry entry
#[derive(Debug, Clone)]
pub struct PluginRegistryEntry {
    pub manifest: PluginManifest,
    pub plugin_path: PathBuf,
    pub is_loaded: bool,
    pub is_enabled: bool,
    pub stats: PluginStats,
}

/// Main plugin host that manages all plugins
pub struct PluginHost {
    /// Plugin registry
    registry: Arc<RwLock<HashMap<Uuid, PluginRegistryEntry>>>,

    /// WASM plugin host
    wasm_host: WasmPluginHost,

    /// Native plugin host
    native_host: NativePluginHost,

    /// Quarantine manager
    quarantine_manager: Arc<RwLock<QuarantineManager>>,

    /// Plugin directory
    plugin_directory: PathBuf,
}

impl PluginHost {
    /// Create a new plugin host
    pub async fn new(plugin_directory: PathBuf) -> PluginResult<Self> {
        Self::new_with_native_config(plugin_directory, NativePluginConfig::default()).await
    }

    /// Create a new plugin host with explicit native plugin verification configuration
    ///
    /// This enables explicit opt-out from secure defaults when needed for
    /// development environments.
    pub async fn new_with_native_config(
        plugin_directory: PathBuf,
        native_config: NativePluginConfig,
    ) -> PluginResult<Self> {
        let wasm_host = WasmPluginHost::new()?;
        let native_host = NativePluginHost::new(TrustStore::new_in_memory(), native_config);
        let quarantine_manager = Arc::new(RwLock::new(QuarantineManager::new(
            QuarantinePolicy::default(),
        )));

        let mut host = Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            wasm_host,
            native_host,
            quarantine_manager,
            plugin_directory,
        };

        // Scan for plugins on startup
        host.scan_plugins().await?;

        Ok(host)
    }

    /// Scan plugin directory for available plugins
    pub async fn scan_plugins(&mut self) -> PluginResult<()> {
        let mut registry = self.registry.write().await;

        // Scan for plugin manifests
        let mut entries = tokio::fs::read_dir(&self.plugin_directory).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_dir() {
                let manifest_path = path.join("plugin.yaml");
                if manifest_path.exists() {
                    match load_manifest(&manifest_path).await {
                        Ok(manifest) => {
                            let plugin_path = match manifest.class {
                                PluginClass::Safe => {
                                    if let Some(wasm_path) =
                                        manifest.entry_points.wasm_module.as_ref()
                                    {
                                        path.join(wasm_path)
                                    } else {
                                        tracing::warn!(
                                            manifest_path = %manifest_path.display(),
                                            plugin_id = %manifest.id,
                                            plugin_class = ?manifest.class,
                                            "Skipping plugin: missing wasm entry point"
                                        );
                                        continue; // Skip plugin with missing wasm module
                                    }
                                }
                                PluginClass::Fast => {
                                    if let Some(native_path) =
                                        manifest.entry_points.native_library.as_ref()
                                    {
                                        path.join(native_path)
                                    } else {
                                        tracing::warn!(
                                            manifest_path = %manifest_path.display(),
                                            plugin_id = %manifest.id,
                                            plugin_class = ?manifest.class,
                                            "Skipping plugin: missing native entry point"
                                        );
                                        continue; // Skip plugin with missing native library
                                    }
                                }
                            };

                            if plugin_path.exists() {
                                let entry = PluginRegistryEntry {
                                    manifest: manifest.clone(),
                                    plugin_path,
                                    is_loaded: false,
                                    is_enabled: true,
                                    stats: PluginStats::default(),
                                };

                                registry.insert(manifest.id, entry);

                                tracing::info!(
                                    plugin_id = %manifest.id,
                                    plugin_name = %manifest.name,
                                    plugin_class = ?manifest.class,
                                    "Discovered plugin"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                manifest_path = %manifest_path.display(),
                                error = %e,
                                "Failed to load plugin manifest"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Load a plugin
    pub async fn load_plugin(&self, plugin_id: Uuid) -> PluginResult<()> {
        // Check if plugin is quarantined
        {
            let mut quarantine = self.quarantine_manager.write().await;
            if quarantine.is_quarantined(plugin_id) {
                return Err(PluginError::Quarantined { plugin_id });
            }
        }

        let mut registry = self.registry.write().await;
        let entry = registry.get_mut(&plugin_id).ok_or_else(|| {
            PluginError::LoadingFailed("Plugin not found in registry".to_string())
        })?;

        if entry.is_loaded {
            return Ok(()); // Already loaded
        }

        if !entry.is_enabled {
            return Err(PluginError::LoadingFailed("Plugin is disabled".to_string()));
        }

        // Load plugin based on class
        match entry.manifest.class {
            PluginClass::Safe => {
                self.wasm_host
                    .load_plugin(entry.manifest.clone(), &entry.plugin_path)
                    .await?;
            }
            PluginClass::Fast => {
                let max_execution_time_us = entry.manifest.constraints.max_execution_time_us;
                self.native_host
                    .load_plugin(
                        entry.manifest.id,
                        entry.manifest.name.clone(),
                        &entry.plugin_path,
                        max_execution_time_us,
                    )
                    .await?;
            }
        }

        entry.is_loaded = true;

        tracing::info!(
            plugin_id = %plugin_id,
            plugin_name = %entry.manifest.name,
            "Plugin loaded successfully"
        );

        Ok(())
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, plugin_id: Uuid) -> PluginResult<()> {
        let mut registry = self.registry.write().await;
        let entry = registry.get_mut(&plugin_id).ok_or_else(|| {
            PluginError::LoadingFailed("Plugin not found in registry".to_string())
        })?;

        if !entry.is_loaded {
            return Ok(()); // Already unloaded
        }

        // Unload plugin based on class
        match entry.manifest.class {
            PluginClass::Safe => {
                self.wasm_host.unload_plugin(plugin_id).await?;
            }
            PluginClass::Fast => {
                self.native_host.unload_plugin(plugin_id).await?;
            }
        }

        entry.is_loaded = false;

        tracing::info!(
            plugin_id = %plugin_id,
            plugin_name = %entry.manifest.name,
            "Plugin unloaded"
        );

        Ok(())
    }

    /// Execute a plugin operation with monitoring
    pub async fn execute_plugin(
        &self,
        plugin_id: Uuid,
        operation: PluginOperation,
        input_data: serde_json::Value,
        context: PluginContext,
    ) -> PluginResult<PluginOutput> {
        // Check if plugin is quarantined
        {
            let mut quarantine = self.quarantine_manager.write().await;
            if quarantine.is_quarantined(plugin_id) {
                return Err(PluginError::Quarantined { plugin_id });
            }
        }

        let start_time = Instant::now();
        let mut success = false;

        // Store budget before moving context
        let budget_us = context.budget_us;

        // Execute plugin
        let result = {
            let registry = self.registry.read().await;
            let entry = registry
                .get(&plugin_id)
                .ok_or_else(|| PluginError::LoadingFailed("Plugin not found".to_string()))?;

            if !entry.is_loaded {
                return Err(PluginError::LoadingFailed("Plugin not loaded".to_string()));
            }

            // Execute based on plugin class
            match entry.manifest.class {
                PluginClass::Safe => {
                    self.wasm_host
                        .execute_plugin(plugin_id, operation, input_data, context)
                        .await
                }
                PluginClass::Fast => {
                    // For native plugins, we need different handling
                    // This is simplified - real implementation would route to native host
                    Err(PluginError::LoadingFailed(
                        "Native plugin execution not implemented in this simplified version"
                            .to_string(),
                    ))
                }
            }
        };

        let execution_time = start_time.elapsed();

        // Handle result and update statistics
        match &result {
            Ok(_) => {
                success = true;

                // Check for budget violation
                if execution_time.as_micros() > budget_us as u128 {
                    let mut quarantine = self.quarantine_manager.write().await;
                    quarantine
                        .record_violation(
                            plugin_id,
                            ViolationType::BudgetViolation,
                            format!(
                                "Execution took {}μs, budget was {}μs",
                                execution_time.as_micros(),
                                budget_us
                            ),
                        )
                        .unwrap_or_else(|e| {
                            tracing::error!(
                                plugin_id = %plugin_id,
                                error = %e,
                                "Failed to record budget violation"
                            );
                        });
                }
            }
            Err(e) => {
                // Record failure
                let mut quarantine = self.quarantine_manager.write().await;
                let violation_type = match e {
                    PluginError::BudgetViolation { .. } => ViolationType::BudgetViolation,
                    PluginError::ExecutionTimeout { .. } => ViolationType::TimeoutViolation,
                    PluginError::CapabilityViolation { .. } => ViolationType::CapabilityViolation,
                    _ => ViolationType::Crash,
                };

                quarantine
                    .record_violation(plugin_id, violation_type, e.to_string())
                    .unwrap_or_else(|e| {
                        tracing::error!(
                            plugin_id = %plugin_id,
                            error = %e,
                            "Failed to record plugin violation"
                        );
                    });
            }
        }

        // Update plugin statistics
        {
            let mut registry = self.registry.write().await;
            if let Some(entry) = registry.get_mut(&plugin_id) {
                entry.stats.executions += 1;
                entry.stats.total_time_us += execution_time.as_micros() as u64;
                entry.stats.avg_time_us =
                    entry.stats.total_time_us as f64 / entry.stats.executions as f64;
                entry.stats.max_time_us = entry
                    .stats
                    .max_time_us
                    .max(execution_time.as_micros() as u32);
                entry.stats.last_execution = Some(chrono::Utc::now());

                if !success {
                    entry.stats.crashes += 1;
                }

                if execution_time.as_micros() > budget_us as u128 {
                    entry.stats.budget_violations += 1;
                }
            }
        }

        result
    }

    /// Get plugin registry
    pub async fn get_registry(&self) -> HashMap<Uuid, PluginRegistryEntry> {
        self.registry.read().await.clone()
    }

    /// Enable/disable a plugin
    pub async fn set_plugin_enabled(&self, plugin_id: Uuid, enabled: bool) -> PluginResult<()> {
        let mut registry = self.registry.write().await;
        if let Some(entry) = registry.get_mut(&plugin_id) {
            entry.is_enabled = enabled;

            // Unload if being disabled
            if !enabled && entry.is_loaded {
                drop(registry); // Release lock before calling unload
                self.unload_plugin(plugin_id).await?;
            }
        }

        Ok(())
    }

    /// Get quarantine statistics
    pub async fn get_quarantine_stats(&self) -> HashMap<Uuid, crate::quarantine::QuarantineState> {
        self.quarantine_manager.read().await.get_quarantine_stats()
    }

    /// Manually quarantine a plugin
    pub async fn quarantine_plugin(
        &self,
        plugin_id: Uuid,
        duration_minutes: i64,
    ) -> PluginResult<()> {
        let mut quarantine = self.quarantine_manager.write().await;
        quarantine.manual_quarantine(plugin_id, duration_minutes)?;

        // Unload the plugin
        self.unload_plugin(plugin_id).await?;

        Ok(())
    }

    /// Release a plugin from quarantine
    pub async fn release_from_quarantine(&self, plugin_id: Uuid) -> PluginResult<()> {
        let mut quarantine = self.quarantine_manager.write().await;
        quarantine.release_from_quarantine(plugin_id)
    }

    /// Load all enabled plugins
    pub async fn load_all_plugins(&self) -> PluginResult<()> {
        let registry = self.registry.read().await;
        let plugin_ids: Vec<Uuid> = registry
            .iter()
            .filter(|(_, entry)| entry.is_enabled && !entry.is_loaded)
            .map(|(id, _)| *id)
            .collect();
        drop(registry);

        for plugin_id in plugin_ids {
            if let Err(e) = self.load_plugin(plugin_id).await {
                tracing::error!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "Failed to load plugin during bulk load"
                );
            }
        }

        Ok(())
    }

    /// Unload all plugins
    pub async fn unload_all_plugins(&self) -> PluginResult<()> {
        let registry = self.registry.read().await;
        let plugin_ids: Vec<Uuid> = registry
            .iter()
            .filter(|(_, entry)| entry.is_loaded)
            .map(|(id, _)| *id)
            .collect();
        drop(registry);

        for plugin_id in plugin_ids {
            if let Err(e) = self.unload_plugin(plugin_id).await {
                tracing::error!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "Failed to unload plugin during bulk unload"
                );
            }
        }

        Ok(())
    }
}
