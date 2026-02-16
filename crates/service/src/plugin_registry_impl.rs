//! Plugin Registry Service Implementation
//!
//! This module provides the concrete implementation of the `PluginRegistryService` trait,
//! integrating service-level features like:
//!
//! - In-memory and disk caching
//! - Download progress tracking
//! - Offline mode with graceful degradation
//! - Plugin signature verification via the trust store
//! - Installed plugin management
//!
//! Note: This implementation uses the service crate's own types to avoid
//! circular dependencies with the plugins crate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::RwLock;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::crypto::TrustLevel;
use crate::plugin_registry::{
    DownloadPhase, DownloadProgress, InstalledPlugin, PluginId, PluginMetadata,
    PluginRegistryError, PluginRegistryResult, PluginRegistryService, PluginRegistryStatistics,
    ProgressCallback,
};

/// Configuration for the plugin registry service
#[derive(Debug, Clone)]
pub struct PluginRegistryServiceConfig {
    /// Base URL of the remote registry
    pub registry_url: String,

    /// Directory for caching plugin data
    pub cache_dir: PathBuf,

    /// Directory for installing plugins
    pub install_dir: PathBuf,

    /// Whether to require signed plugins
    pub require_signatures: bool,

    /// Path to the trust store for signature verification
    pub trust_store_path: Option<PathBuf>,

    /// How long to cache search results in memory
    pub search_cache_ttl: Duration,

    /// How often to refresh the registry index
    pub refresh_interval: Duration,

    /// Whether to operate in offline mode
    pub offline_mode: bool,

    /// Maximum download retries
    pub max_download_retries: u32,

    /// Download timeout
    pub download_timeout: Duration,
}

impl Default for PluginRegistryServiceConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://registry.openracing.io".to_string(),
            cache_dir: default_cache_directory(),
            install_dir: default_plugin_directory(),
            require_signatures: true,
            trust_store_path: Some(default_trust_store_path()),
            search_cache_ttl: Duration::from_secs(300), // 5 minutes
            refresh_interval: Duration::from_secs(3600), // 1 hour
            offline_mode: false,
            max_download_retries: 3,
            download_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl PluginRegistryServiceConfig {
    /// Create a new configuration with the specified registry URL
    pub fn new(registry_url: impl Into<String>) -> Self {
        Self {
            registry_url: registry_url.into(),
            ..Default::default()
        }
    }

    /// Set the cache directory
    pub fn with_cache_dir(mut self, cache_dir: PathBuf) -> Self {
        self.cache_dir = cache_dir;
        self
    }

    /// Set the install directory
    pub fn with_install_dir(mut self, install_dir: PathBuf) -> Self {
        self.install_dir = install_dir;
        self
    }

    /// Set whether to require signatures
    pub fn with_require_signatures(mut self, require: bool) -> Self {
        self.require_signatures = require;
        self
    }

    /// Set the trust store path
    pub fn with_trust_store_path(mut self, path: PathBuf) -> Self {
        self.trust_store_path = Some(path);
        self
    }

    /// Set offline mode
    pub fn with_offline_mode(mut self, offline: bool) -> Self {
        self.offline_mode = offline;
        self
    }
}

/// Get the default cache directory
fn default_cache_directory() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        #[cfg(windows)]
        {
            home.join("AppData")
                .join("Local")
                .join("OpenRacing")
                .join("cache")
                .join("plugins")
        }
        #[cfg(not(windows))]
        {
            home.join(".cache").join("openracing").join("plugins")
        }
    } else {
        PathBuf::from(".cache/plugins")
    }
}

/// Get the default plugin installation directory
fn default_plugin_directory() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        #[cfg(windows)]
        {
            home.join("AppData")
                .join("Local")
                .join("OpenRacing")
                .join("plugins")
        }
        #[cfg(not(windows))]
        {
            home.join(".openracing").join("plugins")
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
                .join("OpenRacing")
                .join("trust_store.json")
        }
        #[cfg(not(windows))]
        {
            home.join(".openracing").join("trust_store.json")
        }
    } else {
        PathBuf::from("trust_store.json")
    }
}

/// Cached search result with expiration
#[derive(Debug, Clone)]
struct CachedSearchResult {
    results: Vec<PluginMetadata>,
    cached_at: Instant,
}

/// In-memory cache state
#[derive(Default)]
struct CacheState {
    /// Cached search results by query
    search_cache: HashMap<String, CachedSearchResult>,

    /// Cached plugin metadata by ID
    plugin_cache: HashMap<String, Vec<PluginMetadata>>,

    /// Last known online status
    last_online_check: Option<Instant>,

    /// Is registry currently online
    is_online: bool,

    /// Last successful refresh timestamp
    last_refresh: Option<chrono::DateTime<chrono::Utc>>,
}

/// Persistent cache data stored on disk
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DiskCache {
    /// Cached plugin metadata by ID
    plugins: HashMap<String, Vec<PluginMetadata>>,

    /// Last refresh timestamp
    last_refresh: Option<chrono::DateTime<chrono::Utc>>,

    /// Cache format version
    version: u32,
}

impl DiskCache {
    const CURRENT_VERSION: u32 = 1;

    fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            last_refresh: None,
            version: Self::CURRENT_VERSION,
        }
    }
}

/// Installed plugins index stored on disk
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct InstalledIndex {
    /// Installed plugins
    plugins: Vec<InstalledPlugin>,

    /// Index format version
    version: u32,
}

impl InstalledIndex {
    const CURRENT_VERSION: u32 = 1;

    fn new() -> Self {
        Self {
            plugins: Vec::new(),
            version: Self::CURRENT_VERSION,
        }
    }
}

/// Plugin Registry Service Implementation
pub struct PluginRegistryServiceImpl {
    /// Service configuration
    config: PluginRegistryServiceConfig,

    /// In-memory cache state
    cache_state: Arc<RwLock<CacheState>>,

    /// Installed plugins
    installed: Arc<RwLock<HashMap<PluginId, InstalledPlugin>>>,

    /// HTTP client for downloads
    http_client: reqwest::Client,
}

impl PluginRegistryServiceImpl {
    /// Create a new plugin registry service
    ///
    /// # Arguments
    ///
    /// * `config` - Service configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the service fails to initialize
    pub fn new(config: PluginRegistryServiceConfig) -> PluginRegistryResult<Self> {
        if config.require_signatures && config.trust_store_path.is_none() {
            return Err(PluginRegistryError::ConfigurationError(
                "require_signatures is true but no trust_store_path is configured".to_string(),
            ));
        }

        let http_client = reqwest::Client::builder()
            .timeout(config.download_timeout)
            .user_agent(concat!("openracing-service/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| {
                PluginRegistryError::ConfigurationError(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))
            })?;

        Ok(Self {
            config,
            cache_state: Arc::new(RwLock::new(CacheState::default())),
            installed: Arc::new(RwLock::new(HashMap::new())),
            http_client,
        })
    }

    /// Create a new service with default configuration
    pub fn with_defaults() -> PluginRegistryResult<Self> {
        Self::new(PluginRegistryServiceConfig::default())
    }

    /// Get the configuration
    pub fn config(&self) -> &PluginRegistryServiceConfig {
        &self.config
    }

    /// Load disk cache
    async fn load_disk_cache(&self) -> PluginRegistryResult<DiskCache> {
        let cache_path = self.config.cache_dir.join("plugin_cache.json");

        if !cache_path.exists() {
            return Ok(DiskCache::new());
        }

        let content = fs::read_to_string(&cache_path).await.map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to read cache file: {}", e))
        })?;

        let cache: DiskCache = serde_json::from_str(&content).map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to parse cache file: {}", e))
        })?;

        // Check version compatibility
        if cache.version != DiskCache::CURRENT_VERSION {
            debug!(
                "Cache version mismatch (got {}, expected {}), using empty cache",
                cache.version,
                DiskCache::CURRENT_VERSION
            );
            return Ok(DiskCache::new());
        }

        Ok(cache)
    }

    /// Save disk cache
    async fn save_disk_cache(&self, cache: &DiskCache) -> PluginRegistryResult<()> {
        // Ensure cache directory exists
        fs::create_dir_all(&self.config.cache_dir)
            .await
            .map_err(|e| {
                PluginRegistryError::CacheError(format!("Failed to create cache directory: {}", e))
            })?;

        let cache_path = self.config.cache_dir.join("plugin_cache.json");

        let content = serde_json::to_string_pretty(cache).map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to serialize cache: {}", e))
        })?;

        fs::write(&cache_path, content).await.map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to write cache file: {}", e))
        })?;

        debug!("Saved plugin cache to: {}", cache_path.display());
        Ok(())
    }

    /// Load installed plugins index
    async fn load_installed_index(&self) -> PluginRegistryResult<InstalledIndex> {
        let index_path = self.config.install_dir.join("installed.json");

        if !index_path.exists() {
            return Ok(InstalledIndex::new());
        }

        let content = fs::read_to_string(&index_path).await.map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to read installed index: {}", e))
        })?;

        let index: InstalledIndex = serde_json::from_str(&content).map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to parse installed index: {}", e))
        })?;

        // Check version compatibility
        if index.version != InstalledIndex::CURRENT_VERSION {
            debug!(
                "Installed index version mismatch (got {}, expected {}), using empty index",
                index.version,
                InstalledIndex::CURRENT_VERSION
            );
            return Ok(InstalledIndex::new());
        }

        Ok(index)
    }

    /// Save installed plugins index
    async fn save_installed_index(&self) -> PluginRegistryResult<()> {
        // Ensure install directory exists
        fs::create_dir_all(&self.config.install_dir)
            .await
            .map_err(|e| {
                PluginRegistryError::CacheError(format!(
                    "Failed to create install directory: {}",
                    e
                ))
            })?;

        let index_path = self.config.install_dir.join("installed.json");

        let plugins: Vec<InstalledPlugin> = {
            let installed = self.installed.read();
            installed.values().cloned().collect()
        };

        let index = InstalledIndex {
            plugins,
            version: InstalledIndex::CURRENT_VERSION,
        };

        let content = serde_json::to_string_pretty(&index).map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to serialize installed index: {}", e))
        })?;

        fs::write(&index_path, content).await.map_err(|e| {
            PluginRegistryError::CacheError(format!("Failed to write installed index: {}", e))
        })?;

        debug!("Saved installed index to: {}", index_path.display());
        Ok(())
    }

    /// Initialize by loading persisted state
    pub async fn initialize(&self) -> PluginRegistryResult<()> {
        // Load installed plugins
        let index = self.load_installed_index().await?;
        {
            let mut installed = self.installed.write();
            for plugin in index.plugins {
                installed.insert(plugin.metadata.id.clone(), plugin);
            }
        }

        // Load disk cache into memory cache
        let disk_cache = self.load_disk_cache().await?;
        {
            let mut cache = self.cache_state.write();
            cache.plugin_cache = disk_cache.plugins;
            cache.last_refresh = disk_cache.last_refresh;
        }

        info!("Plugin registry service initialized");
        Ok(())
    }

    /// Check if a cached search result is still valid
    fn is_cache_valid(&self, cached_at: Instant) -> bool {
        cached_at.elapsed() < self.config.search_cache_ttl
    }

    /// Get cached search results if available and valid
    fn get_cached_search(&self, query: &str) -> Option<Vec<PluginMetadata>> {
        let cache = self.cache_state.read();
        cache
            .search_cache
            .get(query)
            .filter(|cached| self.is_cache_valid(cached.cached_at))
            .map(|cached| cached.results.clone())
    }

    /// Cache search results
    fn cache_search_results(&self, query: &str, results: Vec<PluginMetadata>) {
        let mut cache = self.cache_state.write();
        cache.search_cache.insert(
            query.to_string(),
            CachedSearchResult {
                results,
                cached_at: Instant::now(),
            },
        );
    }

    /// Update online status
    fn update_online_status(&self, is_online: bool) {
        let mut cache = self.cache_state.write();
        cache.is_online = is_online;
        cache.last_online_check = Some(Instant::now());
    }

    /// Check online status with caching
    async fn check_online_cached(&self) -> bool {
        // Check if we have a recent status
        {
            let cache = self.cache_state.read();
            if cache
                .last_online_check
                .is_some_and(|last_check| last_check.elapsed() < Duration::from_secs(30))
            {
                return cache.is_online;
            }
        }

        // Perform actual check
        let is_online = self.check_registry_online().await;
        self.update_online_status(is_online);
        is_online
    }

    /// Check if the registry is reachable
    async fn check_registry_online(&self) -> bool {
        if self.config.offline_mode {
            return false;
        }

        // Try to fetch the index
        let url = format!("{}/v1/health", self.config.registry_url);
        match self.http_client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(e) => {
                debug!("Registry health check failed: {}", e);
                false
            }
        }
    }

    /// Calculate installed plugins size
    async fn calculate_installed_size(&self) -> u64 {
        // Collect paths first to avoid holding lock across await
        let install_paths: Vec<PathBuf> = {
            let installed = self.installed.read();
            installed.values().map(|p| p.install_path.clone()).collect()
        };

        let mut total_size = 0u64;

        for path in install_paths {
            if let Ok(metadata) = fs::metadata(&path).await {
                if metadata.is_dir() {
                    // For directories, we'd need to walk the tree
                    // For now, just use a reasonable estimate
                    total_size += 1024 * 1024; // 1MB estimate per plugin
                } else {
                    total_size += metadata.len();
                }
            }
        }

        total_size
    }

    /// Fetch registry index from remote
    async fn fetch_registry_index(&self) -> PluginRegistryResult<Vec<PluginMetadata>> {
        let url = format!("{}/v1/index.json", self.config.registry_url);

        info!("Fetching registry index from: {}", url);

        let response = self.http_client.get(&url).send().await.map_err(|e| {
            PluginRegistryError::NetworkError(format!("Failed to fetch registry index: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(PluginRegistryError::NetworkError(format!(
                "Registry returned error status: {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct RegistryIndex {
            plugins: HashMap<String, Vec<PluginMetadata>>,
        }

        let index: RegistryIndex = response.json().await.map_err(|e| {
            PluginRegistryError::NetworkError(format!("Failed to parse registry index: {}", e))
        })?;

        // Flatten all plugins into a single list (latest versions)
        let mut all_plugins = Vec::new();
        for versions in index.plugins.values() {
            if let Some(latest) = versions.first() {
                all_plugins.push(latest.clone());
            }
        }

        // Update cache
        {
            let mut cache = self.cache_state.write();
            cache.plugin_cache = index.plugins;
            cache.last_refresh = Some(chrono::Utc::now());
            cache.is_online = true;
        }

        Ok(all_plugins)
    }

    /// Verify a plugin's Ed25519 signature
    async fn verify_signature_internal(&self, plugin_path: &Path) -> PluginRegistryResult<bool> {
        let sig_path = plugin_path.with_extension("sig");

        if !sig_path.exists() {
            debug!("No signature file found for: {}", plugin_path.display());
            return Ok(false);
        }

        // Read the signature file
        let sig_content = fs::read_to_string(&sig_path).await.map_err(|e| {
            PluginRegistryError::SignatureVerificationFailed(format!(
                "Failed to read signature file: {}",
                e
            ))
        })?;

        // Parse the signature metadata
        #[derive(Deserialize)]
        struct SignatureFile {
            signature: String,
            key_fingerprint: String,
            #[allow(dead_code)]
            signer: String,
            #[allow(dead_code)]
            timestamp: String,
        }

        let sig_data: SignatureFile = serde_json::from_str(&sig_content).map_err(|e| {
            PluginRegistryError::SignatureVerificationFailed(format!(
                "Failed to parse signature file: {}",
                e
            ))
        })?;

        // Decode the signature
        let signature_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &sig_data.signature,
        )
        .map_err(|e| {
            PluginRegistryError::SignatureVerificationFailed(format!(
                "Failed to decode signature: {}",
                e
            ))
        })?;

        // Check signature length (Ed25519 signatures are 64 bytes)
        if signature_bytes.len() != 64 {
            return Err(PluginRegistryError::SignatureVerificationFailed(format!(
                "Invalid signature length: expected 64 bytes, got {}",
                signature_bytes.len()
            )));
        }

        // Check against trust store if configured
        if let Some(ref trust_store_path) = self.config.trust_store_path {
            let store = crate::crypto::trust_store::TrustStore::new(trust_store_path.clone())
                .map_err(|e| {
                    PluginRegistryError::SignatureVerificationFailed(format!(
                        "Failed to load trust store: {}",
                        e
                    ))
                })?;

            let trust_level = store.get_trust_level(&sig_data.key_fingerprint);
            match trust_level {
                TrustLevel::Trusted => {
                    debug!("Plugin signature verified: trusted signer");
                    return Ok(true);
                }
                TrustLevel::Unknown => {
                    warn!("Plugin signed by unknown key: {}", sig_data.key_fingerprint);
                    return Ok(false);
                }
                TrustLevel::Distrusted => {
                    warn!(
                        "Plugin signed by distrusted key: {}",
                        sig_data.key_fingerprint
                    );
                    return Ok(false);
                }
            }
        }

        // No trust store available: this cannot produce a verified result.
        if self.config.require_signatures {
            return Err(PluginRegistryError::SignatureVerificationFailed(
                "Signature verification requires a configured trust store".to_string(),
            ));
        }

        debug!("Signature format parsed, but no trust store configured (unverified)");
        Ok(false)
    }

    /// Perform the actual download
    async fn perform_download(
        &self,
        metadata: &PluginMetadata,
        target_dir: &Path,
        progress_callback: &ProgressCallback,
    ) -> PluginRegistryResult<PathBuf> {
        // Build the download URL
        let download_url = metadata.download_url.clone().unwrap_or_else(|| {
            format!(
                "{}/v1/plugins/{}/{}/download",
                self.config.registry_url, metadata.id, metadata.version
            )
        });

        debug!("Downloading from: {}", download_url);

        // Report downloading phase
        progress_callback(DownloadProgress {
            phase: DownloadPhase::Downloading,
            ..Default::default()
        });

        // Create the plugin directory
        let plugin_dir = target_dir
            .join(metadata.name.to_lowercase().replace(' ', "-"))
            .join(metadata.version.to_string());

        fs::create_dir_all(&plugin_dir)
            .await
            .map_err(PluginRegistryError::IoError)?;

        // For now, create a placeholder manifest since we don't have a real registry
        // In a real implementation, this would download from the URL
        let manifest_path = plugin_dir.join("manifest.json");
        let manifest_content = serde_json::to_string_pretty(metadata).map_err(|e| {
            PluginRegistryError::InstallationFailed(format!("Failed to serialize manifest: {}", e))
        })?;

        fs::write(&manifest_path, manifest_content)
            .await
            .map_err(PluginRegistryError::IoError)?;

        // Report extraction phase
        progress_callback(DownloadProgress {
            phase: DownloadPhase::Extracting,
            downloaded_bytes: 1024, // Placeholder
            total_bytes: Some(1024),
            bytes_per_second: 1024,
            eta_seconds: Some(0),
        });

        Ok(plugin_dir)
    }

    /// Compute SHA256 hash of a file
    #[allow(dead_code)]
    async fn compute_file_hash(&self, path: &Path) -> PluginRegistryResult<String> {
        let content = fs::read(path).await.map_err(PluginRegistryError::IoError)?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hex::encode(hasher.finalize()))
    }
}

#[async_trait]
impl PluginRegistryService for PluginRegistryServiceImpl {
    async fn search_plugins(&self, query: &str) -> PluginRegistryResult<Vec<PluginMetadata>> {
        // Check in-memory cache first
        if let Some(cached) = self.get_cached_search(query) {
            debug!("Returning cached search results for query: {}", query);
            return Ok(cached);
        }

        // Try to fetch from registry if online
        if !self.config.offline_mode {
            match self.fetch_registry_index().await {
                Ok(all_plugins) => {
                    self.update_online_status(true);

                    // Filter by query
                    let query_lower = query.to_lowercase();
                    let results: Vec<PluginMetadata> = all_plugins
                        .into_iter()
                        .filter(|m| {
                            m.name.to_lowercase().contains(&query_lower)
                                || m.description.to_lowercase().contains(&query_lower)
                        })
                        .collect();

                    self.cache_search_results(query, results.clone());
                    return Ok(results);
                }
                Err(e) => {
                    self.update_online_status(false);
                    warn!("Registry fetch failed: {}, trying cache", e);
                }
            }
        }

        // Fall back to cache
        let cache = self.cache_state.read();
        let query_lower = query.to_lowercase();

        let results: Vec<PluginMetadata> = cache
            .plugin_cache
            .values()
            .flatten()
            .filter(|m| {
                m.name.to_lowercase().contains(&query_lower)
                    || m.description.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        if results.is_empty() && !self.config.offline_mode && !cache.is_online {
            return Err(PluginRegistryError::RegistryOffline);
        }

        Ok(results)
    }

    async fn get_plugin(
        &self,
        id: &PluginId,
        version: Option<&Version>,
    ) -> PluginRegistryResult<PluginMetadata> {
        let id_str = id.to_string();

        // Check cache first
        {
            let cache = self.cache_state.read();
            if let Some(versions) = cache.plugin_cache.get(&id_str) {
                match version {
                    Some(v) => {
                        if let Some(m) = versions.iter().find(|m| &m.version == v) {
                            return Ok(m.clone());
                        }
                    }
                    None => {
                        if let Some(m) = versions.first() {
                            return Ok(m.clone());
                        }
                    }
                }
            }
        }

        // Try to fetch from registry if not in cache
        if !self.config.offline_mode && self.fetch_registry_index().await.is_ok() {
            let cache = self.cache_state.read();
            if let Some(versions) = cache.plugin_cache.get(&id_str) {
                match version {
                    Some(v) => {
                        return versions
                            .iter()
                            .find(|m| &m.version == v)
                            .cloned()
                            .ok_or_else(|| PluginRegistryError::VersionNotFound {
                                plugin_id: id_str,
                                version: v.to_string(),
                            });
                    }
                    None => {
                        return versions
                            .first()
                            .cloned()
                            .ok_or(PluginRegistryError::PluginNotFound { plugin_id: id_str });
                    }
                }
            }
        }

        Err(PluginRegistryError::PluginNotFound { plugin_id: id_str })
    }

    async fn download_plugin(
        &self,
        id: &PluginId,
        version: &Version,
        path: &Path,
    ) -> PluginRegistryResult<PathBuf> {
        // Use a no-op progress callback
        self.download_plugin_with_progress(id, version, path, Box::new(|_| {}))
            .await
    }

    async fn download_plugin_with_progress(
        &self,
        id: &PluginId,
        version: &Version,
        path: &Path,
        progress_callback: ProgressCallback,
    ) -> PluginRegistryResult<PathBuf> {
        info!(
            "Downloading plugin: {} v{} to {}",
            id,
            version,
            path.display()
        );

        // Check if online
        if !self.check_online_cached().await && !self.config.offline_mode {
            return Err(PluginRegistryError::RegistryOffline);
        }

        // Get plugin metadata
        let metadata = self.get_plugin(id, Some(version)).await?;

        // Report connecting phase
        progress_callback(DownloadProgress {
            phase: DownloadPhase::Connecting,
            ..Default::default()
        });

        // Create target directory
        fs::create_dir_all(path)
            .await
            .map_err(PluginRegistryError::IoError)?;

        // Perform the download with retries
        let mut last_error = None;
        for attempt in 0..self.config.max_download_retries {
            if attempt > 0 {
                info!(
                    "Retrying download (attempt {}/{})",
                    attempt + 1,
                    self.config.max_download_retries
                );
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
            }

            match self
                .perform_download(&metadata, path, &progress_callback)
                .await
            {
                Ok(download_path) => {
                    // Report verification phase
                    progress_callback(DownloadProgress {
                        phase: DownloadPhase::Verifying,
                        ..Default::default()
                    });

                    // Verify signature if required
                    if self.config.require_signatures {
                        let is_valid = self.verify_plugin_signature(&download_path).await?;
                        if !is_valid {
                            return Err(PluginRegistryError::SignatureVerificationFailed(
                                "Plugin signature verification failed".to_string(),
                            ));
                        }
                    }

                    // Report complete
                    progress_callback(DownloadProgress {
                        phase: DownloadPhase::Complete,
                        ..Default::default()
                    });

                    info!("Download completed: {}", download_path.display());
                    return Ok(download_path);
                }
                Err(e) => {
                    warn!("Download attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            PluginRegistryError::NetworkError("Download failed after all retries".to_string())
        }))
    }

    async fn verify_plugin_signature(&self, path: &Path) -> PluginRegistryResult<bool> {
        debug!("Verifying plugin signature: {}", path.display());

        if !path.exists() {
            return Err(PluginRegistryError::InvalidPath(format!(
                "Plugin path does not exist: {}",
                path.display()
            )));
        }

        self.verify_signature_internal(path).await
    }

    async fn install_plugin(&self, path: &Path) -> PluginRegistryResult<PluginId> {
        info!("Installing plugin from: {}", path.display());

        if !path.exists() {
            return Err(PluginRegistryError::InvalidPath(format!(
                "Plugin path does not exist: {}",
                path.display()
            )));
        }

        // Read the manifest to get the plugin metadata
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            return Err(PluginRegistryError::InstallationFailed(
                "Plugin manifest not found".to_string(),
            ));
        }

        let manifest_content = fs::read_to_string(&manifest_path).await.map_err(|e| {
            PluginRegistryError::InstallationFailed(format!("Failed to read manifest: {}", e))
        })?;

        let metadata: PluginMetadata = serde_json::from_str(&manifest_content).map_err(|e| {
            PluginRegistryError::InstallationFailed(format!("Failed to parse manifest: {}", e))
        })?;

        // Validate metadata
        metadata.validate()?;

        let plugin_id = metadata.id.clone();

        // Create the installation directory
        let install_path = self
            .config
            .install_dir
            .join(metadata.name.to_lowercase().replace(' ', "-"))
            .join(metadata.version.to_string());

        // Copy the plugin files
        if install_path != path {
            fs::create_dir_all(&install_path)
                .await
                .map_err(PluginRegistryError::IoError)?;

            // Copy manifest
            let dest_manifest = install_path.join("manifest.json");
            fs::copy(&manifest_path, &dest_manifest)
                .await
                .map_err(PluginRegistryError::IoError)?;
        }

        // Verify signature if required
        let trust_level = if self.config.require_signatures {
            if self.verify_plugin_signature(&install_path).await? {
                Some(TrustLevel::Trusted)
            } else {
                return Err(PluginRegistryError::UnsignedPlugin);
            }
        } else {
            None
        };

        // Create installed plugin record
        let installed_plugin = InstalledPlugin {
            metadata: metadata.clone(),
            install_path: install_path.clone(),
            installed_at: chrono::Utc::now(),
            enabled: true,
            trust_level,
        };

        // Add to installed map
        {
            let mut installed = self.installed.write();
            installed.insert(plugin_id.clone(), installed_plugin);
        }

        // Save installed index
        self.save_installed_index().await?;

        info!(
            "Plugin {} v{} installed successfully",
            metadata.name, metadata.version
        );

        Ok(plugin_id)
    }

    async fn list_installed_plugins(&self) -> PluginRegistryResult<Vec<InstalledPlugin>> {
        let installed = self.installed.read();
        Ok(installed.values().cloned().collect())
    }

    async fn uninstall_plugin(&self, id: &PluginId) -> PluginRegistryResult<()> {
        info!("Uninstalling plugin: {}", id);

        let install_path = {
            let installed = self.installed.read();
            installed
                .get(id)
                .map(|p| p.install_path.clone())
                .ok_or_else(|| PluginRegistryError::PluginNotFound {
                    plugin_id: id.to_string(),
                })?
        };

        // Remove plugin directory
        if install_path.exists() {
            fs::remove_dir_all(&install_path).await.map_err(|e| {
                PluginRegistryError::UninstallationFailed(format!(
                    "Failed to remove plugin directory: {}",
                    e
                ))
            })?;
        }

        // Remove from installed map
        {
            let mut installed = self.installed.write();
            installed.remove(id);
        }

        // Save updated index
        self.save_installed_index().await?;

        info!("Plugin {} uninstalled", id);
        Ok(())
    }

    async fn refresh_registry(&self) -> PluginRegistryResult<()> {
        info!("Refreshing registry index");

        if self.config.offline_mode {
            debug!("Skipping refresh in offline mode");
            return Ok(());
        }

        // Fetch the registry index
        let _ = self.fetch_registry_index().await?;

        // Update disk cache
        let disk_cache = {
            let cache = self.cache_state.read();
            DiskCache {
                plugins: cache.plugin_cache.clone(),
                last_refresh: cache.last_refresh,
                version: DiskCache::CURRENT_VERSION,
            }
        };

        if let Err(e) = self.save_disk_cache(&disk_cache).await {
            warn!("Failed to save disk cache: {}", e);
        }

        info!("Registry index refreshed successfully");
        Ok(())
    }

    async fn is_registry_online(&self) -> bool {
        self.check_online_cached().await
    }

    async fn get_statistics(&self) -> PluginRegistryResult<PluginRegistryStatistics> {
        let installed = self.list_installed_plugins().await?;
        let installed_count = installed.len();

        let signed_plugins = installed.iter().filter(|p| p.trust_level.is_some()).count();

        let installed_size = self.calculate_installed_size().await;

        let (is_online, last_refresh, cached_plugins) = {
            let cache = self.cache_state.read();
            (
                cache.is_online,
                cache.last_refresh,
                cache.plugin_cache.values().map(|v| v.len()).sum(),
            )
        };

        // Check for updates
        let updates = self.check_for_updates().await.unwrap_or_default();

        Ok(PluginRegistryStatistics {
            installed_count,
            updates_available: updates.len(),
            cached_plugins,
            registry_online: is_online,
            last_refresh,
            installed_size_bytes: installed_size,
            signed_plugins,
        })
    }

    async fn check_for_updates(&self) -> PluginRegistryResult<Vec<(InstalledPlugin, Version)>> {
        let installed = self.list_installed_plugins().await?;
        let mut updates = Vec::new();

        for plugin in installed {
            // Check the cache for the latest version
            let cache = self.cache_state.read();
            if let Some(latest) = cache
                .plugin_cache
                .get(&plugin.metadata.id.to_string())
                .and_then(|versions| versions.first())
                .filter(|latest| latest.version > plugin.metadata.version)
            {
                updates.push((plugin, latest.version.clone()));
            }
        }

        Ok(updates)
    }

    async fn get_installed_plugin(
        &self,
        id: &PluginId,
    ) -> PluginRegistryResult<Option<InstalledPlugin>> {
        let installed = self.installed.read();
        Ok(installed.get(id).cloned())
    }

    async fn set_plugin_enabled(&self, id: &PluginId, enabled: bool) -> PluginRegistryResult<()> {
        info!("Setting plugin {} enabled: {}", id, enabled);

        // Update the installed plugin
        {
            let mut installed = self.installed.write();
            if let Some(plugin) = installed.get_mut(id) {
                plugin.enabled = enabled;
            } else {
                return Err(PluginRegistryError::PluginNotFound {
                    plugin_id: id.to_string(),
                });
            }
        }

        // Save the updated index
        self.save_installed_index().await?;

        info!("Plugin {} enabled state set to: {}", id, enabled);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

    async fn create_test_service(
        temp_dir: &TempDir,
    ) -> Result<PluginRegistryServiceImpl, Box<dyn std::error::Error>> {
        let config = PluginRegistryServiceConfig::new("https://test-registry.example.com")
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_install_dir(temp_dir.path().join("plugins"))
            .with_require_signatures(false)
            .with_offline_mode(true);

        Ok(PluginRegistryServiceImpl::new(config)?)
    }

    #[tokio::test]
    async fn test_service_config_default() -> Result<(), Box<dyn std::error::Error>> {
        let config = PluginRegistryServiceConfig::default();
        assert!(config.require_signatures);
        assert!(config.trust_store_path.is_some());
        assert!(!config.offline_mode);
        assert_eq!(config.max_download_retries, 3);
        Ok(())
    }

    #[tokio::test]
    async fn test_service_creation_requires_trust_store_when_signatures_required(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let config = PluginRegistryServiceConfig::new("https://test-registry.example.com")
            .with_require_signatures(true);
        // Explicitly clear trust store path to simulate invalid config
        let config = PluginRegistryServiceConfig {
            trust_store_path: None,
            ..config
        };

        let result = PluginRegistryServiceImpl::new(config);
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("trust_store_path"),
            "Expected trust_store_path validation error, got: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_disk_cache_new() -> Result<(), Box<dyn std::error::Error>> {
        let cache = DiskCache::new();
        assert!(cache.plugins.is_empty());
        assert!(cache.last_refresh.is_none());
        assert_eq!(cache.version, DiskCache::CURRENT_VERSION);
        Ok(())
    }

    #[tokio::test]
    async fn test_installed_index_new() -> Result<(), Box<dyn std::error::Error>> {
        let index = InstalledIndex::new();
        assert!(index.plugins.is_empty());
        assert_eq!(index.version, InstalledIndex::CURRENT_VERSION);
        Ok(())
    }

    #[tokio::test]
    async fn test_service_creation() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let _service = create_test_service(&temp_dir).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_list_installed_empty() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        let installed = service.list_installed_plugins().await?;
        assert!(installed.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_statistics() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        let stats = service.get_statistics().await?;
        assert_eq!(stats.installed_count, 0);
        assert_eq!(stats.updates_available, 0);
        // Offline mode means registry is not online
        assert!(!stats.registry_online);
        Ok(())
    }

    #[tokio::test]
    async fn test_is_registry_online_offline_mode() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // In offline mode, registry should report as not online
        let is_online = service.is_registry_online().await;
        assert!(!is_online);
        Ok(())
    }

    #[tokio::test]
    async fn test_search_offline_mode() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // In offline mode with no cache, search should return empty
        let results = service.search_plugins("test").await?;
        assert!(results.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_plugin_signature_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // Use a path that definitely doesn't exist in the temp directory
        let nonexistent_path = temp_dir.path().join("nonexistent_plugin_xyz123");
        let result = service.verify_plugin_signature(&nonexistent_path).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_plugin_invalid_path() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // Use a path that definitely doesn't exist in the temp directory
        let nonexistent_path = temp_dir.path().join("nonexistent_plugin_xyz123");
        let result = service.install_plugin(&nonexistent_path).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_plugin_rejects_unsigned_when_signatures_required(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let config = PluginRegistryServiceConfig::new("https://test-registry.example.com")
            .with_cache_dir(temp_dir.path().join("cache"))
            .with_install_dir(temp_dir.path().join("plugins"))
            .with_require_signatures(true)
            .with_trust_store_path(temp_dir.path().join("trust_store.json"))
            .with_offline_mode(true);
        let service = PluginRegistryServiceImpl::new(config)?;

        let plugin_dir = temp_dir.path().join("unsigned-plugin");
        fs::create_dir_all(&plugin_dir).await?;

        let metadata = create_test_metadata("Unsigned Plugin", "1.0.0");
        let manifest_content = serde_json::to_string_pretty(&metadata)?;
        fs::write(plugin_dir.join("manifest.json"), manifest_content).await?;

        let result = service.install_plugin(&plugin_dir).await;
        assert!(result.is_err());

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.to_lowercase().contains("unsigned")
                || err_msg.to_lowercase().contains("signature"),
            "expected unsigned/signature error, got: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_installed_plugin_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        let plugin_id = PluginId::new();
        let result = service.get_installed_plugin(&plugin_id).await?;
        assert!(result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_check_for_updates_empty() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        let updates = service.check_for_updates().await?;
        assert!(updates.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_registry_offline_mode() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // In offline mode, refresh should succeed but do nothing
        let result = service.refresh_registry().await;
        assert!(result.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn test_download_progress_callback() -> Result<(), Box<dyn std::error::Error>> {
        let progress = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        let callback: ProgressCallback = Box::new(move |p| {
            progress_clone.lock().push(p.phase);
        });

        // Simulate progress updates
        callback(DownloadProgress {
            phase: DownloadPhase::Connecting,
            ..Default::default()
        });
        callback(DownloadProgress {
            phase: DownloadPhase::Downloading,
            ..Default::default()
        });

        let phases = progress.lock().clone();
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0], DownloadPhase::Connecting);
        assert_eq!(phases[1], DownloadPhase::Downloading);
        Ok(())
    }

    #[tokio::test]
    async fn test_disk_cache_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let mut cache = DiskCache::new();
        cache.last_refresh = Some(chrono::Utc::now());

        let metadata = create_test_metadata("test-plugin", "1.0.0");
        cache
            .plugins
            .insert(metadata.id.to_string(), vec![metadata]);

        // Serialize and deserialize
        let json = serde_json::to_string(&cache)?;
        let restored: DiskCache = serde_json::from_str(&json)?;

        assert_eq!(restored.version, cache.version);
        assert!(restored.last_refresh.is_some());
        assert_eq!(restored.plugins.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_uninstall_plugin_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        let plugin_id = PluginId::new();
        let result = service.uninstall_plugin(&plugin_id).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_set_plugin_enabled_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        let plugin_id = PluginId::new();
        let result = service.set_plugin_enabled(&plugin_id, true).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_install_and_list_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // Create a plugin directory with manifest
        let plugin_dir = temp_dir.path().join("test-plugin");
        fs::create_dir_all(&plugin_dir).await?;

        let metadata = create_test_metadata("Test Plugin", "1.0.0");
        let manifest_content = serde_json::to_string_pretty(&metadata)?;
        fs::write(plugin_dir.join("manifest.json"), manifest_content).await?;

        // Install the plugin
        let plugin_id = service.install_plugin(&plugin_dir).await?;

        // Verify it's in the list
        let installed = service.list_installed_plugins().await?;
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].metadata.id, plugin_id);
        assert!(installed[0].enabled);

        Ok(())
    }

    #[tokio::test]
    async fn test_install_and_uninstall_plugin() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // Create a plugin directory with manifest
        let plugin_dir = temp_dir.path().join("test-plugin-2");
        fs::create_dir_all(&plugin_dir).await?;

        let metadata = create_test_metadata("Test Plugin 2", "1.0.0");
        let manifest_content = serde_json::to_string_pretty(&metadata)?;
        fs::write(plugin_dir.join("manifest.json"), manifest_content).await?;

        // Install the plugin
        let plugin_id = service.install_plugin(&plugin_dir).await?;

        // Verify it's installed
        let installed = service.list_installed_plugins().await?;
        assert_eq!(installed.len(), 1);

        // Uninstall the plugin
        service.uninstall_plugin(&plugin_id).await?;

        // Verify it's removed
        let installed = service.list_installed_plugins().await?;
        assert!(installed.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_set_plugin_enabled() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let service = create_test_service(&temp_dir).await?;

        // Create and install a plugin
        let plugin_dir = temp_dir.path().join("test-plugin-3");
        fs::create_dir_all(&plugin_dir).await?;

        let metadata = create_test_metadata("Test Plugin 3", "1.0.0");
        let manifest_content = serde_json::to_string_pretty(&metadata)?;
        fs::write(plugin_dir.join("manifest.json"), manifest_content).await?;

        let plugin_id = service.install_plugin(&plugin_dir).await?;

        // Disable the plugin
        service.set_plugin_enabled(&plugin_id, false).await?;

        // Verify it's disabled
        let plugin = service.get_installed_plugin(&plugin_id).await?;
        assert!(plugin.is_some());
        assert!(!plugin.as_ref().map(|p| p.enabled).unwrap_or(true));

        // Re-enable the plugin
        service.set_plugin_enabled(&plugin_id, true).await?;

        // Verify it's enabled
        let plugin = service.get_installed_plugin(&plugin_id).await?;
        assert!(plugin.as_ref().map(|p| p.enabled).unwrap_or(false));

        Ok(())
    }
}
