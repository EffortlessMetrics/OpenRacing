//! Integration tests for profile repository

use openracing_profile_repository::prelude::*;
use tempfile::TempDir;

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

mod repository_lifecycle {
    use super::*;

    #[tokio::test]
    async fn test_full_lifecycle() {
        let (repo, temp_dir) = create_test_repository().await;

        let profile = create_test_profile("lifecycle_test");

        must(repo.save_profile(&profile, None).await);

        let loaded = must(repo.load_profile(&profile.id).await);
        assert!(loaded.is_some());

        repo.clear_cache().await;

        let loaded = must(repo.load_profile(&profile.id).await);
        assert!(loaded.is_some());

        drop(repo);

        let config = ProfileRepositoryConfig {
            profiles_dir: temp_dir.path().to_path_buf(),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: true,
        };
        let repo2 = must(ProfileRepository::new(config).await);

        let profiles = must(repo2.list_profiles().await);
        assert_eq!(profiles.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_profiles() {
        let (repo, _temp_dir) = create_test_repository().await;

        for i in 0..10 {
            let profile = create_test_profile(&format!("multi_{}", i));
            must(repo.save_profile(&profile, None).await);
        }

        let profiles = must(repo.list_profiles().await);
        assert_eq!(profiles.len(), 10);
    }
}

mod profile_hierarchy {
    use super::*;
    use racing_wheel_schemas::prelude::{Degrees, Gain, TorqueNm};

    fn valid_gain(value: f32) -> Gain {
        must(Gain::new(value))
    }

    fn valid_dor(value: f32) -> Degrees {
        must(Degrees::new_dor(value))
    }

    fn valid_torque(value: f32) -> TorqueNm {
        must(TorqueNm::new(value))
    }

    #[tokio::test]
    async fn test_global_fallback() {
        let (repo, _temp_dir) = create_test_repository().await;

        let global = Profile::new(
            valid_profile_id("global"),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: valid_gain(0.5),
                degrees_of_rotation: valid_dor(900.0),
                torque_cap: valid_torque(10.0),
                filters: FilterConfig::default(),
            },
            "Global".to_string(),
        );

        must(repo.save_profile(&global, None).await);

        let resolved = must(
            repo.resolve_profile_hierarchy(Some("unknown_game"), None, None, None)
                .await,
        );

        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.5);
    }

    #[tokio::test]
    async fn test_game_specific_override() {
        let (repo, _temp_dir) = create_test_repository().await;

        let global = Profile::new(
            valid_profile_id("global"),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: valid_gain(0.5),
                degrees_of_rotation: valid_dor(900.0),
                torque_cap: valid_torque(10.0),
                filters: FilterConfig::default(),
            },
            "Global".to_string(),
        );

        let iracing = Profile::new(
            valid_profile_id("iracing"),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings {
                ffb_gain: valid_gain(0.7),
                degrees_of_rotation: valid_dor(540.0),
                torque_cap: valid_torque(15.0),
                filters: FilterConfig::default(),
            },
            "iRacing".to_string(),
        );

        must(repo.save_profile(&global, None).await);
        must(repo.save_profile(&iracing, None).await);

        let resolved = must(
            repo.resolve_profile_hierarchy(Some("iracing"), None, None, None)
                .await,
        );

        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.7);
    }

    #[tokio::test]
    async fn test_car_specific_override() {
        let (repo, _temp_dir) = create_test_repository().await;

        let global = Profile::new(
            valid_profile_id("global"),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: valid_gain(0.5),
                degrees_of_rotation: valid_dor(900.0),
                torque_cap: valid_torque(10.0),
                filters: FilterConfig::default(),
            },
            "Global".to_string(),
        );

        let iracing_gt3 = Profile::new(
            valid_profile_id("iracing_gt3"),
            ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
            BaseSettings {
                ffb_gain: valid_gain(0.8),
                degrees_of_rotation: valid_dor(480.0),
                torque_cap: valid_torque(20.0),
                filters: FilterConfig::default(),
            },
            "iRacing GT3".to_string(),
        );

        must(repo.save_profile(&global, None).await);
        must(repo.save_profile(&iracing_gt3, None).await);

        let resolved = must(
            repo.resolve_profile_hierarchy(Some("iracing"), Some("gt3"), None, None)
                .await,
        );

        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.8);
    }
}

mod profile_signing {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[tokio::test]
    async fn test_signed_profile() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("signed_profile");

        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        must(repo.save_profile(&profile, Some(&signing_key)).await);

        let _loaded = must(repo.load_profile(&profile.id).await);
        let sig_info = must(repo.get_profile_signature(&profile.id).await);

        assert!(sig_info.is_some());
        let Some(sig) = sig_info else {
            panic!("signature should be present");
        };
        assert!(sig.is_valid());
    }

    #[tokio::test]
    async fn test_unsigned_profile() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("unsigned_profile");

        must(repo.save_profile(&profile, None).await);

        let sig_info = must(repo.get_profile_signature(&profile.id).await);
        assert!(sig_info.is_none());
    }
}

mod migration_integration {
    use super::*;
    use tokio::fs as async_fs;

    #[tokio::test]
    async fn test_legacy_profile_migration() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile_id = valid_profile_id("legacy_profile");
        let profile_path = repo.get_profile_file_path(&profile_id);

        let legacy_json = r#"{
            "ffb_gain": 0.72,
            "degrees_of_rotation": 900,
            "torque_cap": 13.0
        }"#;
        must(async_fs::write(&profile_path, legacy_json).await);

        let loaded = must(repo.load_profile(&profile_id).await);
        assert!(loaded.is_some());

        let Some(loaded) = loaded else {
            panic!("profile should exist");
        };
        assert!((loaded.base_settings.ffb_gain.value() - 0.72).abs() < 0.0001);
    }
}
