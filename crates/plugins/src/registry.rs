//! Plugin registry for discovering and managing plugins
//!
//! This module provides data structures for a plugin registry/catalog system
//! that allows searching, listing, and managing plugin metadata.
//!
//! ## Semver Compatibility
//!
//! The registry supports semantic versioning compatibility checking:
//! - Same major version with equal or higher minor/patch is compatible
//! - Different major versions are incompatible (breaking changes)
//! - Pre-release versions have special handling (only compatible with exact match)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::manifest::Capability;
use crate::{PluginError, PluginResult};

/// Result of checking version compatibility between two versions
///
/// Based on semantic versioning rules:
/// - Major version changes indicate breaking changes (incompatible)
/// - Minor/patch version increases are backward compatible
/// - Pre-release versions require exact match
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VersionCompatibility {
    /// The available version is compatible with the required version
    /// (same major version, equal or higher minor/patch)
    Compatible,
    /// The available version is incompatible with the required version
    /// (different major version or lower version)
    Incompatible,
    /// Compatibility cannot be determined (e.g., invalid version data)
    Unknown,
}

impl std::fmt::Display for VersionCompatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionCompatibility::Compatible => write!(f, "compatible"),
            VersionCompatibility::Incompatible => write!(f, "incompatible"),
            VersionCompatibility::Unknown => write!(f, "unknown"),
        }
    }
}

/// Check semantic version compatibility between a required version and an available version
///
/// # Semver Rules Applied
///
/// 1. **Major version mismatch**: If major versions differ, versions are incompatible
///    (major version changes indicate breaking changes)
///
/// 2. **Same major version**: If major versions match:
///    - Available version must be >= required version to be compatible
///    - This allows minor/patch updates which are backward compatible
///
/// 3. **Pre-release versions**: Pre-release versions (e.g., 1.0.0-alpha) are only
///    compatible with exact matches, as pre-release versions may have unstable APIs
///
/// 4. **Version 0.x.x**: Major version 0 is special - any change may be breaking,
///    so we require exact minor version match for 0.x versions
///
/// # Examples
///
/// ```
/// use semver::Version;
/// use racing_wheel_plugins::registry::{check_compatibility, VersionCompatibility};
///
/// // Same major, higher minor - compatible
/// let required = Version::new(1, 0, 0);
/// let available = Version::new(1, 2, 0);
/// assert_eq!(check_compatibility(&required, &available), VersionCompatibility::Compatible);
///
/// // Different major - incompatible
/// let required = Version::new(1, 0, 0);
/// let available = Version::new(2, 0, 0);
/// assert_eq!(check_compatibility(&required, &available), VersionCompatibility::Incompatible);
///
/// // Available version lower than required - incompatible
/// let required = Version::new(1, 5, 0);
/// let available = Version::new(1, 2, 0);
/// assert_eq!(check_compatibility(&required, &available), VersionCompatibility::Incompatible);
/// ```
pub fn check_compatibility(
    required: &semver::Version,
    available: &semver::Version,
) -> VersionCompatibility {
    // Handle pre-release versions: they require exact match
    // Pre-release versions may have unstable APIs
    if !required.pre.is_empty() || !available.pre.is_empty() {
        // For pre-release, require exact version match
        if required == available {
            return VersionCompatibility::Compatible;
        }
        // If one is pre-release and they don't match exactly, incompatible
        return VersionCompatibility::Incompatible;
    }

    // Major version 0 is special: any change may be breaking
    // For 0.x versions, require exact minor version match
    if required.major == 0 && available.major == 0 {
        if required.minor == available.minor && available.patch >= required.patch {
            return VersionCompatibility::Compatible;
        }
        return VersionCompatibility::Incompatible;
    }

    // Different major versions are incompatible (breaking changes)
    if required.major != available.major {
        return VersionCompatibility::Incompatible;
    }

    // Same major version: available must be >= required
    // Minor and patch increases are backward compatible
    if available.minor > required.minor {
        return VersionCompatibility::Compatible;
    }

    if available.minor == required.minor && available.patch >= required.patch {
        return VersionCompatibility::Compatible;
    }

    // Available version is lower than required
    VersionCompatibility::Incompatible
}

/// Unique identifier for a plugin in the registry
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PluginId(pub Uuid);

impl PluginId {
    /// Create a new random plugin ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a plugin ID from an existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for PluginId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Plugin metadata for registry
///
/// Contains all information about a plugin that is stored in the registry,
/// including identification, authorship, versioning, and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique identifier for this plugin
    pub id: PluginId,
    /// Human-readable name of the plugin
    pub name: String,
    /// Semantic version of the plugin
    pub version: semver::Version,
    /// Author or organization that created the plugin
    pub author: String,
    /// Description of what the plugin does
    pub description: String,
    /// Optional homepage URL for the plugin
    pub homepage: Option<String>,
    /// License identifier (e.g., "MIT", "Apache-2.0")
    pub license: String,
    /// List of capabilities this plugin requires
    pub capabilities: Vec<Capability>,
    /// Optional Ed25519 signature fingerprint for verification
    pub signature_fingerprint: Option<String>,
    /// Optional download URL for the plugin package
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// Optional SHA256 hash of the plugin package
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_hash: Option<String>,
}

impl PluginMetadata {
    /// Create a new PluginMetadata with required fields
    pub fn new(
        name: impl Into<String>,
        version: semver::Version,
        author: impl Into<String>,
        description: impl Into<String>,
        license: impl Into<String>,
    ) -> Self {
        Self {
            id: PluginId::new(),
            name: name.into(),
            version,
            author: author.into(),
            description: description.into(),
            homepage: None,
            license: license.into(),
            capabilities: Vec::new(),
            signature_fingerprint: None,
            download_url: None,
            package_hash: None,
        }
    }

    /// Builder method to set homepage
    pub fn with_homepage(mut self, homepage: impl Into<String>) -> Self {
        self.homepage = Some(homepage.into());
        self
    }

    /// Builder method to set capabilities
    pub fn with_capabilities(mut self, capabilities: Vec<Capability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Builder method to set signature fingerprint
    pub fn with_signature_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.signature_fingerprint = Some(fingerprint.into());
        self
    }

    /// Builder method to set download URL
    pub fn with_download_url(mut self, url: impl Into<String>) -> Self {
        self.download_url = Some(url.into());
        self
    }

    /// Builder method to set package hash
    pub fn with_package_hash(mut self, hash: impl Into<String>) -> Self {
        self.package_hash = Some(hash.into());
        self
    }

    /// Validate that the metadata has all required non-empty fields
    pub fn validate(&self) -> PluginResult<()> {
        if self.name.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin name cannot be empty".to_string(),
            ));
        }
        if self.author.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin author cannot be empty".to_string(),
            ));
        }
        if self.description.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin description cannot be empty".to_string(),
            ));
        }
        if self.license.is_empty() {
            return Err(PluginError::ManifestValidation(
                "Plugin license cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Plugin catalog for registry
///
/// Stores and manages a collection of plugins, supporting multiple versions
/// of the same plugin. Provides search, retrieval, and management operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCatalog {
    /// Map from plugin ID to list of versions (sorted by version, newest first)
    plugins: HashMap<PluginId, Vec<PluginMetadata>>,
    /// Index for name-based lookups (name -> plugin IDs)
    #[serde(skip)]
    name_index: HashMap<String, Vec<PluginId>>,
}

impl PluginCatalog {
    /// Create a new empty plugin catalog
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            name_index: HashMap::new(),
        }
    }

    /// Add a plugin to the catalog
    ///
    /// If a plugin with the same ID and version already exists, it will be replaced.
    /// Multiple versions of the same plugin (same ID) are supported.
    pub fn add_plugin(&mut self, metadata: PluginMetadata) -> PluginResult<()> {
        // Validate metadata before adding
        metadata.validate()?;

        let id = metadata.id.clone();
        let name = metadata.name.clone();

        // Get or create the version list for this plugin ID
        let versions = self.plugins.entry(id.clone()).or_default();

        // Check if this version already exists and remove it
        versions.retain(|m| m.version != metadata.version);

        // Add the new version
        versions.push(metadata);

        // Sort versions in descending order (newest first)
        versions.sort_by(|a, b| b.version.cmp(&a.version));

        // Update name index
        let name_lower = name.to_lowercase();
        let ids = self.name_index.entry(name_lower).or_default();
        if !ids.contains(&id) {
            ids.push(id);
        }

        Ok(())
    }

    /// Remove a plugin from the catalog
    ///
    /// If `version` is `Some`, only that specific version is removed.
    /// If `version` is `None`, all versions of the plugin are removed.
    ///
    /// Returns `true` if any plugin was removed, `false` otherwise.
    pub fn remove_plugin(&mut self, id: &PluginId, version: Option<&semver::Version>) -> bool {
        match version {
            Some(ver) => {
                // Remove specific version
                if let Some(versions) = self.plugins.get_mut(id) {
                    let original_len = versions.len();
                    versions.retain(|m| &m.version != ver);
                    let removed = versions.len() < original_len;

                    // If no versions left, remove the plugin entirely
                    if versions.is_empty() {
                        self.plugins.remove(id);
                        self.remove_from_name_index(id);
                    }

                    removed
                } else {
                    false
                }
            }
            None => {
                // Remove all versions
                if self.plugins.remove(id).is_some() {
                    self.remove_from_name_index(id);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Helper to remove a plugin ID from the name index
    fn remove_from_name_index(&mut self, id: &PluginId) {
        self.name_index.retain(|_, ids| {
            ids.retain(|i| i != id);
            !ids.is_empty()
        });
    }

    /// Search for plugins by name or description
    ///
    /// Returns all plugins where the name or description contains the query string
    /// (case-insensitive). Only the latest version of each matching plugin is returned.
    pub fn search(&self, query: &str) -> Vec<&PluginMetadata> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for versions in self.plugins.values() {
            // Get the latest version (first in the sorted list)
            if let Some(latest) = versions.first() {
                let name_matches = latest.name.to_lowercase().contains(&query_lower);
                let desc_matches = latest.description.to_lowercase().contains(&query_lower);

                if name_matches || desc_matches {
                    results.push(latest);
                }
            }
        }

        // Sort results by name for consistent ordering
        results.sort_by_key(|a| a.name.to_lowercase());

        results
    }

    /// Get a specific plugin by ID
    ///
    /// If `version` is `Some`, returns that specific version.
    /// If `version` is `None`, returns the latest version.
    pub fn get_plugin(
        &self,
        id: &PluginId,
        version: Option<&semver::Version>,
    ) -> Option<&PluginMetadata> {
        let versions = self.plugins.get(id)?;

        match version {
            Some(ver) => versions.iter().find(|m| &m.version == ver),
            None => versions.first(), // Latest version (sorted descending)
        }
    }

    /// Get all versions of a plugin by ID
    pub fn get_all_versions(&self, id: &PluginId) -> Option<&[PluginMetadata]> {
        self.plugins.get(id).map(|v| v.as_slice())
    }

    /// List all plugins in the catalog
    ///
    /// Returns the latest version of each plugin, sorted by name.
    pub fn list_all(&self) -> Vec<&PluginMetadata> {
        let mut results: Vec<&PluginMetadata> = self
            .plugins
            .values()
            .filter_map(|versions| versions.first())
            .collect();

        // Sort by name for consistent ordering
        results.sort_by_key(|a| a.name.to_lowercase());

        results
    }

    /// Get the total number of unique plugins (not counting versions)
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Get the total number of plugin versions across all plugins
    pub fn version_count(&self) -> usize {
        self.plugins.values().map(|v| v.len()).sum()
    }

    /// Check if a plugin with the given ID exists
    pub fn contains(&self, id: &PluginId) -> bool {
        self.plugins.contains_key(id)
    }

    /// Check if a specific version of a plugin exists
    pub fn contains_version(&self, id: &PluginId, version: &semver::Version) -> bool {
        self.plugins
            .get(id)
            .map(|versions| versions.iter().any(|m| &m.version == version))
            .unwrap_or(false)
    }

    /// Find a compatible version of a plugin based on semver rules
    ///
    /// Given a required version, this method finds the best compatible version
    /// from the available versions of the plugin. It follows semantic versioning
    /// rules:
    ///
    /// - Same major version with equal or higher minor/patch is compatible
    /// - Different major versions are incompatible (breaking changes)
    /// - Pre-release versions require exact match
    /// - For 0.x versions, requires exact minor version match
    ///
    /// # Returns
    ///
    /// Returns the highest compatible version if one exists, or `None` if no
    /// compatible version is found.
    ///
    /// # Examples
    ///
    /// ```
    /// use racing_wheel_plugins::registry::{PluginCatalog, PluginMetadata};
    /// use semver::Version;
    ///
    /// let mut catalog = PluginCatalog::new();
    ///
    /// // Add plugin versions 1.0.0, 1.2.0, 2.0.0
    /// let mut v1_0 = PluginMetadata::new("Test", Version::new(1, 0, 0), "Author", "Desc", "MIT");
    /// let id = v1_0.id.clone();
    /// catalog.add_plugin(v1_0).unwrap();
    ///
    /// let mut v1_2 = PluginMetadata::new("Test", Version::new(1, 2, 0), "Author", "Desc", "MIT");
    /// v1_2.id = id.clone();
    /// catalog.add_plugin(v1_2).unwrap();
    ///
    /// let mut v2_0 = PluginMetadata::new("Test", Version::new(2, 0, 0), "Author", "Desc", "MIT");
    /// v2_0.id = id.clone();
    /// catalog.add_plugin(v2_0).unwrap();
    ///
    /// // Requiring 1.0.0 should find 1.2.0 (highest compatible in major 1)
    /// let required = Version::new(1, 0, 0);
    /// let compatible = catalog.find_compatible_version(&id, &required);
    /// assert!(compatible.is_some());
    /// assert_eq!(compatible.unwrap().version, Version::new(1, 2, 0));
    /// ```
    pub fn find_compatible_version(
        &self,
        id: &PluginId,
        required: &semver::Version,
    ) -> Option<&PluginMetadata> {
        let versions = self.plugins.get(id)?;

        // Find all compatible versions
        let compatible_versions: Vec<&PluginMetadata> = versions
            .iter()
            .filter(|m| {
                check_compatibility(required, &m.version) == VersionCompatibility::Compatible
            })
            .collect();

        // Return the highest compatible version (versions are sorted descending)
        // Since we filter from a descending-sorted list, the first match is the highest
        compatible_versions.into_iter().next()
    }

    /// Check if a specific version is compatible with a required version
    ///
    /// Convenience method that combines `get_plugin` with `check_compatibility`.
    ///
    /// # Returns
    ///
    /// Returns the compatibility status between the required version and the
    /// specified available version. Returns `Unknown` if the plugin or version
    /// doesn't exist.
    pub fn check_version_compatibility(
        &self,
        id: &PluginId,
        available_version: &semver::Version,
        required_version: &semver::Version,
    ) -> VersionCompatibility {
        match self.get_plugin(id, Some(available_version)) {
            Some(metadata) => check_compatibility(required_version, &metadata.version),
            None => VersionCompatibility::Unknown,
        }
    }

    /// Rebuild the name index from the plugins map
    ///
    /// This should be called after deserializing a catalog from storage.
    pub fn rebuild_index(&mut self) {
        self.name_index.clear();
        for (id, versions) in &self.plugins {
            if let Some(latest) = versions.first() {
                let name_lower = latest.name.to_lowercase();
                self.name_index
                    .entry(name_lower)
                    .or_default()
                    .push(id.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metadata(name: &str, version: &str) -> PluginMetadata {
        PluginMetadata::new(
            name,
            semver::Version::parse(version)
                .ok()
                .unwrap_or_else(|| semver::Version::new(1, 0, 0)),
            "Test Author",
            format!("Description for {}", name),
            "MIT",
        )
    }

    #[test]
    fn test_plugin_id_creation() {
        let id1 = PluginId::new();
        let id2 = PluginId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_plugin_metadata_validation() {
        let valid = create_test_metadata("Test Plugin", "1.0.0");
        assert!(valid.validate().is_ok());

        let empty_name = PluginMetadata::new(
            "",
            semver::Version::new(1, 0, 0),
            "Author",
            "Description",
            "MIT",
        );
        assert!(empty_name.validate().is_err());

        let empty_author = PluginMetadata::new(
            "Name",
            semver::Version::new(1, 0, 0),
            "",
            "Description",
            "MIT",
        );
        assert!(empty_author.validate().is_err());
    }

    #[test]
    fn test_catalog_add_and_get() {
        let mut catalog = PluginCatalog::new();
        let metadata = create_test_metadata("Test Plugin", "1.0.0");
        let id = metadata.id.clone();

        assert!(catalog.add_plugin(metadata).is_ok());
        assert!(catalog.contains(&id));

        let retrieved = catalog.get_plugin(&id, None);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.map(|m| &m.name), Some(&"Test Plugin".to_string()));
    }

    #[test]
    fn test_catalog_multiple_versions() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.0.0");
        let id = v1.id.clone();

        let mut v2 = create_test_metadata("Test Plugin", "2.0.0");
        v2.id = id.clone();

        let mut v3 = create_test_metadata("Test Plugin", "1.5.0");
        v3.id = id.clone();

        assert!(catalog.add_plugin(v1).is_ok());
        assert!(catalog.add_plugin(v2).is_ok());
        assert!(catalog.add_plugin(v3).is_ok());

        // Should have 3 versions
        assert_eq!(catalog.version_count(), 3);
        assert_eq!(catalog.plugin_count(), 1);

        // Latest should be 2.0.0
        let latest = catalog.get_plugin(&id, None);
        assert_eq!(
            latest.map(|m| m.version.to_string()),
            Some("2.0.0".to_string())
        );

        // Can get specific version
        let v1_5 = catalog.get_plugin(&id, Some(&semver::Version::new(1, 5, 0)));
        assert!(v1_5.is_some());
    }

    #[test]
    fn test_catalog_remove_plugin() {
        let mut catalog = PluginCatalog::new();
        let metadata = create_test_metadata("Test Plugin", "1.0.0");
        let id = metadata.id.clone();

        assert!(catalog.add_plugin(metadata).is_ok());
        assert!(catalog.contains(&id));

        assert!(catalog.remove_plugin(&id, None));
        assert!(!catalog.contains(&id));
    }

    #[test]
    fn test_catalog_remove_specific_version() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.0.0");
        let id = v1.id.clone();

        let mut v2 = create_test_metadata("Test Plugin", "2.0.0");
        v2.id = id.clone();

        assert!(catalog.add_plugin(v1).is_ok());
        assert!(catalog.add_plugin(v2).is_ok());

        // Remove only v1
        assert!(catalog.remove_plugin(&id, Some(&semver::Version::new(1, 0, 0))));

        // Plugin should still exist with v2
        assert!(catalog.contains(&id));
        assert_eq!(catalog.version_count(), 1);

        let latest = catalog.get_plugin(&id, None);
        assert_eq!(
            latest.map(|m| m.version.to_string()),
            Some("2.0.0".to_string())
        );
    }

    #[test]
    fn test_catalog_search_by_name() {
        let mut catalog = PluginCatalog::new();

        let p1 = create_test_metadata("FFB Filter", "1.0.0");
        let p2 = create_test_metadata("LED Controller", "1.0.0");
        let p3 = create_test_metadata("Telemetry Logger", "1.0.0");

        assert!(catalog.add_plugin(p1).is_ok());
        assert!(catalog.add_plugin(p2).is_ok());
        assert!(catalog.add_plugin(p3).is_ok());

        // Search by name
        let results = catalog.search("filter");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "FFB Filter");

        // Case insensitive
        let results = catalog.search("LED");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "LED Controller");
    }

    #[test]
    fn test_catalog_search_by_description() {
        let mut catalog = PluginCatalog::new();

        let p1 = PluginMetadata::new(
            "Plugin A",
            semver::Version::new(1, 0, 0),
            "Author",
            "This plugin provides force feedback enhancements",
            "MIT",
        );

        let p2 = PluginMetadata::new(
            "Plugin B",
            semver::Version::new(1, 0, 0),
            "Author",
            "This plugin controls LED patterns",
            "MIT",
        );

        assert!(catalog.add_plugin(p1).is_ok());
        assert!(catalog.add_plugin(p2).is_ok());

        // Search by description
        let results = catalog.search("force feedback");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Plugin A");
    }

    #[test]
    fn test_catalog_list_all() {
        let mut catalog = PluginCatalog::new();

        let p1 = create_test_metadata("Zebra Plugin", "1.0.0");
        let p2 = create_test_metadata("Alpha Plugin", "1.0.0");
        let p3 = create_test_metadata("Beta Plugin", "1.0.0");

        assert!(catalog.add_plugin(p1).is_ok());
        assert!(catalog.add_plugin(p2).is_ok());
        assert!(catalog.add_plugin(p3).is_ok());

        let all = catalog.list_all();
        assert_eq!(all.len(), 3);

        // Should be sorted by name
        assert_eq!(all[0].name, "Alpha Plugin");
        assert_eq!(all[1].name, "Beta Plugin");
        assert_eq!(all[2].name, "Zebra Plugin");
    }

    #[test]
    fn test_catalog_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let mut catalog = PluginCatalog::new();
        let metadata = create_test_metadata("Test Plugin", "1.0.0");
        let id = metadata.id.clone();

        catalog.add_plugin(metadata)?;

        // Serialize to JSON
        let json = serde_json::to_string(&catalog)?;

        // Deserialize
        let mut restored: PluginCatalog = serde_json::from_str(&json)?;
        restored.rebuild_index();

        assert!(restored.contains(&id));
        assert_eq!(restored.plugin_count(), 1);

        Ok(())
    }

    #[test]
    fn test_metadata_builder_pattern() {
        let metadata = PluginMetadata::new(
            "Test Plugin",
            semver::Version::new(1, 0, 0),
            "Test Author",
            "Test Description",
            "MIT",
        )
        .with_homepage("https://example.com")
        .with_capabilities(vec![Capability::ReadTelemetry])
        .with_signature_fingerprint("abc123")
        .with_download_url("https://registry.example.com/plugin.zip")
        .with_package_hash("deadbeef");

        assert_eq!(metadata.homepage, Some("https://example.com".to_string()));
        assert_eq!(metadata.capabilities.len(), 1);
        assert_eq!(metadata.signature_fingerprint, Some("abc123".to_string()));
        assert_eq!(
            metadata.download_url,
            Some("https://registry.example.com/plugin.zip".to_string())
        );
        assert_eq!(metadata.package_hash, Some("deadbeef".to_string()));
    }

    // ============================================================================
    // Semver Compatibility Tests
    // ============================================================================

    #[test]
    fn test_check_compatibility_same_version() {
        let v1 = semver::Version::new(1, 0, 0);
        let v2 = semver::Version::new(1, 0, 0);
        assert_eq!(
            check_compatibility(&v1, &v2),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn test_check_compatibility_higher_minor() {
        // Required 1.0.0, available 1.2.0 - compatible (minor increase is backward compatible)
        let required = semver::Version::new(1, 0, 0);
        let available = semver::Version::new(1, 2, 0);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn test_check_compatibility_higher_patch() {
        // Required 1.0.0, available 1.0.5 - compatible (patch increase is backward compatible)
        let required = semver::Version::new(1, 0, 0);
        let available = semver::Version::new(1, 0, 5);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn test_check_compatibility_different_major() {
        // Required 1.0.0, available 2.0.0 - incompatible (major version change is breaking)
        let required = semver::Version::new(1, 0, 0);
        let available = semver::Version::new(2, 0, 0);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
    }

    #[test]
    fn test_check_compatibility_lower_major() {
        // Required 2.0.0, available 1.0.0 - incompatible
        let required = semver::Version::new(2, 0, 0);
        let available = semver::Version::new(1, 0, 0);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
    }

    #[test]
    fn test_check_compatibility_lower_minor() {
        // Required 1.5.0, available 1.2.0 - incompatible (available is older)
        let required = semver::Version::new(1, 5, 0);
        let available = semver::Version::new(1, 2, 0);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
    }

    #[test]
    fn test_check_compatibility_lower_patch() {
        // Required 1.0.5, available 1.0.2 - incompatible (available is older)
        let required = semver::Version::new(1, 0, 5);
        let available = semver::Version::new(1, 0, 2);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
    }

    #[test]
    fn test_check_compatibility_prerelease_exact_match() -> Result<(), Box<dyn std::error::Error>> {
        // Pre-release versions require exact match
        let required = semver::Version::parse("1.0.0-alpha")?;
        let available = semver::Version::parse("1.0.0-alpha")?;
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Compatible
        );
        Ok(())
    }

    #[test]
    fn test_check_compatibility_prerelease_different() -> Result<(), Box<dyn std::error::Error>> {
        // Different pre-release versions are incompatible
        let required = semver::Version::parse("1.0.0-alpha")?;
        let available = semver::Version::parse("1.0.0-beta")?;
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
        Ok(())
    }

    #[test]
    fn test_check_compatibility_prerelease_vs_release() -> Result<(), Box<dyn std::error::Error>> {
        // Pre-release required, release available - incompatible
        let required = semver::Version::parse("1.0.0-alpha")?;
        let available = semver::Version::new(1, 0, 0);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
        Ok(())
    }

    #[test]
    fn test_check_compatibility_zero_major_same_minor() {
        // For 0.x versions, same minor with higher patch is compatible
        let required = semver::Version::new(0, 1, 0);
        let available = semver::Version::new(0, 1, 5);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn test_check_compatibility_zero_major_different_minor() {
        // For 0.x versions, different minor is incompatible (any change may be breaking)
        let required = semver::Version::new(0, 1, 0);
        let available = semver::Version::new(0, 2, 0);
        assert_eq!(
            check_compatibility(&required, &available),
            VersionCompatibility::Incompatible
        );
    }

    #[test]
    fn test_find_compatible_version_basic() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.0.0");
        let id = v1.id.clone();

        let mut v1_2 = create_test_metadata("Test Plugin", "1.2.0");
        v1_2.id = id.clone();

        let mut v2_0 = create_test_metadata("Test Plugin", "2.0.0");
        v2_0.id = id.clone();

        assert!(catalog.add_plugin(v1).is_ok());
        assert!(catalog.add_plugin(v1_2).is_ok());
        assert!(catalog.add_plugin(v2_0).is_ok());

        // Requiring 1.0.0 should find 1.2.0 (highest compatible in major 1)
        let required = semver::Version::new(1, 0, 0);
        let compatible = catalog.find_compatible_version(&id, &required);
        assert!(compatible.is_some());
        assert_eq!(
            compatible.map(|m| m.version.to_string()),
            Some("1.2.0".to_string())
        );
    }

    #[test]
    fn test_find_compatible_version_exact_match() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.5.0");
        let id = v1.id.clone();

        assert!(catalog.add_plugin(v1).is_ok());

        // Requiring exactly what's available
        let required = semver::Version::new(1, 5, 0);
        let compatible = catalog.find_compatible_version(&id, &required);
        assert!(compatible.is_some());
        assert_eq!(
            compatible.map(|m| m.version.to_string()),
            Some("1.5.0".to_string())
        );
    }

    #[test]
    fn test_find_compatible_version_no_compatible() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.0.0");
        let id = v1.id.clone();

        assert!(catalog.add_plugin(v1).is_ok());

        // Requiring 2.0.0 when only 1.0.0 is available
        let required = semver::Version::new(2, 0, 0);
        let compatible = catalog.find_compatible_version(&id, &required);
        assert!(compatible.is_none());
    }

    #[test]
    fn test_find_compatible_version_required_higher_than_available() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.0.0");
        let id = v1.id.clone();

        assert!(catalog.add_plugin(v1).is_ok());

        // Requiring 1.5.0 when only 1.0.0 is available - incompatible
        let required = semver::Version::new(1, 5, 0);
        let compatible = catalog.find_compatible_version(&id, &required);
        assert!(compatible.is_none());
    }

    #[test]
    fn test_find_compatible_version_nonexistent_plugin() {
        let catalog = PluginCatalog::new();
        let id = PluginId::new();

        let required = semver::Version::new(1, 0, 0);
        let compatible = catalog.find_compatible_version(&id, &required);
        assert!(compatible.is_none());
    }

    #[test]
    fn test_find_compatible_version_selects_highest() {
        let mut catalog = PluginCatalog::new();

        let v1_0 = create_test_metadata("Test Plugin", "1.0.0");
        let id = v1_0.id.clone();

        let mut v1_1 = create_test_metadata("Test Plugin", "1.1.0");
        v1_1.id = id.clone();

        let mut v1_2 = create_test_metadata("Test Plugin", "1.2.0");
        v1_2.id = id.clone();

        let mut v1_3 = create_test_metadata("Test Plugin", "1.3.0");
        v1_3.id = id.clone();

        // Add in non-sorted order to verify sorting works
        assert!(catalog.add_plugin(v1_2).is_ok());
        assert!(catalog.add_plugin(v1_0).is_ok());
        assert!(catalog.add_plugin(v1_3).is_ok());
        assert!(catalog.add_plugin(v1_1).is_ok());

        // Requiring 1.0.0 should find 1.3.0 (highest compatible)
        let required = semver::Version::new(1, 0, 0);
        let compatible = catalog.find_compatible_version(&id, &required);
        assert!(compatible.is_some());
        assert_eq!(
            compatible.map(|m| m.version.to_string()),
            Some("1.3.0".to_string())
        );
    }

    #[test]
    fn test_check_version_compatibility_method() {
        let mut catalog = PluginCatalog::new();

        let v1 = create_test_metadata("Test Plugin", "1.5.0");
        let id = v1.id.clone();

        assert!(catalog.add_plugin(v1).is_ok());

        // Check compatibility using the catalog method
        let available = semver::Version::new(1, 5, 0);
        let required = semver::Version::new(1, 0, 0);
        assert_eq!(
            catalog.check_version_compatibility(&id, &available, &required),
            VersionCompatibility::Compatible
        );

        // Check with non-existent version
        let non_existent = semver::Version::new(2, 0, 0);
        assert_eq!(
            catalog.check_version_compatibility(&id, &non_existent, &required),
            VersionCompatibility::Unknown
        );
    }

    #[test]
    fn test_version_compatibility_display() {
        assert_eq!(VersionCompatibility::Compatible.to_string(), "compatible");
        assert_eq!(
            VersionCompatibility::Incompatible.to_string(),
            "incompatible"
        );
        assert_eq!(VersionCompatibility::Unknown.to_string(), "unknown");
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy to generate valid plugin names (non-empty alphanumeric with spaces/hyphens)
    fn plugin_name_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z][a-zA-Z0-9 -]{0,30}[a-zA-Z0-9]?"
            .prop_filter("name must not be empty", |s| !s.trim().is_empty())
    }

    /// Strategy to generate valid plugin descriptions (non-empty text)
    fn plugin_description_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z][a-zA-Z0-9 .,!?-]{5,100}"
            .prop_filter("description must not be empty", |s| !s.trim().is_empty())
    }

    /// Strategy to generate search query substrings
    fn search_query_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z]{1,10}"
    }

    /// Helper to create a valid PluginMetadata for testing
    fn create_valid_metadata(name: &str, description: &str) -> PluginMetadata {
        PluginMetadata::new(
            name,
            semver::Version::new(1, 0, 0),
            "Test Author",
            description,
            "MIT",
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Feature: release-roadmap-v1, Property 26: Plugin Registry Search
        /// **Validates: Requirements 16.1**
        ///
        /// For any plugin added to the catalog with a name containing a substring,
        /// searching for that substring returns the plugin.
        #[test]
        fn prop_search_finds_plugin_by_name_substring(
            base_name in plugin_name_strategy(),
            query in search_query_strategy(),
        ) {
            // Create a name that contains the query
            let name_with_query = format!("{} {}", base_name, query);
            let description = "A test plugin description without the query";

            let metadata = create_valid_metadata(&name_with_query, description);
            let plugin_id = metadata.id.clone();

            let mut catalog = PluginCatalog::new();
            let add_result = catalog.add_plugin(metadata);
            prop_assert!(add_result.is_ok(), "Failed to add plugin: {:?}", add_result);

            // Search for the query (case-insensitive)
            let results = catalog.search(&query);

            // The plugin should be found since its name contains the query
            let found = results.iter().any(|m| m.id == plugin_id);
            prop_assert!(
                found,
                "Plugin with name '{}' not found when searching for '{}'. Results: {:?}",
                name_with_query,
                query,
                results.iter().map(|m| &m.name).collect::<Vec<_>>()
            );
        }

        /// Feature: release-roadmap-v1, Property 26: Plugin Registry Search
        /// **Validates: Requirements 16.1**
        ///
        /// For any plugin added to the catalog with a description containing a substring,
        /// searching for that substring returns the plugin.
        #[test]
        fn prop_search_finds_plugin_by_description_substring(
            name in plugin_name_strategy(),
            base_description in plugin_description_strategy(),
            query in search_query_strategy(),
        ) {
            // Create a description that contains the query
            let description_with_query = format!("{} {}", base_description, query);

            let metadata = create_valid_metadata(&name, &description_with_query);
            let plugin_id = metadata.id.clone();

            let mut catalog = PluginCatalog::new();
            let add_result = catalog.add_plugin(metadata);
            prop_assert!(add_result.is_ok(), "Failed to add plugin: {:?}", add_result);

            // Search for the query (case-insensitive)
            let results = catalog.search(&query);

            // The plugin should be found since its description contains the query
            let found = results.iter().any(|m| m.id == plugin_id);
            prop_assert!(
                found,
                "Plugin with description containing '{}' not found when searching for '{}'. Results: {:?}",
                query,
                query,
                results.iter().map(|m| (&m.name, &m.description)).collect::<Vec<_>>()
            );
        }

        /// Feature: release-roadmap-v1, Property 26: Plugin Registry Search
        /// **Validates: Requirements 16.1**
        ///
        /// Search is case-insensitive: searching with different case variations
        /// of the same query should return the same results.
        #[test]
        fn prop_search_is_case_insensitive(
            name in plugin_name_strategy(),
            description in plugin_description_strategy(),
            query in search_query_strategy(),
        ) {
            // Create a name that contains the query in lowercase
            let name_with_query = format!("{} {}", name, query.to_lowercase());

            let metadata = create_valid_metadata(&name_with_query, &description);
            let plugin_id = metadata.id.clone();

            let mut catalog = PluginCatalog::new();
            let add_result = catalog.add_plugin(metadata);
            prop_assert!(add_result.is_ok(), "Failed to add plugin: {:?}", add_result);

            // Search with uppercase query
            let results_upper = catalog.search(&query.to_uppercase());
            // Search with lowercase query
            let results_lower = catalog.search(&query.to_lowercase());
            // Search with mixed case query
            let mixed_case: String = query
                .chars()
                .enumerate()
                .map(|(i, c)| if i % 2 == 0 { c.to_uppercase().next().unwrap_or(c) } else { c.to_lowercase().next().unwrap_or(c) })
                .collect();
            let results_mixed = catalog.search(&mixed_case);

            // All searches should find the plugin
            let found_upper = results_upper.iter().any(|m| m.id == plugin_id);
            let found_lower = results_lower.iter().any(|m| m.id == plugin_id);
            let found_mixed = results_mixed.iter().any(|m| m.id == plugin_id);

            prop_assert!(
                found_upper,
                "Plugin not found with uppercase query '{}' for name '{}'",
                query.to_uppercase(),
                name_with_query
            );
            prop_assert!(
                found_lower,
                "Plugin not found with lowercase query '{}' for name '{}'",
                query.to_lowercase(),
                name_with_query
            );
            prop_assert!(
                found_mixed,
                "Plugin not found with mixed case query '{}' for name '{}'",
                mixed_case,
                name_with_query
            );
        }

        /// Feature: release-roadmap-v1, Property 26: Plugin Registry Search
        /// **Validates: Requirements 16.1**
        ///
        /// For any search query that does not appear in any plugin's name or description,
        /// the search should return an empty result set.
        #[test]
        fn prop_search_returns_empty_for_non_matching_query(
            name in plugin_name_strategy(),
            description in plugin_description_strategy(),
        ) {
            let metadata = create_valid_metadata(&name, &description);

            let mut catalog = PluginCatalog::new();
            let add_result = catalog.add_plugin(metadata);
            prop_assert!(add_result.is_ok(), "Failed to add plugin: {:?}", add_result);

            // Use a query that is guaranteed not to match (using characters not in our strategies)
            let non_matching_query = "ZZZZXYZNONEXISTENT12345";

            // Verify the query doesn't appear in name or description
            if !name.to_lowercase().contains(&non_matching_query.to_lowercase())
                && !description.to_lowercase().contains(&non_matching_query.to_lowercase())
            {
                let results = catalog.search(non_matching_query);
                prop_assert!(
                    results.is_empty(),
                    "Expected empty results for non-matching query '{}', but found: {:?}",
                    non_matching_query,
                    results.iter().map(|m| &m.name).collect::<Vec<_>>()
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 26: Plugin Registry Search
        /// **Validates: Requirements 16.1**
        ///
        /// For any catalog with multiple plugins, searching should return all plugins
        /// whose name or description contains the query terms.
        #[test]
        fn prop_search_returns_all_matching_plugins(
            query in search_query_strategy(),
            num_matching in 1usize..5,
            num_non_matching in 0usize..5,
        ) {
            let mut catalog = PluginCatalog::new();
            let mut matching_ids = Vec::new();

            // Add plugins that should match (name contains query)
            for i in 0..num_matching {
                let name = format!("Matching Plugin {} {}", i, query);
                let description = format!("Description for matching plugin {}", i);
                let metadata = create_valid_metadata(&name, &description);
                matching_ids.push(metadata.id.clone());
                let add_result = catalog.add_plugin(metadata);
                prop_assert!(add_result.is_ok(), "Failed to add matching plugin: {:?}", add_result);
            }

            // Add plugins that should not match
            for i in 0..num_non_matching {
                let name = format!("Other Plugin {}", i);
                let description = format!("Other description {}", i);
                // Ensure the query doesn't appear in name or description
                if !name.to_lowercase().contains(&query.to_lowercase())
                    && !description.to_lowercase().contains(&query.to_lowercase())
                {
                    let metadata = create_valid_metadata(&name, &description);
                    let add_result = catalog.add_plugin(metadata);
                    prop_assert!(add_result.is_ok(), "Failed to add non-matching plugin: {:?}", add_result);
                }
            }

            let results = catalog.search(&query);

            // All matching plugins should be found
            for id in &matching_ids {
                let found = results.iter().any(|m| &m.id == id);
                prop_assert!(
                    found,
                    "Matching plugin with id {:?} not found in search results for query '{}'",
                    id,
                    query
                );
            }

            // Result count should be at least the number of matching plugins
            prop_assert!(
                results.len() >= matching_ids.len(),
                "Expected at least {} results, got {}",
                matching_ids.len(),
                results.len()
            );
        }

        // ============================================================================
        // Property 27: Plugin Metadata Completeness
        // **Validates: Requirements 16.3**
        //
        // For any plugin in the registry, the metadata SHALL include non-empty name,
        // author, version, and description fields.
        // ============================================================================

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// For any plugin added to the catalog, the metadata has non-empty name,
        /// author, version, and description fields.
        #[test]
        fn prop_catalog_plugins_have_complete_metadata(
            name in plugin_name_strategy(),
            description in plugin_description_strategy(),
            major in 0u64..100,
            minor in 0u64..100,
            patch in 0u64..100,
        ) {
            let version = semver::Version::new(major, minor, patch);
            let metadata = PluginMetadata::new(
                &name,
                version.clone(),
                "Test Author",
                &description,
                "MIT",
            );

            let mut catalog = PluginCatalog::new();
            let add_result = catalog.add_plugin(metadata);
            prop_assert!(add_result.is_ok(), "Failed to add valid plugin: {:?}", add_result);

            // Retrieve all plugins and verify completeness
            for plugin in catalog.list_all() {
                prop_assert!(
                    !plugin.name.is_empty(),
                    "Plugin name should not be empty, got: '{}'",
                    plugin.name
                );
                prop_assert!(
                    !plugin.author.is_empty(),
                    "Plugin author should not be empty, got: '{}'",
                    plugin.author
                );
                prop_assert!(
                    !plugin.description.is_empty(),
                    "Plugin description should not be empty, got: '{}'",
                    plugin.description
                );
                // Version is always valid since semver::Version cannot be empty
                prop_assert!(
                    plugin.version.major > 0 || plugin.version.minor > 0 || plugin.version.patch > 0 || (plugin.version.major == 0 && plugin.version.minor == 0 && plugin.version.patch == 0),
                    "Plugin version should be a valid semver: '{}'",
                    plugin.version
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// The validate() method correctly rejects metadata with empty name field.
        #[test]
        fn prop_validate_rejects_empty_name(
            description in plugin_description_strategy(),
            author in "[a-zA-Z][a-zA-Z0-9 ]{1,20}",
        ) {
            let metadata = PluginMetadata {
                id: PluginId::new(),
                name: String::new(), // Empty name
                version: semver::Version::new(1, 0, 0),
                author,
                description,
                homepage: None,
                license: "MIT".to_string(),
                capabilities: Vec::new(),
                signature_fingerprint: None,
                download_url: None,
                package_hash: None,
            };

            let result = metadata.validate();
            prop_assert!(
                result.is_err(),
                "validate() should reject metadata with empty name"
            );

            // Verify the error message mentions name
            if let Err(e) = result {
                let error_msg = e.to_string().to_lowercase();
                prop_assert!(
                    error_msg.contains("name"),
                    "Error message should mention 'name', got: '{}'",
                    e
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// The validate() method correctly rejects metadata with empty author field.
        #[test]
        fn prop_validate_rejects_empty_author(
            name in plugin_name_strategy(),
            description in plugin_description_strategy(),
        ) {
            let metadata = PluginMetadata {
                id: PluginId::new(),
                name,
                version: semver::Version::new(1, 0, 0),
                author: String::new(), // Empty author
                description,
                homepage: None,
                license: "MIT".to_string(),
                capabilities: Vec::new(),
                signature_fingerprint: None,
                download_url: None,
                package_hash: None,
            };

            let result = metadata.validate();
            prop_assert!(
                result.is_err(),
                "validate() should reject metadata with empty author"
            );

            // Verify the error message mentions author
            if let Err(e) = result {
                let error_msg = e.to_string().to_lowercase();
                prop_assert!(
                    error_msg.contains("author"),
                    "Error message should mention 'author', got: '{}'",
                    e
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// The validate() method correctly rejects metadata with empty description field.
        #[test]
        fn prop_validate_rejects_empty_description(
            name in plugin_name_strategy(),
            author in "[a-zA-Z][a-zA-Z0-9 ]{1,20}",
        ) {
            let metadata = PluginMetadata {
                id: PluginId::new(),
                name,
                version: semver::Version::new(1, 0, 0),
                author,
                description: String::new(), // Empty description
                homepage: None,
                license: "MIT".to_string(),
                capabilities: Vec::new(),
                signature_fingerprint: None,
                download_url: None,
                package_hash: None,
            };

            let result = metadata.validate();
            prop_assert!(
                result.is_err(),
                "validate() should reject metadata with empty description"
            );

            // Verify the error message mentions description
            if let Err(e) = result {
                let error_msg = e.to_string().to_lowercase();
                prop_assert!(
                    error_msg.contains("description"),
                    "Error message should mention 'description', got: '{}'",
                    e
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// The catalog add_plugin() method rejects plugins with incomplete metadata.
        #[test]
        fn prop_catalog_rejects_incomplete_metadata(
            valid_name in plugin_name_strategy(),
            valid_description in plugin_description_strategy(),
            valid_author in "[a-zA-Z][a-zA-Z0-9 ]{1,20}",
            field_to_empty in 0u8..3,
        ) {
            let mut catalog = PluginCatalog::new();

            // Create metadata with one field empty based on field_to_empty
            let metadata = match field_to_empty {
                0 => PluginMetadata {
                    id: PluginId::new(),
                    name: String::new(), // Empty name
                    version: semver::Version::new(1, 0, 0),
                    author: valid_author,
                    description: valid_description,
                    homepage: None,
                    license: "MIT".to_string(),
                    capabilities: Vec::new(),
                    signature_fingerprint: None,
                    download_url: None,
                    package_hash: None,
                },
                1 => PluginMetadata {
                    id: PluginId::new(),
                    name: valid_name,
                    version: semver::Version::new(1, 0, 0),
                    author: String::new(), // Empty author
                    description: valid_description,
                    homepage: None,
                    license: "MIT".to_string(),
                    capabilities: Vec::new(),
                    signature_fingerprint: None,
                    download_url: None,
                    package_hash: None,
                },
                _ => PluginMetadata {
                    id: PluginId::new(),
                    name: valid_name,
                    version: semver::Version::new(1, 0, 0),
                    author: valid_author,
                    description: String::new(), // Empty description
                    homepage: None,
                    license: "MIT".to_string(),
                    capabilities: Vec::new(),
                    signature_fingerprint: None,
                    download_url: None,
                    package_hash: None,
                },
            };

            let result = catalog.add_plugin(metadata);
            prop_assert!(
                result.is_err(),
                "add_plugin() should reject metadata with empty required field (field_to_empty={})",
                field_to_empty
            );
        }

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// All plugins retrieved from the catalog have complete metadata.
        /// This tests that after adding multiple valid plugins, all retrieved
        /// plugins maintain their complete metadata.
        #[test]
        fn prop_all_retrieved_plugins_have_complete_metadata(
            num_plugins in 1usize..10,
        ) {
            let mut catalog = PluginCatalog::new();

            // Add multiple valid plugins
            for i in 0..num_plugins {
                let metadata = PluginMetadata::new(
                    format!("Plugin {}", i),
                    semver::Version::new(1, i as u64, 0),
                    format!("Author {}", i),
                    format!("Description for plugin {}", i),
                    "MIT",
                );
                let add_result = catalog.add_plugin(metadata);
                prop_assert!(add_result.is_ok(), "Failed to add plugin {}: {:?}", i, add_result);
            }

            // Verify all plugins in catalog have complete metadata
            let all_plugins = catalog.list_all();
            prop_assert_eq!(
                all_plugins.len(),
                num_plugins,
                "Expected {} plugins, got {}",
                num_plugins,
                all_plugins.len()
            );

            for plugin in all_plugins {
                // Verify all required fields are non-empty
                prop_assert!(
                    !plugin.name.is_empty(),
                    "Retrieved plugin has empty name"
                );
                prop_assert!(
                    !plugin.author.is_empty(),
                    "Retrieved plugin '{}' has empty author",
                    plugin.name
                );
                prop_assert!(
                    !plugin.description.is_empty(),
                    "Retrieved plugin '{}' has empty description",
                    plugin.name
                );
                // Validate the metadata passes validation
                let validation_result = plugin.validate();
                prop_assert!(
                    validation_result.is_ok(),
                    "Retrieved plugin '{}' fails validation: {:?}",
                    plugin.name,
                    validation_result
                );
            }
        }

        /// Feature: release-roadmap-v1, Property 27: Plugin Metadata Completeness
        /// **Validates: Requirements 16.3**
        ///
        /// For any valid metadata, the validate() method accepts it.
        #[test]
        fn prop_validate_accepts_complete_metadata(
            name in plugin_name_strategy(),
            description in plugin_description_strategy(),
            author in "[a-zA-Z][a-zA-Z0-9 ]{1,20}",
            license in "[A-Z][A-Z0-9-]{1,10}",
            major in 0u64..100,
            minor in 0u64..100,
            patch in 0u64..100,
        ) {
            let metadata = PluginMetadata {
                id: PluginId::new(),
                name,
                version: semver::Version::new(major, minor, patch),
                author,
                description,
                homepage: None,
                license,
                capabilities: Vec::new(),
                signature_fingerprint: None,
                download_url: None,
                package_hash: None,
            };

            let result = metadata.validate();
            prop_assert!(
                result.is_ok(),
                "validate() should accept complete metadata, but got error: {:?}",
                result
            );
        }

        // ============================================================================
        // Property 28: Semver Compatibility
        // **Validates: Requirements 16.6**
        //
        // For any two plugin versions, the registry SHALL correctly determine
        // compatibility based on semantic versioning rules (major version changes
        // are breaking).
        // ============================================================================

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For any version compared to itself, compatibility is Compatible.
        #[test]
        fn prop_semver_self_compatibility(
            major in 1u64..100,
            minor in 0u64..100,
            patch in 0u64..100,
        ) {
            let version = semver::Version::new(major, minor, patch);
            let result = check_compatibility(&version, &version);

            prop_assert_eq!(
                result,
                VersionCompatibility::Compatible,
                "A version should always be compatible with itself: {} vs {}",
                version,
                version
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For any two versions with the same major version (>= 1) where
        /// available >= required, compatibility is Compatible.
        #[test]
        fn prop_semver_same_major_higher_or_equal_is_compatible(
            major in 1u64..100,
            required_minor in 0u64..50,
            required_patch in 0u64..50,
            minor_delta in 0u64..50,
            patch_delta in 0u64..50,
        ) {
            let required = semver::Version::new(major, required_minor, required_patch);

            // Available version has same major, and minor >= required_minor
            // If minor is same, patch >= required_patch
            let available_minor = required_minor + minor_delta;
            let available_patch = if minor_delta == 0 {
                required_patch + patch_delta
            } else {
                patch_delta // Any patch is fine if minor is higher
            };

            let available = semver::Version::new(major, available_minor, available_patch);
            let result = check_compatibility(&required, &available);

            prop_assert_eq!(
                result,
                VersionCompatibility::Compatible,
                "Same major with available >= required should be compatible: required={}, available={}",
                required,
                available
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For any two versions with different major versions (both >= 1),
        /// compatibility is Incompatible.
        #[test]
        fn prop_semver_different_major_is_incompatible(
            major1 in 1u64..100,
            major2 in 1u64..100,
            minor1 in 0u64..100,
            minor2 in 0u64..100,
            patch1 in 0u64..100,
            patch2 in 0u64..100,
        ) {
            // Ensure major versions are different
            prop_assume!(major1 != major2);

            let required = semver::Version::new(major1, minor1, patch1);
            let available = semver::Version::new(major2, minor2, patch2);
            let result = check_compatibility(&required, &available);

            prop_assert_eq!(
                result,
                VersionCompatibility::Incompatible,
                "Different major versions should be incompatible: required={}, available={}",
                required,
                available
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For any two versions with the same major version (>= 1) where
        /// available < required, compatibility is Incompatible.
        #[test]
        fn prop_semver_same_major_lower_is_incompatible(
            major in 1u64..100,
            required_minor in 1u64..100,
            required_patch in 1u64..100,
            minor_reduction in 1u64..50,
        ) {
            // Ensure we can actually reduce the minor version
            prop_assume!(required_minor >= minor_reduction);

            let required = semver::Version::new(major, required_minor, required_patch);

            // Available version has same major but lower minor
            let available_minor = required_minor - minor_reduction;
            let available = semver::Version::new(major, available_minor, 0);

            let result = check_compatibility(&required, &available);

            prop_assert_eq!(
                result,
                VersionCompatibility::Incompatible,
                "Same major with available < required should be incompatible: required={}, available={}",
                required,
                available
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For pre-release versions, only exact matches are compatible.
        #[test]
        fn prop_semver_prerelease_exact_match_compatible(
            major in 1u64..100,
            minor in 0u64..100,
            patch in 0u64..100,
            prerelease_id in "[a-z]{1,5}",
            prerelease_num in 0u64..100,
        ) {
            let prerelease_str = format!("{}.{}", prerelease_id, prerelease_num);
            let version_str = format!("{}.{}.{}-{}", major, minor, patch, prerelease_str);

            // Parse the version - if parsing fails, skip this test case
            let version = match semver::Version::parse(&version_str) {
                Ok(v) => v,
                Err(_) => return Ok(()), // Skip invalid version strings
            };

            // Same pre-release version should be compatible with itself
            let result = check_compatibility(&version, &version);

            prop_assert_eq!(
                result,
                VersionCompatibility::Compatible,
                "Pre-release version should be compatible with itself: {}",
                version
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For pre-release versions, different pre-release identifiers are incompatible.
        #[test]
        fn prop_semver_prerelease_different_is_incompatible(
            major in 1u64..100,
            minor in 0u64..100,
            patch in 0u64..100,
            prerelease_id1 in "[a-z]{1,5}",
            prerelease_id2 in "[a-z]{1,5}",
        ) {
            // Ensure pre-release identifiers are different
            prop_assume!(prerelease_id1 != prerelease_id2);

            let version_str1 = format!("{}.{}.{}-{}", major, minor, patch, prerelease_id1);
            let version_str2 = format!("{}.{}.{}-{}", major, minor, patch, prerelease_id2);

            // Parse the versions - if parsing fails, skip this test case
            let version1 = match semver::Version::parse(&version_str1) {
                Ok(v) => v,
                Err(_) => return Ok(()),
            };
            let version2 = match semver::Version::parse(&version_str2) {
                Ok(v) => v,
                Err(_) => return Ok(()),
            };

            let result = check_compatibility(&version1, &version2);

            prop_assert_eq!(
                result,
                VersionCompatibility::Incompatible,
                "Different pre-release versions should be incompatible: {} vs {}",
                version1,
                version2
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For pre-release required version vs release available version,
        /// compatibility is Incompatible (pre-release requires exact match).
        #[test]
        fn prop_semver_prerelease_vs_release_incompatible(
            major in 1u64..100,
            minor in 0u64..100,
            patch in 0u64..100,
            prerelease_id in "[a-z]{1,5}",
        ) {
            let prerelease_str = format!("{}.{}.{}-{}", major, minor, patch, prerelease_id);

            // Parse the pre-release version
            let prerelease_version = match semver::Version::parse(&prerelease_str) {
                Ok(v) => v,
                Err(_) => return Ok(()),
            };

            // Create a release version with same major.minor.patch
            let release_version = semver::Version::new(major, minor, patch);

            // Pre-release required vs release available should be incompatible
            let result = check_compatibility(&prerelease_version, &release_version);

            prop_assert_eq!(
                result,
                VersionCompatibility::Incompatible,
                "Pre-release required vs release available should be incompatible: {} vs {}",
                prerelease_version,
                release_version
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For 0.x versions (major version 0), different minor versions are
        /// incompatible since any change may be breaking in 0.x versions.
        #[test]
        fn prop_semver_zero_major_different_minor_incompatible(
            minor1 in 0u64..100,
            minor2 in 0u64..100,
            patch1 in 0u64..100,
            patch2 in 0u64..100,
        ) {
            // Ensure minor versions are different
            prop_assume!(minor1 != minor2);

            let required = semver::Version::new(0, minor1, patch1);
            let available = semver::Version::new(0, minor2, patch2);

            let result = check_compatibility(&required, &available);

            prop_assert_eq!(
                result,
                VersionCompatibility::Incompatible,
                "0.x versions with different minor should be incompatible: {} vs {}",
                required,
                available
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For 0.x versions with same minor, higher or equal patch is compatible.
        #[test]
        fn prop_semver_zero_major_same_minor_higher_patch_compatible(
            minor in 0u64..100,
            required_patch in 0u64..50,
            patch_delta in 0u64..50,
        ) {
            let required = semver::Version::new(0, minor, required_patch);
            let available = semver::Version::new(0, minor, required_patch + patch_delta);

            let result = check_compatibility(&required, &available);

            prop_assert_eq!(
                result,
                VersionCompatibility::Compatible,
                "0.x versions with same minor and available.patch >= required.patch should be compatible: {} vs {}",
                required,
                available
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// For 0.x versions with same minor, lower patch is incompatible.
        #[test]
        fn prop_semver_zero_major_same_minor_lower_patch_incompatible(
            minor in 0u64..100,
            required_patch in 1u64..100,
            patch_reduction in 1u64..50,
        ) {
            // Ensure we can actually reduce the patch version
            prop_assume!(required_patch >= patch_reduction);

            let required = semver::Version::new(0, minor, required_patch);
            let available = semver::Version::new(0, minor, required_patch - patch_reduction);

            let result = check_compatibility(&required, &available);

            prop_assert_eq!(
                result,
                VersionCompatibility::Incompatible,
                "0.x versions with same minor but available.patch < required.patch should be incompatible: {} vs {}",
                required,
                available
            );
        }

        /// Feature: release-roadmap-v1, Property 28: Semver Compatibility
        /// **Validates: Requirements 16.6**
        ///
        /// Compatibility check is deterministic: calling check_compatibility
        /// multiple times with the same inputs produces the same result.
        #[test]
        fn prop_semver_compatibility_is_deterministic(
            major1 in 0u64..100,
            minor1 in 0u64..100,
            patch1 in 0u64..100,
            major2 in 0u64..100,
            minor2 in 0u64..100,
            patch2 in 0u64..100,
        ) {
            let required = semver::Version::new(major1, minor1, patch1);
            let available = semver::Version::new(major2, minor2, patch2);

            let result1 = check_compatibility(&required, &available);
            let result2 = check_compatibility(&required, &available);
            let result3 = check_compatibility(&required, &available);

            prop_assert_eq!(
                result1,
                result2,
                "check_compatibility should be deterministic: first call={:?}, second call={:?}",
                result1,
                result2
            );
            prop_assert_eq!(
                result2,
                result3,
                "check_compatibility should be deterministic: second call={:?}, third call={:?}",
                result2,
                result3
            );
        }
    }
}
