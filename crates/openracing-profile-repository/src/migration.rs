//! Profile migration integration

use crate::Result;
use crate::error::ProfileRepositoryError;
use racing_wheel_schemas::migration::{
    BackupInfo, MigrationConfig, MigrationOutcome, ProfileMigrationService, SchemaVersion,
};
use std::path::{Path, PathBuf};

/// Adapter for the profile migration service
pub struct MigrationAdapter {
    service: ProfileMigrationService,
    config: MigrationConfig,
}

impl MigrationAdapter {
    /// Create a new migration adapter
    pub fn new(backup_dir: impl Into<PathBuf>) -> Result<Self> {
        let config = MigrationConfig::new(backup_dir);
        let service = ProfileMigrationService::new(config.clone())
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))?;

        Ok(Self { service, config })
    }

    /// Create with custom configuration
    pub fn with_config(config: MigrationConfig) -> Result<Self> {
        let service = ProfileMigrationService::new(config.clone())
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))?;

        Ok(Self { service, config })
    }

    /// Create without backup support (for testing)
    pub fn without_backups() -> Result<Self> {
        let config = MigrationConfig::without_backups();
        let service = ProfileMigrationService::new(config.clone())
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))?;

        Ok(Self { service, config })
    }

    /// Check if a profile needs migration
    pub fn needs_migration(&self, json: &str) -> Result<bool> {
        self.service
            .needs_migration(json)
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))
    }

    /// Detect the schema version of a profile
    pub fn detect_version(&self, json: &str) -> Result<SchemaVersion> {
        self.service
            .detect_version(json)
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))
    }

    /// Migrate a profile JSON string
    pub fn migrate(&self, json: &str) -> Result<String> {
        self.service
            .migrate_with_backup(json, None)
            .map(|outcome| outcome.migrated_json)
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))
    }

    /// Migrate a profile with backup support
    pub fn migrate_with_backup(
        &self,
        json: &str,
        source_path: Option<&Path>,
    ) -> Result<MigrationOutcome> {
        self.service
            .migrate_with_backup(json, source_path)
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))
    }

    /// Migrate a file in place
    pub fn migrate_file(&self, path: &Path) -> Result<MigrationOutcome> {
        self.service
            .migrate_file(path)
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))
    }

    /// Restore from backup
    pub fn restore_backup(&self, backup_info: &BackupInfo) -> Result<String> {
        self.service
            .restore_from_backup(backup_info)
            .map_err(|e| ProfileRepositoryError::MigrationFailed(e.to_string()))
    }

    /// Get the backup directory
    pub fn backup_dir(&self) -> &Path {
        &self.config.backup_dir
    }

    /// Check if backups are enabled
    pub fn backups_enabled(&self) -> bool {
        self.config.create_backups
    }

    /// Get maximum number of backups
    pub fn max_backups(&self) -> usize {
        self.config.max_backups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_adapter() -> (MigrationAdapter, TempDir) {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let adapter = MigrationAdapter::new(temp_dir.path().join("backups"))
            .expect("adapter should be created");
        (adapter, temp_dir)
    }

    #[test]
    fn test_adapter_creation() {
        let (_adapter, _temp_dir) = create_test_adapter();
    }

    #[test]
    fn test_detect_current_version() {
        let (adapter, _temp_dir) = create_test_adapter();
        let json = r#"{
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

        let version = adapter.detect_version(json).expect("should detect version");
        assert!(version.is_current());
    }

    #[test]
    fn test_detect_legacy_version() {
        let (adapter, _temp_dir) = create_test_adapter();
        let legacy = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        let version = adapter
            .detect_version(legacy)
            .expect("should detect version");
        assert!(!version.is_current());
    }

    #[test]
    fn test_needs_migration() {
        let (adapter, _temp_dir) = create_test_adapter();

        let current = r#"{
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

        assert!(!adapter.needs_migration(current).expect("should check"));

        let legacy = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        assert!(adapter.needs_migration(legacy).expect("should check"));
    }

    #[test]
    fn test_migrate_legacy() {
        let (adapter, _temp_dir) = create_test_adapter();
        let legacy = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        let migrated = adapter.migrate(legacy).expect("should migrate");

        let value: serde_json::Value = serde_json::from_str(&migrated).expect("should parse");
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some("wheel.profile/1")
        );
    }
}
