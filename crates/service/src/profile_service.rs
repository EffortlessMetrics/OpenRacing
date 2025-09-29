//! Profile service for CRUD operations and hierarchy resolution

use anyhow::Result;
use racing_wheel_schemas::{Profile, DeviceId, ProfileId, DeviceCapabilities};
use crate::profile_repository::{ProfileRepository, ProfileRepositoryConfig, TrustState, ProfileSignature};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};
use ed25519_dalek::SigningKey;

/// Profile service for managing profiles with persistence and validation
pub struct ProfileService {
    /// Profile repository for persistence
    repository: ProfileRepository,
    /// Active profiles per device
    active_profiles: Arc<RwLock<HashMap<DeviceId, ProfileId>>>,
    /// Session overrides (temporary profile changes)
    session_overrides: Arc<RwLock<HashMap<DeviceId, Profile>>>,
}

impl ProfileService {
    /// Create new profile service with default configuration
    pub async fn new() -> Result<Self> {
        let config = ProfileRepositoryConfig::default();
        Self::new_with_config(config).await
    }

    /// Create new profile service with custom configuration
    pub async fn new_with_config(config: ProfileRepositoryConfig) -> Result<Self> {
        let repository = ProfileRepository::new(config).await?;
        
        Ok(Self {
            repository,
            active_profiles: Arc::new(RwLock::new(HashMap::new())),
            session_overrides: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new profile
    pub async fn create_profile(&self, profile: Profile) -> Result<ProfileId> {
        info!(profile_id = %profile.id, "Creating new profile");

        // Save to repository
        self.repository.save_profile(&profile, None).await?;

        info!(profile_id = %profile.id, "Profile created successfully");
        Ok(profile.id.clone())
    }

    /// Create a new signed profile
    pub async fn create_signed_profile(&self, profile: Profile, signing_key: &SigningKey) -> Result<ProfileId> {
        info!(profile_id = %profile.id, "Creating new signed profile");

        // Save to repository with signature
        self.repository.save_profile(&profile, Some(signing_key)).await?;

        info!(profile_id = %profile.id, "Signed profile created successfully");
        Ok(profile.id.clone())
    }

    /// Get profile by ID
    pub async fn get_profile(&self, profile_id: &ProfileId) -> Result<Option<Profile>> {
        debug!(profile_id = %profile_id, "Getting profile");
        self.repository.load_profile(profile_id).await
    }

    /// Update an existing profile
    pub async fn update_profile(&self, profile: Profile) -> Result<()> {
        info!(profile_id = %profile.id, "Updating profile");

        // Check if profile exists
        if self.repository.load_profile(&profile.id).await?.is_none() {
            return Err(anyhow::anyhow!("Profile not found: {}", profile.id));
        }

        // Save updated profile
        self.repository.save_profile(&profile, None).await?;

        info!(profile_id = %profile.id, "Profile updated successfully");
        Ok(())
    }

    /// Update an existing profile with signature
    pub async fn update_signed_profile(&self, profile: Profile, signing_key: &SigningKey) -> Result<()> {
        info!(profile_id = %profile.id, "Updating signed profile");

        // Check if profile exists
        if self.repository.load_profile(&profile.id).await?.is_none() {
            return Err(anyhow::anyhow!("Profile not found: {}", profile.id));
        }

        // Save updated profile with signature
        self.repository.save_profile(&profile, Some(signing_key)).await?;

        info!(profile_id = %profile.id, "Signed profile updated successfully");
        Ok(())
    }

    /// Delete a profile
    pub async fn delete_profile(&self, profile_id: &ProfileId) -> Result<()> {
        info!(profile_id = %profile_id, "Deleting profile");

        // Check if profile is currently active
        {
            let active_profiles = self.active_profiles.read().await;
            for (device_id, active_id) in active_profiles.iter() {
                if active_id == profile_id {
                    warn!(
                        profile_id = %profile_id,
                        device_id = %device_id,
                        "Cannot delete profile that is currently active"
                    );
                    return Err(anyhow::anyhow!("Profile is currently active on device {}", device_id));
                }
            }
        }

        // Delete from repository
        self.repository.delete_profile(profile_id).await?;

        info!(profile_id = %profile_id, "Profile deleted successfully");
        Ok(())
    }

    /// List all profiles
    pub async fn list_profiles(&self) -> Result<Vec<Profile>> {
        debug!("Listing all profiles");
        self.repository.list_profiles().await
    }

    /// Apply profile to device using hierarchy resolution
    pub async fn apply_profile_to_device(
        &self,
        device_id: &DeviceId,
        game: Option<&str>,
        car: Option<&str>,
        track: Option<&str>,
        device_capabilities: &DeviceCapabilities,
    ) -> Result<Profile> {
        info!(device_id = %device_id, game = ?game, car = ?car, track = ?track, "Applying profile to device");
        
        // Get session overrides for this device
        let session_overrides = {
            let overrides = self.session_overrides.read().await;
            overrides.get(device_id).cloned()
        };

        // Resolve profile hierarchy
        let resolved_profile = self.repository.resolve_profile_hierarchy(
            game,
            car,
            track,
            session_overrides.as_ref(),
        ).await?;

        // Validate profile against device capabilities
        resolved_profile.validate_for_device(device_capabilities)?;

        // Set as active profile
        {
            let mut active_profiles = self.active_profiles.write().await;
            active_profiles.insert(device_id.clone(), resolved_profile.id.clone());
        }

        info!(device_id = %device_id, profile_id = %resolved_profile.id, "Profile applied successfully");
        Ok(resolved_profile)
    }
    
    /// Load profile by ID string (alias for get_profile for compatibility)
    pub async fn load_profile(&self, profile_id: &str) -> Result<Profile> {
        let profile_id = ProfileId::new(profile_id.to_string())?;
        self.get_profile(&profile_id).await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found: {}", profile_id))
    }

    /// Get profile signature information
    pub async fn get_profile_signature(&self, profile_id: &ProfileId) -> Result<Option<ProfileSignature>> {
        self.repository.get_profile_signature(profile_id).await
    }

    /// Set session override for a device (temporary profile changes)
    pub async fn set_session_override(&self, device_id: &DeviceId, profile: Profile) -> Result<()> {
        info!(device_id = %device_id, profile_id = %profile.id, "Setting session override");
        
        let mut overrides = self.session_overrides.write().await;
        overrides.insert(device_id.clone(), profile);
        
        Ok(())
    }

    /// Clear session override for a device
    pub async fn clear_session_override(&self, device_id: &DeviceId) -> Result<()> {
        info!(device_id = %device_id, "Clearing session override");
        
        let mut overrides = self.session_overrides.write().await;
        overrides.remove(device_id);
        
        Ok(())
    }

    /// Get current session override for a device
    pub async fn get_session_override(&self, device_id: &DeviceId) -> Result<Option<Profile>> {
        let overrides = self.session_overrides.read().await;
        Ok(overrides.get(device_id).cloned())
    }

    /// Get currently active profile for device
    pub async fn get_active_profile(&self, device_id: &DeviceId) -> Result<Option<ProfileId>> {
        let active_profiles = self.active_profiles.read().await;
        Ok(active_profiles.get(device_id).cloned())
    }

    /// Set active profile for device
    pub async fn set_active_profile(&self, device_id: &DeviceId, profile_id: &ProfileId) -> Result<()> {
        let mut active_profiles = self.active_profiles.write().await;
        active_profiles.insert(device_id.clone(), profile_id.clone());
        Ok(())
    }

    /// Clear active profile for device
    pub async fn clear_active_profile(&self, device_id: &DeviceId) -> Result<()> {
        info!(device_id = %device_id, "Clearing active profile");

        {
            let mut active_profiles = self.active_profiles.write().await;
            active_profiles.remove(device_id);
        }

        info!(device_id = %device_id, "Active profile cleared");
        Ok(())
    }

    /// Get profile statistics
    pub async fn get_profile_statistics(&self) -> Result<ProfileStatistics> {
        let profiles = self.list_profiles().await?;
        let active_count = self.active_profiles.read().await.len();
        let session_override_count = self.session_overrides.read().await.len();

        // Count signed profiles
        let mut signed_count = 0;
        let mut trusted_count = 0;
        for profile in &profiles {
            if let Ok(Some(signature)) = self.get_profile_signature(&profile.id).await {
                signed_count += 1;
                if signature.trust_state == TrustState::Trusted {
                    trusted_count += 1;
                }
            }
        }

        Ok(ProfileStatistics {
            total_profiles: profiles.len(),
            active_profiles: active_count,
            cached_profiles: profiles.len(),
            signed_profiles: signed_count,
            trusted_profiles: trusted_count,
            session_overrides: session_override_count,
        })
    }
}

/// Profile service statistics
#[derive(Debug, Clone)]
pub struct ProfileStatistics {
    pub total_profiles: usize,
    pub active_profiles: usize,
    pub cached_profiles: usize,
    pub signed_profiles: usize,
    pub trusted_profiles: usize,
    pub session_overrides: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::{
        ProfileScope, BaseSettings, FilterConfig, Gain, Degrees, TorqueNm,
        DeviceCapabilities, DeviceType, TorqueNm as DeviceTorqueNm
    };
    use tempfile::TempDir;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    async fn create_test_service() -> (ProfileService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = ProfileRepositoryConfig {
            profiles_dir: temp_dir.path().to_path_buf(),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: true,
        };
        let service = ProfileService::new_with_config(config).await.unwrap();
        (service, temp_dir)
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

    fn create_test_device_capabilities() -> DeviceCapabilities {
        DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            DeviceTorqueNm::new(25.0).unwrap(),
            10000,
            1000,
        )
    }

    #[tokio::test]
    async fn test_profile_service_creation() {
        let (_service, _temp_dir) = create_test_service().await;
        // Service creation should succeed
    }

    #[tokio::test]
    async fn test_profile_crud_operations() {
        let (service, _temp_dir) = create_test_service().await;
        let profile = create_test_profile("test1");

        // Test create
        let profile_id = service.create_profile(profile.clone()).await.unwrap();
        assert_eq!(profile_id, profile.id);

        // Test get
        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, profile.id);

        // Test update
        let mut updated_profile = profile.clone();
        updated_profile.base_settings.ffb_gain = Gain::new(0.9).unwrap();
        service.update_profile(updated_profile).await.unwrap();

        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert_eq!(retrieved.unwrap().base_settings.ffb_gain.value(), 0.9);

        // Test delete
        service.delete_profile(&profile_id).await.unwrap();
        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_signed_profile_operations() {
        let (service, _temp_dir) = create_test_service().await;
        let profile = create_test_profile("signed_test");
        
        // Generate signing key
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        // Create signed profile
        let profile_id = service.create_signed_profile(profile.clone(), &signing_key).await.unwrap();
        assert_eq!(profile_id, profile.id);

        // Check signature
        let signature_info = service.get_profile_signature(&profile_id).await.unwrap();
        assert!(signature_info.is_some());
        let sig_info = signature_info.unwrap();
        assert!(!sig_info.signature.is_empty());
        assert!(!sig_info.public_key.is_empty());
    }

    #[tokio::test]
    async fn test_profile_hierarchy_application() {
        let (service, _temp_dir) = create_test_service().await;
        let device_id = DeviceId::new("test_device".to_string()).unwrap();
        let capabilities = create_test_device_capabilities();

        // Create profiles with different scopes
        let global_profile = Profile::new(
            ProfileId::new("global".to_string()).unwrap(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: Gain::new(0.5).unwrap(),
                degrees_of_rotation: Degrees::new(900.0).unwrap(),
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
                degrees_of_rotation: Degrees::new(540.0).unwrap(),
                torque_cap: TorqueNm::new(15.0).unwrap(),
                filters: FilterConfig::default(),
            },
            "iRacing Profile".to_string(),
        );

        // Save profiles
        service.create_profile(global_profile).await.unwrap();
        service.create_profile(game_profile).await.unwrap();

        // Apply profile hierarchy
        let resolved = service.apply_profile_to_device(
            &device_id,
            Some("iracing"),
            None,
            None,
            &capabilities,
        ).await.unwrap();

        // Should use game-specific settings
        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.7);
        assert_eq!(resolved.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(resolved.base_settings.torque_cap.value(), 15.0);

        // Check active profile
        let active_profile_id = service.get_active_profile(&device_id).await.unwrap();
        assert!(active_profile_id.is_some());
    }

    #[tokio::test]
    async fn test_session_overrides() {
        let (service, _temp_dir) = create_test_service().await;
        let device_id = DeviceId::new("test_device".to_string()).unwrap();
        
        let override_profile = create_test_profile("session_override");

        // Set session override
        service.set_session_override(&device_id, override_profile.clone()).await.unwrap();

        // Get session override
        let retrieved = service.get_session_override(&device_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, override_profile.id);

        // Clear session override
        service.clear_session_override(&device_id).await.unwrap();
        let retrieved = service.get_session_override(&device_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_profile_statistics() {
        let (service, _temp_dir) = create_test_service().await;

        // Initially should have no profiles
        let stats = service.get_profile_statistics().await.unwrap();
        assert_eq!(stats.total_profiles, 0);
        assert_eq!(stats.active_profiles, 0);
        assert_eq!(stats.signed_profiles, 0);
        assert_eq!(stats.trusted_profiles, 0);

        // Create profiles
        let profile1 = create_test_profile("stats_test1");
        let profile2 = create_test_profile("stats_test2");
        
        service.create_profile(profile1).await.unwrap();
        service.create_profile(profile2).await.unwrap();

        let stats = service.get_profile_statistics().await.unwrap();
        assert_eq!(stats.total_profiles, 2);
        assert_eq!(stats.cached_profiles, 2);
        assert_eq!(stats.signed_profiles, 0); // No signed profiles yet
    }

    #[tokio::test]
    async fn test_profile_validation_against_device() {
        let (service, _temp_dir) = create_test_service().await;
        let device_id = DeviceId::new("test_device".to_string()).unwrap();
        
        // Create device with limited torque capability
        let limited_capabilities = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            DeviceTorqueNm::new(5.0).unwrap(), // Only 5 Nm max
            10000,
            1000,
        );

        // Create profile that exceeds device capability
        let excessive_profile = Profile::new(
            ProfileId::new("excessive".to_string()).unwrap(),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: Gain::new(0.8).unwrap(),
                degrees_of_rotation: Degrees::new(540.0).unwrap(),
                torque_cap: TorqueNm::new(20.0).unwrap(), // Exceeds device max
                filters: FilterConfig::default(),
            },
            "Excessive Profile".to_string(),
        );

        service.create_profile(excessive_profile).await.unwrap();

        // Applying should fail due to validation
        let result = service.apply_profile_to_device(
            &device_id,
            None,
            None,
            None,
            &limited_capabilities,
        ).await;

        assert!(result.is_err());
    }
}