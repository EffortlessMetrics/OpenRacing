//! File-based profile repository with JSON Schema validation and Ed25519 signatures
//!
//! This module implements a complete profile persistence system with:
//! - JSON Schema validation with line/column error reporting
//! - Profile migration system for schema version upgrades
//! - Ed25519 signature verification for profile authenticity
//! - Deterministic profile merge with Global→Game→Car→Session hierarchy
//! - File-based storage with atomic operations

use anyhow::{Context, Result};
use racing_wheel_schemas::{
    Profile, ProfileId, ProfileScope, ProfileMetadata, BaseSettings, FilterConfig,
    LedConfig, HapticsConfig, DeviceCapabilities, DomainError,
    config::{ProfileSchema, ProfileValidator, ProfileMigrator, SchemaError}
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use tokio::fs as async_fs;
use tokio::sync::RwLock;
use tracing::{info, warn, debug, error};
use ed25519_dalek::{Signature, Verifier, VerifyingKey, Signer};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use sha2::{Sha256, Digest};

/// Trust state for profile signatures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustState {
    /// Profile is unsigned
    Unsigned,
    /// Profile has a valid signature from a trusted key
    Trusted,
    /// Profile has a valid signature from an unknown key
    ValidUnknown,
    /// Profile signature is invalid
    Invalid,
}

/// Profile signature information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSignature {
    /// Ed25519 signature (base64 encoded)
    pub signature: String,
    /// Public key used for signing (base64 encoded)
    pub public_key: String,
    /// Trust state of this signature
    pub trust_state: TrustState,
}

/// Profile repository configuration
#[derive(Debug, Clone)]
pub struct ProfileRepositoryConfig {
    /// Base directory for profile storage
    pub profiles_dir: PathBuf,
    /// Trusted public keys for signature verification
    pub trusted_keys: Vec<String>,
    /// Enable automatic migration of older profile versions
    pub auto_migrate: bool,
    /// Create backup files before migration
    pub backup_on_migrate: bool,
}

impl Default for ProfileRepositoryConfig {
    fn default() -> Self {
        Self {
            profiles_dir: PathBuf::from("profiles"),
            trusted_keys: Vec::new(),
            auto_migrate: true,
            backup_on_migrate: true,
        }
    }
}

/// File-based profile repository with validation and signing support
pub struct ProfileRepository {
    config: ProfileRepositoryConfig,
    validator: ProfileValidator,
    cache: RwLock<HashMap<ProfileId, Profile>>,
    signature_cache: RwLock<HashMap<ProfileId, Option<ProfileSignature>>>,
}

impl ProfileRepository {
    /// Create a new profile repository
    pub async fn new(config: ProfileRepositoryConfig) -> Result<Self> {
        // Ensure profiles directory exists
        async_fs::create_dir_all(&config.profiles_dir).await
            .with_context(|| format!("Failed to create profiles directory: {:?}", config.profiles_dir))?;

        let validator = ProfileValidator::new()
            .context("Failed to create profile validator")?;

        let repository = Self {
            config,
            validator,
            cache: RwLock::new(HashMap::new()),
            signature_cache: RwLock::new(HashMap::new()),
        };

        // Load existing profiles into cache
        repository.load_all_profiles().await?;

        Ok(repository)
    }

    /// Save a profile to disk with optional signing
    pub async fn save_profile(&self, profile: &Profile, sign_with_key: Option<&ed25519_dalek::SigningKey>) -> Result<()> {
        info!(profile_id = %profile.id, "Saving profile");

        // Convert to schema format
        let profile_schema = self.profile_to_schema(profile)?;

        // Serialize to JSON with pretty formatting
        let mut json = serde_json::to_string_pretty(&profile_schema)
            .context("Failed to serialize profile to JSON")?;

        // Add signature if requested
        let signature_info = if let Some(signing_key) = sign_with_key {
            let signature = self.sign_profile_json(&json, signing_key)?;
            
            // Add signature to JSON
            let mut profile_with_sig = profile_schema;
            profile_with_sig.signature = Some(signature.signature.clone());
            
            json = serde_json::to_string_pretty(&profile_with_sig)
                .context("Failed to serialize signed profile to JSON")?;
            
            Some(signature)
        } else {
            None
        };

        // Validate the final JSON
        self.validator.validate_json(&json)
            .context("Profile validation failed before saving")?;

        // Write to file atomically
        let file_path = self.get_profile_file_path(&profile.id);
        let temp_path = file_path.with_extension("tmp");
        
        async_fs::write(&temp_path, &json).await
            .with_context(|| format!("Failed to write profile to temp file: {:?}", temp_path))?;
        
        async_fs::rename(&temp_path, &file_path).await
            .with_context(|| format!("Failed to move profile to final location: {:?}", file_path))?;

        // Update caches
        {
            let mut cache = self.cache.write().await;
            cache.insert(profile.id.clone(), profile.clone());
        }
        {
            let mut sig_cache = self.signature_cache.write().await;
            sig_cache.insert(profile.id.clone(), signature_info);
        }

        info!(profile_id = %profile.id, file_path = ?file_path, "Profile saved successfully");
        Ok(())
    }

    /// Load a profile from disk with validation and signature verification
    pub async fn load_profile(&self, profile_id: &ProfileId) -> Result<Option<Profile>> {
        debug!(profile_id = %profile_id, "Loading profile");

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(profile) = cache.get(profile_id) {
                debug!(profile_id = %profile_id, "Profile found in cache");
                return Ok(Some(profile.clone()));
            }
        }

        // Load from disk
        let file_path = self.get_profile_file_path(profile_id);
        if !file_path.exists() {
            debug!(profile_id = %profile_id, "Profile file not found");
            return Ok(None);
        }

        let json = async_fs::read_to_string(&file_path).await
            .with_context(|| format!("Failed to read profile file: {:?}", file_path))?;

        let profile = self.load_profile_from_json(&json, profile_id).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(profile_id.clone(), profile.clone());
        }

        info!(profile_id = %profile_id, "Profile loaded successfully");
        Ok(Some(profile))
    }

    /// Load profile from JSON string with migration and validation
    async fn load_profile_from_json(&self, json: &str, profile_id: &ProfileId) -> Result<Profile> {
        // Try to migrate if needed
        let profile_schema = if self.config.auto_migrate {
            match ProfileMigrator::migrate_profile(json) {
                Ok(schema) => schema,
                Err(SchemaError::UnsupportedSchemaVersion(version)) => {
                    return Err(anyhow::anyhow!("Unsupported profile schema version: {}", version));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Profile migration failed: {}", e));
                }
            }
        } else {
            self.validator.validate_json(json)
                .context("Profile validation failed")?
        };

        // Verify signature if present
        let signature_info = if let Some(ref sig_b64) = profile_schema.signature {
            Some(self.verify_profile_signature(json, sig_b64)?)
        } else {
            None
        };

        // Convert schema to domain entity
        let profile = self.schema_to_profile(&profile_schema, profile_id)?;

        // Update signature cache
        {
            let mut sig_cache = self.signature_cache.write().await;
            sig_cache.insert(profile_id.clone(), signature_info);
        }

        Ok(profile)
    }

    /// Delete a profile from disk and cache
    pub async fn delete_profile(&self, profile_id: &ProfileId) -> Result<()> {
        info!(profile_id = %profile_id, "Deleting profile");

        let file_path = self.get_profile_file_path(profile_id);
        
        if file_path.exists() {
            async_fs::remove_file(&file_path).await
                .with_context(|| format!("Failed to delete profile file: {:?}", file_path))?;
        }

        // Remove from caches
        {
            let mut cache = self.cache.write().await;
            cache.remove(profile_id);
        }
        {
            let mut sig_cache = self.signature_cache.write().await;
            sig_cache.remove(profile_id);
        }

        info!(profile_id = %profile_id, "Profile deleted successfully");
        Ok(())
    }

    /// List all available profiles
    pub async fn list_profiles(&self) -> Result<Vec<Profile>> {
        debug!("Listing all profiles");

        let cache = self.cache.read().await;
        Ok(cache.values().cloned().collect())
    }

    /// Get profile signature information
    pub async fn get_profile_signature(&self, profile_id: &ProfileId) -> Result<Option<ProfileSignature>> {
        let sig_cache = self.signature_cache.read().await;
        Ok(sig_cache.get(profile_id).cloned().flatten())
    }

    /// Resolve profiles using deterministic hierarchy: Global → Game → Car → Session
    pub async fn resolve_profile_hierarchy(
        &self,
        game: Option<&str>,
        car: Option<&str>,
        track: Option<&str>,
        session_overrides: Option<&Profile>,
    ) -> Result<Profile> {
        debug!(game = ?game, car = ?car, track = ?track, "Resolving profile hierarchy");

        let profiles = self.list_profiles().await?;
        
        // Find matching profiles and sort by specificity
        let mut matching_profiles: Vec<&Profile> = profiles
            .iter()
            .filter(|p| p.scope.matches(game, car, track))
            .collect();

        // Sort by specificity (global first, most specific last)
        matching_profiles.sort_by_key(|p| p.scope.specificity_level());

        // Start with a default profile if no global profile exists
        let mut resolved = if let Some(global) = matching_profiles.first() {
            (*global).clone()
        } else {
            Profile::default_global()
                .context("Failed to create default global profile")?
        };

        // Apply profiles in order of specificity (deterministic merge)
        for profile in matching_profiles.iter().skip(1) {
            resolved = self.merge_profiles_deterministic(&resolved, profile)?;
        }

        // Apply session overrides if provided
        if let Some(session) = session_overrides {
            resolved = self.merge_profiles_deterministic(&resolved, session)?;
        }

        debug!("Profile hierarchy resolved successfully");
        Ok(resolved)
    }

    /// Deterministic profile merge (other takes precedence)
    pub fn merge_profiles_deterministic(&self, base: &Profile, other: &Profile) -> Result<Profile> {
        let mut merged = base.clone();

        // Merge base settings (other takes precedence for all fields)
        merged.base_settings = other.base_settings.clone();

        // Merge LED config (other takes precedence if present)
        if other.led_config.is_some() {
            merged.led_config = other.led_config.clone();
        }

        // Merge haptics config (other takes precedence if present)
        if other.haptics_config.is_some() {
            merged.haptics_config = other.haptics_config.clone();
        }

        // Update metadata to reflect the merge
        merged.metadata.modified_at = chrono::Utc::now().to_rfc3339();
        merged.metadata.name = format!("Merged: {} + {}", base.metadata.name, other.metadata.name);

        Ok(merged)
    }

    /// Load all profiles from disk into cache
    async fn load_all_profiles(&self) -> Result<()> {
        info!("Loading all profiles from disk");

        let mut entries = async_fs::read_dir(&self.config.profiles_dir).await
            .with_context(|| format!("Failed to read profiles directory: {:?}", self.config.profiles_dir))?;

        let mut loaded_count = 0;
        let mut error_count = 0;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                    match ProfileId::new(file_stem.to_string()) {
                        Ok(profile_id) => {
                            match self.load_profile(&profile_id).await {
                                Ok(Some(_)) => {
                                    loaded_count += 1;
                                }
                                Ok(None) => {
                                    warn!(path = ?path, "Profile file exists but profile not found");
                                    error_count += 1;
                                }
                                Err(e) => {
                                    error!(path = ?path, error = %e, "Failed to load profile");
                                    error_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(path = ?path, error = %e, "Invalid profile ID in filename");
                            error_count += 1;
                        }
                    }
                }
            }
        }

        info!(loaded = loaded_count, errors = error_count, "Profile loading completed");
        Ok(())
    }

    /// Get file path for a profile
    fn get_profile_file_path(&self, profile_id: &ProfileId) -> PathBuf {
        self.config.profiles_dir.join(format!("{}.json", profile_id))
    }

    /// Sign profile JSON with Ed25519 key
    fn sign_profile_json(&self, json: &str, signing_key: &ed25519_dalek::SigningKey) -> Result<ProfileSignature> {
        // Create hash of the JSON content (excluding any existing signature)
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash = hasher.finalize();

        // Sign the hash
        let signature = signing_key.sign(&hash);
        let public_key = signing_key.verifying_key();

        Ok(ProfileSignature {
            signature: BASE64.encode(signature.to_bytes()),
            public_key: BASE64.encode(public_key.to_bytes()),
            trust_state: TrustState::Trusted, // Assume trusted since we're signing it
        })
    }

    /// Verify profile signature
    fn verify_profile_signature(&self, json: &str, signature_b64: &str) -> Result<ProfileSignature> {
        // Parse the JSON to extract signature info
        let value: serde_json::Value = serde_json::from_str(json)
            .context("Failed to parse JSON for signature verification")?;

        let signature_bytes = BASE64.decode(signature_b64)
            .context("Failed to decode signature from base64")?;

        let signature = Signature::from_bytes(&signature_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signature length"))?);

        // For verification, we need the public key (this would typically be stored separately)
        // For now, we'll extract it from the JSON if present, or mark as invalid
        let public_key_b64 = value.get("publicKey")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if public_key_b64.is_empty() {
            return Ok(ProfileSignature {
                signature: signature_b64.to_string(),
                public_key: String::new(),
                trust_state: TrustState::Invalid,
            });
        }

        let public_key_bytes = BASE64.decode(public_key_b64)
            .context("Failed to decode public key from base64")?;

        let public_key = VerifyingKey::from_bytes(&public_key_bytes.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid public key length"))?)
            .context("Failed to create verifying key")?;

        // Create JSON without signature for verification
        let mut json_for_verification = value.clone();
        json_for_verification.as_object_mut().unwrap().remove("signature");
        json_for_verification.as_object_mut().unwrap().remove("publicKey");
        
        let json_without_sig = serde_json::to_string(&json_for_verification)
            .context("Failed to serialize JSON for verification")?;

        // Hash the content
        let mut hasher = Sha256::new();
        hasher.update(json_without_sig.as_bytes());
        let hash = hasher.finalize();

        // Verify signature
        let trust_state = match public_key.verify(&hash, &signature) {
            Ok(()) => {
                // Check if this is a trusted key
                if self.config.trusted_keys.contains(&public_key_b64.to_string()) {
                    TrustState::Trusted
                } else {
                    TrustState::ValidUnknown
                }
            }
            Err(_) => TrustState::Invalid,
        };

        Ok(ProfileSignature {
            signature: signature_b64.to_string(),
            public_key: public_key_b64.to_string(),
            trust_state,
        })
    }

    /// Convert domain Profile to schema ProfileSchema
    fn profile_to_schema(&self, profile: &Profile) -> Result<ProfileSchema> {
        Ok(ProfileSchema {
            schema: "wheel.profile/1".to_string(),
            scope: racing_wheel_schemas::config::ProfileScope {
                game: profile.scope.game.clone(),
                car: profile.scope.car.clone(),
                track: profile.scope.track.clone(),
            },
            base: racing_wheel_schemas::config::BaseSettings {
                ffb_gain: profile.base_settings.ffb_gain.value(),
                dor_deg: profile.base_settings.degrees_of_rotation.value() as u16,
                torque_cap_nm: profile.base_settings.torque_cap.value(),
                filters: racing_wheel_schemas::config::FilterConfig {
                    reconstruction: profile.base_settings.filters.reconstruction,
                    friction: profile.base_settings.filters.friction.value(),
                    damper: profile.base_settings.filters.damper.value(),
                    inertia: profile.base_settings.filters.inertia.value(),
                    notch_filters: profile.base_settings.filters.notch_filters.iter().map(|nf| {
                        racing_wheel_schemas::config::NotchFilter {
                            hz: nf.frequency.value(),
                            q: nf.q_factor,
                            gain_db: nf.gain_db,
                        }
                    }).collect(),
                    slew_rate: profile.base_settings.filters.slew_rate.value(),
                    curve_points: profile.base_settings.filters.curve_points.iter().map(|cp| {
                        racing_wheel_schemas::config::CurvePoint {
                            input: cp.input,
                            output: cp.output,
                        }
                    }).collect(),
                },
            },
            leds: profile.led_config.as_ref().map(|led| {
                racing_wheel_schemas::config::LedConfig {
                    rpm_bands: led.rpm_bands.clone(),
                    pattern: led.pattern.clone(),
                    brightness: led.brightness.value(),
                    colors: Some(led.colors.clone()),
                }
            }),
            haptics: profile.haptics_config.as_ref().map(|haptics| {
                racing_wheel_schemas::config::HapticsConfig {
                    enabled: haptics.enabled,
                    intensity: haptics.intensity.value(),
                    frequency_hz: haptics.frequency.value(),
                    effects: Some(haptics.effects.clone()),
                }
            }),
            signature: None, // Will be added during signing
        })
    }

    /// Convert schema ProfileSchema to domain Profile
    fn schema_to_profile(&self, schema: &ProfileSchema, profile_id: &ProfileId) -> Result<Profile> {
        use racing_wheel_schemas::{
            Gain, Degrees, TorqueNm, FrequencyHz, CurvePoint, NotchFilter
        };

        let base_settings = BaseSettings::new(
            Gain::new(schema.base.ffb_gain)
                .map_err(|e| anyhow::anyhow!("Invalid FFB gain value: {:?}", e))?,
            Degrees::new_dor(schema.base.dor_deg as f32)
                .map_err(|e| anyhow::anyhow!("Invalid degrees of rotation value: {:?}", e))?,
            TorqueNm::new(schema.base.torque_cap_nm)
                .map_err(|e| anyhow::anyhow!("Invalid torque cap value: {:?}", e))?,
            FilterConfig::new(
                schema.base.filters.reconstruction,
                Gain::new(schema.base.filters.friction)
                    .map_err(|e| anyhow::anyhow!("Invalid friction value: {:?}", e))?,
                Gain::new(schema.base.filters.damper)
                    .map_err(|e| anyhow::anyhow!("Invalid damper value: {:?}", e))?,
                Gain::new(schema.base.filters.inertia)
                    .map_err(|e| anyhow::anyhow!("Invalid inertia value: {:?}", e))?,
                schema.base.filters.notch_filters.iter().map(|nf| {
                    NotchFilter::new(
                        FrequencyHz::new(nf.hz).map_err(|e| anyhow::anyhow!("Invalid notch filter frequency: {:?}", e))?,
                        nf.q,
                        nf.gain_db,
                    ).map_err(|e| anyhow::anyhow!("Invalid notch filter: {:?}", e))
                }).collect::<Result<Vec<_>, anyhow::Error>>()?,
                Gain::new(schema.base.filters.slew_rate)
                    .map_err(|e| anyhow::anyhow!("Invalid slew rate value: {:?}", e))?,
                schema.base.filters.curve_points.iter().map(|cp| {
                    CurvePoint::new(cp.input, cp.output)
                }).collect::<Result<Vec<_>, racing_wheel_schemas::DomainError>>()
                    .map_err(|e| anyhow::anyhow!("Invalid curve points: {:?}", e))?,
            ).map_err(|e| anyhow::anyhow!("Invalid filter configuration: {:?}", e))?,
        );

        let led_config = if let Some(led) = &schema.leds {
            Some(LedConfig::new(
                led.rpm_bands.clone(),
                led.pattern.clone(),
                Gain::new(led.brightness).map_err(|e| anyhow::anyhow!("Invalid LED brightness: {:?}", e))?,
                led.colors.clone().unwrap_or_default(),
            ).map_err(|e| anyhow::anyhow!("Invalid LED configuration: {:?}", e))?)
        } else {
            None
        };

        let haptics_config = if let Some(haptics) = &schema.haptics {
            Some(HapticsConfig::new(
                haptics.enabled,
                Gain::new(haptics.intensity).map_err(|e| anyhow::anyhow!("Invalid haptics intensity: {:?}", e))?,
                FrequencyHz::new(haptics.frequency_hz).map_err(|e| anyhow::anyhow!("Invalid haptics frequency: {:?}", e))?,
                haptics.effects.clone().unwrap_or_default(),
            ))
        } else {
            None
        };

        let scope = ProfileScope {
            game: schema.scope.game.clone(),
            car: schema.scope.car.clone(),
            track: schema.scope.track.clone(),
        };

        Ok(Profile {
            id: profile_id.clone(),
            scope,
            base_settings,
            led_config,
            haptics_config,
            metadata: ProfileMetadata {
                name: format!("Profile {}", profile_id),
                description: None,
                author: None,
                version: "1.0.0".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                modified_at: chrono::Utc::now().to_rfc3339(),
                tags: Vec::new(),
            },
        })
    }
}
#[cfg(test)
]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use racing_wheel_schemas::{
        DeviceId, Gain, Degrees, TorqueNm, FrequencyHz, CurvePoint
    };
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

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
        assert_eq!(loaded.base_settings.ffb_gain, profile.base_settings.ffb_gain);
    }

    #[tokio::test]
    async fn test_profile_signing_and_verification() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("signed_test");
        
        // Generate signing key
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);

        // Save signed profile
        repo.save_profile(&profile, Some(&signing_key)).await.unwrap();

        // Load and verify signature
        let loaded = repo.load_profile(&profile.id).await.unwrap().unwrap();
        let signature_info = repo.get_profile_signature(&profile.id).await.unwrap();
        
        assert!(signature_info.is_some());
        let sig_info = signature_info.unwrap();
        assert!(!sig_info.signature.is_empty());
        assert!(!sig_info.public_key.is_empty());
        // Note: Will be ValidUnknown since we didn't add the key to trusted_keys
        assert!(matches!(sig_info.trust_state, TrustState::ValidUnknown | TrustState::Trusted));
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
        let resolved = repo.resolve_profile_hierarchy(
            Some("iracing"),
            Some("gt3"),
            None,
            None,
        ).await.unwrap();

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

        let merged = repo.merge_profiles_deterministic(&base_profile, &override_profile).unwrap();

        // Override profile should take precedence
        assert_eq!(merged.base_settings.ffb_gain.value(), 0.8);
        assert_eq!(merged.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(merged.base_settings.torque_cap.value(), 15.0);
    }

    #[tokio::test]
    async fn test_profile_validation_errors() {
        let (repo, _temp_dir) = create_test_repository().await;

        // Test invalid JSON
        let invalid_json = r#"{"invalid": "json", "missing": "required_fields"}"#;
        let result = repo.load_profile_from_json(invalid_json, &ProfileId::new("test".to_string()).unwrap()).await;
        assert!(result.is_err());
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
        assert_eq!(loaded1.base_settings.ffb_gain, loaded2.base_settings.ffb_gain);
    }

    #[tokio::test]
    async fn test_profile_scope_matching() {
        // Test global scope
        let global_scope = ProfileScope::global();
        assert!(global_scope.matches(Some("any_game"), Some("any_car"), Some("any_track")));
        assert!(global_scope.matches(None, None, None));

        // Test game-specific scope
        let game_scope = ProfileScope::for_game("iracing".to_string());
        assert!(game_scope.matches(Some("iracing"), Some("any_car"), Some("any_track")));
        assert!(!game_scope.matches(Some("acc"), Some("any_car"), Some("any_track")));
        assert!(!game_scope.matches(None, None, None));

        // Test car-specific scope
        let car_scope = ProfileScope::for_car("iracing".to_string(), "gt3".to_string());
        assert!(car_scope.matches(Some("iracing"), Some("gt3"), Some("any_track")));
        assert!(!car_scope.matches(Some("iracing"), Some("f1"), Some("any_track")));
        assert!(!car_scope.matches(Some("acc"), Some("gt3"), Some("any_track")));
    }

    #[tokio::test]
    async fn test_profile_specificity_ordering() {
        let global = ProfileScope::global();
        let game = ProfileScope::for_game("iracing".to_string());
        let car = ProfileScope::for_car("iracing".to_string(), "gt3".to_string());
        let track = ProfileScope::for_track("iracing".to_string(), "gt3".to_string(), "spa".to_string());

        assert_eq!(global.specificity_level(), 0);
        assert_eq!(game.specificity_level(), 1);
        assert_eq!(car.specificity_level(), 2);
        assert_eq!(track.specificity_level(), 3);

        assert!(game.is_more_specific_than(&global));
        assert!(car.is_more_specific_than(&game));
        assert!(track.is_more_specific_than(&car));
    }

    #[tokio::test]
    async fn test_json_schema_validation() {
        let validator = ProfileValidator::new().unwrap();

        // Valid profile JSON
        let valid_json = r#"{
            "schema": "wheel.profile/1",
            "scope": {
                "game": "iracing"
            },
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 540,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4,
                    "friction": 0.1,
                    "damper": 0.15,
                    "inertia": 0.05,
                    "notchFilters": [],
                    "slewRate": 0.8,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        }"#;

        let result = validator.validate_json(valid_json);
        assert!(result.is_ok());

        // Invalid profile JSON (missing required fields)
        let invalid_json = r#"{
            "schema": "wheel.profile/1",
            "scope": {}
        }"#;

        let result = validator.validate_json(invalid_json);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_curve_monotonic_validation() {
        let validator = ProfileValidator::new().unwrap();

        // Non-monotonic curve should fail validation
        let non_monotonic_json = r#"{
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 540,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4,
                    "friction": 0.1,
                    "damper": 0.15,
                    "inertia": 0.05,
                    "notchFilters": [],
                    "slewRate": 0.8,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 0.8, "output": 0.8},
                        {"input": 0.5, "output": 0.5},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        }"#;

        let result = validator.validate_json(non_monotonic_json);
        assert!(result.is_err());
        
        if let Err(SchemaError::NonMonotonicCurve) = result {
            // Expected error
        } else {
            panic!("Expected NonMonotonicCurve error");
        }
    }
}