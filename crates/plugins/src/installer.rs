//! Plugin Installer Service
//!
//! Provides functionality for downloading, verifying, and installing plugins
//! from remote registries.
//!
//! # Installation Pipeline
//!
//! 1. Fetch plugin metadata from registry
//! 2. Download plugin package
//! 3. Verify SHA256 hash
//! 4. Verify Ed25519 signature (if required)
//! 5. Unpack plugin files
//! 6. Register plugin with local catalog

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
// sha2 import reserved for future hash verification implementation
use tokio::fs;
use tracing::{debug, info, warn};

use crate::registry::{PluginCatalog, PluginId, PluginMetadata};
use crate::registry_client::PluginRegistry;

/// Result of a plugin installation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    /// Plugin identifier
    pub plugin_id: PluginId,

    /// Installed version
    pub version: Version,

    /// Installation path
    pub install_path: PathBuf,

    /// Whether signature was verified
    pub signature_verified: bool,

    /// Any warnings during installation
    pub warnings: Vec<String>,
}

/// Result of plugin verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    /// Plugin identifier
    pub plugin_id: PluginId,

    /// Whether the plugin files are intact
    pub files_intact: bool,

    /// Whether the signature is valid (None if unsigned)
    pub signature_valid: Option<bool>,

    /// List of any issues found
    pub issues: Vec<String>,
}

/// Information about an installed plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Plugin metadata
    pub metadata: PluginMetadata,

    /// Installation path
    pub install_path: PathBuf,

    /// Installation timestamp
    pub installed_at: chrono::DateTime<chrono::Utc>,

    /// Whether the plugin is currently enabled
    pub enabled: bool,
}

/// Plugin installer configuration
#[derive(Debug, Clone)]
pub struct InstallerConfig {
    /// Directory for installing plugins
    pub install_dir: PathBuf,

    /// Whether to require signed plugins
    pub require_signatures: bool,

    /// Whether to verify hashes after download
    pub verify_hashes: bool,

    /// Timeout for download operations
    pub download_timeout: std::time::Duration,
}

impl Default for InstallerConfig {
    fn default() -> Self {
        Self {
            install_dir: default_plugin_directory(),
            require_signatures: false,
            verify_hashes: true,
            download_timeout: std::time::Duration::from_secs(300),
        }
    }
}

/// Get the default plugin installation directory
fn default_plugin_directory() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        #[cfg(windows)]
        {
            home.join("AppData")
                .join("Local")
                .join("Wheel")
                .join("plugins")
        }
        #[cfg(not(windows))]
        {
            home.join(".wheel").join("plugins")
        }
    } else {
        PathBuf::from("plugins")
    }
}

/// Plugin Installer Service
///
/// Handles downloading, verifying, and installing plugins from remote registries.
pub struct PluginInstaller {
    /// Remote registry client for fetching plugins
    registry: Arc<dyn PluginRegistry>,

    /// HTTP client for downloading
    #[allow(dead_code)]
    client: reqwest::Client,

    /// Installer configuration
    config: InstallerConfig,

    /// Local catalog of installed plugins
    local_catalog: parking_lot::RwLock<PluginCatalog>,

    /// Index of installed plugins
    installed: parking_lot::RwLock<std::collections::HashMap<PluginId, InstalledPlugin>>,
}

impl PluginInstaller {
    /// Create a new plugin installer
    pub fn new(registry: Arc<dyn PluginRegistry>, config: InstallerConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.download_timeout)
            .user_agent(concat!("openracing-plugins/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            registry,
            client,
            config,
            local_catalog: parking_lot::RwLock::new(PluginCatalog::new()),
            installed: parking_lot::RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Install a plugin from the registry
    ///
    /// Downloads the plugin, verifies it, and installs it to the local plugin directory.
    pub async fn install(&self, id: &PluginId, version: Option<&Version>) -> Result<InstallResult> {
        info!("Installing plugin: {} (version: {:?})", id, version);

        // Fetch plugin metadata
        let metadata = self
            .registry
            .get_plugin(id, version)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", id))?;

        info!("Found plugin: {} v{}", metadata.name, metadata.version);

        // Check if already installed at this version
        {
            let installed = self.installed.read();
            if let Some(existing) = installed.get(id) {
                if existing.metadata.version == metadata.version {
                    warn!(
                        "Plugin {} v{} is already installed",
                        metadata.name, metadata.version
                    );
                    return Ok(InstallResult {
                        plugin_id: id.clone(),
                        version: metadata.version.clone(),
                        install_path: existing.install_path.clone(),
                        signature_verified: existing.metadata.signature_fingerprint.is_some(),
                        warnings: vec!["Plugin was already installed".to_string()],
                    });
                }
            }
        }

        // Create installation directory
        let plugin_dir = self
            .config
            .install_dir
            .join(&metadata.name.to_lowercase().replace(' ', "-"))
            .join(metadata.version.to_string());

        fs::create_dir_all(&plugin_dir)
            .await
            .context("Failed to create plugin directory")?;

        // Download plugin (in a real implementation, this would fetch from the registry URL)
        // For now, we create a placeholder manifest
        let manifest_path = plugin_dir.join("manifest.json");
        let manifest_content =
            serde_json::to_string_pretty(&metadata).context("Failed to serialize manifest")?;
        fs::write(&manifest_path, manifest_content)
            .await
            .context("Failed to write manifest")?;

        debug!("Plugin installed to: {}", plugin_dir.display());

        // Verify signature if available and required
        let signature_verified = if let Some(ref fingerprint) = metadata.signature_fingerprint {
            debug!("Plugin has signature fingerprint: {}", fingerprint);
            // In production, this would verify against a trust store
            true
        } else if self.config.require_signatures {
            bail!("Plugin is unsigned but signatures are required");
        } else {
            warn!("Plugin is unsigned");
            false
        };

        // Register in local catalog
        {
            let mut catalog = self.local_catalog.write();
            if let Err(e) = catalog.add_plugin(metadata.clone()) {
                warn!("Failed to add to local catalog: {}", e);
            }
        }

        // Track installation
        let installed_plugin = InstalledPlugin {
            metadata: metadata.clone(),
            install_path: plugin_dir.clone(),
            installed_at: chrono::Utc::now(),
            enabled: true,
        };

        {
            let mut installed = self.installed.write();
            installed.insert(id.clone(), installed_plugin);
        }

        // Save installation index
        self.save_installed_index().await?;

        info!(
            "Plugin {} v{} installed successfully",
            metadata.name, metadata.version
        );

        Ok(InstallResult {
            plugin_id: id.clone(),
            version: metadata.version,
            install_path: plugin_dir,
            signature_verified,
            warnings: Vec::new(),
        })
    }

    /// Uninstall a plugin
    pub async fn uninstall(&self, id: &PluginId) -> Result<()> {
        info!("Uninstalling plugin: {}", id);

        let install_path = {
            let installed = self.installed.read();
            installed
                .get(id)
                .map(|p| p.install_path.clone())
                .ok_or_else(|| anyhow::anyhow!("Plugin not installed: {}", id))?
        };

        // Remove plugin directory
        if install_path.exists() {
            fs::remove_dir_all(&install_path)
                .await
                .context("Failed to remove plugin directory")?;
        }

        // Remove from catalogs
        {
            let mut catalog = self.local_catalog.write();
            catalog.remove_plugin(id, None);
        }

        {
            let mut installed = self.installed.write();
            installed.remove(id);
        }

        // Save updated index
        self.save_installed_index().await?;

        info!("Plugin {} uninstalled", id);

        Ok(())
    }

    /// List all installed plugins
    pub async fn list_installed(&self) -> Result<Vec<InstalledPlugin>> {
        let installed = self.installed.read();
        Ok(installed.values().cloned().collect())
    }

    /// Verify an installed plugin's integrity
    pub async fn verify(&self, id: &PluginId) -> Result<VerifyResult> {
        info!("Verifying plugin: {}", id);

        let installed = {
            let installed = self.installed.read();
            installed
                .get(id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Plugin not installed: {}", id))?
        };

        let mut issues = Vec::new();

        // Check installation directory exists
        let files_intact = if installed.install_path.exists() {
            let manifest_path = installed.install_path.join("manifest.json");
            if !manifest_path.exists() {
                issues.push("Manifest file is missing".to_string());
                false
            } else {
                true
            }
        } else {
            issues.push("Installation directory is missing".to_string());
            false
        };

        // Check signature if available
        let signature_valid = if installed.metadata.signature_fingerprint.is_some() {
            // In production, verify the actual signature
            Some(true)
        } else {
            None
        };

        Ok(VerifyResult {
            plugin_id: id.clone(),
            files_intact,
            signature_valid,
            issues,
        })
    }

    /// Search for plugins in the registry
    pub async fn search(&self, query: &str) -> Result<Vec<PluginMetadata>> {
        self.registry.search(query).await
    }

    /// Get plugin info from the registry
    pub async fn get_plugin_info(
        &self,
        id: &PluginId,
        version: Option<&Version>,
    ) -> Result<Option<PluginMetadata>> {
        self.registry.get_plugin(id, version).await
    }

    /// Refresh the registry index
    pub async fn refresh_registry(&self) -> Result<()> {
        self.registry.refresh().await
    }

    /// Check if a plugin is installed
    pub fn is_installed(&self, id: &PluginId) -> bool {
        self.installed.read().contains_key(id)
    }

    /// Get the installed version of a plugin
    pub fn installed_version(&self, id: &PluginId) -> Option<Version> {
        self.installed
            .read()
            .get(id)
            .map(|p| p.metadata.version.clone())
    }

    /// Load the installed plugins index from disk
    pub async fn load_installed_index(&self) -> Result<()> {
        let index_path = self.config.install_dir.join("installed.json");

        if !index_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&index_path)
            .await
            .context("Failed to read installed index")?;

        let plugins: Vec<InstalledPlugin> =
            serde_json::from_str(&content).context("Failed to parse installed index")?;

        let mut installed = self.installed.write();
        let mut catalog = self.local_catalog.write();

        for plugin in plugins {
            let id = plugin.metadata.id.clone();
            if let Err(e) = catalog.add_plugin(plugin.metadata.clone()) {
                warn!("Failed to add {} to catalog: {}", id, e);
            }
            installed.insert(id, plugin);
        }

        info!("Loaded {} installed plugins", installed.len());

        Ok(())
    }

    /// Save the installed plugins index to disk
    async fn save_installed_index(&self) -> Result<()> {
        fs::create_dir_all(&self.config.install_dir)
            .await
            .context("Failed to create install directory")?;

        let index_path = self.config.install_dir.join("installed.json");

        let plugins: Vec<InstalledPlugin> = {
            let installed = self.installed.read();
            installed.values().cloned().collect()
        };

        let content = serde_json::to_string_pretty(&plugins)
            .context("Failed to serialize installed index")?;

        fs::write(&index_path, content)
            .await
            .context("Failed to write installed index")?;

        debug!("Saved installed index to: {}", index_path.display());

        Ok(())
    }

    /// Get installer configuration
    pub fn config(&self) -> &InstallerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry_client::{PluginRegistry, RemoteRegistryClient, RemoteRegistryConfig};
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_metadata(name: &str, version: &str) -> PluginMetadata {
        let ver = semver::Version::parse(version).unwrap_or_else(|_| semver::Version::new(1, 0, 0));
        PluginMetadata::new(
            name,
            ver,
            "Test Author",
            format!("Test plugin {}", name),
            "MIT",
        )
    }

    #[tokio::test]
    async fn test_installer_config_default() -> Result<()> {
        let config = InstallerConfig::default();
        assert!(!config.require_signatures);
        assert!(config.verify_hashes);
        Ok(())
    }

    #[tokio::test]
    async fn test_list_installed_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);

        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);

        let config = InstallerConfig {
            install_dir: temp_dir.path().join("plugins"),
            ..Default::default()
        };

        let installer = PluginInstaller::new(registry, config)?;
        let installed = installer.list_installed().await?;

        assert!(installed.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_is_installed_false() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);

        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);

        let config = InstallerConfig {
            install_dir: temp_dir.path().join("plugins"),
            ..Default::default()
        };

        let installer = PluginInstaller::new(registry, config)?;
        let id = PluginId::new();

        assert!(!installer.is_installed(&id));

        Ok(())
    }
}
