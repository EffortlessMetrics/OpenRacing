//! Property-based and edge-case tests for the profile repository.
#![allow(clippy::redundant_closure)]

use openracing_profile_repository::ProfileSigner;
use openracing_profile_repository::prelude::*;
use proptest::prelude::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn must<T, E: std::fmt::Debug>(r: std::result::Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

fn valid_profile_id(value: &str) -> ProfileId {
    must(ProfileId::new(value.to_string()))
}

async fn create_test_repository() -> (ProfileRepository, TempDir) {
    let temp_dir = must(TempDir::new());
    let config = ProfileRepositoryConfig {
        profiles_dir: temp_dir.path().to_path_buf(),
        trusted_keys: Vec::new(),
        auto_migrate: true,
        backup_on_migrate: true,
    };
    let repo = must(ProfileRepository::new(config).await);
    (repo, temp_dir)
}

fn create_test_profile(id: &str) -> Profile {
    let profile_id = valid_profile_id(id);
    Profile::new(
        profile_id,
        ProfileScope::global(),
        BaseSettings::default(),
        format!("Test Profile {}", id),
    )
}

/// Generate a valid profile-id character set for proptest
fn profile_id_strategy() -> impl Strategy<Value = String> {
    // ProfileId allows alphanumeric, dash, dot, underscore; 1..64 chars
    proptest::string::string_regex("[a-z][a-z0-9._-]{0,30}")
        .expect("regex should compile")
        .prop_filter("must not be empty after trim", |s| !s.trim().is_empty())
}

// ---------------------------------------------------------------------------
// Repository CRUD operations
// ---------------------------------------------------------------------------

mod crud {
    use super::*;

    #[tokio::test]
    async fn save_then_load_returns_same_id() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profile = create_test_profile("crud_save_load");

        repo.save_profile(&profile, None).await?;
        let loaded = repo.load_profile(&profile.id).await?;

        assert!(loaded.is_some());
        let loaded = loaded.ok_or("profile should exist")?;
        assert_eq!(loaded.id, profile.id);
        Ok(())
    }

    #[tokio::test]
    async fn delete_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profile = create_test_profile("crud_idempotent_del");

        repo.save_profile(&profile, None).await?;
        repo.delete_profile(&profile.id).await?;
        // Deleting again should not error
        repo.delete_profile(&profile.id).await?;
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn save_overwrites_existing() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;

        let profile = create_test_profile("crud_overwrite");
        repo.save_profile(&profile, None).await?;

        // Save again (overwrite)
        repo.save_profile(&profile, None).await?;

        let profiles = repo.list_profiles().await?;
        let count = profiles.iter().filter(|p| p.id == profile.id).count();
        assert_eq!(count, 1, "duplicate entries should not appear");
        Ok(())
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let id = valid_profile_id("does_not_exist");
        let loaded = repo.load_profile(&id).await?;
        assert!(loaded.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn list_on_empty_repo() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profiles = repo.list_profiles().await?;
        assert!(profiles.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn cache_clear_and_reload() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;

        let profile = create_test_profile("cache_test");
        repo.save_profile(&profile, None).await?;

        repo.clear_cache().await;

        // After clearing cache, reload from disk
        repo.reload().await?;
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_some());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Profile search and filtering
// ---------------------------------------------------------------------------

mod search_and_filter {
    use super::*;

    #[tokio::test]
    async fn filter_by_game_scope() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;

        let iracing = Profile::new(
            valid_profile_id("iracing"),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "iRacing".to_string(),
        );
        let acc = Profile::new(
            valid_profile_id("acc"),
            ProfileScope::for_game("acc".to_string()),
            BaseSettings::default(),
            "ACC".to_string(),
        );

        repo.save_profile(&iracing, None).await?;
        repo.save_profile(&acc, None).await?;

        let profiles = repo.list_profiles().await?;
        let iracing_profiles: Vec<_> = profiles
            .iter()
            .filter(|p| p.scope.game.as_deref() == Some("iracing"))
            .collect();
        assert_eq!(iracing_profiles.len(), 1);
        assert_eq!(iracing_profiles[0].id, iracing.id);
        Ok(())
    }

    #[tokio::test]
    async fn filter_by_car_scope() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;

        let gt3 = Profile::new(
            valid_profile_id("iracing_gt3"),
            ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
            BaseSettings::default(),
            "GT3".to_string(),
        );
        let lmp2 = Profile::new(
            valid_profile_id("iracing_lmp2"),
            ProfileScope::for_car("iracing".to_string(), "lmp2".to_string()),
            BaseSettings::default(),
            "LMP2".to_string(),
        );

        repo.save_profile(&gt3, None).await?;
        repo.save_profile(&lmp2, None).await?;

        let profiles = repo.list_profiles().await?;
        let gt3_profiles: Vec<_> = profiles
            .iter()
            .filter(|p| p.scope.car.as_deref() == Some("gt3"))
            .collect();
        assert_eq!(gt3_profiles.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn hierarchy_resolution_with_no_profiles() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        // Should return a default global profile when nothing is saved
        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), None, None, None)
            .await?;
        // Just verify it returns something reasonable
        assert!(resolved.base_settings.ffb_gain.value() >= 0.0);
        assert!(resolved.base_settings.ffb_gain.value() <= 1.0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Proptest: idempotency and round-trip
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        #[test]
        fn save_is_idempotent(id in profile_id_strategy()) {
            let rt = tokio::runtime::Runtime::new().expect("runtime should be created");
            rt.block_on(async {
                let (repo, _tmp) = create_test_repository().await;
                let profile = create_test_profile(&id);

                // Save twice
                repo.save_profile(&profile, None).await.expect("first save");
                repo.save_profile(&profile, None).await.expect("second save");

                let profiles = repo.list_profiles().await.expect("list");
                let count = profiles.iter().filter(|p| p.id == profile.id).count();
                prop_assert_eq!(count, 1);
                Ok(())
            })?;
        }

        #[test]
        fn delete_then_load_is_none(id in profile_id_strategy()) {
            let rt = tokio::runtime::Runtime::new().expect("runtime should be created");
            rt.block_on(async {
                let (repo, _tmp) = create_test_repository().await;
                let profile = create_test_profile(&id);

                repo.save_profile(&profile, None).await.expect("save");
                repo.delete_profile(&profile.id).await.expect("delete");

                let loaded = repo.load_profile(&profile.id).await.expect("load");
                prop_assert!(loaded.is_none());
                Ok(())
            })?;
        }

        #[test]
        fn round_trip_preserves_scope(id in profile_id_strategy()) {
            let rt = tokio::runtime::Runtime::new().expect("runtime should be created");
            rt.block_on(async {
                let (repo, _tmp) = create_test_repository().await;
                let profile = create_test_profile(&id);

                repo.save_profile(&profile, None).await.expect("save");
                repo.clear_cache().await;
                let loaded = repo.load_profile(&profile.id).await.expect("load");
                let loaded = loaded.expect("profile should exist");

                prop_assert_eq!(loaded.scope, profile.scope);
                Ok(())
            })?;
        }
    }
}

// ---------------------------------------------------------------------------
// Concurrent access patterns
// ---------------------------------------------------------------------------

mod concurrent {
    use super::*;

    #[tokio::test]
    async fn concurrent_reads_are_safe() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profile = create_test_profile("concurrent_read");
        repo.save_profile(&profile, None).await?;

        let repo = std::sync::Arc::new(repo);
        let mut handles = Vec::new();

        for _ in 0..10 {
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
    async fn concurrent_writes_dont_lose_data() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let repo = std::sync::Arc::new(repo);
        let mut handles = Vec::new();

        for i in 0..5 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                let profile = create_test_profile(&format!("cw_{}", i));
                repo.save_profile(&profile, None)
                    .await
                    .expect("save should succeed");
            }));
        }

        for handle in handles {
            handle.await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 5, "all profiles should be persisted");
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_read_write_is_safe() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profile = create_test_profile("rw_target");
        repo.save_profile(&profile, None).await?;

        let repo = std::sync::Arc::new(repo);
        let mut handles = Vec::new();

        // Readers
        for _ in 0..5 {
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
                let p = create_test_profile(&format!("rw_extra_{}", i));
                let _ = repo.save_profile(&p, None).await;
            }));
        }

        for handle in handles {
            handle.await?;
        }

        // Original profile should still be readable
        let loaded = repo.load_profile(&profile.id).await?;
        assert!(loaded.is_some());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

mod edge_cases {
    use super::*;

    #[tokio::test]
    async fn empty_repo_list_returns_empty_vec() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profiles = repo.list_profiles().await?;
        assert!(profiles.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn duplicate_save_keeps_single_entry() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;
        let profile = create_test_profile("dup_test");

        for _ in 0..5 {
            repo.save_profile(&profile, None).await?;
        }

        let profiles = repo.list_profiles().await?;
        let matching = profiles.iter().filter(|p| p.id == profile.id).count();
        assert_eq!(matching, 1);
        Ok(())
    }

    #[test]
    fn very_long_profile_id_is_accepted() {
        // ProfileId has no length limit, only character validation
        let long_id = "a".repeat(200);
        let result = ProfileId::new(long_id);
        assert!(result.is_ok(), "long alphanumeric IDs should be accepted");
    }

    #[test]
    fn empty_string_profile_id_is_rejected() {
        let result = ProfileId::new(String::new());
        assert!(result.is_err(), "empty profile ID should be rejected");
    }

    #[test]
    fn whitespace_only_profile_id_is_rejected() {
        let result = ProfileId::new("   ".to_string());
        assert!(result.is_err(), "whitespace-only ID should be rejected");
    }

    #[test]
    fn special_chars_profile_id_is_rejected() {
        let result = ProfileId::new("profile@home".to_string());
        assert!(result.is_err(), "special chars should be rejected");
    }

    #[test]
    fn profile_id_with_spaces_is_rejected() {
        let result = ProfileId::new("my profile".to_string());
        assert!(result.is_err(), "spaces in profile ID should be rejected");
    }

    #[tokio::test]
    async fn save_and_delete_many_profiles() -> Result<(), Box<dyn std::error::Error>> {
        let (repo, _tmp) = create_test_repository().await;

        // Save many profiles
        for i in 0..20 {
            let p = create_test_profile(&format!("batch_{}", i));
            repo.save_profile(&p, None).await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 20);

        // Delete half
        for i in 0..10 {
            let id = valid_profile_id(&format!("batch_{}", i));
            repo.delete_profile(&id).await?;
        }

        let profiles = repo.list_profiles().await?;
        assert_eq!(profiles.len(), 10);
        Ok(())
    }

    #[tokio::test]
    async fn config_builder_methods() -> Result<(), Box<dyn std::error::Error>> {
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

    #[test]
    fn error_recoverability() {
        use openracing_profile_repository::ProfileRepositoryError;

        assert!(ProfileRepositoryError::ProfileNotFound("x".into()).is_recoverable());
        assert!(!ProfileRepositoryError::ValidationFailed("x".into()).is_recoverable());
        assert!(ProfileRepositoryError::MigrationFailed("x".into()).is_recoverable());
        assert!(!ProfileRepositoryError::UnsupportedSchemaVersion("x".into()).is_recoverable());
        assert!(ProfileRepositoryError::SignatureError("x".into()).is_recoverable());
        assert!(!ProfileRepositoryError::InvalidProfileId("x".into()).is_recoverable());
        assert!(!ProfileRepositoryError::ConfigError("x".into()).is_recoverable());
        assert!(ProfileRepositoryError::CacheError("x".into()).is_recoverable());
        assert!(ProfileRepositoryError::HierarchyResolutionFailed("x".into()).is_recoverable());
    }

    #[test]
    fn validation_error_constructors() {
        use openracing_profile_repository::ValidationError;

        let missing = ValidationError::missing_field("ffb_gain");
        assert!(missing.to_string().contains("ffb_gain"));

        let invalid = ValidationError::invalid_value("torque", "negative value");
        assert!(invalid.to_string().contains("torque"));
        assert!(invalid.to_string().contains("negative value"));

        let oor = ValidationError::out_of_range("gain", 1.5, 0.0, 1.0);
        assert!(oor.to_string().contains("1.5"));
        assert!(oor.to_string().contains("gain"));
    }

    #[test]
    fn storage_error_constructors() {
        use openracing_profile_repository::StorageError;

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let read_err = StorageError::read_failed("/tmp/x.json", io_err);
        assert!(read_err.to_string().contains("x.json"));

        let io_err2 = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no access");
        let write_err = StorageError::write_failed("/tmp/y.json", io_err2);
        assert!(write_err.to_string().contains("y.json"));
    }

    #[test]
    fn trust_state_display() {
        assert_eq!(TrustState::Unsigned.to_string(), "unsigned");
        assert_eq!(TrustState::Trusted.to_string(), "trusted");
        assert_eq!(TrustState::ValidUnknown.to_string(), "valid_unknown");
        assert_eq!(TrustState::Invalid.to_string(), "invalid");
    }

    #[test]
    fn profile_signature_validity_checks() {
        let trusted = ProfileSignature::new("s".into(), "k".into(), TrustState::Trusted);
        assert!(trusted.is_valid());
        assert!(trusted.is_trusted());

        let unknown = ProfileSignature::new("s".into(), "k".into(), TrustState::ValidUnknown);
        assert!(unknown.is_valid());
        assert!(!unknown.is_trusted());

        let invalid = ProfileSignature::new("s".into(), "k".into(), TrustState::Invalid);
        assert!(!invalid.is_valid());
        assert!(!invalid.is_trusted());

        let unsigned = ProfileSignature::new("s".into(), "k".into(), TrustState::Unsigned);
        assert!(!unsigned.is_valid());
        assert!(!unsigned.is_trusted());
    }

    #[test]
    fn profile_signer_trusted_key_management() {
        let mut signer = ProfileSigner::new();
        assert!(!signer.is_trusted("key1"));

        signer.add_trusted_key("key1".to_string());
        assert!(signer.is_trusted("key1"));
        assert!(!signer.is_trusted("key2"));
    }

    #[test]
    fn profile_signer_hash_deterministic() {
        let json = r#"{"key": "value"}"#;
        let hash1 = ProfileSigner::hash_json(json);
        let hash2 = ProfileSigner::hash_json(json);
        assert_eq!(hash1, hash2);

        let different = r#"{"key": "other"}"#;
        let hash3 = ProfileSigner::hash_json(different);
        assert_ne!(hash1, hash3);
    }
}
