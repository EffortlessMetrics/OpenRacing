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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use racing_wheel_service::crypto::verification::VerificationService;
use racing_wheel_service::crypto::{VerificationConfig, utils as crypto_utils};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::task;
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

    /// Whether this installation was cryptographically verified at install time
    #[serde(default)]
    pub signature_verified: bool,
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

    /// Path to trust store used for cryptographic signature verification
    pub trust_store_path: Option<PathBuf>,
}

impl Default for InstallerConfig {
    fn default() -> Self {
        Self {
            install_dir: default_plugin_directory(),
            require_signatures: true,
            verify_hashes: true,
            download_timeout: std::time::Duration::from_secs(300),
            trust_store_path: Some(default_trust_store_path()),
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

/// Get the default trust store path
fn default_trust_store_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        #[cfg(windows)]
        {
            home.join("AppData")
                .join("Local")
                .join("Wheel")
                .join("trust_store.json")
        }
        #[cfg(not(windows))]
        {
            home.join(".config").join("wheel").join("trust_store.json")
        }
    } else {
        PathBuf::from("trust_store.json")
    }
}

/// Persisted installation receipt used for later integrity verification
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct InstallReceipt {
    /// Relative path from plugin install root to downloaded artifact
    artifact_rel_path: Option<PathBuf>,
    /// Expected SHA256 hash for the artifact
    expected_sha256: Option<String>,
    /// Source URL for the downloaded artifact
    download_url: Option<String>,
}

/// Plugin Installer Service
///
/// Handles downloading, verifying, and installing plugins from remote registries.
pub struct PluginInstaller {
    /// Remote registry client for fetching plugins
    registry: Arc<dyn PluginRegistry>,

    /// HTTP client for downloading
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
            if let Some(existing) = installed.get(id)
                && existing.metadata.version == metadata.version
            {
                warn!(
                    "Plugin {} v{} is already installed",
                    metadata.name, metadata.version
                );
                return Ok(InstallResult {
                    plugin_id: id.clone(),
                    version: metadata.version.clone(),
                    install_path: existing.install_path.clone(),
                    signature_verified: existing.signature_verified,
                    warnings: vec!["Plugin was already installed".to_string()],
                });
            }
        }

        // Create installation directory
        let plugin_dir = self
            .config
            .install_dir
            .join(metadata.name.to_lowercase().replace(' ', "-"))
            .join(metadata.version.to_string());

        let install_attempt = async {
            fs::create_dir_all(&plugin_dir)
                .await
                .context("Failed to create plugin directory")?;

            let mut warnings = Vec::new();
            let mut receipt = InstallReceipt {
                download_url: metadata.download_url.clone(),
                ..Default::default()
            };

            let artifact_path = self
                .download_plugin_artifact(&metadata, &plugin_dir)
                .await?;

            let expected_hash = self
                .resolve_expected_hash(&metadata, &mut warnings)
                .await?
                .map(|hash| hash.to_lowercase());

            if self.config.verify_hashes {
                if let Some(ref expected_hash) = expected_hash {
                    self.verify_file_hash(&artifact_path, expected_hash).await?;
                } else {
                    warnings.push(
                        "No package hash available; hash verification was skipped".to_string(),
                    );
                }
            }

            receipt.expected_sha256 = expected_hash;
            receipt.artifact_rel_path = artifact_path
                .strip_prefix(&plugin_dir)
                .map(|p| p.to_path_buf())
                .ok();

            // Verify signatures before unpacking so untrusted artifacts are never extracted.
            let signature_verified = self
                .verify_signature_for_artifact(&metadata, &artifact_path, true, &mut warnings)
                .await?;

            self.unpack_if_supported(&artifact_path, &plugin_dir, &mut warnings)
                .await?;

            Ok::<(bool, Vec<String>, InstallReceipt), anyhow::Error>((
                signature_verified,
                warnings,
                receipt,
            ))
        }
        .await;

        let (signature_verified, warnings, receipt) = match install_attempt {
            Ok(result) => result,
            Err(e) => {
                self.cleanup_failed_install(&plugin_dir).await;
                return Err(e);
            }
        };

        // Persist metadata and installation receipt
        self.write_manifest(&plugin_dir, &metadata).await?;
        self.write_install_receipt(&plugin_dir, &receipt).await?;

        debug!("Plugin installed to: {}", plugin_dir.display());

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
            signature_verified,
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
            warnings,
        })
    }

    async fn cleanup_failed_install(&self, plugin_dir: &Path) {
        if !plugin_dir.exists() {
            return;
        }

        if let Err(e) = fs::remove_dir_all(plugin_dir).await {
            warn!(
                "Failed to clean up partial installation at {}: {}",
                plugin_dir.display(),
                e
            );
        }
    }

    async fn write_manifest(&self, plugin_dir: &Path, metadata: &PluginMetadata) -> Result<()> {
        let manifest_path = plugin_dir.join("manifest.json");
        let manifest_content =
            serde_json::to_string_pretty(metadata).context("Failed to serialize manifest")?;
        fs::write(&manifest_path, manifest_content)
            .await
            .context("Failed to write manifest")?;
        Ok(())
    }

    async fn write_install_receipt(
        &self,
        plugin_dir: &Path,
        receipt: &InstallReceipt,
    ) -> Result<()> {
        let receipt_path = plugin_dir.join("install_receipt.json");
        let content =
            serde_json::to_string_pretty(receipt).context("Failed to serialize install receipt")?;
        fs::write(&receipt_path, content)
            .await
            .context("Failed to write install receipt")?;
        Ok(())
    }

    async fn read_install_receipt(&self, plugin_dir: &Path) -> Result<Option<InstallReceipt>> {
        let receipt_path = plugin_dir.join("install_receipt.json");
        if !receipt_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&receipt_path)
            .await
            .context("Failed to read install receipt")?;
        let receipt: InstallReceipt =
            serde_json::from_str(&content).context("Failed to parse install receipt")?;
        Ok(Some(receipt))
    }

    async fn download_plugin_artifact(
        &self,
        metadata: &PluginMetadata,
        plugin_dir: &Path,
    ) -> Result<PathBuf> {
        let download_url = metadata
            .download_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Plugin metadata is missing download URL"))?;

        info!("Downloading plugin package from {}", download_url);
        let response = self
            .client
            .get(download_url)
            .send()
            .await
            .with_context(|| format!("Failed to download plugin package from {}", download_url))?;

        if !response.status().is_success() {
            bail!(
                "Failed to download plugin package: {} returned {}",
                download_url,
                response.status()
            );
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read plugin package bytes")?;
        if bytes.is_empty() {
            bail!("Downloaded plugin package is empty");
        }

        let artifact_dir = plugin_dir.join("artifact");
        fs::create_dir_all(&artifact_dir)
            .await
            .context("Failed to create artifact directory")?;

        let file_name = Self::artifact_filename_from_url(download_url);
        let artifact_path = artifact_dir.join(file_name);
        fs::write(&artifact_path, bytes)
            .await
            .context("Failed to write downloaded plugin artifact")?;

        debug!("Downloaded artifact to {}", artifact_path.display());
        Ok(artifact_path)
    }

    async fn resolve_expected_hash(
        &self,
        metadata: &PluginMetadata,
        warnings: &mut Vec<String>,
    ) -> Result<Option<String>> {
        if let Some(hash) = metadata.package_hash.as_deref() {
            let parsed = Self::parse_sha256_hash(hash)
                .ok_or_else(|| anyhow::anyhow!("Invalid package hash in metadata: {}", hash))?;
            return Ok(Some(parsed));
        }

        let Some(download_url) = metadata.download_url.as_deref() else {
            return Ok(None);
        };

        let hash_url = format!("{}.sha256", download_url);
        let response = self.client.get(&hash_url).send().await;
        let Ok(response) = response else {
            warnings.push(format!(
                "Unable to fetch optional hash sidecar from {}",
                hash_url
            ));
            return Ok(None);
        };

        if !response.status().is_success() {
            warnings.push(format!(
                "Hash sidecar {} unavailable (HTTP {})",
                hash_url,
                response.status()
            ));
            return Ok(None);
        }

        let text = response
            .text()
            .await
            .context("Failed to read hash sidecar body")?;

        match Self::parse_sha256_hash(&text) {
            Some(hash) => Ok(Some(hash)),
            None => {
                warnings.push("Hash sidecar did not contain a valid SHA256 hash".to_string());
                Ok(None)
            }
        }
    }

    fn parse_sha256_hash(raw: &str) -> Option<String> {
        let token = raw.split_whitespace().next()?.trim();
        let token = token
            .strip_prefix("sha256:")
            .or_else(|| token.strip_prefix("SHA256:"))
            .unwrap_or(token)
            .to_lowercase();

        let is_hex = token.chars().all(|c| c.is_ascii_hexdigit());
        if is_hex && token.len() == 64 {
            Some(token)
        } else {
            None
        }
    }

    async fn verify_file_hash(&self, file_path: &Path, expected_hash: &str) -> Result<()> {
        let actual_hash = Self::compute_file_sha256_hex(file_path).await?;
        if actual_hash != expected_hash {
            bail!(
                "Package hash mismatch for {} (expected {}, got {})",
                file_path.display(),
                expected_hash,
                actual_hash
            );
        }
        Ok(())
    }

    async fn compute_file_sha256_hex(file_path: &Path) -> Result<String> {
        let content = fs::read(file_path)
            .await
            .with_context(|| format!("Failed to read {}", file_path.display()))?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hex::encode(hasher.finalize()))
    }

    async fn unpack_if_supported(
        &self,
        artifact_path: &Path,
        plugin_dir: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<()> {
        let extension = artifact_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        match extension.as_deref() {
            Some("zip") => {
                let output_dir = plugin_dir.join("files");
                Self::extract_zip_archive(artifact_path, &output_dir).await?;
            }
            _ => {
                warnings.push(format!(
                    "Artifact {} is not an archive; stored without extraction",
                    artifact_path.display()
                ));
            }
        }

        Ok(())
    }

    async fn extract_zip_archive(archive_path: &Path, output_dir: &Path) -> Result<()> {
        let archive_path = archive_path.to_path_buf();
        let output_dir = output_dir.to_path_buf();

        task::spawn_blocking(move || -> Result<()> {
            std::fs::create_dir_all(&output_dir).with_context(|| {
                format!(
                    "Failed to create extraction directory {}",
                    output_dir.display()
                )
            })?;

            let file = std::fs::File::open(&archive_path).with_context(|| {
                format!("Failed to open zip archive {}", archive_path.display())
            })?;
            let mut archive =
                zip::ZipArchive::new(file).context("Failed to parse zip archive structure")?;

            for index in 0..archive.len() {
                let mut entry = archive
                    .by_index(index)
                    .with_context(|| format!("Failed to read zip entry {}", index))?;

                let Some(enclosed_name) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
                    continue;
                };

                let destination = output_dir.join(enclosed_name);
                if entry.is_dir() {
                    std::fs::create_dir_all(&destination).with_context(|| {
                        format!(
                            "Failed to create extracted directory {}",
                            destination.display()
                        )
                    })?;
                    continue;
                }

                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("Failed to create extracted parent {}", parent.display())
                    })?;
                }

                let mut out_file = std::fs::File::create(&destination).with_context(|| {
                    format!("Failed to create extracted file {}", destination.display())
                })?;
                std::io::copy(&mut entry, &mut out_file).with_context(|| {
                    format!("Failed to extract entry to {}", destination.display())
                })?;
            }

            Ok(())
        })
        .await
        .context("Zip extraction task failed to join")?
    }

    fn artifact_filename_from_url(url: &str) -> String {
        reqwest::Url::parse(url)
            .ok()
            .and_then(|parsed| {
                parsed
                    .path_segments()
                    .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
                    .map(ToString::to_string)
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "plugin.bundle".to_string())
    }

    async fn verify_signature_for_artifact(
        &self,
        metadata: &PluginMetadata,
        artifact_path: &Path,
        enforce_required: bool,
        warnings: &mut Vec<String>,
    ) -> Result<bool> {
        let Some(expected_fingerprint) = metadata.signature_fingerprint.as_deref() else {
            if enforce_required && self.config.require_signatures {
                bail!("Plugin is unsigned but signatures are required");
            }
            return Ok(false);
        };

        self.ensure_signature_file(metadata, artifact_path, warnings)
            .await?;

        let signature_metadata = crypto_utils::extract_signature_metadata(artifact_path)
            .with_context(|| {
                format!(
                    "Failed to extract signature metadata from {}",
                    artifact_path.display()
                )
            })?;

        let Some(signature_metadata) = signature_metadata else {
            if enforce_required && self.config.require_signatures {
                bail!(
                    "Plugin signature is required but no detached or embedded signature was found"
                );
            }
            warnings.push(
                "No detached or embedded signature was found; skipped signature verification"
                    .to_string(),
            );
            return Ok(false);
        };

        if signature_metadata.key_fingerprint != expected_fingerprint {
            bail!(
                "Signature fingerprint mismatch (expected {}, got {})",
                expected_fingerprint,
                signature_metadata.key_fingerprint
            );
        }

        let Some(trust_store_path) = self.config.trust_store_path.clone() else {
            if enforce_required && self.config.require_signatures {
                bail!(
                    "trust_store_path is not configured; cannot verify plugin signature cryptographically"
                );
            }
            warnings.push(
                "Signature fingerprint matches metadata, but cryptographic verification was skipped (no trust_store_path configured)".to_string(),
            );
            return Ok(false);
        };

        let verifier = VerificationService::new(VerificationConfig {
            require_binary_signatures: false,
            require_firmware_signatures: false,
            require_plugin_signatures: self.config.require_signatures,
            allow_unknown_signers: !self.config.require_signatures,
            trust_store_path,
            max_signature_age_seconds: None,
        })
        .context("Failed to initialize verification service")?;

        let verification_result = verifier
            .verify_plugin(artifact_path)
            .context("Cryptographic signature verification failed")?;

        if !verification_result.signature_valid {
            if enforce_required && self.config.require_signatures {
                bail!("Plugin signature is invalid");
            }
            warnings.push("Plugin signature did not verify cryptographically".to_string());
            return Ok(false);
        }

        Ok(true)
    }

    async fn ensure_signature_file(
        &self,
        metadata: &PluginMetadata,
        artifact_path: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<PathBuf> {
        let signature_path = crypto_utils::get_signature_path(artifact_path);
        if signature_path.exists() {
            return Ok(signature_path);
        }

        let Some(download_url) = metadata.download_url.as_deref() else {
            return Ok(signature_path);
        };

        let signature_url = format!("{}.sig", download_url);
        let response = self.client.get(&signature_url).send().await;
        let Ok(response) = response else {
            warnings.push(format!(
                "Failed to fetch detached signature from {}",
                signature_url
            ));
            return Ok(signature_path);
        };

        if !response.status().is_success() {
            warnings.push(format!(
                "Detached signature {} unavailable (HTTP {})",
                signature_url,
                response.status()
            ));
            return Ok(signature_path);
        }

        let body = response
            .bytes()
            .await
            .context("Failed to read detached signature response body")?;

        fs::write(&signature_path, body)
            .await
            .with_context(|| format!("Failed to write {}", signature_path.display()))?;

        Ok(signature_path)
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
        let mut files_intact = true;

        if !installed.install_path.exists() {
            issues.push("Installation directory is missing".to_string());
            files_intact = false;
        }

        let manifest_path = installed.install_path.join("manifest.json");
        if !manifest_path.exists() {
            issues.push("Manifest file is missing".to_string());
            files_intact = false;
        }

        let mut artifact_path: Option<PathBuf> = None;
        let receipt = self.read_install_receipt(&installed.install_path).await?;
        if let Some(receipt) = receipt {
            if let Some(rel_path) = receipt.artifact_rel_path {
                let candidate = installed.install_path.join(rel_path);
                if candidate.exists() {
                    artifact_path = Some(candidate);
                } else {
                    issues.push("Downloaded artifact is missing".to_string());
                    files_intact = false;
                }
            }

            if let Some(expected_hash) = receipt.expected_sha256.as_deref() {
                if let Some(path) = artifact_path.as_deref() {
                    let actual_hash = Self::compute_file_sha256_hex(path).await?;
                    if actual_hash != expected_hash {
                        issues.push(format!(
                            "Artifact hash mismatch (expected {}, got {})",
                            expected_hash, actual_hash
                        ));
                        files_intact = false;
                    }
                } else {
                    issues.push(
                        "Expected artifact hash is present but downloaded artifact is unavailable"
                            .to_string(),
                    );
                    files_intact = false;
                }
            }
        }

        let signature_valid = if installed.metadata.signature_fingerprint.is_some() {
            if let Some(path) = artifact_path.as_deref() {
                let mut signature_notes = Vec::new();
                match self
                    .verify_signature_for_artifact(
                        &installed.metadata,
                        path,
                        false,
                        &mut signature_notes,
                    )
                    .await
                {
                    Ok(valid) => {
                        if !signature_notes.is_empty() {
                            issues.extend(signature_notes);
                        }
                        if !valid {
                            files_intact = false;
                        }
                        Some(valid)
                    }
                    Err(e) => {
                        issues.push(format!("Signature verification failed: {}", e));
                        files_intact = false;
                        Some(false)
                    }
                }
            } else {
                issues.push("Signature expected but install artifact is unavailable".to_string());
                files_intact = false;
                Some(false)
            }
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
    use sha2::{Digest, Sha256};
    use std::io::Write;
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

    fn create_zip_package() -> Result<Vec<u8>> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        writer
            .start_file("plugin.wasm", options)
            .context("Failed to start zip entry")?;
        writer
            .write_all(b"(module)")
            .context("Failed to write zip entry")?;

        let finished = writer.finish().context("Failed to finish zip package")?;
        Ok(finished.into_inner())
    }

    #[tokio::test]
    async fn test_installer_config_default() -> Result<()> {
        let config = InstallerConfig::default();
        assert!(config.require_signatures);
        assert!(config.verify_hashes);
        assert!(config.trust_store_path.is_some());
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
            require_signatures: false,
            ..Default::default()
        };

        let installer = PluginInstaller::new(registry, config)?;
        let installed = installer.list_installed().await?;

        assert!(installed.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_install_unsigned_plugin_rejected_when_signatures_required() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;
        let package_bytes = create_zip_package()?;

        let mut catalog = PluginCatalog::new();
        let metadata = create_test_metadata("Unsigned Plugin", "1.0.0")
            .with_download_url(format!("{}/v1/plugins/unsigned.zip", mock_server.uri()));
        let plugin_id = metadata.id.clone();
        let _ = catalog.add_plugin(metadata);
        let index = crate::registry_client::RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/unsigned.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(package_bytes))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/unsigned.zip.sha256"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);
        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);
        registry.refresh().await?;

        let installer = PluginInstaller::new(
            registry,
            InstallerConfig {
                install_dir: temp_dir.path().join("plugins"),
                require_signatures: true,
                ..Default::default()
            },
        )?;

        let result = installer.install(&plugin_id, None).await;
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.to_lowercase().contains("unsigned"),
            "expected unsigned-plugin rejection, got: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_install_rejects_missing_download_url() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let metadata = create_test_metadata("No Download URL", "1.0.0");
        let plugin_id = metadata.id.clone();

        let mut catalog = PluginCatalog::new();
        let _ = catalog.add_plugin(metadata);
        let index = crate::registry_client::RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);
        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);
        registry.refresh().await?;

        let install_root = temp_dir.path().join("plugins");
        let installer = PluginInstaller::new(
            registry,
            InstallerConfig {
                install_dir: install_root.clone(),
                require_signatures: false,
                ..Default::default()
            },
        )?;

        let result = installer.install(&plugin_id, None).await;
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.to_lowercase().contains("download url"),
            "expected missing download URL error, got: {}",
            err_msg
        );

        let plugin_dir = install_root.join("no-download-url").join("1.0.0");
        assert!(
            !plugin_dir.exists(),
            "failed install should not leave partial directory at {}",
            plugin_dir.display()
        );

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
            require_signatures: false,
            ..Default::default()
        };

        let installer = PluginInstaller::new(registry, config)?;
        let id = PluginId::new();

        assert!(!installer.is_installed(&id));

        Ok(())
    }

    #[tokio::test]
    async fn test_install_downloads_verifies_hash_and_extracts_zip() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let package_bytes = create_zip_package()?;
        let mut hasher = Sha256::new();
        hasher.update(&package_bytes);
        let package_hash = hex::encode(hasher.finalize());

        let metadata = create_test_metadata("Downloadable Plugin", "1.0.0")
            .with_download_url(format!("{}/v1/plugins/downloadable.zip", mock_server.uri()))
            .with_package_hash(package_hash);
        let plugin_id = metadata.id.clone();

        let mut catalog = PluginCatalog::new();
        let _ = catalog.add_plugin(metadata);
        let index = crate::registry_client::RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/downloadable.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(package_bytes))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/downloadable.zip.sha256"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);
        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);
        registry.refresh().await?;

        let installer = PluginInstaller::new(
            registry,
            InstallerConfig {
                install_dir: temp_dir.path().join("plugins"),
                require_signatures: false,
                verify_hashes: true,
                trust_store_path: None,
                ..Default::default()
            },
        )?;

        let result = installer.install(&plugin_id, None).await?;

        let extracted_file = result.install_path.join("files").join("plugin.wasm");
        assert!(extracted_file.exists());
        assert!(result.install_path.join("manifest.json").exists());
        assert!(result.install_path.join("install_receipt.json").exists());

        Ok(())
    }

    #[tokio::test]
    async fn test_install_rejects_hash_mismatch() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let package_bytes = create_zip_package()?;
        let bad_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let metadata = create_test_metadata("Hash Mismatch Plugin", "1.0.0")
            .with_download_url(format!(
                "{}/v1/plugins/hash-mismatch.zip",
                mock_server.uri()
            ))
            .with_package_hash(bad_hash);
        let plugin_id = metadata.id.clone();

        let mut catalog = PluginCatalog::new();
        let _ = catalog.add_plugin(metadata);
        let index = crate::registry_client::RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/hash-mismatch.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(package_bytes))
            .mount(&mock_server)
            .await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);
        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);
        registry.refresh().await?;

        let installer = PluginInstaller::new(
            registry,
            InstallerConfig {
                install_dir: temp_dir.path().join("plugins"),
                require_signatures: false,
                verify_hashes: true,
                trust_store_path: None,
                ..Default::default()
            },
        )?;

        let result = installer.install(&plugin_id, None).await;
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.to_lowercase().contains("hash mismatch"),
            "expected hash mismatch error, got: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_detects_artifact_tampering() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let package_bytes = create_zip_package()?;
        let mut hasher = Sha256::new();
        hasher.update(&package_bytes);
        let package_hash = hex::encode(hasher.finalize());

        let metadata = create_test_metadata("Integrity Plugin", "1.0.0")
            .with_download_url(format!("{}/v1/plugins/integrity.zip", mock_server.uri()))
            .with_package_hash(package_hash);
        let plugin_id = metadata.id.clone();

        let mut catalog = PluginCatalog::new();
        let _ = catalog.add_plugin(metadata);
        let index = crate::registry_client::RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/integrity.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(package_bytes))
            .mount(&mock_server)
            .await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);
        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);
        registry.refresh().await?;

        let installer = PluginInstaller::new(
            registry,
            InstallerConfig {
                install_dir: temp_dir.path().join("plugins"),
                require_signatures: false,
                verify_hashes: true,
                trust_store_path: None,
                ..Default::default()
            },
        )?;

        let install_result = installer.install(&plugin_id, None).await?;
        let artifact_path = install_result
            .install_path
            .join("artifact")
            .join("integrity.zip");
        fs::write(&artifact_path, b"tampered-package").await?;

        let verify_result = installer.verify(&plugin_id).await?;
        assert!(!verify_result.files_intact);
        assert!(
            verify_result
                .issues
                .iter()
                .any(|issue| issue.to_lowercase().contains("hash mismatch")),
            "expected hash mismatch issue, got: {:?}",
            verify_result.issues
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_install_rolls_back_on_invalid_zip_archive() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mock_server = MockServer::start().await;

        let package_bytes = b"this-is-not-a-valid-zip-archive".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&package_bytes);
        let package_hash = hex::encode(hasher.finalize());

        let metadata = create_test_metadata("Broken Archive Plugin", "1.0.0")
            .with_download_url(format!("{}/v1/plugins/broken.zip", mock_server.uri()))
            .with_package_hash(package_hash);
        let plugin_id = metadata.id.clone();

        let mut catalog = PluginCatalog::new();
        let _ = catalog.add_plugin(metadata);
        let index = crate::registry_client::RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/plugins/broken.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(package_bytes))
            .mount(&mock_server)
            .await;

        let registry_config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_require_signed_index(false);
        let registry = Arc::new(RemoteRegistryClient::new(registry_config)?);
        registry.refresh().await?;

        let install_root = temp_dir.path().join("plugins");
        let installer = PluginInstaller::new(
            registry,
            InstallerConfig {
                install_dir: install_root.clone(),
                require_signatures: false,
                verify_hashes: true,
                trust_store_path: None,
                ..Default::default()
            },
        )?;

        let result = installer.install(&plugin_id, None).await;
        assert!(result.is_err());

        let plugin_dir = install_root.join("broken-archive-plugin").join("1.0.0");
        assert!(
            !plugin_dir.exists(),
            "failed install should clean up {}",
            plugin_dir.display()
        );

        Ok(())
    }
}
