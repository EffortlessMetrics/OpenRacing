//! Profile management service implementing domain policies
//!
//! This service orchestrates profile loading, hierarchy resolution, and validation
//! according to the domain policies. It serves as the application layer that
//! coordinates between the domain logic and infrastructure adapters.

use crate::{ProfileContext, ProfileHierarchyPolicy, ProfileRepo, ProfileRepoError, SafetyPolicy};
use racing_wheel_schemas::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Profile management service
///
/// This service implements the use cases for profile management, including
/// loading, saving, hierarchy resolution, and validation. It coordinates
/// between the domain policies and the profile repository.
pub struct ProfileService {
    /// Profile repository for persistence
    repo: Arc<dyn ProfileRepo>,

    /// Cached profiles for performance
    profile_cache: Arc<RwLock<HashMap<ProfileId, Profile>>>,

    /// Global default profile
    global_profile: Arc<RwLock<Option<Profile>>>,

    /// Profile hierarchy policy
    /// TODO: Used for future profile hierarchy implementation
    #[allow(dead_code)]
    hierarchy_policy: ProfileHierarchyPolicy,

    /// Safety policy for validation
    safety_policy: SafetyPolicy,

    /// Cache of resolved profile hashes to detect changes
    resolved_cache: Arc<RwLock<HashMap<String, (u64, Profile)>>>,
}

impl ProfileService {
    /// Create a new profile service
    pub fn new(repo: Arc<dyn ProfileRepo>) -> Self {
        Self {
            repo,
            profile_cache: Arc::new(RwLock::new(HashMap::new())),
            global_profile: Arc::new(RwLock::new(None)),
            hierarchy_policy: ProfileHierarchyPolicy,
            safety_policy: SafetyPolicy::new(),
            resolved_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the service by loading the global profile
    pub async fn initialize(&self) -> Result<(), ProfileServiceError> {
        info!("Initializing profile service");

        // Load or create global profile
        let global_profile = match self.repo.load_global_profile().await {
            Ok(profile) => {
                info!("Loaded existing global profile");
                profile
            }
            Err(ProfileRepoError::ProfileNotFound(_)) => {
                info!("Creating default global profile");
                let default_profile =
                    Profile::default_global().map_err(ProfileServiceError::DomainError)?;

                self.repo
                    .save_global_profile(&default_profile)
                    .await
                    .map_err(ProfileServiceError::RepoError)?;

                default_profile
            }
            Err(e) => {
                error!("Failed to load global profile: {}", e);
                return Err(ProfileServiceError::RepoError(e));
            }
        };

        // Cache the global profile
        {
            let mut global = self.global_profile.write().await;
            *global = Some(global_profile);
        }

        info!("Profile service initialized successfully");
        Ok(())
    }

    /// Resolve the effective profile for a given context
    ///
    /// This method implements the profile hierarchy resolution according to
    /// the domain policy: Global → Game → Car → Session overrides.
    pub async fn resolve_profile(
        &self,
        context: &ProfileContext,
        session_overrides: Option<&BaseSettings>,
    ) -> Result<Profile, ProfileServiceError> {
        debug!("Resolving profile for context: {:?}", context);

        // Check cache first
        let cache_key = self.create_cache_key(context, session_overrides);
        {
            let cache = self.resolved_cache.read().await;
            if let Some((hash, cached_profile)) = cache.get(&cache_key) {
                debug!("Found cached resolved profile with hash: {}", hash);
                return Ok(cached_profile.clone());
            }
        }

        // Load global profile
        let global_profile = {
            let global = self.global_profile.read().await;
            global
                .as_ref()
                .ok_or(ProfileServiceError::GlobalProfileNotLoaded)?
                .clone()
        };

        // Load game-specific profile if applicable
        let game_profile = if let Some(ref game) = context.game {
            match self
                .load_profile_for_scope(&ProfileScope::for_game(game.clone()))
                .await
            {
                Ok(Some(profile)) => Some(profile),
                Ok(None) => {
                    debug!("No game-specific profile found for: {}", game);
                    None
                }
                Err(e) => {
                    warn!("Failed to load game profile for {}: {}", game, e);
                    None
                }
            }
        } else {
            None
        };

        // Load car-specific profile if applicable
        let car_profile = if let (Some(game), Some(car)) = (&context.game, &context.car) {
            match self
                .load_profile_for_scope(&ProfileScope::for_car(game.clone(), car.clone()))
                .await
            {
                Ok(Some(profile)) => Some(profile),
                Ok(None) => {
                    debug!("No car-specific profile found for: {}/{}", game, car);
                    None
                }
                Err(e) => {
                    warn!("Failed to load car profile for {}/{}: {}", game, car, e);
                    None
                }
            }
        } else {
            None
        };

        // Resolve the hierarchy
        let resolved_profile = ProfileHierarchyPolicy::resolve_profile_hierarchy(
            &global_profile,
            game_profile.as_ref(),
            car_profile.as_ref(),
            session_overrides,
        );

        // Calculate hash for caching
        let profile_hash = ProfileHierarchyPolicy::calculate_hierarchy_hash(
            &global_profile,
            game_profile.as_ref(),
            car_profile.as_ref(),
            session_overrides,
        );

        // Cache the resolved profile
        {
            let mut cache = self.resolved_cache.write().await;
            cache.insert(cache_key, (profile_hash, resolved_profile.clone()));
        }

        debug!("Resolved profile with hash: {}", profile_hash);
        Ok(resolved_profile)
    }

    /// Load a profile by ID with caching
    pub async fn load_profile(&self, id: &ProfileId) -> Result<Profile, ProfileServiceError> {
        // Check cache first
        {
            let cache = self.profile_cache.read().await;
            if let Some(cached_profile) = cache.get(id) {
                debug!("Found cached profile: {}", id);
                return Ok(cached_profile.clone());
            }
        }

        // Load from repository
        let profile = self
            .repo
            .load_profile(id)
            .await
            .map_err(ProfileServiceError::RepoError)?;

        // Cache the profile
        {
            let mut cache = self.profile_cache.write().await;
            cache.insert(id.clone(), profile.clone());
        }

        debug!("Loaded and cached profile: {}", id);
        Ok(profile)
    }

    /// Save a profile with validation
    pub async fn save_profile(
        &self,
        profile: &Profile,
        device_capabilities: &DeviceCapabilities,
    ) -> Result<(), ProfileServiceError> {
        info!("Saving profile: {}", profile.id);

        // Validate profile against device capabilities
        profile
            .validate_for_device(device_capabilities)
            .map_err(ProfileServiceError::DomainError)?;

        // Save to repository
        self.repo
            .save_profile(profile)
            .await
            .map_err(ProfileServiceError::RepoError)?;

        // Update cache
        {
            let mut cache = self.profile_cache.write().await;
            cache.insert(profile.id.clone(), profile.clone());
        }

        // Clear resolved cache since profiles may have changed
        self.clear_resolved_cache().await;

        info!("Successfully saved profile: {}", profile.id);
        Ok(())
    }

    /// Delete a profile
    pub async fn delete_profile(&self, id: &ProfileId) -> Result<(), ProfileServiceError> {
        info!("Deleting profile: {}", id);

        // Delete from repository
        self.repo
            .delete_profile(id)
            .await
            .map_err(ProfileServiceError::RepoError)?;

        // Remove from cache
        {
            let mut cache = self.profile_cache.write().await;
            cache.remove(id);
        }

        // Clear resolved cache
        self.clear_resolved_cache().await;

        info!("Successfully deleted profile: {}", id);
        Ok(())
    }

    /// List all available profiles
    pub async fn list_profiles(&self) -> Result<Vec<ProfileId>, ProfileServiceError> {
        self.repo
            .list_profiles()
            .await
            .map_err(ProfileServiceError::RepoError)
    }

    /// Validate a profile against safety policies and device capabilities
    pub async fn validate_profile(
        &self,
        profile: &Profile,
        device_capabilities: &DeviceCapabilities,
    ) -> Result<ProfileValidationResult, ProfileServiceError> {
        let mut result = ProfileValidationResult {
            is_valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        };

        // Validate against device capabilities
        if let Err(e) = profile.validate_for_device(device_capabilities) {
            result.is_valid = false;
            result
                .errors
                .push(format!("Device capability validation failed: {}", e));
        }

        // Validate torque limits against safety policy
        let requested_torque = profile.base_settings.torque_cap;
        if let Err(e) = self.safety_policy.validate_torque_limits(
            requested_torque,
            false, // Assume safe mode for validation
            device_capabilities,
        ) {
            result.warnings.push(format!("Torque limit warning: {}", e));
        }

        // Check for reasonable settings
        if profile.base_settings.ffb_gain.value() > 0.9 {
            result
                .warnings
                .push("FFB gain is very high (>90%), consider reducing for safety".to_string());
        }

        if profile.base_settings.torque_cap.value() > 20.0 {
            result
                .warnings
                .push("Torque cap is very high (>20Nm), ensure proper safety measures".to_string());
        }

        Ok(result)
    }

    /// Get profile statistics
    pub async fn get_statistics(&self) -> Result<ProfileStatistics, ProfileServiceError> {
        let profile_ids = self.list_profiles().await?;
        let total_profiles = profile_ids.len();

        let cache_size = {
            let cache = self.profile_cache.read().await;
            cache.len()
        };

        let resolved_cache_size = {
            let cache = self.resolved_cache.read().await;
            cache.len()
        };

        Ok(ProfileStatistics {
            total_profiles,
            cached_profiles: cache_size,
            resolved_cache_entries: resolved_cache_size,
        })
    }

    /// Clear all caches
    pub async fn clear_caches(&self) {
        {
            let mut cache = self.profile_cache.write().await;
            cache.clear();
        }

        self.clear_resolved_cache().await;

        info!("Cleared all profile caches");
    }

    /// Load profile for a specific scope
    async fn load_profile_for_scope(
        &self,
        scope: &ProfileScope,
    ) -> Result<Option<Profile>, ProfileServiceError> {
        let profiles = self
            .repo
            .find_profiles_for_scope(scope)
            .await
            .map_err(ProfileServiceError::RepoError)?;

        // Find the most specific matching profile
        let game = scope.game.as_deref();
        let car = scope.car.as_deref();
        let track = scope.track.as_deref();

        let best_match =
            ProfileHierarchyPolicy::find_most_specific_profile(&profiles, game, car, track);

        Ok(best_match.cloned())
    }

    /// Create a cache key for resolved profiles
    fn create_cache_key(
        &self,
        context: &ProfileContext,
        session_overrides: Option<&BaseSettings>,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        context.device_id.hash(&mut hasher);
        context.game.hash(&mut hasher);
        context.car.hash(&mut hasher);
        context.track.hash(&mut hasher);
        context.session_type.hash(&mut hasher);

        if let Some(overrides) = session_overrides {
            overrides.ffb_gain.value().to_bits().hash(&mut hasher);
            overrides
                .degrees_of_rotation
                .value()
                .to_bits()
                .hash(&mut hasher);
            overrides.torque_cap.value().to_bits().hash(&mut hasher);
        }

        format!("profile_cache_{:x}", hasher.finish())
    }

    /// Clear the resolved profile cache
    async fn clear_resolved_cache(&self) {
        let mut cache = self.resolved_cache.write().await;
        cache.clear();
    }
}

/// Profile service error types
#[derive(Debug, thiserror::Error)]
pub enum ProfileServiceError {
    #[error("Repository error: {0}")]
    RepoError(#[from] ProfileRepoError),

    #[error("Domain error: {0}")]
    DomainError(#[from] DomainError),

    #[error("Global profile not loaded")]
    GlobalProfileNotLoaded,

    #[error("Profile validation failed: {0}")]
    ValidationFailed(String),

    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Profile validation result
#[derive(Debug, Clone)]
pub struct ProfileValidationResult {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Profile service statistics
#[derive(Debug, Clone)]
pub struct ProfileStatistics {
    pub total_profiles: usize,
    pub cached_profiles: usize,
    pub resolved_cache_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProfileRepoError, RepositoryStatus};

    use async_trait::async_trait;
    use std::path::Path;

    // Mock profile repository for testing
    struct MockProfileRepo {
        profiles: Arc<RwLock<HashMap<ProfileId, Profile>>>,
        global_profile: Arc<RwLock<Option<Profile>>>,
    }

    impl MockProfileRepo {
        fn new() -> Self {
            Self {
                profiles: Arc::new(RwLock::new(HashMap::new())),
                global_profile: Arc::new(RwLock::new(None)),
            }
        }

        async fn add_profile(&self, profile: Profile) {
            let mut profiles = self.profiles.write().await;
            profiles.insert(profile.id.clone(), profile);
        }
    }

    #[async_trait]
    impl ProfileRepo for MockProfileRepo {
        async fn load_profile(&self, id: &ProfileId) -> Result<Profile, ProfileRepoError> {
            let profiles = self.profiles.read().await;
            profiles
                .get(id)
                .cloned()
                .ok_or_else(|| ProfileRepoError::ProfileNotFound(id.clone()))
        }

        async fn save_profile(&self, profile: &Profile) -> Result<(), ProfileRepoError> {
            let mut profiles = self.profiles.write().await;
            profiles.insert(profile.id.clone(), profile.clone());
            Ok(())
        }

        async fn delete_profile(&self, id: &ProfileId) -> Result<(), ProfileRepoError> {
            let mut profiles = self.profiles.write().await;
            profiles.remove(id);
            Ok(())
        }

        async fn list_profiles(&self) -> Result<Vec<ProfileId>, ProfileRepoError> {
            let profiles = self.profiles.read().await;
            Ok(profiles.keys().cloned().collect())
        }

        async fn find_profiles_for_scope(
            &self,
            scope: &ProfileScope,
        ) -> Result<Vec<Profile>, ProfileRepoError> {
            let profiles = self.profiles.read().await;
            let matching: Vec<Profile> = profiles
                .values()
                .filter(|p| &p.scope == scope)
                .cloned()
                .collect();
            Ok(matching)
        }

        async fn load_global_profile(&self) -> Result<Profile, ProfileRepoError> {
            let global = self.global_profile.read().await;
            global.as_ref().cloned().ok_or_else(|| {
                ProfileRepoError::ProfileNotFound(ProfileId::from_raw("global".to_string()))
            })
        }

        async fn save_global_profile(&self, profile: &Profile) -> Result<(), ProfileRepoError> {
            let mut global = self.global_profile.write().await;
            *global = Some(profile.clone());
            Ok(())
        }

        async fn profile_exists(&self, id: &ProfileId) -> Result<bool, ProfileRepoError> {
            let profiles = self.profiles.read().await;
            Ok(profiles.contains_key(id))
        }

        async fn get_profile_metadata(
            &self,
            id: &ProfileId,
        ) -> Result<ProfileMetadata, ProfileRepoError> {
            let profiles = self.profiles.read().await;
            profiles
                .get(id)
                .map(|p| p.metadata.clone())
                .ok_or_else(|| ProfileRepoError::ProfileNotFound(id.clone()))
        }

        async fn backup_profiles(&self, _backup_path: &Path) -> Result<(), ProfileRepoError> {
            Ok(())
        }

        async fn restore_profiles(&self, _backup_path: &Path) -> Result<(), ProfileRepoError> {
            Ok(())
        }

        async fn validate_repository(&self) -> Result<RepositoryStatus, ProfileRepoError> {
            Ok(RepositoryStatus {
                is_healthy: true,
                total_profiles: 0,
                corrupted_profiles: Vec::new(),
                missing_files: Vec::new(),
                permission_issues: Vec::new(),
                last_backup: None,
                disk_usage_bytes: 0,
            })
        }
    }

    fn create_test_capabilities() -> DeviceCapabilities {
        DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            TorqueNm::from_raw(25.0),
            10000,
            1000,
        )
    }

    #[tokio::test]
    async fn test_profile_service_initialization() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ProfileService::new(repo);

        let result = service.initialize().await;
        assert!(result.is_ok());

        // Should have created a global profile
        let global = service.global_profile.read().await;
        assert!(global.is_some());
    }

    #[tokio::test]
    async fn test_profile_service_resolve_hierarchy() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ProfileService::new(repo.clone());

        // Initialize service
        service.initialize().await.unwrap();

        // Add game-specific profile
        let game_profile = Profile::new(
            ProfileId::from_raw("iracing".to_string()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::new(
                Gain::from_raw(0.8),
                Degrees::from_raw(540.0),
                TorqueNm::from_raw(20.0),
                FilterConfig::default(),
            ),
            "iRacing Profile".to_string(),
        );

        repo.add_profile(game_profile).await;

        // Create context
        let device_id = DeviceId::from_raw("test-device".to_string());
        let context = ProfileContext::new(device_id).with_game("iracing".to_string());

        // Resolve profile
        let resolved = service.resolve_profile(&context, None).await.unwrap();

        // Should have game-specific settings
        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.8);
        assert_eq!(resolved.base_settings.degrees_of_rotation.value(), 540.0);
    }

    #[tokio::test]
    async fn test_profile_service_caching() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ProfileService::new(repo.clone());

        service.initialize().await.unwrap();

        let device_id = DeviceId::from_raw("test-device".to_string());
        let context = ProfileContext::new(device_id);

        // First resolution should hit the repository
        let resolved1 = service.resolve_profile(&context, None).await.unwrap();

        // Second resolution should hit the cache
        let resolved2 = service.resolve_profile(&context, None).await.unwrap();

        // Results should be identical
        assert_eq!(resolved1.calculate_hash(), resolved2.calculate_hash());

        // Cache should have entries
        let stats = service.get_statistics().await.unwrap();
        assert!(stats.resolved_cache_entries > 0);
    }

    #[tokio::test]
    async fn test_profile_service_validation() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ProfileService::new(repo);

        service.initialize().await.unwrap();

        let capabilities = create_test_capabilities();

        // Create profile with excessive torque
        let mut profile = Profile::new(
            ProfileId::from_raw("test".to_string()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );

        profile.base_settings.torque_cap = TorqueNm::from_raw(30.0); // Exceeds device limit

        let validation_result = service
            .validate_profile(&profile, &capabilities)
            .await
            .unwrap();
        assert!(!validation_result.is_valid);
        assert!(!validation_result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_profile_service_save_and_load() {
        let repo = Arc::new(MockProfileRepo::new());
        let service = ProfileService::new(repo);

        service.initialize().await.unwrap();

        let capabilities = create_test_capabilities();

        let profile = Profile::new(
            ProfileId::from_raw("test-profile".to_string()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );

        // Save profile
        let result = service.save_profile(&profile, &capabilities).await;
        assert!(result.is_ok());

        // Load profile
        let loaded = service.load_profile(&profile.id).await.unwrap();
        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.metadata.name, profile.metadata.name);
    }
}
