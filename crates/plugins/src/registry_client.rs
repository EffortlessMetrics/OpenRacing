//! Remote registry client for plugin discovery and management
//!
//! This module provides a client for interacting with remote plugin registries,
//! supporting index caching, signature verification, and configurable refresh intervals.
//!
//! # Registry Index Signature Format
//!
//! The registry index supports Ed25519 digital signatures for supply chain security.
//!
//! ## What is Signed
//!
//! The signature is computed over the **32-byte SHA256 digest** of the canonical
//! catalog JSON serialization. This is:
//!
//! ```text
//! signature = Ed25519.sign(SHA256(canonical_catalog_json))
//! ```
//!
//! **Important:** The signature is over the raw digest bytes (32 bytes), NOT the
//! hex-encoded digest string (64 characters), and NOT the raw JSON bytes.
//!
//! ## Canonical Serialization
//!
//! The catalog is serialized deterministically using `BTreeMap` to ensure consistent
//! key ordering. This makes the hash reproducible regardless of the internal `HashMap`
//! iteration order.
//!
//! The canonical structure is:
//! ```json
//! {
//!   "plugin-id-1": [plugin_metadata_v1, plugin_metadata_v2, ...],
//!   "plugin-id-2": [...],
//!   ...
//! }
//! ```
//!
//! Where:
//! - Plugin IDs are sorted lexicographically
//! - Each plugin's versions are sorted by semver (ascending)
//!
//! ## Signature Metadata
//!
//! The [`IndexSignature`] struct embeds the full signature metadata in the index:
//! - `signature`: Base64-encoded Ed25519 signature (64 bytes)
//! - `key_fingerprint`: SHA256 of the public key (64-char hex string)
//! - `signer`: Human-readable signer identity
//! - `timestamp`: When the signature was created (ISO 8601)
//! - `comment`: Optional release notes
//!
//! **Security:** The timestamp and signer identity are bound to the signature.
//! The client MUST NOT fabricate these values - they come from the index.
//!
//! ## Verification Requirements
//!
//! When `require_signed_index` is enabled:
//! 1. A `trust_store_path` MUST be configured (checked at client creation)
//! 2. The signature MUST be cryptographically valid
//! 3. The signer MUST be in the trust store with `TrustLevel::Trusted`
//!    (unless `allow_untrusted_signers` is enabled for dev environments)

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use parking_lot::RwLock;
use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs;
use tracing::{debug, info, warn};

use crate::registry::{PluginCatalog, PluginId, PluginMetadata};

/// Signature metadata for a registry index
///
/// This struct is embedded in the registry index JSON and contains all information
/// needed to verify the index signature. The signature is over the SHA256 digest
/// of the canonical catalog JSON serialization.
///
/// SECURITY NOTE: The timestamp and signer identity MUST come from this struct,
/// not be fabricated by the client. This ensures signature freshness can be enforced
/// and the signer's claimed identity is bound to the signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSignature {
    /// Ed25519 signature in base64 format over the catalog hash
    pub signature: String,

    /// SHA256 fingerprint of the signing public key (64-char hex string)
    pub key_fingerprint: String,

    /// Human-readable signer identity
    pub signer: String,

    /// Timestamp when the signature was created (ISO 8601 format)
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Optional comment describing the signature/release
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// Configuration for the remote registry client
#[derive(Debug, Clone)]
pub struct RemoteRegistryConfig {
    /// Base URL of the remote registry
    pub registry_url: String,
    /// Directory for caching the registry index
    pub cache_dir: PathBuf,
    /// How often to refresh the registry index
    pub refresh_interval: Duration,
    /// Whether to require signature verification for the index
    ///
    /// SECURITY: When true, `trust_store_path` MUST also be configured.
    /// Signature verification without a trust anchor provides no security guarantee.
    pub require_signed_index: bool,
    /// Path to trust store for signature verification
    ///
    /// REQUIRED when `require_signed_index` is true. The trust store contains
    /// public keys of trusted registry signers.
    pub trust_store_path: Option<PathBuf>,
    /// Expected key fingerprint for registry signing key (optional)
    /// If set, only signatures from this key will be accepted
    pub registry_key_fingerprint: Option<String>,
    /// Whether to allow unknown signers (signers not in trust store)
    ///
    /// WARNING: Setting this to true significantly weakens security.
    /// Only use for development/testing environments.
    /// Default: false (require trusted signers)
    pub allow_untrusted_signers: bool,
}

impl Default for RemoteRegistryConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://registry.openracing.io".to_string(),
            cache_dir: PathBuf::from(".cache/plugins"),
            refresh_interval: Duration::from_secs(3600), // 1 hour
            // NOTE: Default is false since trust_store_path is None.
            // In production, users should configure both require_signed_index: true
            // AND a valid trust_store_path.
            require_signed_index: false,
            trust_store_path: None,
            registry_key_fingerprint: None,
            allow_untrusted_signers: false,
        }
    }
}

impl RemoteRegistryConfig {
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

    /// Set the refresh interval
    pub fn with_refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = interval;
        self
    }

    /// Set whether to require signed index
    pub fn with_require_signed_index(mut self, require: bool) -> Self {
        self.require_signed_index = require;
        self
    }

    /// Set the trust store path for signature verification
    pub fn with_trust_store_path(mut self, path: PathBuf) -> Self {
        self.trust_store_path = Some(path);
        self
    }

    /// Set the expected registry signing key fingerprint
    pub fn with_registry_key_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.registry_key_fingerprint = Some(fingerprint.into());
        self
    }

    /// Set whether to allow untrusted signers (signers not in trust store)
    ///
    /// WARNING: Setting this to true significantly weakens security.
    /// Only use for development/testing environments.
    pub fn with_allow_untrusted_signers(mut self, allow: bool) -> Self {
        self.allow_untrusted_signers = allow;
        self
    }
}

/// Cached registry index with metadata
///
/// The index contains a plugin catalog along with integrity information.
///
/// ## Signature Verification
///
/// When `signature` is present, it contains full metadata about the signature
/// including the signer's identity, timestamp, and key fingerprint. The signature
/// is computed over the 32-byte SHA256 digest of the canonical catalog JSON
/// (NOT the hex-encoded hash string, and NOT the raw JSON bytes).
///
/// The canonical catalog JSON uses `BTreeMap` to ensure deterministic key ordering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    /// Version of the index format
    pub version: String,
    /// Timestamp when the index was generated (Unix epoch seconds)
    pub generated_at: u64,
    /// Optional cryptographic signature of the index
    ///
    /// When present, contains full signature metadata including the Ed25519
    /// signature, signer identity, timestamp, and key fingerprint.
    pub signature: Option<IndexSignature>,
    /// SHA256 hash of the catalog data (hex-encoded)
    ///
    /// This hash is computed from the canonical JSON serialization of the catalog
    /// (using BTreeMap for deterministic key ordering).
    pub catalog_hash: String,
    /// The plugin catalog containing all plugin metadata
    pub catalog: PluginCatalog,
}

impl RegistryIndex {
    /// Create a new registry index
    ///
    /// The catalog hash is computed from the canonical JSON serialization of the catalog.
    pub fn new(catalog: PluginCatalog) -> Self {
        let catalog_hash = Self::compute_canonical_catalog_hash(&catalog);

        Self {
            version: "1.0.0".to_string(),
            generated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            signature: None,
            catalog_hash,
            catalog,
        }
    }

    /// Compute the SHA256 hash of a catalog using canonical (deterministic) serialization
    ///
    /// This function produces a deterministic hash of the catalog by:
    /// 1. Converting the internal `HashMap` to a `BTreeMap` (sorted keys)
    /// 2. Sorting each plugin's versions by semver (ascending)
    /// 3. Serializing to compact JSON (no pretty-printing)
    /// 4. Computing SHA256 of the JSON bytes
    ///
    /// ## Canonical Format
    ///
    /// The canonical JSON structure is:
    /// ```json
    /// {"plugin-id-1":[{...v1...},{...v2...}],"plugin-id-2":[...]}
    /// ```
    ///
    /// ## Signature Usage
    ///
    /// Registry signatures are computed over the **raw SHA256 digest bytes** (32 bytes),
    /// NOT the hex-encoded string returned by this function. To verify:
    ///
    /// ```ignore
    /// let hash_hex = RegistryIndex::compute_canonical_catalog_hash(&catalog);
    /// let hash_bytes = hex::decode(&hash_hex)?; // 32 bytes
    /// verifier.verify_content(&hash_bytes, &signature_metadata)?;
    /// ```
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn compute_canonical_catalog_hash(catalog: &PluginCatalog) -> String {
        // Build a canonical representation with sorted keys
        // Structure: { plugin_id_string: [sorted_versions] }
        let mut canonical: BTreeMap<String, Vec<&PluginMetadata>> = BTreeMap::new();

        for plugin in catalog.list_all() {
            // Get all versions of this plugin
            if let Some(versions) = catalog.get_all_versions(&plugin.id) {
                // Sort versions by semver (ascending) for determinism
                let mut sorted_versions: Vec<&PluginMetadata> = versions.iter().collect();
                sorted_versions.sort_by(|a, b| a.version.cmp(&b.version));
                canonical.insert(plugin.id.0.to_string(), sorted_versions);
            }
        }

        // Serialize to compact JSON (BTreeMap ensures key ordering)
        // Note: Uses compact format, not pretty-printed, for consistency
        let canonical_json = serde_json::to_string(&canonical).unwrap_or_default();

        let mut hasher = Sha256::new();
        hasher.update(canonical_json.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify the catalog hash matches the stored hash
    ///
    /// Note: This verifies that the catalog data has not been tampered with
    /// since the index was created. The hash is computed from the canonical JSON
    /// serialization of the catalog (using sorted structures for determinism).
    ///
    /// SECURITY: Empty hashes are rejected - they are never valid.
    /// This prevents bypassing verification by providing an empty hash.
    pub fn verify_hash(&self) -> bool {
        // SECURITY: Empty hash is NEVER valid - reject it
        // This prevents attackers from bypassing hash verification
        if self.catalog_hash.is_empty() {
            warn!("Registry index has empty catalog hash - rejecting as invalid");
            return false;
        }

        let computed_hash = Self::compute_canonical_catalog_hash(&self.catalog);
        computed_hash == self.catalog_hash
    }

    /// Recompute and update the catalog hash using canonical serialization
    ///
    /// This should be called after modifying the catalog to ensure the hash
    /// reflects the current state.
    pub fn update_hash(&mut self) {
        self.catalog_hash = Self::compute_canonical_catalog_hash(&self.catalog);
    }
}

/// Plugin registry trait for searching and retrieving plugins
#[async_trait]
pub trait PluginRegistry: Send + Sync {
    /// Search for plugins matching a query string
    ///
    /// The query is matched against plugin names and descriptions (case-insensitive).
    async fn search(&self, query: &str) -> Result<Vec<PluginMetadata>>;

    /// Get a specific plugin by ID and optional version
    ///
    /// If version is None, returns the latest version.
    async fn get_plugin(
        &self,
        id: &PluginId,
        version: Option<&Version>,
    ) -> Result<Option<PluginMetadata>>;

    /// Refresh the registry index from the remote source
    async fn refresh(&self) -> Result<()>;
}

/// Internal state for the remote registry client
#[derive(Default)]
struct RegistryState {
    /// The cached registry index
    index: Option<RegistryIndex>,
    /// When the index was last refreshed
    last_refresh: Option<Instant>,
}

/// Remote registry client implementation
///
/// This client fetches and caches a registry index from a remote server,
/// supporting signature verification and configurable refresh intervals.
pub struct RemoteRegistryClient {
    /// Client configuration
    config: RemoteRegistryConfig,
    /// HTTP client for fetching the registry
    client: Client,
    /// Cached registry state
    state: Arc<RwLock<RegistryState>>,
    /// Optional verification service for cryptographic signature verification
    verifier: Option<racing_wheel_service::crypto::verification::VerificationService>,
}

impl RemoteRegistryClient {
    /// Create a new remote registry client with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `require_signed_index` is true but no `trust_store_path` is configured
    ///   (signatures cannot be cryptographically verified without a trust anchor)
    /// - The HTTP client fails to initialize
    /// - The verification service fails to initialize
    pub fn new(config: RemoteRegistryConfig) -> Result<Self> {
        // SECURITY: If signatures are required, we MUST have a trust store configured.
        // Without a trust store, we can only do format validation (signature looks like
        // a valid Ed25519 signature), which provides no security guarantee.
        if config.require_signed_index && config.trust_store_path.is_none() {
            bail!(
                "Configuration error: require_signed_index is true but no trust_store_path is configured. \
                 Signature verification requires a trust anchor. Either:\n\
                 - Configure trust_store_path with a valid trust store, or\n\
                 - Set require_signed_index to false (not recommended for production)"
            );
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("openracing-plugins/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to create HTTP client")?;

        // Initialize verifier if trust store path is provided
        let verifier = if let Some(ref trust_store_path) = config.trust_store_path {
            let verification_config = racing_wheel_service::crypto::VerificationConfig {
                trust_store_path: trust_store_path.clone(),
                require_firmware_signatures: false,
                require_binary_signatures: false,
                require_plugin_signatures: config.require_signed_index,
                allow_unknown_signers: config.allow_untrusted_signers,
                max_signature_age_seconds: None,
            };
            Some(
                racing_wheel_service::crypto::verification::VerificationService::new(
                    verification_config,
                )
                .context("Failed to initialize verification service")?,
            )
        } else {
            None
        };

        Ok(Self {
            config,
            client,
            state: Arc::new(RwLock::new(RegistryState::default())),
            verifier,
        })
    }

    /// Create a new remote registry client with default configuration
    pub fn with_defaults() -> Result<Self> {
        Self::new(RemoteRegistryConfig::default())
    }

    /// Get the current configuration
    pub fn config(&self) -> &RemoteRegistryConfig {
        &self.config
    }

    /// Check if the cached index needs to be refreshed
    fn needs_refresh(&self) -> bool {
        let state = self.state.read();
        match state.last_refresh {
            Some(last) => last.elapsed() >= self.config.refresh_interval,
            None => true,
        }
    }

    /// Load the cached index from disk
    async fn load_cached_index(&self) -> Result<Option<RegistryIndex>> {
        let cache_path = self.config.cache_dir.join("registry_index.json");

        if !cache_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&cache_path)
            .await
            .context("Failed to read cached index")?;

        let mut index: RegistryIndex =
            serde_json::from_str(&content).context("Failed to parse cached index")?;

        // Rebuild the name index after deserialization
        index.catalog.rebuild_index();

        // Verify hash integrity
        if !index.verify_hash() {
            warn!("Cached index hash verification failed, will refresh from remote");
            return Ok(None);
        }

        // Verify signature integrity when configured.
        if self.config.require_signed_index
            && let Err(e) = self.verify_index_signature(&index)
        {
            warn!("Cached index signature verification failed, will refresh from remote: {e}");
            return Ok(None);
        }

        Ok(Some(index))
    }

    /// Save the index to the cache directory
    async fn save_cached_index(&self, index: &RegistryIndex) -> Result<()> {
        // Ensure cache directory exists
        fs::create_dir_all(&self.config.cache_dir)
            .await
            .context("Failed to create cache directory")?;

        let cache_path = self.config.cache_dir.join("registry_index.json");
        let content = serde_json::to_string_pretty(index).context("Failed to serialize index")?;

        fs::write(&cache_path, content)
            .await
            .context("Failed to write cached index")?;

        debug!("Saved registry index to cache: {:?}", cache_path);
        Ok(())
    }

    /// Fetch the registry index from the remote server
    async fn fetch_remote_index(&self) -> Result<RegistryIndex> {
        let url = format!("{}/v1/index.json", self.config.registry_url);

        info!("Fetching registry index from: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch registry index")?;

        if !response.status().is_success() {
            bail!(
                "Registry returned error status: {} - {}",
                response.status(),
                response.status().canonical_reason().unwrap_or("Unknown")
            );
        }

        let mut index: RegistryIndex = response
            .json()
            .await
            .context("Failed to parse registry index")?;

        // Rebuild the name index after deserialization
        index.catalog.rebuild_index();

        // Verify signature if required
        if self.config.require_signed_index {
            self.verify_index_signature(&index)?;
        }

        // Verify hash integrity
        if !index.verify_hash() {
            bail!("Registry index hash verification failed");
        }

        Ok(index)
    }

    /// Verify the Ed25519 signature of the registry index
    ///
    /// This performs cryptographic verification of the signature against the catalog hash.
    /// The signature metadata (signer, timestamp, key fingerprint) comes from the index itself,
    /// NOT fabricated by the client. This ensures the signature is bound to its claimed metadata.
    ///
    /// ## What is signed
    ///
    /// The signature is computed over the 32-byte SHA256 digest of the canonical catalog JSON.
    /// This is the raw bytes of the digest, NOT the hex-encoded string.
    fn verify_index_signature(&self, index: &RegistryIndex) -> Result<()> {
        use racing_wheel_service::crypto::{ContentType, SignatureMetadata, TrustLevel};

        let index_sig = index
            .signature
            .as_ref()
            .ok_or_else(|| anyhow!("Registry index is not signed but signature is required"))?;

        // Decode the signature to verify it's valid base64
        let signature_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &index_sig.signature,
        )
        .context("Failed to decode registry index signature")?;

        // Verify signature length (Ed25519 signatures are 64 bytes)
        if signature_bytes.len() != 64 {
            bail!(
                "Invalid signature length: expected 64 bytes, got {}",
                signature_bytes.len()
            );
        }

        // If a specific key fingerprint is required, verify it matches
        if let Some(ref expected_fingerprint) = self.config.registry_key_fingerprint
            && index_sig.key_fingerprint != *expected_fingerprint
        {
            bail!(
                "Registry key fingerprint mismatch: expected '{}', got '{}'. \
                 The registry is signed by a different key than expected.",
                expected_fingerprint,
                index_sig.key_fingerprint
            );
        }

        // If we have a verifier, perform full cryptographic verification
        if let Some(ref verifier) = self.verifier {
            // Get the hash bytes that were signed
            // IMPORTANT: We sign the raw SHA256 digest bytes, NOT the hex string
            let hash_bytes =
                hex::decode(&index.catalog_hash).context("Failed to decode catalog hash")?;

            // Convert IndexSignature to SignatureMetadata for the verification service
            // SECURITY: We use the metadata from the index, NOT fabricated values.
            // This binds the signature to its claimed timestamp and signer identity.
            let metadata = SignatureMetadata {
                signature: index_sig.signature.clone(),
                key_fingerprint: index_sig.key_fingerprint.clone(),
                signer: index_sig.signer.clone(),
                timestamp: index_sig.timestamp,
                content_type: ContentType::Update, // Registry index is like an update manifest
                comment: index_sig.comment.clone(),
            };

            // Verify the signature cryptographically
            let verification_result = verifier
                .verify_content(&hash_bytes, &metadata)
                .context("Failed to verify registry index signature")?;

            if !verification_result.signature_valid {
                bail!("Registry index signature is cryptographically invalid");
            }

            // Check trust level
            // SECURITY: By default, require Trusted signers when signature verification is enabled.
            // Unknown signers are only allowed if explicitly configured via allow_untrusted_signers.
            match verification_result.trust_level {
                TrustLevel::Trusted => {
                    info!("Registry index signature verified: trusted signer");
                }
                TrustLevel::Unknown => {
                    if self.config.allow_untrusted_signers {
                        warn!(
                            "Registry index signed by UNKNOWN key (fingerprint: {}). \
                             This is allowed due to allow_untrusted_signers=true, but is NOT SECURE. \
                             Add the key to your trust store for production use.",
                            metadata.key_fingerprint
                        );
                    } else {
                        bail!(
                            "Registry index signed by unknown key (fingerprint: {}). \
                             The signer is not in the trust store. Either:\n\
                             - Add the registry's public key to your trust store, or\n\
                             - Set allow_untrusted_signers=true (not recommended for production)",
                            metadata.key_fingerprint
                        );
                    }
                }
                TrustLevel::Distrusted => {
                    bail!(
                        "Registry index signed by distrusted key: {}",
                        metadata.key_fingerprint
                    );
                }
            }

            debug!("Registry index signature cryptographically verified");
        } else {
            // No verifier configured - this should not happen if require_signed_index is true
            // due to the check in RemoteRegistryClient::new()
            debug!("Registry index signature format verified (no trust store configured)");
        }

        Ok(())
    }

    /// Ensure the index is loaded and up to date
    async fn ensure_index(&self) -> Result<()> {
        // Check if we need to refresh
        if !self.needs_refresh() {
            return Ok(());
        }

        // Try to fetch from remote
        match self.fetch_remote_index().await {
            Ok(index) => {
                // Save to cache
                if let Err(e) = self.save_cached_index(&index).await {
                    warn!("Failed to save index to cache: {}", e);
                }

                // Update state
                let mut state = self.state.write();
                state.index = Some(index);
                state.last_refresh = Some(Instant::now());
                Ok(())
            }
            Err(e) => {
                warn!("Failed to fetch remote index: {}, trying cached", e);

                // Try to load from cache
                if let Some(index) = self.load_cached_index().await? {
                    let mut state = self.state.write();
                    state.index = Some(index);
                    // Don't update last_refresh so we try again next time
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}

#[async_trait]
impl PluginRegistry for RemoteRegistryClient {
    async fn search(&self, query: &str) -> Result<Vec<PluginMetadata>> {
        self.ensure_index().await?;

        let state = self.state.read();
        let index = state
            .index
            .as_ref()
            .ok_or_else(|| anyhow!("Registry index not loaded"))?;

        let results: Vec<PluginMetadata> =
            index.catalog.search(query).into_iter().cloned().collect();

        Ok(results)
    }

    async fn get_plugin(
        &self,
        id: &PluginId,
        version: Option<&Version>,
    ) -> Result<Option<PluginMetadata>> {
        self.ensure_index().await?;

        let state = self.state.read();
        let index = state
            .index
            .as_ref()
            .ok_or_else(|| anyhow!("Registry index not loaded"))?;

        let result = index.catalog.get_plugin(id, version).cloned();

        Ok(result)
    }

    async fn refresh(&self) -> Result<()> {
        let index = self.fetch_remote_index().await?;

        // Save to cache
        if let Err(e) = self.save_cached_index(&index).await {
            warn!("Failed to save index to cache: {}", e);
        }

        // Update state
        let mut state = self.state.write();
        state.index = Some(index);
        state.last_refresh = Some(Instant::now());

        info!("Registry index refreshed successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_metadata(name: &str, version: &str) -> PluginMetadata {
        let ver = semver::Version::parse(version).unwrap_or_else(|_| semver::Version::new(1, 0, 0));
        PluginMetadata::new(
            name,
            ver,
            "Test Author",
            format!("Description for {}", name),
            "MIT",
        )
    }

    /// Create a test index for use with HTTP mocking.
    ///
    /// Uses canonical hash computation which is deterministic, so hash verification
    /// should work correctly after JSON round-trips.
    fn create_test_index() -> RegistryIndex {
        let mut catalog = PluginCatalog::new();
        let p1 = create_test_metadata("FFB Filter Pro", "1.2.0");
        let p2 = create_test_metadata("LED Controller", "2.0.0");
        let p3 = create_test_metadata("Telemetry Logger", "1.0.0");

        // Ignore errors in test setup - these should always succeed with valid metadata
        let _ = catalog.add_plugin(p1);
        let _ = catalog.add_plugin(p2);
        let _ = catalog.add_plugin(p3);

        // Create index with canonical hash
        RegistryIndex::new(catalog)
    }

    /// Create a test index with an empty hash (for testing hash validation rejection)
    fn create_test_index_with_empty_hash() -> RegistryIndex {
        let mut index = create_test_index();
        index.catalog_hash = String::new();
        index
    }

    #[tokio::test]
    async fn test_registry_config_default() -> Result<()> {
        let config = RemoteRegistryConfig::default();
        assert_eq!(config.registry_url, "https://registry.openracing.io");
        assert_eq!(config.refresh_interval, Duration::from_secs(3600));
        // Default is false when no trust store is configured (safe default)
        assert!(!config.require_signed_index);
        assert!(!config.allow_untrusted_signers);
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_config_builder() -> Result<()> {
        let config = RemoteRegistryConfig::new("https://custom.registry.io")
            .with_cache_dir(PathBuf::from("/tmp/cache"))
            .with_refresh_interval(Duration::from_secs(1800))
            .with_require_signed_index(false);

        assert_eq!(config.registry_url, "https://custom.registry.io");
        assert_eq!(config.cache_dir, PathBuf::from("/tmp/cache"));
        assert_eq!(config.refresh_interval, Duration::from_secs(1800));
        assert!(!config.require_signed_index);
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_index_hash_verification() -> Result<()> {
        let index = create_test_index();
        assert!(
            index.verify_hash(),
            "Index hash should be valid with canonical hash"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_index_hash_fails_on_tampering() -> Result<()> {
        let mut index = create_test_index();
        // Tamper with the hash
        index.catalog_hash = "invalid_hash".to_string();
        assert!(
            !index.verify_hash(),
            "Index hash should be invalid after tampering"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_index_empty_hash_fails() -> Result<()> {
        // SECURITY: Empty hashes must be rejected
        let index = create_test_index_with_empty_hash();
        assert!(
            !index.verify_hash(),
            "Empty hash should be rejected for security"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_index_hash_deterministic_on_same_catalog() -> Result<()> {
        // Create one index and compute its hash multiple times
        let index = create_test_index();

        // Hash should be consistent across multiple computations
        let hash1 = RegistryIndex::compute_canonical_catalog_hash(&index.catalog);
        let hash2 = RegistryIndex::compute_canonical_catalog_hash(&index.catalog);

        assert_eq!(
            hash1, hash2,
            "Canonical hash should be deterministic for the same catalog"
        );

        // Also verify the stored hash matches
        assert_eq!(
            index.catalog_hash, hash1,
            "Stored hash should match computed hash"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_registry_index_hash_survives_roundtrip() -> Result<()> {
        // Create an index and serialize/deserialize it
        let original_index = create_test_index();
        let original_hash = original_index.catalog_hash.clone();

        // Serialize to JSON
        let json = serde_json::to_string(&original_index)?;

        // Deserialize back
        let mut restored_index: RegistryIndex = serde_json::from_str(&json)?;
        restored_index.catalog.rebuild_index();

        // The hash should still verify after round-trip
        assert!(
            restored_index.verify_hash(),
            "Hash should verify after JSON round-trip"
        );

        // And the stored hash should still match
        assert_eq!(
            restored_index.catalog_hash, original_hash,
            "Hash should be preserved through serialization"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_remote_index_success_unsigned() -> Result<()> {
        // Test fetching an unsigned index when signatures are not required
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        // Search should work after refresh
        let results = client.search("FFB").await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "FFB Filter Pro");

        Ok(())
    }

    #[tokio::test]
    async fn test_require_signed_index_without_trust_store_errors() -> Result<()> {
        // SECURITY: Requiring signatures without a trust store should fail at config time
        let temp_dir = TempDir::new()?;

        let config = RemoteRegistryConfig::new("https://example.com")
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(true);
        // Note: trust_store_path is None

        let result = RemoteRegistryClient::new(config);
        assert!(
            result.is_err(),
            "Should fail when require_signed_index is true but no trust_store_path"
        );

        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("trust_store_path") || err_msg.contains("trust anchor"),
            "Error should mention trust store requirement: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_remote_index_without_signature_required() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        let results = client.search("LED").await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "LED Controller");

        Ok(())
    }

    #[tokio::test]
    async fn test_search_empty_query() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        // Empty string matches all plugins (as per catalog.search behavior)
        let results = client.search("").await?;
        assert_eq!(results.len(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_search_no_matches() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        let results = client.search("nonexistent_plugin_xyz").await?;
        assert!(results.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_search_case_insensitive() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        // Test different case variations
        let results_lower = client.search("ffb").await?;
        let results_upper = client.search("FFB").await?;
        let results_mixed = client.search("FfB").await?;

        assert_eq!(results_lower.len(), 1);
        assert_eq!(results_upper.len(), 1);
        assert_eq!(results_mixed.len(), 1);
        assert_eq!(results_lower[0].name, "FFB Filter Pro");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_plugin_by_id() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let mut catalog = PluginCatalog::new();
        let p1 = create_test_metadata("Test Plugin", "1.0.0");
        let plugin_id = p1.id.clone();
        let _ = catalog.add_plugin(p1);
        let index = RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        let result = client.get_plugin(&plugin_id, None).await?;
        assert!(result.is_some());
        assert_eq!(
            result.as_ref().map(|p| &p.name),
            Some(&"Test Plugin".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_plugin_specific_version() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let mut catalog = PluginCatalog::new();
        let v1 = create_test_metadata("Test Plugin", "1.0.0");
        let plugin_id = v1.id.clone();
        let _ = catalog.add_plugin(v1);

        let mut v2 = create_test_metadata("Test Plugin", "2.0.0");
        v2.id = plugin_id.clone();
        let _ = catalog.add_plugin(v2);

        let index = RegistryIndex::new(catalog);

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        // Get latest (should be 2.0.0)
        let latest = client.get_plugin(&plugin_id, None).await?;
        assert_eq!(
            latest.as_ref().map(|p| p.version.to_string()),
            Some("2.0.0".to_string())
        );

        // Get specific version 1.0.0
        let v1_0 = client
            .get_plugin(&plugin_id, Some(&Version::new(1, 0, 0)))
            .await?;
        assert_eq!(
            v1_0.as_ref().map(|p| p.version.to_string()),
            Some("1.0.0".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_nonexistent_plugin() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        client.refresh().await?;

        let nonexistent_id = PluginId::new();
        let result = client.get_plugin(&nonexistent_id, None).await?;
        assert!(result.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_persistence() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .expect(1) // Should only be called once
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false)
            .with_refresh_interval(Duration::from_secs(3600)); // Long interval

        // First client - fetches from remote
        {
            let client = RemoteRegistryClient::new(config.clone())?;
            client.refresh().await?;
            let results = client.search("FFB").await?;
            assert_eq!(results.len(), 1);
        }

        // Second client - should use cache
        {
            let client = RemoteRegistryClient::new(config)?;
            // Load from cache explicitly
            let cached = client.load_cached_index().await?;
            assert!(cached.is_some());

            let cached_index = cached.as_ref();
            assert!(cached_index.is_some());

            let results = cached_index
                .map(|idx| idx.catalog.search("FFB"))
                .unwrap_or_default();
            assert_eq!(results.len(), 1);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_remote_fetch_failure_uses_cache() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        // First mock - success
        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false)
            .with_refresh_interval(Duration::from_millis(1)); // Very short interval

        let client = RemoteRegistryClient::new(config)?;

        // First refresh - success
        client.refresh().await?;

        // Wait for refresh interval to expire
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Mount failure response
        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        // Should still work using cache
        let results = client.search("FFB").await?;
        assert_eq!(results.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_cached_index_rejected_when_signature_required_and_missing() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir)?;

        // Trust store is required for signed-index mode.
        let trust_store_path = temp_dir.path().join("trust_store.json");
        std::fs::write(&trust_store_path, "{}")?;

        // Write an unsigned index into cache.
        let index = create_test_index();
        let cache_path = cache_dir.join("registry_index.json");
        std::fs::write(&cache_path, serde_json::to_string_pretty(&index)?)?;

        let config = RemoteRegistryConfig::new("https://example.com")
            .with_cache_dir(cache_dir)
            .with_require_signed_index(true)
            .with_trust_store_path(trust_store_path);

        let client = RemoteRegistryClient::new(config)?;
        let cached = client.load_cached_index().await?;

        // Unsigned cache must be rejected in secure mode.
        assert!(cached.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_signature_required_but_missing() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        // Create a trust store file (empty map is fine for this test)
        let trust_store_path = temp_dir.path().join("trust_store.json");
        std::fs::write(&trust_store_path, "{}")?;

        let index = create_test_index(); // No signature

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(true)
            .with_trust_store_path(trust_store_path);

        let client = RemoteRegistryClient::new(config)?;
        let result = client.refresh().await;

        assert!(result.is_err());
        let err_msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            err_msg.contains("not signed") || err_msg.contains("signature"),
            "Error should mention missing signature: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_updates_state() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        let index = create_test_index();

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&index))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;

        // Before refresh
        assert!(client.needs_refresh());

        client.refresh().await?;

        // After refresh
        assert!(!client.needs_refresh());

        Ok(())
    }

    #[tokio::test]
    async fn test_http_error_handling() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        let result = client.refresh().await;

        assert!(result.is_err());
        let err_msg = result
            .as_ref()
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            err_msg.contains("404") || err_msg.contains("error status"),
            "Error should mention HTTP error: {}",
            err_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_json_response() -> Result<()> {
        let mock_server = MockServer::start().await;
        let temp_dir = TempDir::new()?;

        Mock::given(method("GET"))
            .and(path("/v1/index.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let config = RemoteRegistryConfig::new(mock_server.uri())
            .with_cache_dir(temp_dir.path().to_path_buf())
            .with_require_signed_index(false);

        let client = RemoteRegistryClient::new(config)?;
        let result = client.refresh().await;

        assert!(result.is_err());

        Ok(())
    }
}
