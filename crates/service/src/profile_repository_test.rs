//! Standalone tests for profile repository functionality

#[cfg(test)]
mod tests {
    use super::super::profile_repository::*;
    use ed25519_dalek::SigningKey;
    use racing_wheel_schemas::prelude::{BaseSettings, Degrees, FilterConfig, Gain, Profile, ProfileId, ProfileScope, TorqueNm};
    use rand_core::OsRng;
    use tempfile::TempDir;

    async fn create_test_repository() -> (ProfileRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = ProfileRepositoryConfig {
            profiles_dir: temp_dir.path().to_path_buf(),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: true,
        };
        let repo = ProfileRepository::new(config).await.unwrap();
        (repo, temp_dir)
    }

    fn create_test_profile(id: &str) -> Profile {
        let profile_id = ProfileId::new(id.to_string()).unwrap();
        Profile::new(
            profile_id,
            ProfileScope::global(),
            BaseSettings::default(),
            format!("Test Profile {}", id),
        )
    }

    #[tokio::test]
    async fn test_profile_repository_creation() {
        let (_repo, _temp_dir) = create_test_repository().await;
        // Repository creation should succeed
    }

    #[tokio::test]
    async fn test_save_and_load_profile() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("test1");

        // Save profile
        repo.save_profile(&profile, None).await.unwrap();

        // Load profile
        let loaded = repo.load_profile(&profile.id).await.unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();

        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.scope, profile.scope);
        assert_eq!(
            loaded.base_settings.ffb_gain,
            profile.base_settings.ffb_gain
        );
    }

    #[tokio::test]
    async fn test_profile_signing_and_verification() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("signed_test");

        // Generate signing key
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        // Save signed profile
        repo.save_profile(&profile, Some(&signing_key))
            .await
            .unwrap();

        // Load and verify signature
        let _loaded = repo.load_profile(&profile.id).await.unwrap().unwrap();
        let signature_info = repo.get_profile_signature(&profile.id).await.unwrap();

        assert!(signature_info.is_some());
        let sig_info = signature_info.unwrap();
        assert!(!sig_info.signature.is_empty());
        assert!(!sig_info.public_key.is_empty());
        // Note: Will be ValidUnknown since we didn't add the key to trusted_keys
        assert!(matches!(
            sig_info.trust_state,
            TrustState::ValidUnknown | TrustState::Trusted
        ));
    }

    #[tokio::test]
    async fn test_profile_hierarchy_resolution() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Create profiles with different scopes
        let global_profile = Profile::new(
            ProfileId::new("global".to_string()).unwrap(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: Gain::new(0.5).unwrap(),
                degrees_of_rotation: Degrees::new_dor(900.0).unwrap(),
                torque_cap: TorqueNm::new(10.0).unwrap(),
                filters: FilterConfig::default(),
            },
            "Global Profile".to_string(),
        );

        let game_profile = Profile::new(
            ProfileId::new("iracing".to_string()).unwrap(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings {
                ffb_gain: Gain::new(0.7).unwrap(),
                degrees_of_rotation: Degrees::new_dor(540.0).unwrap(),
                torque_cap: TorqueNm::new(15.0).unwrap(),
                filters: FilterConfig::default(),
            },
            "iRacing Profile".to_string(),
        );

        let car_profile = Profile::new(
            ProfileId::new("iracing_gt3".to_string()).unwrap(),
            ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
            BaseSettings {
                ffb_gain: Gain::new(0.8).unwrap(),
                degrees_of_rotation: Degrees::new_dor(480.0).unwrap(),
                torque_cap: TorqueNm::new(20.0).unwrap(),
                filters: FilterConfig::default(),
            },
            "iRacing GT3 Profile".to_string(),
        );

        // Save all profiles
        repo.save_profile(&global_profile, None).await.unwrap();
        repo.save_profile(&game_profile, None).await.unwrap();
        repo.save_profile(&car_profile, None).await.unwrap();

        // Test hierarchy resolution
        let resolved = repo
            .resolve_profile_hierarchy(Some("iracing"), Some("gt3"), None, None)
            .await
            .unwrap();

        // Should use car-specific settings (most specific)
        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.8);
        assert_eq!(resolved.base_settings.degrees_of_rotation.value(), 480.0);
        assert_eq!(resolved.base_settings.torque_cap.value(), 20.0);
    }

    #[tokio::test]
    async fn test_deterministic_profile_merge() {
        let (repo, _temp_dir) = create_test_repository().await;

        let base_profile = Profile::new(
            ProfileId::new("base".to_string()).unwrap(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: Gain::new(0.5).unwrap(),
                degrees_of_rotation: Degrees::new_dor(900.0).unwrap(),
                torque_cap: TorqueNm::new(10.0).unwrap(),
                filters: FilterConfig::default(),
            },
            "Base Profile".to_string(),
        );

        let override_profile = Profile::new(
            ProfileId::new("override".to_string()).unwrap(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: Gain::new(0.8).unwrap(),
                degrees_of_rotation: Degrees::new_dor(540.0).unwrap(),
                torque_cap: TorqueNm::new(15.0).unwrap(),
                filters: FilterConfig::default(),
            },
            "Override Profile".to_string(),
        );

        let merged = repo
            .merge_profiles_deterministic(&base_profile, &override_profile)
            .unwrap();

        // Override profile should take precedence
        assert_eq!(merged.base_settings.ffb_gain.value(), 0.8);
        assert_eq!(merged.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(merged.base_settings.torque_cap.value(), 15.0);
    }

    #[tokio::test]
    async fn test_profile_deletion() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("delete_test");

        // Save profile
        repo.save_profile(&profile, None).await.unwrap();

        // Verify it exists
        let loaded = repo.load_profile(&profile.id).await.unwrap();
        assert!(loaded.is_some());

        // Delete profile
        repo.delete_profile(&profile.id).await.unwrap();

        // Verify it's gone
        let loaded = repo.load_profile(&profile.id).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Save multiple profiles
        let profile1 = create_test_profile("list_test1");
        let profile2 = create_test_profile("list_test2");
        let profile3 = create_test_profile("list_test3");

        repo.save_profile(&profile1, None).await.unwrap();
        repo.save_profile(&profile2, None).await.unwrap();
        repo.save_profile(&profile3, None).await.unwrap();

        // List profiles
        let profiles = repo.list_profiles().await.unwrap();
        assert_eq!(profiles.len(), 3);

        let profile_ids: Vec<String> = profiles.iter().map(|p| p.id.to_string()).collect();
        assert!(profile_ids.contains(&"list_test1".to_string()));
        assert!(profile_ids.contains(&"list_test2".to_string()));
        assert!(profile_ids.contains(&"list_test3".to_string()));
    }

    #[tokio::test]
    async fn test_profile_caching() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("cache_test");

        // Save profile
        repo.save_profile(&profile, None).await.unwrap();

        // Load profile (should cache it)
        let loaded1 = repo.load_profile(&profile.id).await.unwrap().unwrap();

        // Load again (should come from cache)
        let loaded2 = repo.load_profile(&profile.id).await.unwrap().unwrap();

        assert_eq!(loaded1.id, loaded2.id);
        assert_eq!(
            loaded1.base_settings.ffb_gain,
            loaded2.base_settings.ffb_gain
        );
    }
}
