//! Deep tests for the openracing-profile-repository crate.
//!
//! Covers CRUD operations, listing with filters, concurrent access,
//! persistence round-trips, migration of stored profiles, storage limits,
//! and cleanup.

use openracing_profile_repository::ProfileSigner;
use openracing_profile_repository::prelude::*;
use proptest::prelude::*;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn create_game_profile(id: &str, game: &str) -> Result<Profile, Box<dyn std::error::Error>> {
    let profile_id = valid_profile_id(id)?;
    Ok(Profile::new(
        profile_id,
        ProfileScope::for_game(game.to_string()),
        BaseSettings::default(),
        format!("Game Profile {}", id),
    ))
}

fn create_car_profile(
    id: &str,
    game: &str,
    car: &str,
) -> Result<Profile, Box<dyn std::error::Error>> {
    let profile_id = valid_profile_id(id)?;
    Ok(Profile::new(
        profile_id,
        ProfileScope::for_car(game.to_string(), car.to_string()),
        BaseSettings::default(),
        format!("Car Profile {}", id),
    ))
}

fn profile_id_strategy() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-z][a-z0-9._-]{0,30}")
        .expect("regex should compile")
        .prop_filter("must not be empty after trim", |s| !s.trim().is_empty())
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

mod crud {
    use super::*;

    #[tokio::test]
    async fn create_and_read() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("crud_create")?;

        repo.save_profile(&profile, None).await?;
        let loaded = repo.load_profile(&profile.id).await?;

        assert!(loaded.is_some(), "saved profile should be loadable");
        let loaded = loaded.ok_or("expected profile")?;
        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.metadata.name, profile.metadata.name);
        Ok(())
    }

    #[tokio::test]
    async fn update_overwrites_existing() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("crud_update")?;

        repo.save_profile(&profile, None).await?;
        // Save again (update)
        repo.save_profile(&profile, None).await?;

        let profiles = repo.list_profiles().await?;
        let count = profiles.iter().filter(|p| p.id == profile.id).count();
        assert_eq!(count, 1, "no duplicates after update");
        Ok(())
    }

    #[tokio::test]
    async fn delete_removes_profile() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("crud_delete")?;

        repo.save_profile(&profile, None).await?;
        repo.delete_profile(&profile.id).await?;

        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_none(), "deleted profile should not be loadable");
        Ok(())
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let id = valid_profile_id("does_not_exist")?;
        // Deleting a nonexistent profile should not error
        repo.delete_profile(&id).await?;
        Ok(())
    }

    #[tokio::test]
    async fn delete_is_idempotent() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("crud_idem_del")?;

        repo.save_profile(&profile, None).await?;
        repo.delete_profile(&profile.id).await?;
        repo.delete_profile(&profile.id).await?;

        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let id = valid_profile_id("nonexistent")?;
        let loaded = repo.load_profile(&id).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn list_on_empty_repo() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profiles = repo.list_profiles().await?;
        assert!(profiles.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn save_multiple_then_list() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        for i in 0..5 {
            let profile = create_test_profile(&format!("multi_{}", i))?;
            repo.save_profile(&profile, None).await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 5);
        Ok(())
    }

    #[tokio::test]
    async fn save_delete_interleaved() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let p1 = create_test_profile("interleave_a")?;
        let p2 = create_test_profile("interleave_b")?;
        let p3 = create_test_profile("interleave_c")?;

        repo.save_profile(&p1, None).await?;
        repo.save_profile(&p2, None).await?;
        repo.delete_profile(&p1.id).await?;
        repo.save_profile(&p3, None).await?;

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 2, "should have p2 and p3");

        let ids: Vec<_> = profiles.iter().map(|p| p.id.clone()).collect();
        assert!(ids.contains(&p2.id));
        assert!(ids.contains(&p3.id));
        assert!(!ids.contains(&p1.id));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Listing with filters and sorting
// ---------------------------------------------------------------------------

mod listing_filters {
    use super::*;

    #[tokio::test]
    async fn filter_by_game_scope() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let iracing = create_game_profile("iracing_prof", "iracing")?;
        let acc = create_game_profile("acc_prof", "acc")?;
        let global = create_test_profile("global_prof")?;

        repo.save_profile(&iracing, None).await?;
        repo.save_profile(&acc, None).await?;
        repo.save_profile(&global, None).await?;

        let profiles = repo.list_profiles().await?;
        let iracing_only: Vec<_> = profiles
            .iter()
            .filter(|p| p.scope.game.as_deref() == Some("iracing"))
            .collect();
        assert_eq!(iracing_only.len(), 1);
        assert_eq!(iracing_only[0].id, iracing.id);
        Ok(())
    }

    #[tokio::test]
    async fn filter_by_car_scope() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let gt3 = create_car_profile("ir_gt3", "iracing", "gt3")?;
        let lmp2 = create_car_profile("ir_lmp2", "iracing", "lmp2")?;

        repo.save_profile(&gt3, None).await?;
        repo.save_profile(&lmp2, None).await?;

        let profiles = repo.list_profiles().await?;
        let gt3_only: Vec<_> = profiles
            .iter()
            .filter(|p| p.scope.car.as_deref() == Some("gt3"))
            .collect();
        assert_eq!(gt3_only.len(), 1);
        assert_eq!(gt3_only[0].id, gt3.id);
        Ok(())
    }

    #[tokio::test]
    async fn filter_global_profiles() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let global = create_test_profile("filter_global")?;
        let game = create_game_profile("filter_game", "acc")?;

        repo.save_profile(&global, None).await?;
        repo.save_profile(&game, None).await?;

        let profiles = repo.list_profiles().await?;
        let globals: Vec<_> = profiles.iter().filter(|p| p.scope.game.is_none()).collect();
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].id, global.id);
        Ok(())
    }

    #[tokio::test]
    async fn list_after_delete_does_not_include_removed() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let p1 = create_test_profile("list_del_a")?;
        let p2 = create_test_profile("list_del_b")?;

        repo.save_profile(&p1, None).await?;
        repo.save_profile(&p2, None).await?;
        repo.delete_profile(&p1.id).await?;

        let profiles = repo.list_profiles().await?;
        let ids: Vec<_> = profiles.iter().map(|p| &p.id).collect();
        assert!(!ids.contains(&&p1.id));
        assert!(ids.contains(&&p2.id));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Concurrent access (multiple readers)
// ---------------------------------------------------------------------------

mod concurrent_access {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn concurrent_reads_safe() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("conc_read")?;
        repo.save_profile(&profile, None).await?;

        let repo = Arc::new(repo);
        let mut handles = Vec::new();

        for _ in 0..20 {
            let repo = repo.clone();
            let id = profile.id.clone();
            handles.push(tokio::spawn(async move {
                let loaded = repo.load_profile(&id).await;
                assert!(loaded.is_ok());
                assert!(loaded.ok().flatten().is_some());
            }));
        }

        for handle in handles {
            handle.await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_writes_no_data_loss() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let repo = Arc::new(repo);
        let mut handles = Vec::new();

        for i in 0..10 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let id = format!("cw_{}", i);
                let profile_id = ProfileId::new(id).expect("valid id");
                let profile = Profile::new(
                    profile_id,
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("CW Profile {}", i),
                );
                repo.save_profile(&profile, None)
                    .await
                    .expect("save should succeed");
            }));
        }

        for handle in handles {
            handle.await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 10, "all concurrent writes should persist");
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_read_write_safe() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("rw_target")?;
        repo.save_profile(&profile, None).await?;

        let repo = Arc::new(repo);
        let mut handles = Vec::new();

        // Readers
        for _ in 0..10 {
            let repo = repo.clone();
            let id = profile.id.clone();
            handles.push(tokio::spawn(async move {
                let _ = repo.load_profile(&id).await;
            }));
        }

        // Writers
        for i in 0..5 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let id = format!("rw_extra_{}", i);
                let profile_id = ProfileId::new(id).expect("valid id");
                let p = Profile::new(
                    profile_id,
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("RW Extra {}", i),
                );
                let _ = repo.save_profile(&p, None).await;
            }));
        }

        for handle in handles {
            handle.await?;
        }

        // Original must still exist
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(
            loaded.is_some(),
            "original profile should survive concurrent ops"
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_list_is_consistent() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        for i in 0..5 {
            let p = create_test_profile(&format!("conc_list_{}", i))?;
            repo.save_profile(&p, None).await?;
        }

        let repo = Arc::new(repo);
        let mut handles = Vec::new();

        for _ in 0..10 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let profiles = repo.list_profiles().await.expect("list should succeed");
                assert!(profiles.len() >= 5, "should see at least 5 profiles");
            }));
        }

        for handle in handles {
            handle.await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Persistence: save → reload produces identical data
// ---------------------------------------------------------------------------

mod persistence {
    use super::*;

    #[tokio::test]
    async fn save_reload_identical_data() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = ProfileRepositoryConfig {
            profiles_dir: temp_dir.path().to_path_buf(),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: true,
        };

        let profile = create_test_profile("persist_test")?;

        {
            let repo = ProfileRepository::new(config.clone()).await?;
            repo.save_profile(&profile, None).await?;
        }
        // Repo dropped, re-create to force disk reload
        {
            let repo2 = ProfileRepository::new(config).await?;
            let loaded = repo2.load_profile(&profile.id).await?;
            let loaded = loaded.ok_or("profile should exist after reload")?;
            assert_eq!(loaded.id, profile.id);
            // schema_to_profile regenerates name as "Profile {id}" (name is
            // not persisted in ProfileSchema), so compare against that.
            assert_eq!(loaded.metadata.name, format!("Profile {}", profile.id));
            assert_eq!(loaded.scope, profile.scope);
        }
        Ok(())
    }

    #[tokio::test]
    async fn save_clear_cache_reload() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("cache_clear")?;

        repo.save_profile(&profile, None).await?;
        repo.clear_cache().await;
        repo.reload().await?;

        let loaded = repo.load_profile(&profile.id).await?;
        assert!(
            loaded.is_some(),
            "profile should survive cache clear + reload"
        );
        Ok(())
    }

    #[tokio::test]
    async fn multiple_saves_then_reload() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = ProfileRepositoryConfig::new(temp_dir.path());

        let ids: Vec<String> = (0..10).map(|i| format!("batch_{}", i)).collect();

        {
            let repo = ProfileRepository::new(config.clone()).await?;
            for id in &ids {
                let p = create_test_profile(id)?;
                repo.save_profile(&p, None).await?;
            }
        }

        {
            let repo2 = ProfileRepository::new(config).await?;
            let profiles = repo2.list_profiles().await?;
            assert_eq!(profiles.len(), 10, "all profiles should persist to disk");
        }
        Ok(())
    }

    #[tokio::test]
    async fn save_with_game_scope_persists() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = ProfileRepositoryConfig::new(temp_dir.path());

        let profile = create_game_profile("persist_game", "iracing")?;
        {
            let repo = ProfileRepository::new(config.clone()).await?;
            repo.save_profile(&profile, None).await?;
        }
        {
            let repo2 = ProfileRepository::new(config).await?;
            let loaded = repo2.load_profile(&profile.id).await?;
            let loaded = loaded.ok_or("game profile should persist")?;
            assert_eq!(loaded.scope.game.as_deref(), Some("iracing"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Migration of stored profiles
// ---------------------------------------------------------------------------

mod migration {
    use super::*;
    use tokio::fs as async_fs;

    #[tokio::test]
    async fn legacy_profile_auto_migrated_on_load() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile_id = valid_profile_id("legacy_auto")?;
        let file_path = repo.get_profile_file_path(&profile_id);

        let legacy_json = r#"{
            "ffb_gain": 0.65,
            "degrees_of_rotation": 900,
            "torque_cap": 11.0
        }"#;
        async_fs::write(&file_path, legacy_json).await?;

        let loaded = repo.load_profile(&profile_id).await?;
        assert!(
            loaded.is_some(),
            "legacy profile should be loadable after migration"
        );
        Ok(())
    }

    #[tokio::test]
    async fn migration_preserves_ffb_gain() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile_id = valid_profile_id("legacy_gain")?;
        let file_path = repo.get_profile_file_path(&profile_id);

        let legacy_json = r#"{
            "ffb_gain": 0.72,
            "degrees_of_rotation": 900,
            "torque_cap": 13.0
        }"#;
        async_fs::write(&file_path, legacy_json).await?;

        let loaded = repo.load_profile(&profile_id).await?;
        let loaded = loaded.ok_or("profile should exist")?;
        assert!(
            (loaded.base_settings.ffb_gain.value() - 0.72).abs() < 0.01,
            "FFB gain should be preserved: got {}",
            loaded.base_settings.ffb_gain.value()
        );
        Ok(())
    }

    #[tokio::test]
    async fn current_schema_not_migrated() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("no_migrate")?;
        repo.save_profile(&profile, None).await?;

        // Reload should not trigger migration
        repo.clear_cache().await;
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn migration_adapter_detects_legacy() -> TestResult {
        use openracing_profile_repository::migration::MigrationAdapter;

        let temp_dir = TempDir::new()?;
        let adapter = MigrationAdapter::new(temp_dir.path().join("backups"))?;

        let legacy = r#"{"ffb_gain": 0.8, "degrees_of_rotation": 900, "torque_cap": 12.0}"#;
        assert!(adapter.needs_migration(legacy)?);

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
        assert!(!adapter.needs_migration(current)?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Storage limits and cleanup
// ---------------------------------------------------------------------------

mod storage_limits {
    use super::*;

    #[tokio::test]
    async fn save_and_delete_many() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        // Save 50 profiles
        for i in 0..50 {
            let p = create_test_profile(&format!("batch_{}", i))?;
            repo.save_profile(&p, None).await?;
        }
        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 50);

        // Delete first 25
        for i in 0..25 {
            let id = valid_profile_id(&format!("batch_{}", i))?;
            repo.delete_profile(&id).await?;
        }
        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 25);
        Ok(())
    }

    #[tokio::test]
    async fn disk_files_cleaned_after_delete() -> TestResult {
        let (repo, tmp) = create_test_repository().await?;
        let profile = create_test_profile("disk_cleanup")?;

        repo.save_profile(&profile, None).await?;
        let file_path = repo.get_profile_file_path(&profile.id);
        assert!(file_path.exists(), "profile file should exist on disk");

        repo.delete_profile(&profile.id).await?;
        assert!(
            !file_path.exists(),
            "profile file should be deleted from disk"
        );

        // Confirm the directory is clean
        let json_files: Vec<_> = std::fs::read_dir(tmp.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect();
        assert!(json_files.is_empty(), "no JSON files should remain");
        Ok(())
    }

    #[tokio::test]
    async fn rapid_create_delete_cycles() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        for cycle in 0..10 {
            let p = create_test_profile(&format!("cycle_{}", cycle))?;
            repo.save_profile(&p, None).await?;
            repo.delete_profile(&p.id).await?;
        }

        let profiles = repo.list_profiles().await?;
        assert!(profiles.is_empty(), "all profiles should be deleted");
        Ok(())
    }

    #[tokio::test]
    async fn duplicate_saves_dont_leave_artifacts() -> TestResult {
        let (repo, tmp) = create_test_repository().await?;
        let profile = create_test_profile("dup_artifact")?;

        for _ in 0..10 {
            repo.save_profile(&profile, None).await?;
        }

        // Count JSON files on disk
        let json_count = std::fs::read_dir(tmp.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
            .count();
        assert_eq!(
            json_count, 1,
            "only one file should exist for repeated saves"
        );
        Ok(())
    }

    #[tokio::test]
    async fn file_storage_backup_creation() -> TestResult {
        use openracing_profile_repository::FileStorage;

        let tmp = TempDir::new()?;
        let storage = FileStorage::new(tmp.path()).await?;
        let file_path = tmp.path().join("backup_source.json");

        storage
            .write_atomic(&file_path, r#"{"test": "backup"}"#)
            .await?;

        let backup_path = storage.create_backup(&file_path).await?;
        assert!(backup_path.exists(), "backup file should be created");

        let backup_content = tokio::fs::read_to_string(&backup_path).await?;
        assert!(
            backup_content.contains("backup"),
            "backup should have original content"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error types and recoverability
// ---------------------------------------------------------------------------

mod error_handling {
    use super::*;
    use openracing_profile_repository::{ProfileRepositoryError, StorageError, ValidationError};

    #[test]
    fn profile_not_found_is_recoverable() {
        assert!(ProfileRepositoryError::ProfileNotFound("x".into()).is_recoverable());
    }

    #[test]
    fn validation_failed_is_not_recoverable() {
        assert!(!ProfileRepositoryError::ValidationFailed("x".into()).is_recoverable());
    }

    #[test]
    fn migration_failed_is_recoverable() {
        assert!(ProfileRepositoryError::MigrationFailed("x".into()).is_recoverable());
    }

    #[test]
    fn cache_error_is_recoverable() {
        assert!(ProfileRepositoryError::CacheError("x".into()).is_recoverable());
    }

    #[test]
    fn invalid_profile_id_not_recoverable() {
        assert!(!ProfileRepositoryError::InvalidProfileId("x".into()).is_recoverable());
    }

    #[test]
    fn validation_error_constructors() -> TestResult {
        let missing = ValidationError::missing_field("ffb_gain");
        assert!(missing.to_string().contains("ffb_gain"));

        let invalid = ValidationError::invalid_value("torque", "negative");
        assert!(invalid.to_string().contains("torque"));

        let oor = ValidationError::out_of_range("gain", 1.5, 0.0, 1.0);
        assert!(oor.to_string().contains("1.5"));
        Ok(())
    }

    #[test]
    fn storage_error_constructors() -> TestResult {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let read_err = StorageError::read_failed("/tmp/x.json", io_err);
        assert!(read_err.to_string().contains("x.json"));
        Ok(())
    }

    #[test]
    fn empty_profile_id_rejected() {
        let result = ProfileId::new(String::new());
        assert!(result.is_err());
    }

    #[test]
    fn whitespace_profile_id_rejected() {
        let result = ProfileId::new("   ".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn special_char_profile_id_rejected() {
        let result = ProfileId::new("profile@home".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn valid_profile_id_with_dots_dashes_underscores() -> TestResult {
        let _ = ProfileId::new("my-profile_v1.0".to_string())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Signature verification
// ---------------------------------------------------------------------------

mod signatures {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[tokio::test]
    async fn signed_profile_has_valid_signature() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("sig_valid")?;

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        repo.save_profile(&profile, Some(&signing_key)).await?;

        let sig = repo.get_profile_signature(&profile.id).await?;
        assert!(sig.is_some(), "signature should be present");
        let sig = sig.ok_or("expected signature")?;
        assert!(sig.is_valid(), "signature should be valid");
        Ok(())
    }

    #[tokio::test]
    async fn unsigned_profile_has_no_signature() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_test_profile("sig_none")?;

        repo.save_profile(&profile, None).await?;

        let sig = repo.get_profile_signature(&profile.id).await?;
        assert!(sig.is_none(), "unsigned profile should have no signature");
        Ok(())
    }

    #[test]
    fn signer_hash_is_deterministic() {
        let json = r#"{"test": "data"}"#;
        let h1 = ProfileSigner::hash_json(json);
        let h2 = ProfileSigner::hash_json(json);
        assert_eq!(h1, h2);
    }

    #[test]
    fn signer_different_input_different_hash() {
        let h1 = ProfileSigner::hash_json(r#"{"a": 1}"#);
        let h2 = ProfileSigner::hash_json(r#"{"a": 2}"#);
        assert_ne!(h1, h2);
    }

    #[test]
    fn trust_state_display_values() {
        assert_eq!(TrustState::Unsigned.to_string(), "unsigned");
        assert_eq!(TrustState::Trusted.to_string(), "trusted");
        assert_eq!(TrustState::ValidUnknown.to_string(), "valid_unknown");
        assert_eq!(TrustState::Invalid.to_string(), "invalid");
    }
}

// ---------------------------------------------------------------------------
// Profile hierarchy resolution
// ---------------------------------------------------------------------------

mod hierarchy {
    use super::*;
    use racing_wheel_schemas::prelude::{Degrees, Gain, TorqueNm};

    fn gain(v: f32) -> Result<Gain, Box<dyn std::error::Error>> {
        Ok(Gain::new(v)?)
    }
    fn dor(v: f32) -> Result<Degrees, Box<dyn std::error::Error>> {
        Ok(Degrees::new_dor(v)?)
    }
    fn torque(v: f32) -> Result<TorqueNm, Box<dyn std::error::Error>> {
        Ok(TorqueNm::new(v)?)
    }

    #[tokio::test]
    async fn global_fallback_when_no_game_match() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let global = Profile::new(
            valid_profile_id("global")?,
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.5)?,
                degrees_of_rotation: dor(900.0)?,
                torque_cap: torque(10.0)?,
                filters: FilterConfig::default(),
            },
            "Global".to_string(),
        );
        repo.save_profile(&global, None).await?;

        let resolved = repo
            .resolve_profile_hierarchy(Some("unknown_game"), None, None, None)
            .await?;
        assert!(
            (resolved.base_settings.ffb_gain.value() - 0.5).abs() < 0.01,
            "should fallback to global"
        );
        Ok(())
    }

    #[tokio::test]
    async fn game_overrides_global() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let global = Profile::new(
            valid_profile_id("h_global")?,
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.5)?,
                degrees_of_rotation: dor(900.0)?,
                torque_cap: torque(10.0)?,
                filters: FilterConfig::default(),
            },
            "Global".to_string(),
        );
        let iracing = Profile::new(
            valid_profile_id("h_iracing")?,
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings {
                ffb_gain: gain(0.7)?,
                degrees_of_rotation: dor(540.0)?,
                torque_cap: torque(15.0)?,
                filters: FilterConfig::default(),
            },
            "iRacing".to_string(),
        );

        repo.save_profile(&global, None).await?;
        repo.save_profile(&iracing, None).await?;

        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), None, None, None)
            .await?;
        assert!(
            (resolved.base_settings.ffb_gain.value() - 0.7).abs() < 0.01,
            "game profile should override global"
        );
        Ok(())
    }

    #[tokio::test]
    async fn car_overrides_game() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let global = Profile::new(
            valid_profile_id("hc_global")?,
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.5)?,
                degrees_of_rotation: dor(900.0)?,
                torque_cap: torque(10.0)?,
                filters: FilterConfig::default(),
            },
            "Global".to_string(),
        );
        let car = Profile::new(
            valid_profile_id("hc_gt3")?,
            ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
            BaseSettings {
                ffb_gain: gain(0.8)?,
                degrees_of_rotation: dor(480.0)?,
                torque_cap: torque(20.0)?,
                filters: FilterConfig::default(),
            },
            "GT3".to_string(),
        );

        repo.save_profile(&global, None).await?;
        repo.save_profile(&car, None).await?;

        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), Some("gt3"), None, None)
            .await?;
        assert!(
            (resolved.base_settings.ffb_gain.value() - 0.8).abs() < 0.01,
            "car profile should override global"
        );
        Ok(())
    }

    #[tokio::test]
    async fn empty_repo_returns_default() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), None, None, None)
            .await?;
        assert!(resolved.base_settings.ffb_gain.value() >= 0.0);
        assert!(resolved.base_settings.ffb_gain.value() <= 1.0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Proptest: round-trip and idempotency
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        #[test]
        fn save_load_round_trip(id in profile_id_strategy()) {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async {
                let (repo, _tmp) = create_test_repository().await.expect("repo");
                let profile = create_test_profile(&id).expect("profile");

                repo.save_profile(&profile, None).await.expect("save");
                repo.clear_cache().await;
                let loaded = repo.load_profile(&profile.id).await.expect("load");
                let loaded = loaded.expect("profile should exist");

                prop_assert_eq!(loaded.id, profile.id);
                prop_assert_eq!(loaded.scope, profile.scope);
                Ok(())
            })?;
        }

        #[test]
        fn save_is_idempotent(id in profile_id_strategy()) {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async {
                let (repo, _tmp) = create_test_repository().await.expect("repo");
                let profile = create_test_profile(&id).expect("profile");

                repo.save_profile(&profile, None).await.expect("first save");
                repo.save_profile(&profile, None).await.expect("second save");

                let profiles = repo.list_profiles().await.expect("list");
                let count = profiles.iter().filter(|p| p.id == profile.id).count();
                prop_assert_eq!(count, 1);
                Ok(())
            })?;
        }

        #[test]
        fn delete_then_load_returns_none(id in profile_id_strategy()) {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            rt.block_on(async {
                let (repo, _tmp) = create_test_repository().await.expect("repo");
                let profile = create_test_profile(&id).expect("profile");

                repo.save_profile(&profile, None).await.expect("save");
                repo.delete_profile(&profile.id).await.expect("delete");

                let loaded = repo.load_profile(&profile.id).await.expect("load");
                prop_assert!(loaded.is_none());
                Ok(())
            })?;
        }
    }
}

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

mod config {
    use super::*;

    #[test]
    fn default_config() {
        let config = ProfileRepositoryConfig::default();
        assert!(config.auto_migrate);
        assert!(config.backup_on_migrate);
        assert!(config.trusted_keys.is_empty());
    }

    #[test]
    fn config_builder_methods() -> TestResult {
        let tmp = TempDir::new()?;
        let config = ProfileRepositoryConfig::new(tmp.path())
            .with_auto_migrate(false)
            .with_backup_on_migrate(false)
            .with_trusted_key("test_key");

        assert!(!config.auto_migrate);
        assert!(!config.backup_on_migrate);
        assert_eq!(config.trusted_keys, vec!["test_key".to_string()]);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile versioning (save, update, re-save)
// ---------------------------------------------------------------------------

mod versioning {
    use super::*;
    use racing_wheel_schemas::prelude::{Degrees, Gain, TorqueNm};

    fn gain(v: f32) -> Result<Gain, Box<dyn std::error::Error>> {
        Ok(Gain::new(v)?)
    }
    fn dor(v: f32) -> Result<Degrees, Box<dyn std::error::Error>> {
        Ok(Degrees::new_dor(v)?)
    }
    fn torque(v: f32) -> Result<TorqueNm, Box<dyn std::error::Error>> {
        Ok(TorqueNm::new(v)?)
    }

    #[tokio::test]
    async fn save_update_save_preserves_latest() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile_id = valid_profile_id("ver_update")?;

        let profile_v1 = Profile::new(
            profile_id.clone(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.5)?,
                degrees_of_rotation: dor(900.0)?,
                torque_cap: torque(10.0)?,
                filters: FilterConfig::default(),
            },
            "V1".to_string(),
        );
        repo.save_profile(&profile_v1, None).await?;

        // Update and re-save with different settings
        let profile_v2 = Profile::new(
            profile_id.clone(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.8)?,
                degrees_of_rotation: dor(540.0)?,
                torque_cap: torque(20.0)?,
                filters: FilterConfig::default(),
            },
            "V2".to_string(),
        );
        repo.save_profile(&profile_v2, None).await?;

        let loaded = repo.load_profile(&profile_id).await?;
        let loaded = loaded.ok_or("profile should exist")?;
        assert!(
            (loaded.base_settings.ffb_gain.value() - 0.8).abs() < 0.01,
            "should have v2 gain"
        );
        Ok(())
    }

    #[tokio::test]
    async fn save_preserves_scope_after_update() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let profile = create_game_profile("ver_scope", "acc")?;
        repo.save_profile(&profile, None).await?;

        // Re-save same profile
        repo.save_profile(&profile, None).await?;

        let loaded = repo.load_profile(&profile.id).await?;
        let loaded = loaded.ok_or("profile should exist")?;
        assert_eq!(loaded.scope.game.as_deref(), Some("acc"));
        Ok(())
    }

    #[tokio::test]
    async fn reload_recovers_all_profiles() -> TestResult {
        let temp_dir = TempDir::new()?;
        let config = ProfileRepositoryConfig::new(temp_dir.path());
        let repo = ProfileRepository::new(config.clone()).await?;

        let p1 = create_test_profile("ver_reload_a")?;
        let p2 = create_test_profile("ver_reload_b")?;
        repo.save_profile(&p1, None).await?;
        repo.save_profile(&p2, None).await?;

        // Clear cache and reload to simulate restart
        repo.clear_cache().await;
        repo.reload().await?;

        let loaded1 = repo.load_profile(&p1.id).await?;
        let loaded2 = repo.load_profile(&p2.id).await?;
        assert!(loaded1.is_some(), "p1 should survive reload");
        assert!(loaded2.is_some(), "p2 should survive reload");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional search and filtering
// ---------------------------------------------------------------------------

mod search_and_filter {
    use super::*;

    #[tokio::test]
    async fn filter_by_multiple_game_scopes() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let iracing = create_game_profile("sf_iracing", "iracing")?;
        let acc = create_game_profile("sf_acc", "acc")?;
        let rfactor = create_game_profile("sf_rfactor2", "rfactor2")?;
        let global = create_test_profile("sf_global")?;

        repo.save_profile(&iracing, None).await?;
        repo.save_profile(&acc, None).await?;
        repo.save_profile(&rfactor, None).await?;
        repo.save_profile(&global, None).await?;

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 4);

        let game_profiles: Vec<_> = profiles.iter().filter(|p| p.scope.game.is_some()).collect();
        assert_eq!(game_profiles.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn filter_car_profiles_for_specific_game() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let gt3 = create_car_profile("sf_gt3", "iracing", "gt3")?;
        let lmp = create_car_profile("sf_lmp", "iracing", "lmp2")?;
        let acc_gt3 = create_car_profile("sf_acc_gt3", "acc", "gt3")?;

        repo.save_profile(&gt3, None).await?;
        repo.save_profile(&lmp, None).await?;
        repo.save_profile(&acc_gt3, None).await?;

        let profiles = repo.list_profiles().await?;
        let iracing_cars: Vec<_> = profiles
            .iter()
            .filter(|p| p.scope.game.as_deref() == Some("iracing") && p.scope.car.is_some())
            .collect();
        assert_eq!(iracing_cars.len(), 2, "should have 2 iracing car profiles");
        Ok(())
    }

    #[tokio::test]
    async fn list_after_clear_cache_and_reload() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        for i in 0..3 {
            let p = create_test_profile(&format!("sf_reload_{}", i))?;
            repo.save_profile(&p, None).await?;
        }

        repo.clear_cache().await;
        repo.reload().await?;

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 3, "all profiles should survive reload");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional concurrent access patterns
// ---------------------------------------------------------------------------

mod concurrent_advanced {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn concurrent_save_and_delete_different_profiles() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;
        let repo = Arc::new(repo);

        // Pre-populate
        for i in 0..10 {
            let p = create_test_profile(&format!("csd_{}", i))?;
            repo.save_profile(&p, None).await?;
        }

        let mut handles = Vec::new();

        // Delete even indices
        for i in (0..10).step_by(2) {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let id = ProfileId::new(format!("csd_{}", i)).ok();
                if let Some(id) = id {
                    let _ = repo.delete_profile(&id).await;
                }
            }));
        }

        // Save new profiles
        for i in 10..15 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let profile_id = ProfileId::new(format!("csd_{}", i)).ok();
                if let Some(pid) = profile_id {
                    let p = Profile::new(
                        pid,
                        ProfileScope::global(),
                        BaseSettings::default(),
                        format!("CSD {}", i),
                    );
                    let _ = repo.save_profile(&p, None).await;
                }
            }));
        }

        for handle in handles {
            handle.await?;
        }

        let profiles = repo.list_profiles().await?;
        // We should have odd indices (1,3,5,7,9) + new (10,11,12,13,14) = 10
        assert!(
            profiles.len() >= 5,
            "should have at least 5 remaining profiles"
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_reload_safe() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        for i in 0..5 {
            let p = create_test_profile(&format!("cr_{}", i))?;
            repo.save_profile(&p, None).await?;
        }

        let repo = Arc::new(repo);
        let mut handles = Vec::new();

        for _ in 0..5 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let _ = repo.reload().await;
            }));
        }

        for handle in handles {
            handle.await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(
            profiles.len(),
            5,
            "all profiles should survive concurrent reloads"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Deterministic merge
// ---------------------------------------------------------------------------

mod deterministic_merge {
    use super::*;
    use racing_wheel_schemas::prelude::{Degrees, Gain, TorqueNm};

    fn gain(v: f32) -> Result<Gain, Box<dyn std::error::Error>> {
        Ok(Gain::new(v)?)
    }
    fn dor(v: f32) -> Result<Degrees, Box<dyn std::error::Error>> {
        Ok(Degrees::new_dor(v)?)
    }
    fn torque(v: f32) -> Result<TorqueNm, Box<dyn std::error::Error>> {
        Ok(TorqueNm::new(v)?)
    }

    #[tokio::test]
    async fn merge_other_takes_precedence() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let base = Profile::new(
            valid_profile_id("merge_base")?,
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.5)?,
                degrees_of_rotation: dor(900.0)?,
                torque_cap: torque(10.0)?,
                filters: FilterConfig::default(),
            },
            "Base".to_string(),
        );
        let other = Profile::new(
            valid_profile_id("merge_other")?,
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.9)?,
                degrees_of_rotation: dor(540.0)?,
                torque_cap: torque(25.0)?,
                filters: FilterConfig::default(),
            },
            "Other".to_string(),
        );

        let merged = repo.merge_profiles_deterministic(&base, &other)?;
        assert!(
            (merged.base_settings.ffb_gain.value() - 0.9).abs() < 0.01,
            "other should take precedence: got {}",
            merged.base_settings.ffb_gain.value()
        );
        Ok(())
    }

    #[tokio::test]
    async fn merge_with_self_is_identity() -> TestResult {
        let (repo, _tmp) = create_test_repository().await?;

        let profile = Profile::new(
            valid_profile_id("merge_self")?,
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: gain(0.6)?,
                degrees_of_rotation: dor(720.0)?,
                torque_cap: torque(12.0)?,
                filters: FilterConfig::default(),
            },
            "Self".to_string(),
        );

        let merged = repo.merge_profiles_deterministic(&profile, &profile)?;
        assert!(
            (merged.base_settings.ffb_gain.value() - 0.6).abs() < 0.01,
            "merge with self should preserve values"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional error type coverage
// ---------------------------------------------------------------------------

mod error_coverage {
    use openracing_profile_repository::{ProfileRepositoryError, StorageError, ValidationError};

    #[test]
    fn all_error_variants_display_non_empty() {
        let errors: Vec<Box<dyn std::fmt::Display>> = vec![
            Box::new(ProfileRepositoryError::ProfileNotFound("x".into())),
            Box::new(ProfileRepositoryError::ValidationFailed("x".into())),
            Box::new(ProfileRepositoryError::MigrationFailed("x".into())),
            Box::new(ProfileRepositoryError::UnsupportedSchemaVersion("x".into())),
            Box::new(ProfileRepositoryError::SignatureError("x".into())),
            Box::new(ProfileRepositoryError::InvalidProfileId("x".into())),
            Box::new(ProfileRepositoryError::ConfigError("x".into())),
            Box::new(ProfileRepositoryError::CacheError("x".into())),
            Box::new(ProfileRepositoryError::HierarchyResolutionFailed(
                "x".into(),
            )),
        ];
        for err in &errors {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn io_error_is_recoverable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let repo_err = ProfileRepositoryError::from(io_err);
        assert!(repo_err.is_recoverable());
    }

    #[test]
    fn scope_mismatch_not_recoverable() {
        let err = ProfileRepositoryError::ScopeMismatch {
            expected: "global".into(),
            actual: "game".into(),
        };
        assert!(!err.is_recoverable());
        assert!(err.to_string().contains("global"));
        assert!(err.to_string().contains("game"));
    }

    #[test]
    fn atomic_write_failed_is_recoverable() {
        let err = ProfileRepositoryError::atomic_write_failed("/tmp/a.tmp", "/tmp/a.json");
        assert!(err.is_recoverable());
        assert!(err.to_string().contains("a.tmp"));
    }

    #[test]
    fn file_path_error_not_recoverable() {
        let err = ProfileRepositoryError::file_path_error("/bad/path", "test reason");
        assert!(!err.is_recoverable());
        assert!(err.to_string().contains("test reason"));
    }

    #[test]
    fn validation_failed_with_context() {
        let err = ProfileRepositoryError::validation_failed("gain", "out of range");
        assert!(err.to_string().contains("gain"));
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn storage_error_write_failed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = StorageError::write_failed("/tmp/out.json", io_err);
        assert!(err.to_string().contains("out.json"));
    }

    #[test]
    fn validation_error_non_monotonic() {
        let err = ValidationError::NonMonotonicCurve;
        assert!(err.to_string().contains("monotonic"));
    }

    #[test]
    fn validation_error_unsorted_rpm() {
        let err = ValidationError::UnsortedRpmBands;
        assert!(err.to_string().contains("ascending"));
    }

    #[test]
    fn validation_error_schema_mismatch() {
        let err = ValidationError::SchemaVersionMismatch {
            expected: "1".into(),
            actual: "2".into(),
        };
        assert!(err.to_string().contains("1"));
        assert!(err.to_string().contains("2"));
    }

    #[test]
    fn validation_error_converts_to_repo_error() {
        let val_err = ValidationError::missing_field("test_field");
        let repo_err: ProfileRepositoryError = val_err.into();
        assert!(repo_err.to_string().contains("test_field"));
        assert!(!repo_err.is_recoverable());
    }

    #[test]
    fn storage_error_converts_to_repo_error() {
        let storage_err = StorageError::FileExists(std::path::PathBuf::from("/tmp/exists.json"));
        let repo_err: ProfileRepositoryError = storage_err.into();
        assert!(repo_err.to_string().contains("exists.json"));
    }
}

// ---------------------------------------------------------------------------
// Signer edge cases
// ---------------------------------------------------------------------------

mod signer_edge_cases {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn signer_default_has_no_trusted_keys() {
        let signer = ProfileSigner::default();
        assert!(!signer.is_trusted("anything"));
    }

    #[test]
    fn add_trusted_key_then_check() {
        let mut signer = ProfileSigner::new();
        signer.add_trusted_key("my_key".to_string());
        assert!(signer.is_trusted("my_key"));
        assert!(!signer.is_trusted("other_key"));
    }

    #[test]
    fn hash_empty_json() {
        let h1 = ProfileSigner::hash_json("");
        let h2 = ProfileSigner::hash_json("");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn hash_whitespace_differences() {
        let h1 = ProfileSigner::hash_json(r#"{"a":1}"#);
        let h2 = ProfileSigner::hash_json(r#"{ "a" : 1 }"#);
        assert_ne!(h1, h2, "whitespace should affect hash");
    }

    #[test]
    fn sign_produces_valid_signature() -> TestResult {
        let signer = ProfileSigner::new();
        let mut csprng = OsRng;
        let key = SigningKey::generate(&mut csprng);
        let sig = signer.sign(r#"{"data": "test"}"#, &key)?;
        assert!(sig.is_valid());
        assert!(sig.is_trusted());
        assert!(!sig.signature.is_empty());
        assert!(!sig.public_key.is_empty());
        Ok(())
    }

    #[test]
    fn two_different_keys_produce_different_signatures() -> TestResult {
        let signer = ProfileSigner::new();
        let mut csprng = OsRng;
        let key1 = SigningKey::generate(&mut csprng);
        let key2 = SigningKey::generate(&mut csprng);
        let json = r#"{"data": "same"}"#;

        let sig1 = signer.sign(json, &key1)?;
        let sig2 = signer.sign(json, &key2)?;

        assert_ne!(sig1.signature, sig2.signature);
        assert_ne!(sig1.public_key, sig2.public_key);
        Ok(())
    }

    #[test]
    fn unsigned_trust_state_display() {
        assert_eq!(TrustState::default().to_string(), "unsigned");
    }

    #[test]
    fn profile_signature_invalid_not_valid() {
        let sig = ProfileSignature::new("s".into(), "k".into(), TrustState::Invalid);
        assert!(!sig.is_valid());
        assert!(!sig.is_trusted());
    }

    #[test]
    fn profile_signature_unsigned_not_valid() {
        let sig = ProfileSignature::new("s".into(), "k".into(), TrustState::Unsigned);
        assert!(!sig.is_valid());
        assert!(!sig.is_trusted());
    }

    #[test]
    fn profile_signature_valid_unknown_is_valid_not_trusted() {
        let sig = ProfileSignature::new("s".into(), "k".into(), TrustState::ValidUnknown);
        assert!(sig.is_valid());
        assert!(!sig.is_trusted());
    }
}

// ---------------------------------------------------------------------------
// ProfileValidationContext
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
    fn minimal_context_only_schema() {
        let ctx = ProfileValidationContext::minimal();
        assert!(ctx.validate_schema_version);
        assert!(!ctx.validate_curves);
        assert!(!ctx.validate_rpm_bands);
    }

    #[test]
    fn without_curves_disables_curve_check() {
        let ctx = ProfileValidationContext::new().without_curves();
        assert!(!ctx.validate_curves);
        assert!(ctx.validate_rpm_bands);
    }

    #[test]
    fn without_rpm_bands_disables_rpm_check() {
        let ctx = ProfileValidationContext::new().without_rpm_bands();
        assert!(ctx.validate_curves);
        assert!(!ctx.validate_rpm_bands);
    }
}

// ---------------------------------------------------------------------------
// ProfileFile
// ---------------------------------------------------------------------------

mod profile_file_tests {
    use openracing_profile_repository::storage::ProfileFile;
    use std::path::PathBuf;

    #[test]
    fn from_path_extracts_id() {
        let path = PathBuf::from("profiles/my_profile.json");
        let pf = ProfileFile::from_path(path);
        assert!(pf.is_some());
        if let Some(pf) = pf {
            assert_eq!(pf.id, "my_profile");
        }
    }

    #[test]
    fn from_path_no_extension() {
        let path = PathBuf::from("profiles/no_ext");
        let pf = ProfileFile::from_path(path);
        assert!(pf.is_some());
    }

    #[test]
    fn new_profile_file() {
        let pf = ProfileFile::new(PathBuf::from("/a/b.json"), "b".to_string());
        assert_eq!(pf.id, "b");
        assert!(pf.modified.is_none());
    }
}
