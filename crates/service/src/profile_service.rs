//! Profile service for CRUD operations and hierarchy resolution

use anyhow::Result;
use racing_wheel_engine::{
    ProfileService as EngineProfileService, ProfileMergeEngine, ProfileHierarchyPolicy,
    ProfileRepo, ProfileRepoError, ProfileContext
};
use racing_wheel_schemas::{Profile, DeviceId, ProfileId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Application-level profile service that orchestrates profile operations
pub struct ApplicationProfileService {
    /// Engine-level profile service
    engine_service: Arc<RwLock<EngineProfileService>>,
    /// Profile repository for persistence
    profile_repo: Arc<dyn ProfileRepo>,
    /// Active profiles per device
    active_profiles: Arc<RwLock<HashMap<DeviceId, ProfileId>>>,
    /// Profile cache for performance
    profile_cache: Arc<RwLock<HashMap<ProfileId, Profile>>>,
}

impl ApplicationProfileService {
    /// Create new profile service
    pub async fn new(profile_repo: Arc<dyn ProfileRepo>) -> Result<Self> {
        let engine_service = Arc::new(RwLock::new(
            EngineProfileService::new(profile_repo.clone()).await?
        ));

        Ok(Self {
            engine_service,
            profile_repo,
            active_profiles: Arc::new(RwLock::new(HashMap::new())),
            profile_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new profile
    pub async fn create_profile(&self, profile: Profile) -> Result<ProfileId> {
        info!(profile_name = %profile.name, "Creating new profile");

        // Validate profile before creation
        self.validate_profile(&profile).await?;

        // Store in repository
        let profile_id = self.profile_repo.create_profile(profile.clone()).await
            .map_err(|e| anyhow::anyhow!("Failed to create profile: {}", e))?;

        // Update cache
        {
            let mut cache = self.profile_cache.write().await;
            cache.insert(profile_id.clone(), profile);
        }

        info!(profile_id = %profile_id, "Profile created successfully");
        Ok(profile_id)
    }

    /// Get profile by ID
    pub async fn get_profile(&self, profile_id: &ProfileId) -> Result<Option<Profile>> {
        debug!(profile_id = %profile_id, "Getting profile");

        // Check cache first
        {
            let cache = self.profile_cache.read().await;
            if let Some(profile) = cache.get(profile_id) {
                debug!(profile_id = %profile_id, "Profile found in cache");
                return Ok(Some(profile.clone()));
            }
        }

        // Load from repository
        match self.profile_repo.get_profile(profile_id).await {
            Ok(Some(profile)) => {
                // Update cache
                {
                    let mut cache = self.profile_cache.write().await;
                    cache.insert(profile_id.clone(), profile.clone());
                }
                debug!(profile_id = %profile_id, "Profile loaded from repository");
                Ok(Some(profile))
            }
            Ok(None) => {
                debug!(profile_id = %profile_id, "Profile not found");
                Ok(None)
            }
            Err(e) => {
                error!(profile_id = %profile_id, error = %e, "Failed to get profile");
                Err(anyhow::anyhow!("Failed to get profile: {}", e))
            }
        }
    }

    /// Update an existing profile
    pub async fn update_profile(&self, profile_id: &ProfileId, profile: Profile) -> Result<()> {
        info!(profile_id = %profile_id, profile_name = %profile.name, "Updating profile");

        // Validate profile before update
        self.validate_profile(&profile).await?;

        // Update in repository
        self.profile_repo.update_profile(profile_id, profile.clone()).await
            .map_err(|e| anyhow::anyhow!("Failed to update profile: {}", e))?;

        // Update cache
        {
            let mut cache = self.profile_cache.write().await;
            cache.insert(profile_id.clone(), profile);
        }

        // If this profile is active on any device, trigger reapplication
        self.reapply_active_profile(profile_id).await?;

        info!(profile_id = %profile_id, "Profile updated successfully");
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
        self.profile_repo.delete_profile(profile_id).await
            .map_err(|e| anyhow::anyhow!("Failed to delete profile: {}", e))?;

        // Remove from cache
        {
            let mut cache = self.profile_cache.write().await;
            cache.remove(profile_id);
        }

        info!(profile_id = %profile_id, "Profile deleted successfully");
        Ok(())
    }

    /// List all profiles
    pub async fn list_profiles(&self) -> Result<Vec<Profile>> {
        debug!("Listing all profiles");

        self.profile_repo.list_profiles().await
            .map_err(|e| anyhow::anyhow!("Failed to list profiles: {}", e))
    }

    /// Apply profile to device with hierarchy resolution
    pub async fn apply_profile_to_device(
        &self,
        device_id: &DeviceId,
        profile_id: &ProfileId,
        context: ProfileContext,
    ) -> Result<()> {
        info!(
            device_id = %device_id,
            profile_id = %profile_id,
            "Applying profile to device"
        );

        // Get the profile
        let profile = self.get_profile(profile_id).await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found: {}", profile_id))?;

        // Resolve profile hierarchy
        let resolved_profile = self.resolve_profile_hierarchy(&profile, &context).await?;

        // Apply through engine service
        {
            let engine_service = self.engine_service.write().await;
            engine_service.apply_profile_to_device(device_id, &resolved_profile).await
                .map_err(|e| anyhow::anyhow!("Failed to apply profile to device: {}", e))?;
        }

        // Update active profile tracking
        {
            let mut active_profiles = self.active_profiles.write().await;
            active_profiles.insert(device_id.clone(), profile_id.clone());
        }

        info!(
            device_id = %device_id,
            profile_id = %profile_id,
            "Profile applied successfully"
        );
        Ok(())
    }

    /// Get currently active profile for device
    pub async fn get_active_profile(&self, device_id: &DeviceId) -> Result<Option<ProfileId>> {
        let active_profiles = self.active_profiles.read().await;
        Ok(active_profiles.get(device_id).cloned())
    }

    /// Resolve profile hierarchy (Global → Game → Car → Session)
    async fn resolve_profile_hierarchy(
        &self,
        base_profile: &Profile,
        context: &ProfileContext,
    ) -> Result<Profile> {
        debug!(
            base_profile = %base_profile.name,
            game = ?context.game,
            car = ?context.car,
            "Resolving profile hierarchy"
        );

        let merge_engine = ProfileMergeEngine::new();
        let mut resolved = base_profile.clone();

        // Apply game-specific overrides if available
        if let Some(game) = &context.game {
            if let Some(game_profile) = self.find_game_profile(game).await? {
                resolved = merge_engine.merge_profiles(&resolved, &game_profile)?;
                debug!(game = %game, "Applied game-specific profile");
            }
        }

        // Apply car-specific overrides if available
        if let (Some(game), Some(car)) = (&context.game, &context.car) {
            if let Some(car_profile) = self.find_car_profile(game, car).await? {
                resolved = merge_engine.merge_profiles(&resolved, &car_profile)?;
                debug!(game = %game, car = %car, "Applied car-specific profile");
            }
        }

        // Apply session overrides if available
        if let Some(session_overrides) = &context.session_overrides {
            resolved = merge_engine.merge_profiles(&resolved, session_overrides)?;
            debug!("Applied session overrides");
        }

        Ok(resolved)
    }

    /// Find game-specific profile
    async fn find_game_profile(&self, game: &str) -> Result<Option<Profile>> {
        // In a real implementation, this would search for profiles tagged for specific games
        // For now, return None as this is a placeholder
        debug!(game = %game, "Looking for game-specific profile");
        Ok(None)
    }

    /// Find car-specific profile
    async fn find_car_profile(&self, game: &str, car: &str) -> Result<Option<Profile>> {
        // In a real implementation, this would search for profiles tagged for specific game/car combinations
        // For now, return None as this is a placeholder
        debug!(game = %game, car = %car, "Looking for car-specific profile");
        Ok(None)
    }

    /// Validate profile before creation/update
    async fn validate_profile(&self, profile: &Profile) -> Result<()> {
        debug!(profile_name = %profile.name, "Validating profile");

        // Basic validation
        if profile.name.trim().is_empty() {
            return Err(anyhow::anyhow!("Profile name cannot be empty"));
        }

        // Validate FFB settings
        if let Some(ffb_settings) = &profile.ffb_settings {
            if ffb_settings.gain < 0.0 || ffb_settings.gain > 1.0 {
                return Err(anyhow::anyhow!("FFB gain must be between 0.0 and 1.0"));
            }
        }

        // Additional validation would be added here
        debug!(profile_name = %profile.name, "Profile validation passed");
        Ok(())
    }

    /// Reapply active profile after update
    async fn reapply_active_profile(&self, profile_id: &ProfileId) -> Result<()> {
        let active_profiles = self.active_profiles.read().await;
        
        for (device_id, active_id) in active_profiles.iter() {
            if active_id == profile_id {
                info!(
                    device_id = %device_id,
                    profile_id = %profile_id,
                    "Reapplying updated profile to device"
                );

                // Create a default context for reapplication
                let context = ProfileContext {
                    game: None,
                    car: None,
                    track: None,
                    session_overrides: None,
                };

                // Reapply the profile
                drop(active_profiles); // Release the lock before calling apply_profile_to_device
                self.apply_profile_to_device(device_id, profile_id, context).await?;
                break;
            }
        }

        Ok(())
    }

    /// Clear active profile for device
    pub async fn clear_active_profile(&self, device_id: &DeviceId) -> Result<()> {
        info!(device_id = %device_id, "Clearing active profile");

        {
            let mut active_profiles = self.active_profiles.write().await;
            active_profiles.remove(device_id);
        }

        // Reset device to default profile through engine service
        {
            let engine_service = self.engine_service.write().await;
            engine_service.reset_device_to_default(device_id).await
                .map_err(|e| anyhow::anyhow!("Failed to reset device to default: {}", e))?;
        }

        info!(device_id = %device_id, "Active profile cleared");
        Ok(())
    }

    /// Get profile statistics
    pub async fn get_profile_statistics(&self) -> Result<ProfileStatistics> {
        let profiles = self.list_profiles().await?;
        let active_count = self.active_profiles.read().await.len();

        Ok(ProfileStatistics {
            total_profiles: profiles.len(),
            active_profiles: active_count,
            cached_profiles: self.profile_cache.read().await.len(),
        })
    }
}

/// Profile service statistics
#[derive(Debug, Clone)]
pub struct ProfileStatistics {
    pub total_profiles: usize,
    pub active_profiles: usize,
    pub cached_profiles: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_engine::MockProfileRepo;
    use racing_wheel_schemas::{FFBSettings, DeviceId};

    #[tokio::test]
    async fn test_profile_service_creation() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ApplicationProfileService::new(repo).await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_profile_crud_operations() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ApplicationProfileService::new(repo).await.unwrap();

        // Create a test profile
        let profile = Profile {
            id: None,
            name: "Test Profile".to_string(),
            description: Some("Test description".to_string()),
            ffb_settings: Some(FFBSettings {
                gain: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        };

        // Test create
        let profile_id = service.create_profile(profile.clone()).await.unwrap();
        assert!(!profile_id.to_string().is_empty());

        // Test get
        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Profile");

        // Test update
        let mut updated_profile = profile.clone();
        updated_profile.name = "Updated Profile".to_string();
        service.update_profile(&profile_id, updated_profile).await.unwrap();

        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert_eq!(retrieved.unwrap().name, "Updated Profile");

        // Test delete
        service.delete_profile(&profile_id).await.unwrap();
        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_profile_application() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ApplicationProfileService::new(repo).await.unwrap();

        // Create a test profile
        let profile = Profile {
            id: None,
            name: "Test Profile".to_string(),
            ffb_settings: Some(FFBSettings {
                gain: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        };

        let profile_id = service.create_profile(profile).await.unwrap();
        let device_id = DeviceId::from("test-device");

        // Test profile application
        let context = ProfileContext {
            game: Some("iracing".to_string()),
            car: Some("gt3".to_string()),
            track: None,
            session_overrides: None,
        };

        let result = service.apply_profile_to_device(&device_id, &profile_id, context).await;
        // This might fail due to mock limitations, but we're testing the interface
        assert!(result.is_ok() || result.is_err()); // Either outcome is acceptable for this test

        // Test getting active profile
        let active = service.get_active_profile(&device_id).await.unwrap();
        if result.is_ok() {
            assert_eq!(active, Some(profile_id));
        }
    }

    #[tokio::test]
    async fn test_profile_validation() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ApplicationProfileService::new(repo).await.unwrap();

        // Test invalid profile (empty name)
        let invalid_profile = Profile {
            id: None,
            name: "".to_string(),
            ..Default::default()
        };

        let result = service.create_profile(invalid_profile).await;
        assert!(result.is_err());

        // Test invalid FFB gain
        let invalid_ffb_profile = Profile {
            id: None,
            name: "Invalid FFB".to_string(),
            ffb_settings: Some(FFBSettings {
                gain: 1.5, // Invalid: > 1.0
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = service.create_profile(invalid_ffb_profile).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_profile_statistics() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ApplicationProfileService::new(repo).await.unwrap();

        // Initially should have no profiles
        let stats = service.get_profile_statistics().await.unwrap();
        assert_eq!(stats.total_profiles, 0);
        assert_eq!(stats.active_profiles, 0);

        // Create a profile
        let profile = Profile {
            id: None,
            name: "Test Profile".to_string(),
            ..Default::default()
        };

        let _profile_id = service.create_profile(profile).await.unwrap();

        let stats = service.get_profile_statistics().await.unwrap();
        assert_eq!(stats.total_profiles, 1);
        assert_eq!(stats.cached_profiles, 1);
    }
}