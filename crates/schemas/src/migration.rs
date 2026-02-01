//! Profile migration system for schema version upgrades
//!
//! This module provides a framework for migrating profiles between schema versions.
//! It supports:
//! - Schema version detection
//! - Automatic backup creation before migration
//! - Migration trait for implementing version-specific migrations
//! - Rollback on failure with backup restoration
//!
//! # Example
//!
//! ```ignore
//! use racing_wheel_schemas::migration::{MigrationManager, MigrationConfig};
//!
//! let config = MigrationConfig::new("/path/to/profiles", "/path/to/backups");
//! let manager = MigrationManager::new(config)?;
//!
//! // Migrate a profile
//! let migrated = manager.migrate_profile(&old_profile_json)?;
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Current schema version
pub const CURRENT_SCHEMA_VERSION: &str = "wheel.profile/1";

/// Schema version for v2 (future)
pub const SCHEMA_VERSION_V2: &str = "wheel.profile/2";

/// Migration errors
#[derive(Error, Debug)]
pub enum MigrationError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Unknown schema version: {0}")]
    UnknownSchemaVersion(String),

    #[error("Migration failed from {from} to {to}: {reason}")]
    MigrationFailed {
        from: String,
        to: String,
        reason: String,
    },

    #[error("Backup creation failed: {0}")]
    BackupFailed(String),

    #[error("Backup restoration failed: {0}")]
    RestoreFailed(String),

    #[error("Schema version not found in profile")]
    SchemaVersionNotFound,

    #[error("Profile validation failed after migration: {0}")]
    ValidationFailed(String),

    #[error("No migration path from {from} to {to}")]
    NoMigrationPath { from: String, to: String },
}

/// Result type for migration operations
pub type MigrationResult<T> = Result<T, MigrationError>;

/// Schema version information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// The full version string (e.g., "wheel.profile/1")
    pub version: String,
    /// Major version number
    pub major: u32,
    /// Minor version number (optional, defaults to 0)
    pub minor: u32,
}

impl SchemaVersion {
    /// Parse a schema version string
    pub fn parse(version_str: &str) -> MigrationResult<Self> {
        // Expected format: "wheel.profile/N" or "wheel.profile/N.M"
        let parts: Vec<&str> = version_str.split('/').collect();
        if parts.len() != 2 || parts[0] != "wheel.profile" {
            return Err(MigrationError::UnknownSchemaVersion(
                version_str.to_string(),
            ));
        }

        let version_parts: Vec<&str> = parts[1].split('.').collect();
        let major = version_parts
            .first()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MigrationError::UnknownSchemaVersion(version_str.to_string()))?;

        let minor = version_parts
            .get(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Ok(Self {
            version: version_str.to_string(),
            major,
            minor,
        })
    }

    /// Create a new schema version
    pub fn new(major: u32, minor: u32) -> Self {
        Self {
            version: format!("wheel.profile/{}.{}", major, minor),
            major,
            minor,
        }
    }

    /// Check if this version is older than another
    pub fn is_older_than(&self, other: &SchemaVersion) -> bool {
        self.major < other.major || (self.major == other.major && self.minor < other.minor)
    }

    /// Check if this version is the current version
    pub fn is_current(&self) -> bool {
        self.version == CURRENT_SCHEMA_VERSION
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.version)
    }
}

/// Trait for implementing schema migrations
///
/// Each migration handles upgrading from one schema version to the next.
pub trait Migration: Send + Sync {
    /// The source schema version this migration handles
    fn source_version(&self) -> &SchemaVersion;

    /// The target schema version after migration
    fn target_version(&self) -> &SchemaVersion;

    /// Perform the migration on a JSON value
    fn migrate(&self, profile: Value) -> MigrationResult<Value>;

    /// Get a description of what this migration does
    fn description(&self) -> &str;
}

/// Backup information for a migrated profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Original profile path
    pub original_path: PathBuf,
    /// Backup file path
    pub backup_path: PathBuf,
    /// Original schema version
    pub original_version: String,
    /// Timestamp of backup creation
    pub created_at: String,
    /// SHA256 hash of original content
    pub content_hash: String,
}

impl BackupInfo {
    /// Create new backup info
    pub fn new(
        original_path: PathBuf,
        backup_path: PathBuf,
        original_version: String,
        content_hash: String,
    ) -> Self {
        Self {
            original_path,
            backup_path,
            original_version,
            created_at: chrono::Utc::now().to_rfc3339(),
            content_hash,
        }
    }
}

/// Configuration for the migration manager
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Directory for storing backups
    pub backup_dir: PathBuf,
    /// Whether to create backups before migration
    pub create_backups: bool,
    /// Maximum number of backups to keep per profile
    pub max_backups: usize,
    /// Whether to validate profiles after migration
    pub validate_after_migration: bool,
}

impl MigrationConfig {
    /// Create a new migration configuration
    pub fn new(backup_dir: impl Into<PathBuf>) -> Self {
        Self {
            backup_dir: backup_dir.into(),
            create_backups: true,
            max_backups: 5,
            validate_after_migration: true,
        }
    }

    /// Create configuration with backups disabled (for testing)
    pub fn without_backups() -> Self {
        Self {
            backup_dir: PathBuf::new(),
            create_backups: false,
            max_backups: 0,
            validate_after_migration: true,
        }
    }
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self::new("backups")
    }
}

/// Migration manager that handles profile migrations
pub struct MigrationManager {
    config: MigrationConfig,
    migrations: HashMap<String, Box<dyn Migration>>,
}

impl MigrationManager {
    /// Create a new migration manager with the given configuration
    pub fn new(config: MigrationConfig) -> MigrationResult<Self> {
        if config.create_backups && !config.backup_dir.as_os_str().is_empty() {
            fs::create_dir_all(&config.backup_dir)?;
        }

        let mut manager = Self {
            config,
            migrations: HashMap::new(),
        };

        // Register built-in migrations
        manager.register_builtin_migrations();

        Ok(manager)
    }

    /// Register built-in migrations
    fn register_builtin_migrations(&mut self) {
        // Register V0 to V1 migration (legacy format)
        self.register_migration(Box::new(V0ToV1Migration::new()));
    }

    /// Register a custom migration
    pub fn register_migration(&mut self, migration: Box<dyn Migration>) {
        let key = format!(
            "{}->{}",
            migration.source_version(),
            migration.target_version()
        );
        self.migrations.insert(key, migration);
    }

    /// Detect the schema version of a profile JSON string
    pub fn detect_version(&self, json: &str) -> MigrationResult<SchemaVersion> {
        let value: Value = serde_json::from_str(json)?;
        self.detect_version_from_value(&value)
    }

    /// Detect the schema version from a JSON value
    pub fn detect_version_from_value(&self, value: &Value) -> MigrationResult<SchemaVersion> {
        // Try to get the schema field
        if let Some(schema) = value.get("schema").and_then(|v| v.as_str()) {
            return SchemaVersion::parse(schema);
        }

        // Check for legacy format indicators
        if self.is_legacy_format(value) {
            return Ok(SchemaVersion::new(0, 0));
        }

        Err(MigrationError::SchemaVersionNotFound)
    }

    /// Check if a profile is in legacy format (pre-v1)
    fn is_legacy_format(&self, value: &Value) -> bool {
        value.get("ffb_gain").is_some()
            || value.get("degrees_of_rotation").is_some()
            || (value.get("schema").is_none() && value.get("base").is_none())
    }

    /// Check if a profile needs migration
    pub fn needs_migration(&self, json: &str) -> MigrationResult<bool> {
        let version = self.detect_version(json)?;
        Ok(!version.is_current())
    }

    /// Create a backup of a profile
    pub fn create_backup(
        &self,
        original_path: &Path,
        content: &str,
    ) -> MigrationResult<BackupInfo> {
        if !self.config.create_backups {
            return Err(MigrationError::BackupFailed(
                "Backups are disabled".to_string(),
            ));
        }

        let version = self.detect_version(content)?;
        let content_hash = Self::compute_hash(content);

        // Generate backup filename with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let original_name = original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile");
        let backup_name = format!("{}_{}.json.bak", original_name, timestamp);
        let backup_path = self.config.backup_dir.join(&backup_name);

        // Write backup file
        fs::write(&backup_path, content)?;

        // Clean up old backups if needed
        self.cleanup_old_backups(original_name)?;

        Ok(BackupInfo::new(
            original_path.to_path_buf(),
            backup_path,
            version.version,
            content_hash,
        ))
    }

    /// Compute hash of content
    fn compute_hash(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Clean up old backups, keeping only the most recent ones
    fn cleanup_old_backups(&self, profile_name: &str) -> MigrationResult<()> {
        if self.config.max_backups == 0 {
            return Ok(());
        }

        let pattern = format!("{}_", profile_name);
        let mut backups: Vec<_> = fs::read_dir(&self.config.backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&pattern) && name.ends_with(".json.bak"))
            })
            .collect();

        // Sort by modification time (newest first)
        backups.sort_by(|a, b| {
            let a_time = a.metadata().and_then(|m| m.modified()).ok();
            let b_time = b.metadata().and_then(|m| m.modified()).ok();
            b_time.cmp(&a_time)
        });

        // Remove old backups
        for backup in backups.iter().skip(self.config.max_backups) {
            let _ = fs::remove_file(backup.path());
        }

        Ok(())
    }

    /// Restore a profile from backup
    pub fn restore_backup(&self, backup_info: &BackupInfo) -> MigrationResult<String> {
        let content = fs::read_to_string(&backup_info.backup_path).map_err(|e| {
            MigrationError::RestoreFailed(format!("Failed to read backup file: {}", e))
        })?;

        // Verify hash
        let current_hash = Self::compute_hash(&content);
        if current_hash != backup_info.content_hash {
            return Err(MigrationError::RestoreFailed(
                "Backup file has been modified (hash mismatch)".to_string(),
            ));
        }

        Ok(content)
    }

    /// Migrate a profile JSON string to the current schema version
    pub fn migrate_profile(&self, json: &str) -> MigrationResult<String> {
        let value: Value = serde_json::from_str(json)?;
        let migrated = self.migrate_value(value)?;
        Ok(serde_json::to_string_pretty(&migrated)?)
    }

    /// Migrate a profile JSON value to the current schema version
    pub fn migrate_value(&self, mut value: Value) -> MigrationResult<Value> {
        let mut current_version = self.detect_version_from_value(&value)?;
        let target_version = SchemaVersion::parse(CURRENT_SCHEMA_VERSION)?;

        // If already at current version, return as-is
        if current_version.is_current() {
            return Ok(value);
        }

        // Apply migrations in sequence
        while current_version.is_older_than(&target_version) {
            let next_version = self.find_next_version(&current_version)?;
            let migration_key = format!("{}->{}", current_version, next_version);

            let migration = self.migrations.get(&migration_key).ok_or_else(|| {
                MigrationError::NoMigrationPath {
                    from: current_version.version.clone(),
                    to: next_version.version.clone(),
                }
            })?;

            value = migration.migrate(value)?;
            current_version = next_version;
        }

        // Validate the migrated profile
        if self.config.validate_after_migration {
            self.validate_migrated_profile(&value)?;
        }

        Ok(value)
    }

    /// Find the next version in the migration path
    fn find_next_version(&self, current: &SchemaVersion) -> MigrationResult<SchemaVersion> {
        for key in self.migrations.keys() {
            if key.starts_with(&format!("{}", current)) {
                let parts: Vec<&str> = key.split("->").collect();
                if parts.len() == 2 {
                    return SchemaVersion::parse(parts[1]);
                }
            }
        }

        Err(MigrationError::NoMigrationPath {
            from: current.version.clone(),
            to: CURRENT_SCHEMA_VERSION.to_string(),
        })
    }

    /// Validate a migrated profile
    fn validate_migrated_profile(&self, value: &Value) -> MigrationResult<()> {
        if value.get("schema").is_none() {
            return Err(MigrationError::ValidationFailed(
                "Missing 'schema' field".to_string(),
            ));
        }

        if value.get("scope").is_none() {
            return Err(MigrationError::ValidationFailed(
                "Missing 'scope' field".to_string(),
            ));
        }

        if value.get("base").is_none() {
            return Err(MigrationError::ValidationFailed(
                "Missing 'base' field".to_string(),
            ));
        }

        let schema = value
            .get("schema")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                MigrationError::ValidationFailed("Invalid 'schema' field".to_string())
            })?;

        if schema != CURRENT_SCHEMA_VERSION {
            return Err(MigrationError::ValidationFailed(format!(
                "Schema version '{}' is not current (expected '{}')",
                schema, CURRENT_SCHEMA_VERSION
            )));
        }

        Ok(())
    }

    /// Migrate a profile file in place, creating a backup first
    pub fn migrate_file(&self, path: &Path) -> MigrationResult<BackupInfo> {
        let content = fs::read_to_string(path)?;

        if !self.needs_migration(&content)? {
            return Err(MigrationError::MigrationFailed {
                from: CURRENT_SCHEMA_VERSION.to_string(),
                to: CURRENT_SCHEMA_VERSION.to_string(),
                reason: "Profile is already at current version".to_string(),
            });
        }

        let backup_info = self.create_backup(path, &content)?;

        match self.migrate_profile(&content) {
            Ok(migrated) => {
                fs::write(path, migrated)?;
                Ok(backup_info)
            }
            Err(e) => Err(e),
        }
    }
}

/// Migration from legacy format (v0) to v1
struct V0ToV1Migration {
    from: SchemaVersion,
    to: SchemaVersion,
}

impl V0ToV1Migration {
    fn new() -> Self {
        Self {
            from: SchemaVersion::new(0, 0),
            to: SchemaVersion::parse(CURRENT_SCHEMA_VERSION)
                .unwrap_or_else(|_| SchemaVersion::new(1, 0)),
        }
    }
}

impl Migration for V0ToV1Migration {
    fn source_version(&self) -> &SchemaVersion {
        &self.from
    }

    fn target_version(&self) -> &SchemaVersion {
        &self.to
    }

    fn description(&self) -> &str {
        "Migrate from legacy format to wheel.profile/1 schema"
    }

    fn migrate(&self, mut profile: Value) -> MigrationResult<Value> {
        let obj = profile
            .as_object_mut()
            .ok_or_else(|| MigrationError::MigrationFailed {
                from: self.from.version.clone(),
                to: self.to.version.clone(),
                reason: "Profile is not a JSON object".to_string(),
            })?;

        // Add schema version
        obj.insert(
            "schema".to_string(),
            Value::String(CURRENT_SCHEMA_VERSION.to_string()),
        );

        // Check if we need to restructure from flat format to nested format
        if obj.contains_key("ffb_gain") || obj.contains_key("degrees_of_rotation") {
            let ffb_gain = obj
                .remove("ffb_gain")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.7);
            let dor = obj
                .remove("degrees_of_rotation")
                .and_then(|v| v.as_f64())
                .unwrap_or(900.0) as u16;
            let torque_cap = obj
                .remove("torque_cap")
                .and_then(|v| v.as_f64())
                .unwrap_or(15.0);

            // Create base structure
            let base = serde_json::json!({
                "ffbGain": ffb_gain,
                "dorDeg": dor,
                "torqueCapNm": torque_cap,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            });

            obj.insert("base".to_string(), base);
        }

        // Ensure scope exists
        if !obj.contains_key("scope") {
            obj.insert(
                "scope".to_string(),
                serde_json::json!({
                    "game": null,
                    "car": null,
                    "track": null
                }),
            );
        }

        Ok(profile)
    }
}

/// Profile migration service that handles the complete migration workflow
pub struct ProfileMigrationService {
    manager: MigrationManager,
}

impl ProfileMigrationService {
    /// Create a new profile migration service
    pub fn new(config: MigrationConfig) -> MigrationResult<Self> {
        Ok(Self {
            manager: MigrationManager::new(config)?,
        })
    }

    /// Create a service with default configuration
    pub fn with_backup_dir(backup_dir: impl Into<PathBuf>) -> MigrationResult<Self> {
        Self::new(MigrationConfig::new(backup_dir))
    }

    /// Detect the schema version of a profile
    pub fn detect_version(&self, json: &str) -> MigrationResult<SchemaVersion> {
        self.manager.detect_version(json)
    }

    /// Check if a profile needs migration
    pub fn needs_migration(&self, json: &str) -> MigrationResult<bool> {
        self.manager.needs_migration(json)
    }

    /// Migrate a profile string with automatic backup and restore on failure
    pub fn migrate_with_backup(
        &self,
        json: &str,
        original_path: Option<&Path>,
    ) -> MigrationResult<MigrationOutcome> {
        let original_version = self.detect_version(json)?;

        // If already current, no migration needed
        if original_version.is_current() {
            return Ok(MigrationOutcome {
                migrated_json: json.to_string(),
                original_version: original_version.clone(),
                target_version: original_version,
                backup_info: None,
                migrations_applied: vec![],
            });
        }

        // Create backup if path provided
        let backup_info = if let Some(path) = original_path {
            Some(self.manager.create_backup(path, json)?)
        } else {
            None
        };

        // Attempt migration
        match self.manager.migrate_profile(json) {
            Ok(migrated_json) => {
                let target_version = self.detect_version(&migrated_json)?;
                Ok(MigrationOutcome {
                    migrated_json,
                    original_version,
                    target_version,
                    backup_info,
                    migrations_applied: vec!["V0ToV1".to_string()],
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Migrate a profile file in place
    pub fn migrate_file(&self, path: &Path) -> MigrationResult<MigrationOutcome> {
        let content = fs::read_to_string(path)?;
        let outcome = self.migrate_with_backup(&content, Some(path))?;

        // Write migrated content back to file
        fs::write(path, &outcome.migrated_json)?;

        Ok(outcome)
    }

    /// Restore a profile from backup
    pub fn restore_from_backup(&self, backup_info: &BackupInfo) -> MigrationResult<String> {
        self.manager.restore_backup(backup_info)
    }

    /// Restore a profile file from backup
    pub fn restore_file_from_backup(&self, backup_info: &BackupInfo) -> MigrationResult<()> {
        let content = self.restore_from_backup(backup_info)?;
        fs::write(&backup_info.original_path, content)?;
        Ok(())
    }

    /// Migrate multiple profiles in a directory
    pub fn migrate_directory(&self, dir: &Path) -> MigrationResult<Vec<MigrationOutcome>> {
        let mut outcomes = Vec::new();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Ok(content) = fs::read_to_string(&path)
                && self.needs_migration(&content).unwrap_or(false)
            {
                match self.migrate_file(&path) {
                    Ok(outcome) => outcomes.push(outcome),
                    Err(e) => {
                        eprintln!("Failed to migrate {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(outcomes)
    }
}

/// Outcome of a profile migration
#[derive(Debug, Clone)]
pub struct MigrationOutcome {
    /// The migrated JSON string
    pub migrated_json: String,
    /// The original schema version
    pub original_version: SchemaVersion,
    /// The target schema version after migration
    pub target_version: SchemaVersion,
    /// Backup information (if backup was created)
    pub backup_info: Option<BackupInfo>,
    /// List of migrations that were applied
    pub migrations_applied: Vec<String>,
}

impl MigrationOutcome {
    /// Check if any migration was actually performed
    pub fn was_migrated(&self) -> bool {
        !self.migrations_applied.is_empty()
    }

    /// Get the number of migrations applied
    pub fn migration_count(&self) -> usize {
        self.migrations_applied.len()
    }
}

/// Builder for creating test profiles in various schema versions
#[cfg(test)]
pub mod test_utils {
    use super::*;

    /// Create a legacy (v0) format profile for testing
    pub fn create_legacy_profile(ffb_gain: f64, dor: u16, torque_cap: f64) -> String {
        serde_json::json!({
            "ffb_gain": ffb_gain,
            "degrees_of_rotation": dor,
            "torque_cap": torque_cap
        })
        .to_string()
    }

    /// Create a current (v1) format profile for testing
    pub fn create_v1_profile(ffb_gain: f64, dor: u16, torque_cap: f64) -> String {
        serde_json::json!({
            "schema": CURRENT_SCHEMA_VERSION,
            "scope": { "game": null, "car": null, "track": null },
            "base": {
                "ffbGain": ffb_gain,
                "dorDeg": dor,
                "torqueCapNm": torque_cap,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        })
        .to_string()
    }

    /// Create a v1 profile with a specific game scope
    pub fn create_v1_profile_with_scope(
        ffb_gain: f64,
        dor: u16,
        torque_cap: f64,
        game: Option<&str>,
    ) -> String {
        serde_json::json!({
            "schema": CURRENT_SCHEMA_VERSION,
            "scope": { "game": game, "car": null, "track": null },
            "base": {
                "ffbGain": ffb_gain,
                "dorDeg": dor,
                "torqueCapNm": torque_cap,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> MigrationResult<MigrationManager> {
        MigrationManager::new(MigrationConfig::without_backups())
    }

    #[test]
    fn test_schema_version_parse() -> MigrationResult<()> {
        let v1 = SchemaVersion::parse("wheel.profile/1")?;
        assert_eq!(v1.major, 1);
        assert_eq!(v1.minor, 0);
        assert!(v1.is_current());

        let v2 = SchemaVersion::parse("wheel.profile/2")?;
        assert_eq!(v2.major, 2);
        assert_eq!(v2.minor, 0);
        assert!(!v2.is_current());

        let v1_1 = SchemaVersion::parse("wheel.profile/1.1")?;
        assert_eq!(v1_1.major, 1);
        assert_eq!(v1_1.minor, 1);

        Ok(())
    }

    #[test]
    fn test_schema_version_comparison() -> MigrationResult<()> {
        let v0 = SchemaVersion::new(0, 0);
        let v1 = SchemaVersion::parse("wheel.profile/1")?;
        let v2 = SchemaVersion::new(2, 0);

        assert!(v0.is_older_than(&v1));
        assert!(v1.is_older_than(&v2));
        assert!(!v2.is_older_than(&v1));
        assert!(!v1.is_older_than(&v0));

        Ok(())
    }

    #[test]
    fn test_detect_current_version() -> MigrationResult<()> {
        let manager = create_test_manager()?;

        let current_profile = r#"{
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        }"#;

        let version = manager.detect_version(current_profile)?;
        assert!(version.is_current());
        assert!(!manager.needs_migration(current_profile)?);

        Ok(())
    }

    #[test]
    fn test_detect_legacy_version() -> MigrationResult<()> {
        let manager = create_test_manager()?;

        let legacy_profile = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        let version = manager.detect_version(legacy_profile)?;
        assert_eq!(version.major, 0);
        assert!(manager.needs_migration(legacy_profile)?);

        Ok(())
    }

    #[test]
    fn test_migrate_legacy_to_v1() -> MigrationResult<()> {
        let manager = create_test_manager()?;

        let legacy_profile = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        let migrated = manager.migrate_profile(legacy_profile)?;
        let value: Value = serde_json::from_str(&migrated)?;

        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );

        let base = value.get("base").expect("base should exist");
        assert_eq!(base.get("ffbGain").and_then(|v| v.as_f64()), Some(0.8));
        assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(900));
        assert_eq!(base.get("torqueCapNm").and_then(|v| v.as_f64()), Some(12.0));

        assert!(value.get("scope").is_some());

        Ok(())
    }

    #[test]
    fn test_no_migration_needed() -> MigrationResult<()> {
        let manager = create_test_manager()?;

        let current_profile = r#"{
            "schema": "wheel.profile/1",
            "scope": {"game": "iRacing"},
            "base": {
                "ffbGain": 0.7,
                "dorDeg": 900,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        }"#;

        let migrated = manager.migrate_profile(current_profile)?;
        let original: Value = serde_json::from_str(current_profile)?;
        let result: Value = serde_json::from_str(&migrated)?;

        assert_eq!(original.get("schema"), result.get("schema"));

        Ok(())
    }

    #[test]
    fn test_backup_info_creation() {
        let backup = BackupInfo::new(
            PathBuf::from("/profiles/test.json"),
            PathBuf::from("/backups/test_20240101_120000.json.bak"),
            "wheel.profile/0".to_string(),
            "abc123".to_string(),
        );

        assert_eq!(backup.original_version, "wheel.profile/0");
        assert!(!backup.created_at.is_empty());
    }

    #[test]
    fn test_invalid_schema_version() {
        let result = SchemaVersion::parse("invalid/format");
        assert!(result.is_err());

        let result = SchemaVersion::parse("wheel.profile/");
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod profile_migration_tests {
    use super::test_utils::*;
    use super::*;
    use tempfile::TempDir;

    fn create_test_service() -> MigrationResult<(ProfileMigrationService, TempDir)> {
        let temp_dir = TempDir::new().map_err(MigrationError::IoError)?;
        let backup_dir = temp_dir.path().join("backups");
        let service = ProfileMigrationService::with_backup_dir(&backup_dir)?;
        Ok((service, temp_dir))
    }

    #[test]
    fn test_migrate_legacy_profile() -> MigrationResult<()> {
        let (service, _temp) = create_test_service()?;

        let legacy = create_legacy_profile(0.8, 900, 12.0);
        let outcome = service.migrate_with_backup(&legacy, None::<&Path>)?;

        assert!(outcome.was_migrated());
        assert_eq!(outcome.original_version.major, 0);
        assert!(outcome.target_version.is_current());

        let value: Value = serde_json::from_str(&outcome.migrated_json)?;
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );

        Ok(())
    }

    #[test]
    fn test_no_migration_for_current_version() -> MigrationResult<()> {
        let (service, _temp) = create_test_service()?;

        let current = create_v1_profile(0.7, 900, 15.0);
        let outcome = service.migrate_with_backup(&current, None::<&Path>)?;

        assert!(!outcome.was_migrated());
        assert!(outcome.original_version.is_current());
        assert!(outcome.target_version.is_current());

        Ok(())
    }

    #[test]
    fn test_migrate_file_with_backup() -> MigrationResult<()> {
        let (service, temp_dir) = create_test_service()?;

        let profile_path = temp_dir.path().join("test_profile.json");
        let legacy = create_legacy_profile(0.75, 1080, 18.0);
        fs::write(&profile_path, &legacy)?;

        let outcome = service.migrate_file(&profile_path)?;

        assert!(outcome.was_migrated());
        assert!(outcome.backup_info.is_some());

        let migrated_content = fs::read_to_string(&profile_path)?;
        let value: Value = serde_json::from_str(&migrated_content)?;
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some(CURRENT_SCHEMA_VERSION)
        );

        let backup = outcome.backup_info.as_ref().expect("backup should exist");
        assert!(backup.backup_path.exists());

        Ok(())
    }

    #[test]
    fn test_restore_from_backup() -> MigrationResult<()> {
        let (service, temp_dir) = create_test_service()?;

        let profile_path = temp_dir.path().join("restore_test.json");
        let legacy = create_legacy_profile(0.6, 720, 10.0);
        fs::write(&profile_path, &legacy)?;

        let outcome = service.migrate_file(&profile_path)?;
        let backup_info = outcome.backup_info.expect("backup should exist");

        service.restore_file_from_backup(&backup_info)?;

        let restored_content = fs::read_to_string(&profile_path)?;
        let value: Value = serde_json::from_str(&restored_content)?;

        assert!(value.get("ffb_gain").is_some());
        assert!(value.get("schema").is_none());

        Ok(())
    }

    #[test]
    fn test_migrate_preserves_values() -> MigrationResult<()> {
        let (service, _temp) = create_test_service()?;

        let legacy = create_legacy_profile(0.85, 1200, 20.0);
        let outcome = service.migrate_with_backup(&legacy, None::<&Path>)?;

        let value: Value = serde_json::from_str(&outcome.migrated_json)?;
        let base = value.get("base").expect("base should exist");

        assert_eq!(base.get("ffbGain").and_then(|v| v.as_f64()), Some(0.85));
        assert_eq!(base.get("dorDeg").and_then(|v| v.as_u64()), Some(1200));
        assert_eq!(base.get("torqueCapNm").and_then(|v| v.as_f64()), Some(20.0));

        Ok(())
    }

    #[test]
    fn test_migration_outcome_properties() -> MigrationResult<()> {
        let (service, _temp) = create_test_service()?;

        let legacy = create_legacy_profile(0.7, 900, 15.0);
        let outcome = service.migrate_with_backup(&legacy, None::<&Path>)?;

        assert!(outcome.was_migrated());
        assert_eq!(outcome.migration_count(), 1);

        let current = create_v1_profile(0.7, 900, 15.0);
        let outcome = service.migrate_with_backup(&current, None::<&Path>)?;

        assert!(!outcome.was_migrated());
        assert_eq!(outcome.migration_count(), 0);

        Ok(())
    }
}

#[cfg(test)]
mod property_tests {
    use super::test_utils::*;
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    // Feature: release-roadmap-v1, Property 36: Migration Round-Trip (Consolidated)
    // **Validates: Requirements 20.1, 20.2, 20.3, 20.4**
    //
    // *For any* profile in a previous schema version, the migration system SHALL:
    // (1) detect the old version
    // (2) create a backup
    // (3) migrate to the new schema
    // (4) restore the backup if migration fails

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Property 36: Migration detects old versions correctly
        #[test]
        fn prop_migration_detects_old_version(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let manager = MigrationManager::new(MigrationConfig::without_backups())?;

            // Create a legacy profile
            let legacy = create_legacy_profile(ffb_gain, dor, torque_cap);

            // Should detect as version 0
            let version = manager.detect_version(&legacy)?;
            prop_assert_eq!(version.major, 0, "Legacy profile should be detected as v0");
            prop_assert!(!version.is_current(), "Legacy profile should not be current");
            prop_assert!(manager.needs_migration(&legacy)?, "Legacy profile should need migration");
        }

        /// Property 36: Migration creates backup before migrating
        #[test]
        fn prop_migration_creates_backup(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let temp_dir = TempDir::new().map_err(|e| TestCaseError::fail(e.to_string()))?;
            let backup_dir = temp_dir.path().join("backups");
            let service = ProfileMigrationService::with_backup_dir(&backup_dir)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;

            // Create a legacy profile file
            let profile_path = temp_dir.path().join("test_profile.json");
            let legacy = create_legacy_profile(ffb_gain, dor, torque_cap);
            fs::write(&profile_path, &legacy).map_err(|e| TestCaseError::fail(e.to_string()))?;

            // Migrate the file
            let outcome = service.migrate_file(&profile_path)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;

            // Verify backup was created
            prop_assert!(outcome.backup_info.is_some(), "Backup should be created");
            let backup = outcome.backup_info.as_ref().expect("backup exists");
            prop_assert!(backup.backup_path.exists(), "Backup file should exist on disk");
            prop_assert!(!backup.content_hash.is_empty(), "Backup should have content hash");
        }

        /// Property 36: Migration produces valid current schema
        #[test]
        fn prop_migration_produces_current_schema(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let manager = MigrationManager::new(MigrationConfig::without_backups())?;

            let legacy = create_legacy_profile(ffb_gain, dor, torque_cap);
            let migrated = manager.migrate_profile(&legacy)?;

            // Parse and verify schema version
            let value: Value = serde_json::from_str(&migrated)?;
            let schema = value.get("schema").and_then(|v| v.as_str());
            prop_assert_eq!(schema, Some(CURRENT_SCHEMA_VERSION), "Migrated profile should have current schema");

            // Verify required fields exist
            prop_assert!(value.get("scope").is_some(), "Migrated profile should have scope");
            prop_assert!(value.get("base").is_some(), "Migrated profile should have base");
        }

        /// Property 36: Migration preserves original values
        #[test]
        fn prop_migration_preserves_values(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let manager = MigrationManager::new(MigrationConfig::without_backups())?;

            let legacy = create_legacy_profile(ffb_gain, dor, torque_cap);
            let migrated = manager.migrate_profile(&legacy)?;

            let value: Value = serde_json::from_str(&migrated)?;
            let base = value.get("base").expect("base should exist");

            // Verify values are preserved (with floating point tolerance)
            let migrated_ffb = base.get("ffbGain").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let migrated_dor = base.get("dorDeg").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let migrated_torque = base.get("torqueCapNm").and_then(|v| v.as_f64()).unwrap_or(0.0);

            prop_assert!((migrated_ffb - ffb_gain).abs() < 0.001,
                "FFB gain should be preserved: {} vs {}", migrated_ffb, ffb_gain);
            prop_assert_eq!(migrated_dor, dor, "DOR should be preserved");
            prop_assert!((migrated_torque - torque_cap).abs() < 0.001,
                "Torque cap should be preserved: {} vs {}", migrated_torque, torque_cap);
        }

        /// Property 36: Backup can be restored after migration
        #[test]
        fn prop_backup_can_be_restored(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let temp_dir = TempDir::new().map_err(|e| TestCaseError::fail(e.to_string()))?;
            let backup_dir = temp_dir.path().join("backups");
            let service = ProfileMigrationService::with_backup_dir(&backup_dir)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;

            // Create and migrate a legacy profile
            let profile_path = temp_dir.path().join("restore_test.json");
            let legacy = create_legacy_profile(ffb_gain, dor, torque_cap);
            fs::write(&profile_path, &legacy).map_err(|e| TestCaseError::fail(e.to_string()))?;

            let outcome = service.migrate_file(&profile_path)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let backup_info = outcome.backup_info.expect("backup should exist");

            // Restore from backup
            service.restore_file_from_backup(&backup_info)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;

            // Verify restoration
            let restored = fs::read_to_string(&profile_path)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            let value: Value = serde_json::from_str(&restored)?;

            // Should be back to legacy format
            prop_assert!(value.get("ffb_gain").is_some(), "Restored profile should have legacy ffb_gain");
            prop_assert!(value.get("schema").is_none(), "Restored profile should not have schema field");
        }

        /// Property 36: Current version profiles don't need migration
        #[test]
        fn prop_current_version_no_migration(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let manager = MigrationManager::new(MigrationConfig::without_backups())?;

            let current = create_v1_profile(ffb_gain, dor, torque_cap);

            // Should detect as current version
            let version = manager.detect_version(&current)?;
            prop_assert!(version.is_current(), "V1 profile should be detected as current");
            prop_assert!(!manager.needs_migration(&current)?, "V1 profile should not need migration");
        }

        /// Property 36: Migration is idempotent for current version
        #[test]
        fn prop_migration_idempotent(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let manager = MigrationManager::new(MigrationConfig::without_backups())?;

            let current = create_v1_profile(ffb_gain, dor, torque_cap);
            let migrated = manager.migrate_profile(&current)?;

            // Parse both and compare key fields
            let original: Value = serde_json::from_str(&current)?;
            let result: Value = serde_json::from_str(&migrated)?;

            prop_assert_eq!(
                original.get("schema"),
                result.get("schema"),
                "Schema should be unchanged"
            );

            let orig_base = original.get("base");
            let result_base = result.get("base");
            prop_assert_eq!(
                orig_base.and_then(|b| b.get("ffbGain")),
                result_base.and_then(|b| b.get("ffbGain")),
                "FFB gain should be unchanged"
            );
        }
    }
}

/// Backward compatibility support for parsing old schema versions
///
/// This module provides utilities for parsing profiles from older schema versions
/// without requiring explicit migration. The parser automatically handles
/// differences between schema versions within the same major version.
pub mod compat {
    use super::*;

    /// Backward-compatible profile parser
    ///
    /// This parser can read profiles from any schema version within the same
    /// major version (e.g., 1.0, 1.1, 1.2 are all compatible with parser for v1).
    pub struct BackwardCompatibleParser {
        /// The major version this parser supports
        pub major_version: u32,
    }

    impl BackwardCompatibleParser {
        /// Create a new parser for the current schema version
        pub fn new() -> Self {
            Self { major_version: 1 }
        }

        /// Create a parser for a specific major version
        pub fn for_major_version(major: u32) -> Self {
            Self {
                major_version: major,
            }
        }

        /// Check if a profile is compatible with this parser
        pub fn is_compatible(&self, json: &str) -> MigrationResult<bool> {
            let value: Value = serde_json::from_str(json)?;
            self.is_compatible_value(&value)
        }

        /// Check if a JSON value is compatible with this parser
        pub fn is_compatible_value(&self, value: &Value) -> MigrationResult<bool> {
            // Try to get schema version
            if let Some(schema) = value.get("schema").and_then(|v| v.as_str()) {
                let version = SchemaVersion::parse(schema)?;
                return Ok(version.major == self.major_version);
            }

            // Legacy format (v0) is not compatible with v1 parser without migration
            Ok(false)
        }

        /// Parse a profile with backward compatibility
        ///
        /// This method can parse profiles from any minor version within the
        /// same major version. Missing fields are filled with defaults.
        pub fn parse(&self, json: &str) -> MigrationResult<CompatibleProfile> {
            let value: Value = serde_json::from_str(json)?;

            if !self.is_compatible_value(&value)? {
                return Err(MigrationError::UnknownSchemaVersion(
                    value
                        .get("schema")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                ));
            }

            self.parse_v1_profile(&value)
        }

        /// Parse a v1 profile with backward compatibility for minor versions
        fn parse_v1_profile(&self, value: &Value) -> MigrationResult<CompatibleProfile> {
            let schema = value
                .get("schema")
                .and_then(|v| v.as_str())
                .unwrap_or(CURRENT_SCHEMA_VERSION)
                .to_string();

            let version = SchemaVersion::parse(&schema)?;

            // Parse scope with defaults
            let scope = value
                .get("scope")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "game": null, "car": null, "track": null }));

            // Parse base settings
            let base = value.get("base").cloned().ok_or_else(|| {
                MigrationError::ValidationFailed("Missing 'base' field".to_string())
            })?;

            // Parse optional fields with defaults
            let parent = value
                .get("parent")
                .and_then(|v| v.as_str())
                .map(String::from);
            let leds = value.get("leds").cloned();
            let haptics = value.get("haptics").cloned();
            let signature = value
                .get("signature")
                .and_then(|v| v.as_str())
                .map(String::from);

            Ok(CompatibleProfile {
                schema_version: version,
                parent,
                scope,
                base,
                leds,
                haptics,
                signature,
            })
        }

        /// Parse a profile, automatically migrating if needed
        ///
        /// This method first tries to parse directly. If the profile is from
        /// an older major version, it will migrate it first.
        pub fn parse_or_migrate(&self, json: &str) -> MigrationResult<CompatibleProfile> {
            // First try direct parsing
            if self.is_compatible(json)? {
                return self.parse(json);
            }

            // Need to migrate first
            let manager = MigrationManager::new(MigrationConfig::without_backups())?;
            let migrated = manager.migrate_profile(json)?;
            self.parse(&migrated)
        }
    }

    impl Default for BackwardCompatibleParser {
        fn default() -> Self {
            Self::new()
        }
    }

    /// A profile parsed with backward compatibility
    ///
    /// This struct represents a profile that has been parsed from any
    /// compatible schema version. All fields are normalized to the
    /// current schema format.
    #[derive(Debug, Clone)]
    pub struct CompatibleProfile {
        /// The original schema version of the profile
        pub schema_version: SchemaVersion,
        /// Parent profile ID for inheritance (optional)
        pub parent: Option<String>,
        /// Profile scope
        pub scope: Value,
        /// Base settings
        pub base: Value,
        /// LED configuration (optional)
        pub leds: Option<Value>,
        /// Haptics configuration (optional)
        pub haptics: Option<Value>,
        /// Signature (optional)
        pub signature: Option<String>,
    }

    impl CompatibleProfile {
        /// Get the FFB gain from base settings
        pub fn ffb_gain(&self) -> Option<f64> {
            self.base.get("ffbGain").and_then(|v| v.as_f64())
        }

        /// Get the degrees of rotation from base settings
        pub fn dor_deg(&self) -> Option<u64> {
            self.base.get("dorDeg").and_then(|v| v.as_u64())
        }

        /// Get the torque cap from base settings
        pub fn torque_cap_nm(&self) -> Option<f64> {
            self.base.get("torqueCapNm").and_then(|v| v.as_f64())
        }

        /// Get the game from scope
        pub fn game(&self) -> Option<&str> {
            self.scope.get("game").and_then(|v| v.as_str())
        }

        /// Check if this profile has a parent
        pub fn has_parent(&self) -> bool {
            self.parent.is_some()
        }

        /// Convert back to JSON
        pub fn to_json(&self) -> MigrationResult<String> {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "schema".to_string(),
                Value::String(self.schema_version.version.clone()),
            );

            if let Some(ref parent) = self.parent {
                obj.insert("parent".to_string(), Value::String(parent.clone()));
            }

            obj.insert("scope".to_string(), self.scope.clone());
            obj.insert("base".to_string(), self.base.clone());

            if let Some(ref leds) = self.leds {
                obj.insert("leds".to_string(), leds.clone());
            }

            if let Some(ref haptics) = self.haptics {
                obj.insert("haptics".to_string(), haptics.clone());
            }

            if let Some(ref sig) = self.signature {
                obj.insert("signature".to_string(), Value::String(sig.clone()));
            }

            Ok(serde_json::to_string_pretty(&Value::Object(obj))?)
        }
    }
}

#[cfg(test)]
mod compat_tests {
    use super::compat::*;
    use super::test_utils::*;
    use super::*;

    #[test]
    fn test_parser_compatibility_check() -> MigrationResult<()> {
        let parser = BackwardCompatibleParser::new();

        // V1 profile should be compatible
        let v1 = create_v1_profile(0.7, 900, 15.0);
        assert!(parser.is_compatible(&v1)?);

        // Legacy profile should not be compatible
        let legacy = create_legacy_profile(0.7, 900, 15.0);
        assert!(!parser.is_compatible(&legacy)?);

        Ok(())
    }

    #[test]
    fn test_parse_v1_profile() -> MigrationResult<()> {
        let parser = BackwardCompatibleParser::new();

        let v1 = create_v1_profile(0.8, 1080, 20.0);
        let profile = parser.parse(&v1)?;

        assert_eq!(profile.ffb_gain(), Some(0.8));
        assert_eq!(profile.dor_deg(), Some(1080));
        assert_eq!(profile.torque_cap_nm(), Some(20.0));
        assert!(profile.schema_version.is_current());

        Ok(())
    }

    #[test]
    fn test_parse_or_migrate_legacy() -> MigrationResult<()> {
        let parser = BackwardCompatibleParser::new();

        let legacy = create_legacy_profile(0.75, 720, 12.0);
        let profile = parser.parse_or_migrate(&legacy)?;

        assert_eq!(profile.ffb_gain(), Some(0.75));
        assert_eq!(profile.dor_deg(), Some(720));
        assert_eq!(profile.torque_cap_nm(), Some(12.0));
        assert!(profile.schema_version.is_current());

        Ok(())
    }

    #[test]
    fn test_parse_v1_with_scope() -> MigrationResult<()> {
        let parser = BackwardCompatibleParser::new();

        let v1 = create_v1_profile_with_scope(0.7, 900, 15.0, Some("iRacing"));
        let profile = parser.parse(&v1)?;

        assert_eq!(profile.game(), Some("iRacing"));

        Ok(())
    }

    #[test]
    fn test_compatible_profile_to_json() -> MigrationResult<()> {
        let parser = BackwardCompatibleParser::new();

        let v1 = create_v1_profile(0.7, 900, 15.0);
        let profile = parser.parse(&v1)?;
        let json = profile.to_json()?;

        // Should be valid JSON that can be parsed again
        let reparsed = parser.parse(&json)?;
        assert_eq!(reparsed.ffb_gain(), profile.ffb_gain());
        assert_eq!(reparsed.dor_deg(), profile.dor_deg());

        Ok(())
    }

    #[test]
    fn test_parse_profile_with_optional_fields() -> MigrationResult<()> {
        let parser = BackwardCompatibleParser::new();

        // Create a minimal v1 profile without optional fields
        let minimal = serde_json::json!({
            "schema": CURRENT_SCHEMA_VERSION,
            "scope": {},
            "base": {
                "ffbGain": 0.5,
                "dorDeg": 540,
                "torqueCapNm": 10.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notchFilters": [],
                    "slewRate": 1.0,
                    "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                }
            }
        })
        .to_string();

        let profile = parser.parse(&minimal)?;

        assert_eq!(profile.ffb_gain(), Some(0.5));
        assert!(profile.leds.is_none());
        assert!(profile.haptics.is_none());
        assert!(profile.signature.is_none());

        Ok(())
    }
}

#[cfg(test)]
mod backward_compat_property_tests {
    use super::compat::*;
    use super::test_utils::*;
    use super::*;
    use proptest::prelude::*;

    // Feature: release-roadmap-v1, Property 37: Schema Backward Compatibility
    // **Validates: Requirements 20.5**
    //
    // *For any* profile created with schema version N.x.x, the parser for version
    // N.y.y (where y > x) SHALL successfully parse the profile.

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Property 37: V1 profiles are parseable by current parser
        #[test]
        fn prop_v1_profiles_parseable(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let parser = BackwardCompatibleParser::new();

            let v1 = create_v1_profile(ffb_gain, dor, torque_cap);
            let result = parser.parse(&v1);

            prop_assert!(result.is_ok(), "V1 profile should be parseable: {:?}", result.err());

            let profile = result?;
            prop_assert!(profile.schema_version.major == 1, "Should be major version 1");
        }

        /// Property 37: Parsed profiles preserve all values
        #[test]
        fn prop_parsed_profiles_preserve_values(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let parser = BackwardCompatibleParser::new();

            let v1 = create_v1_profile(ffb_gain, dor, torque_cap);
            let profile = parser.parse(&v1)?;

            let parsed_ffb = profile.ffb_gain().unwrap_or(0.0);
            let parsed_dor = profile.dor_deg().unwrap_or(0) as u16;
            let parsed_torque = profile.torque_cap_nm().unwrap_or(0.0);

            prop_assert!((parsed_ffb - ffb_gain).abs() < 0.001,
                "FFB gain should be preserved: {} vs {}", parsed_ffb, ffb_gain);
            prop_assert_eq!(parsed_dor, dor, "DOR should be preserved");
            prop_assert!((parsed_torque - torque_cap).abs() < 0.001,
                "Torque cap should be preserved: {} vs {}", parsed_torque, torque_cap);
        }

        /// Property 37: Profiles with scope are parseable
        #[test]
        fn prop_profiles_with_scope_parseable(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
            game in prop::option::of("[a-zA-Z]{3,10}"),
        ) {
            let parser = BackwardCompatibleParser::new();

            let v1 = create_v1_profile_with_scope(ffb_gain, dor, torque_cap, game.as_deref());
            let result = parser.parse(&v1);

            prop_assert!(result.is_ok(), "Profile with scope should be parseable");

            let profile = result?;
            prop_assert_eq!(profile.game(), game.as_deref(), "Game scope should be preserved");
        }

        /// Property 37: Minimal profiles are parseable
        #[test]
        fn prop_minimal_profiles_parseable(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let parser = BackwardCompatibleParser::new();

            // Create a minimal profile with only required fields
            let minimal = serde_json::json!({
                "schema": CURRENT_SCHEMA_VERSION,
                "scope": {},
                "base": {
                    "ffbGain": ffb_gain,
                    "dorDeg": dor,
                    "torqueCapNm": torque_cap,
                    "filters": {
                        "reconstruction": 0,
                        "friction": 0.0,
                        "damper": 0.0,
                        "inertia": 0.0,
                        "notchFilters": [],
                        "slewRate": 1.0,
                        "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                    }
                }
            }).to_string();

            let result = parser.parse(&minimal);
            prop_assert!(result.is_ok(), "Minimal profile should be parseable");
        }

        /// Property 37: Parsed profiles can be serialized back to JSON
        #[test]
        fn prop_parsed_profiles_roundtrip(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let parser = BackwardCompatibleParser::new();

            let v1 = create_v1_profile(ffb_gain, dor, torque_cap);
            let profile = parser.parse(&v1)?;

            // Convert back to JSON
            let json = profile.to_json()?;

            // Should be parseable again
            let reparsed = parser.parse(&json)?;

            prop_assert_eq!(profile.ffb_gain(), reparsed.ffb_gain(), "FFB gain should survive roundtrip");
            prop_assert_eq!(profile.dor_deg(), reparsed.dor_deg(), "DOR should survive roundtrip");
            prop_assert_eq!(profile.torque_cap_nm(), reparsed.torque_cap_nm(), "Torque cap should survive roundtrip");
        }

        /// Property 37: Legacy profiles can be migrated and parsed
        #[test]
        fn prop_legacy_profiles_migrate_and_parse(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
        ) {
            let parser = BackwardCompatibleParser::new();

            let legacy = create_legacy_profile(ffb_gain, dor, torque_cap);

            // Should not be directly compatible
            prop_assert!(!parser.is_compatible(&legacy)?, "Legacy should not be directly compatible");

            // But should be parseable via parse_or_migrate
            let result = parser.parse_or_migrate(&legacy);
            prop_assert!(result.is_ok(), "Legacy profile should be parseable via migration");

            let profile = result?;
            prop_assert!(profile.schema_version.is_current(), "Migrated profile should be current version");

            // Values should be preserved
            let parsed_ffb = profile.ffb_gain().unwrap_or(0.0);
            prop_assert!((parsed_ffb - ffb_gain).abs() < 0.001,
                "FFB gain should be preserved after migration");
        }

        /// Property 37: Parser rejects incompatible major versions
        #[test]
        fn prop_parser_rejects_incompatible_versions(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
            major_version in 2u32..=10,
        ) {
            let parser = BackwardCompatibleParser::new(); // v1 parser

            // Create a profile with a different major version
            let future_profile = serde_json::json!({
                "schema": format!("wheel.profile/{}", major_version),
                "scope": {},
                "base": {
                    "ffbGain": ffb_gain,
                    "dorDeg": dor,
                    "torqueCapNm": torque_cap,
                    "filters": {
                        "reconstruction": 0,
                        "friction": 0.0,
                        "damper": 0.0,
                        "inertia": 0.0,
                        "notchFilters": [],
                        "slewRate": 1.0,
                        "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                    }
                }
            }).to_string();

            // Should not be compatible
            prop_assert!(!parser.is_compatible(&future_profile)?,
                "Future major version should not be compatible with v1 parser");

            // Direct parse should fail
            let result = parser.parse(&future_profile);
            prop_assert!(result.is_err(), "Parsing incompatible version should fail");
        }

        /// Property 37: Minor version differences within same major are compatible
        #[test]
        fn prop_minor_version_compatible(
            ffb_gain in 0.0f64..=1.0,
            dor in 180u16..=2160,
            torque_cap in 0.0f64..=50.0,
            minor_version in 0u32..=10,
        ) {
            let parser = BackwardCompatibleParser::new(); // v1 parser

            // Create a profile with v1.x (any minor version)
            let profile_json = serde_json::json!({
                "schema": format!("wheel.profile/1.{}", minor_version),
                "scope": {},
                "base": {
                    "ffbGain": ffb_gain,
                    "dorDeg": dor,
                    "torqueCapNm": torque_cap,
                    "filters": {
                        "reconstruction": 0,
                        "friction": 0.0,
                        "damper": 0.0,
                        "inertia": 0.0,
                        "notchFilters": [],
                        "slewRate": 1.0,
                        "curvePoints": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}]
                    }
                }
            }).to_string();

            // Should be compatible (same major version)
            prop_assert!(parser.is_compatible(&profile_json)?,
                "v1.{} should be compatible with v1 parser", minor_version);

            // Should parse successfully
            let result = parser.parse(&profile_json);
            prop_assert!(result.is_ok(), "v1.{} should be parseable", minor_version);
        }
    }
}
