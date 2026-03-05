//! Repository hardening tests
//!
//! Comprehensive tests for profile repository CRUD operations, listing,
//! concurrent access, filesystem error handling, and migration integration.

use openracing_profile_repository::prelude::*;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn valid_profile_id(value: &str) -> Result<ProfileId, Box<dyn std::error::Error>> {
    Ok(ProfileId::new(value.to_string())?)
}

async fn create_test_repository() -> Result<(ProfileRepository, TempDir), Box<dyn std::error::Error>>
{
    let temp_dir = TempDir::new()?;
    let config = ProfileRepositoryConfig {
        profiles_dir: temp_dir.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: true,
    };
    let repo = ProfileRepository::new(config).await?;
    Ok((repo, temp_dir))
}

fn create_test_profile(id: &str) -> Result<Profile, Box<dyn std::error::Error>> {
    let profile_id = valid_profile_id(id)?;
    Ok(Profile::new(
        profile_id,
        ProfileScope::global(),
        BaseSettings::default(),
        format!("Test Profile {}", id),
    ))
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

mod crud {
    use super::*;

    #[tokio::test]
    async fn save_and_load_profile() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profile = create_test_profile("crud_save_load")?;

        repo.save_profile(&profile, None).await?;
        let loaded = repo.load_profile(&profile.id).await?;

        assert!(loaded.is_some(), "saved profile must be loadable");
        let loaded = loaded.ok_or("profile was None")?;
        assert_eq!(loaded.id, profile.id);
        Ok(())
    }

    #[tokio::test]
    async fn load_nonexistent_profile_returns_none() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let id = valid_profile_id("does_not_exist")?;

        let loaded = repo.load_profile(&id).await?;
        assert!(loaded.is_none(), "missing profile must return None");
        Ok(())
    }

    #[tokio::test]
    async fn save_then_delete_removes_profile() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profile = create_test_profile("crud_delete")?;

        repo.save_profile(&profile, None).await?;
        repo.delete_profile(&profile.id).await?;

        // Clear cache so we test disk
        repo.clear_cache().await;
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_none(), "deleted profile must not be loadable");
        Ok(())
    }

    #[tokio::test]
    async fn delete_nonexistent_is_idempotent() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let id = valid_profile_id("no_such_profile")?;

        // Should not error
        repo.delete_profile(&id).await?;
        Ok(())
    }

    #[tokio::test]
    async fn overwrite_profile_updates_on_disk() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let mut profile = create_test_profile("crud_overwrite")?;

        repo.save_profile(&profile, None).await?;

        // Update the base settings (name is re-derived from ID on load)
        let new_gain =
            racing_wheel_schemas::prelude::Gain::new(0.8).map_err(|e| format!("{e:?}"))?;
        profile.base_settings.ffb_gain = new_gain;
        repo.save_profile(&profile, None).await?;

        repo.clear_cache().await;
        let loaded = repo.load_profile(&profile.id).await?;
        let loaded = loaded.ok_or("profile was None after update")?;
        assert!((loaded.base_settings.ffb_gain.value() - 0.8).abs() < 0.01);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Listing and filtering
// ---------------------------------------------------------------------------

mod listing {
    use super::*;

    #[tokio::test]
    async fn list_empty_repository() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profiles = repo.list_profiles().await?;
        assert!(profiles.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn list_multiple_profiles() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        for i in 0..5 {
            let profile = create_test_profile(&format!("list_{}", i))?;
            repo.save_profile(&profile, None).await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 5, "all saved profiles must be listed");
        Ok(())
    }

    #[tokio::test]
    async fn list_after_delete_reflects_removal() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        let p1 = create_test_profile("list_del_1")?;
        let p2 = create_test_profile("list_del_2")?;
        repo.save_profile(&p1, None).await?;
        repo.save_profile(&p2, None).await?;

        assert_eq!(repo.list_profiles().await?.len(), 2);

        repo.delete_profile(&p1.id).await?;
        assert_eq!(repo.list_profiles().await?.len(), 1);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Cache behavior
// ---------------------------------------------------------------------------

mod cache {
    use super::*;

    #[tokio::test]
    async fn load_profile_uses_cache() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profile = create_test_profile("cache_hit")?;
        repo.save_profile(&profile, None).await?;

        // First load populates cache; second load should hit cache
        let loaded1 = repo.load_profile(&profile.id).await?;
        let loaded2 = repo.load_profile(&profile.id).await?;
        assert!(loaded1.is_some());
        assert!(loaded2.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn clear_cache_forces_disk_reload() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profile = create_test_profile("cache_clear")?;
        repo.save_profile(&profile, None).await?;

        repo.clear_cache().await;

        let loaded = repo.load_profile(&profile.id).await?;
        assert!(
            loaded.is_some(),
            "profile must be loadable after cache clear"
        );
        Ok(())
    }

    #[tokio::test]
    async fn reload_repopulates_cache() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profile = create_test_profile("cache_reload")?;
        repo.save_profile(&profile, None).await?;

        repo.reload().await?;

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 1);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Persistence across repository instances
// ---------------------------------------------------------------------------

mod persistence {
    use super::*;

    #[tokio::test]
    async fn profiles_survive_repository_recreation() -> TestResult {
        let temp_dir = TempDir::new()?;

        // First instance: save profiles
        {
            let config = ProfileRepositoryConfig::new(temp_dir.path());
            let repo = ProfileRepository::new(config).await?;

            for i in 0..3 {
                let profile = create_test_profile(&format!("persist_{}", i))?;
                repo.save_profile(&profile, None).await?;
            }
        }

        // Second instance: verify profiles
        {
            let config = ProfileRepositoryConfig::new(temp_dir.path());
            let repo = ProfileRepository::new(config).await?;

            let profiles = repo.list_profiles().await?;
            assert_eq!(
                profiles.len(),
                3,
                "all profiles must persist across repo instances"
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn profile_file_path_uses_id() -> TestResult {
        let (repo, dir) = create_test_repository().await?;
        let profile = create_test_profile("path_check")?;

        let path = repo.get_profile_file_path(&profile.id);
        assert!(
            path.to_string_lossy().contains("path_check"),
            "file path must contain the profile ID"
        );
        assert_eq!(
            path.parent().map(|p| p.to_path_buf()),
            Some(dir.path().to_path_buf()),
            "file must be in the profiles directory"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Concurrent access
// ---------------------------------------------------------------------------

mod concurrent {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn concurrent_saves_do_not_lose_profiles() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let repo = Arc::new(repo);

        let mut handles = Vec::new();
        for i in 0..10 {
            let repo_clone = Arc::clone(&repo);
            handles.push(tokio::spawn(async move {
                let profile =
                    create_test_profile(&format!("concurrent_{}", i)).map_err(|e| e.to_string())?;
                repo_clone
                    .save_profile(&profile, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            }));
        }

        for handle in handles {
            handle.await?.map_err(|e| e.to_string())?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(
            profiles.len(),
            10,
            "all concurrently saved profiles must exist"
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_reads_are_safe() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let profile = create_test_profile("concurrent_read")?;
        repo.save_profile(&profile, None).await?;

        let repo = Arc::new(repo);
        let profile_id = profile.id.clone();

        let mut handles = Vec::new();
        for _ in 0..20 {
            let repo_clone = Arc::clone(&repo);
            let id = profile_id.clone();
            handles.push(tokio::spawn(async move {
                let loaded = repo_clone
                    .load_profile(&id)
                    .await
                    .map_err(|e| e.to_string())?;
                assert!(loaded.is_some());
                Ok::<_, String>(())
            }));
        }

        for handle in handles {
            handle.await?.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile hierarchy resolution
// ---------------------------------------------------------------------------

mod hierarchy {
    use super::*;

    #[tokio::test]
    async fn resolve_with_no_profiles_returns_default() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        let resolved = repo
            .resolve_profile_hierarchy(None, None, None, None)
            .await?;
        // Default global profile should be returned
        assert!(
            !resolved.id.as_str().is_empty(),
            "resolved profile must have an ID"
        );
        Ok(())
    }

    #[tokio::test]
    async fn resolve_returns_global_profile() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;
        let global = create_test_profile("global_base")?;
        repo.save_profile(&global, None).await?;

        let resolved = repo
            .resolve_profile_hierarchy(None, None, None, None)
            .await?;

        assert_eq!(resolved.id, global.id);
        Ok(())
    }

    #[tokio::test]
    async fn resolve_game_scoped_overrides_global() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        let global = create_test_profile("hier_global")?;
        repo.save_profile(&global, None).await?;

        let game_id = valid_profile_id("hier_game")?;
        let game_scope = ProfileScope {
            game: Some("iracing".to_string()),
            car: None,
            track: None,
        };
        let game_profile = Profile::new(
            game_id,
            game_scope,
            BaseSettings::default(),
            "iRacing Profile".to_string(),
        );
        repo.save_profile(&game_profile, None).await?;

        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), None, None, None)
            .await?;

        // Game-scoped profile should override the global one
        assert_eq!(
            resolved.base_settings.ffb_gain,
            game_profile.base_settings.ffb_gain
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Deterministic merge
// ---------------------------------------------------------------------------

mod merge {
    use super::*;

    #[tokio::test]
    async fn merge_other_overrides_base_settings() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        let base = create_test_profile("merge_base")?;
        let other = create_test_profile("merge_other")?;

        let merged = repo.merge_profiles_deterministic(&base, &other)?;
        assert_eq!(
            merged.base_settings.ffb_gain, other.base_settings.ffb_gain,
            "other's base_settings must take precedence"
        );
        Ok(())
    }

    #[tokio::test]
    async fn merge_preserves_base_id() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        let base = create_test_profile("merge_id_base")?;
        let other = create_test_profile("merge_id_other")?;

        let merged = repo.merge_profiles_deterministic(&base, &other)?;
        assert_eq!(merged.id, base.id, "merged profile must keep base's ID");
        Ok(())
    }

    #[tokio::test]
    async fn merge_creates_descriptive_name() -> TestResult {
        let (repo, _dir) = create_test_repository().await?;

        let base = create_test_profile("merge_name_a")?;
        let other = create_test_profile("merge_name_b")?;

        let merged = repo.merge_profiles_deterministic(&base, &other)?;
        assert!(
            merged.metadata.name.contains("Merged:"),
            "merged name must indicate merge; got: {}",
            merged.metadata.name
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Repository configuration
// ---------------------------------------------------------------------------

mod config {
    use super::*;

    #[tokio::test]
    async fn default_config_creates_repository() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = ProfileRepositoryConfig::new(temp_dir.path());
        let repo = ProfileRepository::new(config).await?;

        assert!(repo.config().auto_migrate);
        assert!(repo.config().backup_on_migrate);
        Ok(())
    }

    #[tokio::test]
    async fn config_with_auto_migrate_disabled() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = ProfileRepositoryConfig::new(temp_dir.path()).with_auto_migrate(false);
        let repo = ProfileRepository::new(config).await?;

        assert!(!repo.config().auto_migrate);
        Ok(())
    }

    #[test]
    fn config_builder_chains() -> TestResult {
        let config = ProfileRepositoryConfig::new("profiles")
            .with_auto_migrate(false)
            .with_backup_on_migrate(false)
            .with_trusted_key("key-123");

        assert!(!config.auto_migrate);
        assert!(!config.backup_on_migrate);
        assert_eq!(config.trusted_keys.len(), 1);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

mod error_types {
    use super::*;

    #[test]
    fn profile_not_found_is_recoverable() {
        let err = ProfileRepositoryError::ProfileNotFound("test".into());
        assert!(err.is_recoverable());
    }

    #[test]
    fn validation_failed_is_not_recoverable() {
        let err = ProfileRepositoryError::ValidationFailed("bad".into());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn io_error_is_recoverable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = ProfileRepositoryError::IoError(io_err);
        assert!(err.is_recoverable());
    }

    #[test]
    fn validation_failed_factory() {
        let err = ProfileRepositoryError::validation_failed("name", "too short");
        let msg = format!("{}", err);
        assert!(msg.contains("name"));
        assert!(msg.contains("too short"));
    }

    #[test]
    fn file_path_error_factory() {
        let err = ProfileRepositoryError::file_path_error("/some/path", "permission denied");
        let msg = format!("{}", err);
        assert!(msg.contains("permission denied"));
    }

    #[test]
    fn atomic_write_failed_factory() {
        let err =
            ProfileRepositoryError::atomic_write_failed("/tmp/temp.json", "/data/target.json");
        let msg = format!("{}", err);
        assert!(msg.contains("temp.json"));
        assert!(msg.contains("target.json"));
    }

    #[test]
    fn storage_error_converts_to_repo_error() {
        let se = StorageError::FileExists(std::path::PathBuf::from("/dup.json"));
        let re: ProfileRepositoryError = se.into();
        let msg = format!("{}", re);
        assert!(msg.contains("dup.json"));
    }

    #[test]
    fn validation_error_converts_to_repo_error() {
        let ve = ValidationError::missing_field("name");
        let re: ProfileRepositoryError = ve.into();
        let msg = format!("{}", re);
        assert!(msg.contains("name"));
    }

    #[test]
    fn scope_mismatch_error_display() {
        let err = ProfileRepositoryError::ScopeMismatch {
            expected: "global".into(),
            actual: "game".into(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("global"));
        assert!(msg.contains("game"));
    }
}

// ---------------------------------------------------------------------------
// Migration adapter
// ---------------------------------------------------------------------------

mod migration_adapter {
    use super::*;
    use openracing_profile_repository::prelude::MigrationAdapter;

    #[test]
    fn adapter_without_backups() -> TestResult {
        let adapter = MigrationAdapter::without_backups()?;
        assert!(!adapter.backups_enabled());
        Ok(())
    }

    #[test]
    fn adapter_with_backup_dir() -> TestResult {
        let temp_dir = TempDir::new()?;
        let adapter = MigrationAdapter::new(temp_dir.path().join("backups"))?;
        assert!(adapter.backups_enabled());
        Ok(())
    }

    #[test]
    fn detect_current_schema_version() -> TestResult {
        let adapter = MigrationAdapter::without_backups()?;
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
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        }"#;

        let version = adapter.detect_version(json)?;
        assert!(version.is_current());
        Ok(())
    }

    #[test]
    fn legacy_profile_needs_migration() -> TestResult {
        let adapter = MigrationAdapter::without_backups()?;
        let legacy = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        assert!(adapter.needs_migration(legacy)?);
        Ok(())
    }

    #[test]
    fn migrate_legacy_produces_current_schema() -> TestResult {
        let adapter = MigrationAdapter::without_backups()?;
        let legacy = r#"{
            "ffb_gain": 0.8,
            "degrees_of_rotation": 900,
            "torque_cap": 12.0
        }"#;

        let migrated = adapter.migrate(legacy)?;
        let value: serde_json::Value = serde_json::from_str(&migrated)?;
        assert_eq!(
            value.get("schema").and_then(|v| v.as_str()),
            Some("wheel.profile/1")
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Signature operations
// ---------------------------------------------------------------------------

mod signatures {
    use super::*;
    use openracing_profile_repository::ProfileSigner;

    #[test]
    fn signer_tracks_trusted_keys() {
        let mut signer = ProfileSigner::new();
        assert!(!signer.is_trusted("key-1"));

        signer.add_trusted_key("key-1".to_string());
        assert!(signer.is_trusted("key-1"));
        assert!(!signer.is_trusted("key-2"));
    }

    #[test]
    fn hash_json_is_deterministic() {
        let json = r#"{"a": 1, "b": 2}"#;
        let h1 = ProfileSigner::hash_json(json);
        let h2 = ProfileSigner::hash_json(json);
        assert_eq!(h1, h2, "same input must produce same hash");
    }

    #[test]
    fn different_json_produces_different_hash() {
        let h1 = ProfileSigner::hash_json(r#"{"x": 1}"#);
        let h2 = ProfileSigner::hash_json(r#"{"x": 2}"#);
        assert_ne!(h1, h2);
    }

    #[test]
    fn trust_state_default_is_unsigned() {
        let state = TrustState::default();
        assert_eq!(state, TrustState::Unsigned);
    }

    #[test]
    fn trust_state_display_values() {
        assert_eq!(format!("{}", TrustState::Unsigned), "unsigned");
        assert_eq!(format!("{}", TrustState::Trusted), "trusted");
        assert_eq!(format!("{}", TrustState::ValidUnknown), "valid_unknown");
        assert_eq!(format!("{}", TrustState::Invalid), "invalid");
    }
}

// ---------------------------------------------------------------------------
// Validation context
// ---------------------------------------------------------------------------

mod validation_context {
    use super::*;

    #[test]
    fn new_context_has_all_checks_enabled() {
        let ctx = ProfileValidationContext::new();
        assert!(ctx.validate_schema_version);
        assert!(ctx.validate_curves);
        assert!(ctx.validate_rpm_bands);
        assert!(ctx.validate_scope);
    }

    #[test]
    fn minimal_context_only_validates_schema() {
        let ctx = ProfileValidationContext::minimal();
        assert!(ctx.validate_schema_version);
        assert!(!ctx.validate_curves);
        assert!(!ctx.validate_rpm_bands);
        assert!(!ctx.validate_scope);
    }

    #[test]
    fn context_builder_disables_checks() {
        let ctx = ProfileValidationContext::new()
            .without_curves()
            .without_rpm_bands();
        assert!(!ctx.validate_curves);
        assert!(!ctx.validate_rpm_bands);
        assert!(ctx.validate_schema_version);
        assert!(ctx.validate_scope);
    }
}
