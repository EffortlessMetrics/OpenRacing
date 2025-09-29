//! Profile service for CRUD operations and hierarchy resolution

use anyhow::Result;
use racing_wheel_schemas::{Profile, DeviceId, ProfileId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

/// Simple profile service for managing profiles
pub struct ProfileService {
    /// Profile storage
    profiles: Arc<RwLock<HashMap<ProfileId, Profile>>>,
    /// Active profiles per device
    active_profiles: Arc<RwLock<HashMap<DeviceId, ProfileId>>>,
    /// Next profile ID counter
    next_id: Arc<RwLock<u64>>,
}

impl ProfileService {
    /// Create new profile service
    pub async fn new() -> Result<Self> {
        Ok(Self {
            profiles: Arc::new(RwLock::new(HashMap::new())),
            active_profiles: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        })
    }

    /// Create a new profile
    pub async fn create_profile(&self, profile: Profile) -> Result<ProfileId> {
        info!("Creating new profile");

        // Generate new profile ID
        let profile_id = {
            let mut next_id = self.next_id.write().await;
            let id = ProfileId::new(format!("profile_{}", *next_id));
            *next_id += 1;
            id
        };

        // Store profile
        {
            let mut profiles = self.profiles.write().await;
            profiles.insert(profile_id.clone(), profile);
        }

        info!(profile_id = %profile_id, "Profile created successfully");
        Ok(profile_id)
    }

    /// Get profile by ID
    pub async fn get_profile(&self, profile_id: &ProfileId) -> Result<Option<Profile>> {
        debug!(profile_id = %profile_id, "Getting profile");

        let profiles = self.profiles.read().await;
        Ok(profiles.get(profile_id).cloned())
    }

    /// Update an existing profile
    pub async fn update_profile(&self, profile_id: &ProfileId, profile: Profile) -> Result<()> {
        info!(profile_id = %profile_id, "Updating profile");

        let mut profiles = self.profiles.write().await;
        if profiles.contains_key(profile_id) {
            profiles.insert(profile_id.clone(), profile);
            info!(profile_id = %profile_id, "Profile updated successfully");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Profile not found: {}", profile_id))
        }
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

        // Remove from storage
        {
            let mut profiles = self.profiles.write().await;
            profiles.remove(profile_id);
        }

        info!(profile_id = %profile_id, "Profile deleted successfully");
        Ok(())
    }

    /// List all profiles
    pub async fn list_profiles(&self) -> Result<Vec<Profile>> {
        debug!("Listing all profiles");

        let profiles = self.profiles.read().await;
        Ok(profiles.values().cloned().collect())
    }

    /// Apply profile to device
    pub async fn apply_profile(&self, profile: Profile) -> Result<()> {
        info!("Applying profile");
        
        // In a real implementation, this would apply the profile to the FFB engine
        // For now, just log the operation
        debug!("Profile applied successfully");
        Ok(())
    }
    
    /// Load profile by ID (alias for get_profile for compatibility)
    pub async fn load_profile(&self, profile_id: &str) -> Result<Profile> {
        let profile_id = ProfileId::new(profile_id.to_string());
        self.get_profile(&profile_id).await?
            .ok_or_else(|| anyhow::anyhow!("Profile not found: {}", profile_id))
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

        Ok(ProfileStatistics {
            total_profiles: profiles.len(),
            active_profiles: active_count,
            cached_profiles: profiles.len(), // Same as total for in-memory storage
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
    use racing_wheel_schemas::{ProfileScope, BaseSettings, FilterConfig};

    #[tokio::test]
    async fn test_profile_service_creation() {
        let service = ProfileService::new().await;
        assert!(service.is_ok());
    }

    #[tokio::test]
    async fn test_profile_crud_operations() {
        let service = ProfileService::new().await.unwrap();

        // Create a test profile
        let profile = Profile {
            scope: ProfileScope {
                game: Some("iracing".to_string()),
                car: None,
                track: None,
            },
            base_settings: BaseSettings {
                ffb_gain: 0.8,
                dor_degrees: 540,
                torque_cap: 10.0,
                filters: FilterConfig {
                    reconstruction: 4,
                    friction: 0.1,
                    damper: 0.2,
                    inertia: 0.1,
                    notch_filters: Vec::new(),
                    slew_rate: 0.8,
                    curve_points: Vec::new(),
                },
            },
            led_config: None,
            haptics_config: None,
            metadata: Default::default(),
        };

        // Test create
        let profile_id = service.create_profile(profile.clone()).await.unwrap();
        assert!(!profile_id.to_string().is_empty());

        // Test get
        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert!(retrieved.is_some());

        // Test update
        let mut updated_profile = profile.clone();
        updated_profile.base_settings.ffb_gain = 0.9;
        service.update_profile(&profile_id, updated_profile).await.unwrap();

        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert_eq!(retrieved.unwrap().base_settings.ffb_gain, 0.9);

        // Test delete
        service.delete_profile(&profile_id).await.unwrap();
        let retrieved = service.get_profile(&profile_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_profile_statistics() {
        let service = ProfileService::new().await.unwrap();

        // Initially should have no profiles
        let stats = service.get_profile_statistics().await.unwrap();
        assert_eq!(stats.total_profiles, 0);
        assert_eq!(stats.active_profiles, 0);

        // Create a profile
        let profile = Profile {
            scope: ProfileScope {
                game: Some("test".to_string()),
                car: None,
                track: None,
            },
            base_settings: BaseSettings {
                ffb_gain: 0.8,
                dor_degrees: 540,
                torque_cap: 10.0,
                filters: FilterConfig {
                    reconstruction: 4,
                    friction: 0.1,
                    damper: 0.2,
                    inertia: 0.1,
                    notch_filters: Vec::new(),
                    slew_rate: 0.8,
                    curve_points: Vec::new(),
                },
            },
            led_config: None,
            haptics_config: None,
            metadata: Default::default(),
        };

        let _profile_id = service.create_profile(profile).await.unwrap();

        let stats = service.get_profile_statistics().await.unwrap();
        assert_eq!(stats.total_profiles, 1);
        assert_eq!(stats.cached_profiles, 1);
    }
}