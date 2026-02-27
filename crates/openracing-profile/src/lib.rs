//! Profile types and serialization
//!
//! This crate provides profile definitions for racing wheel configurations.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod types;
pub mod validation;

pub use types::*;
pub use validation::*;

use thiserror::Error;
use uuid::Uuid;

/// Current profile schema version.
/// Increment this when the `WheelProfile` structure changes incompatibly.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Error, Debug)]
pub enum ProfileError {
    #[error("Invalid profile: {0}")]
    InvalidProfile(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Profile not found: {0}")]
    NotFound(String),

    #[error("Unsupported schema version {0}: maximum supported is {1}")]
    UnsupportedVersion(u32, u32),
}

pub type ProfileResult<T> = Result<T, ProfileError>;

pub fn generate_profile_id() -> String {
    Uuid::new_v4().to_string()
}

/// Migrate a profile to the current schema version.
///
/// Returns `Ok(true)` if migration was performed, `Ok(false)` if the profile
/// was already at the current version.
///
/// # Errors
///
/// Returns [`ProfileError::UnsupportedVersion`] if `profile.schema_version`
/// is greater than [`CURRENT_SCHEMA_VERSION`] (future profile from a newer
/// release).
pub fn migrate_profile(profile: &mut WheelProfile) -> ProfileResult<bool> {
    if profile.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(ProfileError::UnsupportedVersion(
            profile.schema_version,
            CURRENT_SCHEMA_VERSION,
        ));
    }
    if profile.schema_version == CURRENT_SCHEMA_VERSION {
        return Ok(false);
    }

    // Migration v0 → v1: no structural changes; just set the version field.
    // Add additional `if profile.schema_version < N` blocks for future versions.
    profile.schema_version = CURRENT_SCHEMA_VERSION;
    Ok(true)
}

/// Persist a serialized copy of a profile to `backup_path`.
///
/// The caller is responsible for serializing the profile (e.g. via
/// `serde_json::to_string`). This function merely writes the bytes atomically
/// so the original file is never partially overwritten.
///
/// # Errors
///
/// Returns [`ProfileError::SerializationError`] if the file cannot be written.
pub fn backup_profile(serialized: &str, backup_path: &std::path::Path) -> ProfileResult<()> {
    std::fs::write(backup_path, serialized.as_bytes())
        .map_err(|e| ProfileError::SerializationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_profile_id() -> Result<(), Box<dyn std::error::Error>> {
        let id = generate_profile_id();
        assert!(!id.is_empty());
        Uuid::parse_str(&id)?;
        Ok(())
    }

    #[test]
    fn test_new_profile_has_current_schema_version() {
        let profile = WheelProfile::new("Test", "device-1");
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_migrate_profile_already_current() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = WheelProfile::new("Test", "device-1");
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        let migrated = migrate_profile(&mut profile)?;
        assert!(!migrated, "already-current profile must not be marked as migrated");
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        Ok(())
    }

    #[test]
    fn test_migrate_profile_from_v0() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = WheelProfile::new("Test", "device-1");
        profile.schema_version = 0; // simulate a pre-versioned profile

        let migrated = migrate_profile(&mut profile)?;
        assert!(migrated, "v0 profile must be migrated");
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        Ok(())
    }

    #[test]
    fn test_migrate_profile_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        let mut profile = WheelProfile::new("Test", "device-1");
        profile.schema_version = 0;

        migrate_profile(&mut profile)?;
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        // Running again must be a no-op
        let migrated_again = migrate_profile(&mut profile)?;
        assert!(!migrated_again, "second migration must be a no-op");
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        Ok(())
    }

    #[test]
    fn test_migrate_profile_invalid_version_returns_error() {
        let mut profile = WheelProfile::new("Test", "device-1");
        profile.schema_version = CURRENT_SCHEMA_VERSION + 1; // future version

        let result = migrate_profile(&mut profile);
        assert!(
            result.is_err(),
            "unsupported future version must return an error"
        );
        assert!(
            matches!(result, Err(ProfileError::UnsupportedVersion(_, _))),
            "error must be UnsupportedVersion"
        );
    }

    #[test]
    fn test_backup_profile_creates_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("profile_backup_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let backup_path = dir.join("profile.json.bak");

        let payload = r#"{"id":"test","schema_version":1}"#;
        backup_profile(payload, &backup_path)?;

        assert!(backup_path.exists(), "backup file must be created");
        let contents = std::fs::read_to_string(&backup_path)?;
        assert_eq!(contents, payload);

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn test_backup_and_then_migrate() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("profile_migrate_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let backup_path = dir.join("before_migration.json.bak");

        let mut profile = WheelProfile::new("Test", "device-1");
        profile.schema_version = 0;

        // Serialize old state as backup
        let old_state = format!(
            r#"{{"id":"{}","schema_version":0,"name":"Test"}}"#,
            profile.id
        );
        backup_profile(&old_state, &backup_path)?;
        assert!(backup_path.exists(), "backup must exist before migration");

        // Now migrate
        let migrated = migrate_profile(&mut profile)?;
        assert!(migrated);
        assert_eq!(profile.schema_version, CURRENT_SCHEMA_VERSION);

        // Backup still preserved
        let backup_contents = std::fs::read_to_string(&backup_path)?;
        assert!(
            backup_contents.contains("\"schema_version\":0"),
            "backup must contain pre-migration state"
        );

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}

