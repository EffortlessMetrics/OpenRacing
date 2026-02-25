//! Native plugin loader and host management.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use libloading::{Library, Symbol};
use openracing_crypto::SignatureMetadata;
use openracing_crypto::trust_store::TrustStore;
use tokio::sync::RwLock;

use crate::abi_check::CURRENT_ABI_VERSION;
use crate::error::{NativePluginError, NativePluginLoadError};
use crate::plugin::{NativePlugin, PluginVTable};
use crate::signature::{SignatureVerificationConfig, SignatureVerifier};

/// Configuration for native plugin loading.
///
/// This struct controls how the native plugin loader handles signature verification
/// and unsigned plugins.
#[derive(Debug, Clone)]
pub struct NativePluginConfig {
    /// Whether to allow loading unsigned plugins.
    pub allow_unsigned: bool,
    /// Whether to require signature verification for signed plugins.
    pub require_signatures: bool,
}

impl Default for NativePluginConfig {
    fn default() -> Self {
        Self {
            allow_unsigned: false,
            require_signatures: true,
        }
    }
}

impl NativePluginConfig {
    /// Create a strict configuration (production mode).
    pub fn strict() -> Self {
        Self {
            allow_unsigned: false,
            require_signatures: true,
        }
    }

    /// Create a permissive configuration.
    pub fn permissive() -> Self {
        Self {
            allow_unsigned: true,
            require_signatures: true,
        }
    }

    /// Create a development configuration.
    pub fn development() -> Self {
        Self {
            allow_unsigned: true,
            require_signatures: false,
        }
    }

    /// Convert to signature verification config.
    pub fn to_signature_config(&self) -> SignatureVerificationConfig {
        SignatureVerificationConfig {
            require_signatures: self.require_signatures,
            allow_unsigned: self.allow_unsigned,
        }
    }
}

/// Native plugin loader.
pub struct NativePluginLoader<'a> {
    trust_store: &'a TrustStore,
    config: NativePluginConfig,
}

impl<'a> NativePluginLoader<'a> {
    /// Create a new plugin loader.
    pub fn new(trust_store: &'a TrustStore, config: NativePluginConfig) -> Self {
        Self {
            trust_store,
            config,
        }
    }

    /// Create a loader with default configuration.
    pub fn with_defaults(trust_store: &'a TrustStore) -> Self {
        Self::new(trust_store, NativePluginConfig::default())
    }

    /// Load a native plugin.
    ///
    /// # Arguments
    ///
    /// * `id` - Plugin ID.
    /// * `name` - Plugin name.
    /// * `library_path` - Path to the shared library.
    /// * `max_execution_time_us` - Maximum execution time in microseconds.
    ///
    /// # Returns
    ///
    /// The loaded native plugin.
    pub fn load(
        &self,
        id: uuid::Uuid,
        name: String,
        library_path: &Path,
        max_execution_time_us: u32,
    ) -> Result<NativePlugin, NativePluginError> {
        let signature = self.verify_signature(library_path)?;

        let library = unsafe {
            Library::new(library_path).map_err(|e| NativePluginLoadError::LibraryLoadFailed {
                reason: e.to_string(),
            })?
        };

        let vtable = unsafe {
            let get_vtable: Symbol<'_, extern "C" fn() -> PluginVTable> = library
                .get(b"get_plugin_vtable")
                .map_err(|e| NativePluginLoadError::LibraryLoadFailed {
                    reason: format!("Missing vtable function: {}", e),
                })?;
            get_vtable()
        };

        if vtable.abi_version != CURRENT_ABI_VERSION {
            return Err(NativePluginLoadError::AbiMismatch {
                expected: CURRENT_ABI_VERSION,
                actual: vtable.abi_version,
            }
            .into());
        }

        let config_json = serde_json::to_string(&serde_json::Value::Null)?;
        let plugin_state = (vtable.create)(config_json.as_ptr(), config_json.len());

        if plugin_state.is_null() {
            return Err(NativePluginLoadError::InitializationFailed {
                reason: "Plugin create returned null".to_string(),
            }
            .into());
        }

        tracing::info!(
            plugin_id = %id,
            name = %name,
            abi_version = vtable.abi_version,
            signed = signature.is_some(),
            "Native plugin loaded successfully"
        );

        Ok(unsafe {
            NativePlugin::new(
                id,
                name,
                library,
                vtable,
                plugin_state,
                signature,
                max_execution_time_us,
            )
        })
    }

    /// Verify the plugin signature.
    fn verify_signature(
        &self,
        library_path: &Path,
    ) -> Result<Option<SignatureMetadata>, NativePluginError> {
        let verifier = SignatureVerifier::new(self.trust_store, self.config.to_signature_config());
        let result = verifier.verify(library_path)?;
        Ok(result.metadata)
    }

    /// Get the trust store.
    pub fn trust_store(&self) -> &TrustStore {
        self.trust_store
    }

    /// Get the configuration.
    pub fn config(&self) -> &NativePluginConfig {
        &self.config
    }
}

/// Native plugin host manager.
pub struct NativePluginHost {
    plugins: Arc<RwLock<HashMap<uuid::Uuid, NativePlugin>>>,
    trust_store: Arc<TrustStore>,
    config: NativePluginConfig,
}

impl NativePluginHost {
    /// Create a new native plugin host.
    pub fn new(trust_store: TrustStore, config: NativePluginConfig) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            trust_store: Arc::new(trust_store),
            config,
        }
    }

    /// Create a host with secure default configuration.
    pub fn new_with_defaults() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            trust_store: Arc::new(TrustStore::new_in_memory()),
            config: NativePluginConfig::default(),
        }
    }

    /// Create a host with permissive development defaults.
    pub fn new_permissive_for_development() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            trust_store: Arc::new(TrustStore::new_in_memory()),
            config: NativePluginConfig::development(),
        }
    }

    /// Load a native plugin.
    pub async fn load_plugin(
        &self,
        id: uuid::Uuid,
        name: String,
        library_path: &Path,
        max_execution_time_us: u32,
    ) -> Result<uuid::Uuid, NativePluginError> {
        let loader = NativePluginLoader::new(&self.trust_store, self.config.clone());
        let plugin = loader.load(id, name, library_path, max_execution_time_us)?;

        let mut plugins = self.plugins.write().await;
        plugins.insert(id, plugin);

        Ok(id)
    }

    /// Unload a plugin.
    pub async fn unload_plugin(&self, plugin_id: uuid::Uuid) -> Result<(), NativePluginError> {
        let mut plugins = self.plugins.write().await;
        if let Some(mut plugin) = plugins.remove(&plugin_id) {
            unsafe { plugin.shutdown()? };
        }
        Ok(())
    }

    /// Get a plugin by ID.
    pub async fn get_plugin(&self, plugin_id: uuid::Uuid) -> Option<NativePluginRef> {
        let plugins = self.plugins.read().await;
        plugins.contains_key(&plugin_id).then(|| NativePluginRef {
            plugin_id,
            plugins: self.plugins.clone(),
        })
    }

    /// Check if a plugin is loaded.
    pub async fn is_loaded(&self, plugin_id: uuid::Uuid) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(&plugin_id)
    }

    /// Get the number of loaded plugins.
    pub async fn plugin_count(&self) -> usize {
        let plugins = self.plugins.read().await;
        plugins.len()
    }

    /// Get the trust store.
    pub fn trust_store(&self) -> &TrustStore {
        &self.trust_store
    }

    /// Get the configuration.
    pub fn config(&self) -> &NativePluginConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: NativePluginConfig) {
        self.config = config;
    }
}

/// Reference to a loaded plugin.
pub struct NativePluginRef {
    plugin_id: uuid::Uuid,
    #[allow(dead_code)]
    plugins: Arc<RwLock<HashMap<uuid::Uuid, NativePlugin>>>,
}

impl NativePluginRef {
    /// Get the plugin ID.
    pub fn id(&self) -> uuid::Uuid {
        self.plugin_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_is_strict() {
        let config = NativePluginConfig::default();
        assert!(!config.allow_unsigned);
        assert!(config.require_signatures);
    }

    #[test]
    fn test_config_strict() {
        let config = NativePluginConfig::strict();
        assert!(!config.allow_unsigned);
        assert!(config.require_signatures);
    }

    #[test]
    fn test_config_permissive() {
        let config = NativePluginConfig::permissive();
        assert!(config.allow_unsigned);
        assert!(config.require_signatures);
    }

    #[test]
    fn test_config_development() {
        let config = NativePluginConfig::development();
        assert!(config.allow_unsigned);
        assert!(!config.require_signatures);
    }

    #[tokio::test]
    async fn test_host_new_with_defaults() {
        let host = NativePluginHost::new_with_defaults();
        assert!(!host.config().allow_unsigned);
        assert!(host.config().require_signatures);
        assert_eq!(host.plugin_count().await, 0);
    }

    #[tokio::test]
    async fn test_host_new_permissive_for_development() {
        let host = NativePluginHost::new_permissive_for_development();
        assert!(host.config().allow_unsigned);
        assert!(!host.config().require_signatures);
    }

    #[tokio::test]
    async fn test_host_set_config() {
        let trust_store = TrustStore::new_in_memory();
        let initial_config = NativePluginConfig::development();
        let mut host = NativePluginHost::new(trust_store, initial_config);

        assert!(host.config().allow_unsigned);

        host.set_config(NativePluginConfig::strict());
        assert!(!host.config().allow_unsigned);
    }

    #[test]
    fn test_loader_with_defaults() {
        let trust_store = TrustStore::new_in_memory();
        let loader = NativePluginLoader::with_defaults(&trust_store);
        assert!(!loader.config().allow_unsigned);
        assert!(loader.config().require_signatures);
    }
}
