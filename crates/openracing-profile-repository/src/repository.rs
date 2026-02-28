//! Profile repository core implementation

use crate::signature::{ProfileSignature, TrustState};
use crate::storage::FileStorage;
use anyhow::Context;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use racing_wheel_schemas::config::{ProfileSchema, ProfileValidator, SchemaError};
use racing_wheel_schemas::migration::{MigrationConfig, ProfileMigrationService};
use racing_wheel_schemas::prelude::{
    BaseSettings, FilterConfig, HapticsConfig, LedConfig, Profile, ProfileId, ProfileMetadata,
    ProfileScope,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use ed25519_dalek::Signature;

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

impl ProfileRepositoryConfig {
    /// Create a new configuration with the specified profiles directory
    pub fn new(profiles_dir: impl Into<PathBuf>) -> Self {
        Self {
            profiles_dir: profiles_dir.into(),
            ..Default::default()
        }
    }

    /// Add a trusted public key
    pub fn with_trusted_key(mut self, key: impl Into<String>) -> Self {
        self.trusted_keys.push(key.into());
        self
    }

    /// Set auto-migrate option
    pub fn with_auto_migrate(mut self, enabled: bool) -> Self {
        self.auto_migrate = enabled;
        self
    }

    /// Set backup on migrate option
    pub fn with_backup_on_migrate(mut self, enabled: bool) -> Self {
        self.backup_on_migrate = enabled;
        self
    }
}

/// File-based profile repository with validation and signing support
pub struct ProfileRepository {
    config: ProfileRepositoryConfig,
    validator: ProfileValidator,
    storage: FileStorage,
    cache: RwLock<HashMap<ProfileId, Profile>>,
    signature_cache: RwLock<HashMap<ProfileId, Option<ProfileSignature>>>,
}

impl ProfileRepository {
    /// Create a new profile repository
    ///
    /// # Error Recovery
    ///
    /// If directory creation fails, the error is returned immediately.
    /// Profile loading failures are logged but don't prevent repository creation.
    pub async fn new(config: ProfileRepositoryConfig) -> anyhow::Result<Self> {
        let storage = FileStorage::new(&config.profiles_dir).await?;
        let validator = ProfileValidator::new().context("Failed to create profile validator")?;

        let repository = Self {
            config,
            validator,
            storage,
            cache: RwLock::new(HashMap::new()),
            signature_cache: RwLock::new(HashMap::new()),
        };

        repository.load_all_profiles().await?;

        Ok(repository)
    }

    /// Save a profile to disk with optional signing
    ///
    /// # Error Recovery
    ///
    /// - Uses atomic write operations (write to temp, then rename)
    /// - Failed writes don't affect existing files
    /// - Cache is only updated after successful disk write
    pub async fn save_profile(
        &self,
        profile: &Profile,
        sign_with_key: Option<&SigningKey>,
    ) -> anyhow::Result<()> {
        info!(profile_id = %profile.id, "Saving profile");

        let profile_schema = self.profile_to_schema(profile)?;

        let mut json = serde_json::to_string_pretty(&profile_schema)
            .context("Failed to serialize profile to JSON")?;

        let signature_info = if let Some(signing_key) = sign_with_key {
            let signature = self.sign_profile_json(&json, signing_key)?;

            let mut profile_with_sig = profile_schema;
            profile_with_sig.signature = Some(signature.signature.clone());

            json = serde_json::to_string_pretty(&profile_with_sig)
                .context("Failed to serialize signed profile to JSON")?;

            Some(signature)
        } else {
            None
        };

        self.validator
            .validate_json(&json)
            .context("Profile validation failed before saving")?;

        let file_path = self.get_profile_file_path(&profile.id);
        self.storage.write_atomic(&file_path, &json).await?;

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
    ///
    /// # Error Recovery
    ///
    /// - Returns Ok(None) if profile doesn't exist (not an error)
    /// - Returns Err for corrupt/invalid profiles
    /// - Cache is checked first to minimize disk I/O
    pub async fn load_profile(&self, profile_id: &ProfileId) -> anyhow::Result<Option<Profile>> {
        debug!(profile_id = %profile_id, "Loading profile");

        {
            let cache = self.cache.read().await;
            if let Some(profile) = cache.get(profile_id) {
                debug!(profile_id = %profile_id, "Profile found in cache");
                return Ok(Some(profile.clone()));
            }
        }

        let file_path = self.get_profile_file_path(profile_id);
        if !file_path.exists() {
            debug!(profile_id = %profile_id, "Profile file not found");
            return Ok(None);
        }

        let json = self.storage.read_to_string(&file_path).await?;

        let profile = self
            .load_profile_from_json(&json, profile_id, Some(file_path.as_path()))
            .await?;

        {
            let mut cache = self.cache.write().await;
            cache.insert(profile_id.clone(), profile.clone());
        }

        info!(profile_id = %profile_id, "Profile loaded successfully");
        Ok(Some(profile))
    }

    /// Load profile from JSON string with migration and validation
    async fn load_profile_from_json(
        &self,
        json: &str,
        profile_id: &ProfileId,
        source_path: Option<&std::path::Path>,
    ) -> anyhow::Result<Profile> {
        let effective_json = if self.config.auto_migrate {
            self.migrate_profile_json_if_needed(json, source_path)
                .await?
        } else {
            json.to_string()
        };

        let profile_schema =
            self.validator
                .validate_json(&effective_json)
                .map_err(|e| match e {
                    SchemaError::UnsupportedSchemaVersion(version) => {
                        anyhow::anyhow!("Unsupported profile schema version: {}", version)
                    }
                    other => anyhow::anyhow!("Profile validation failed: {}", other),
                })?;

        let signature_info = if let Some(ref sig_b64) = profile_schema.signature {
            Some(self.verify_profile_signature(&effective_json, sig_b64)?)
        } else {
            None
        };

        let profile = self.schema_to_profile(&profile_schema, profile_id)?;

        {
            let mut sig_cache = self.signature_cache.write().await;
            sig_cache.insert(profile_id.clone(), signature_info);
        }

        Ok(profile)
    }

    /// Migrate profile JSON to current schema and persist migration when needed
    async fn migrate_profile_json_if_needed(
        &self,
        json: &str,
        source_path: Option<&std::path::Path>,
    ) -> anyhow::Result<String> {
        let migration_service = ProfileMigrationService::new(MigrationConfig {
            backup_dir: self.config.profiles_dir.join("backups"),
            create_backups: self.config.backup_on_migrate,
            max_backups: 5,
            validate_after_migration: true,
        })
        .map_err(|e| anyhow::anyhow!("Failed to initialize migration service: {}", e))?;

        let outcome = migration_service
            .migrate_with_backup(
                json,
                if self.config.backup_on_migrate {
                    source_path
                } else {
                    None
                },
            )
            .map_err(|e| anyhow::anyhow!("Profile migration failed: {}", e))?;

        if !outcome.was_migrated() {
            return Ok(json.to_string());
        }

        if let Some(path) = source_path {
            self.storage
                .write_atomic(path, &outcome.migrated_json)
                .await?;
        }

        if let Some(backup_info) = &outcome.backup_info {
            info!(
                profile_path = ?backup_info.original_path,
                backup_path = ?backup_info.backup_path,
                from = %outcome.original_version,
                to = %outcome.target_version,
                "Profile migrated with backup"
            );
        } else {
            info!(
                from = %outcome.original_version,
                to = %outcome.target_version,
                "Profile migrated"
            );
        }

        Ok(outcome.migrated_json)
    }

    /// Delete a profile from disk and cache
    ///
    /// # Error Recovery
    ///
    /// - Returns Ok even if file doesn't exist (idempotent)
    /// - Cache is always cleared regardless of file deletion result
    pub async fn delete_profile(&self, profile_id: &ProfileId) -> anyhow::Result<()> {
        info!(profile_id = %profile_id, "Deleting profile");

        let file_path = self.get_profile_file_path(profile_id);

        if file_path.exists() {
            tokio::fs::remove_file(&file_path)
                .await
                .with_context(|| format!("Failed to delete profile file: {:?}", file_path))?;
        }

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
    pub async fn list_profiles(&self) -> anyhow::Result<Vec<Profile>> {
        debug!("Listing all profiles");

        let cache = self.cache.read().await;
        Ok(cache.values().cloned().collect())
    }

    /// Get profile signature information
    pub async fn get_profile_signature(
        &self,
        profile_id: &ProfileId,
    ) -> anyhow::Result<Option<ProfileSignature>> {
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
    ) -> anyhow::Result<Profile> {
        debug!(game = ?game, car = ?car, track = ?track, "Resolving profile hierarchy");

        let profiles = self.list_profiles().await?;

        let mut matching_profiles: Vec<&Profile> = profiles
            .iter()
            .filter(|p| p.scope.matches(game, car, track))
            .collect();

        matching_profiles.sort_by_key(|p| p.scope.specificity_level());

        let mut resolved = if let Some(global) = matching_profiles.first() {
            (*global).clone()
        } else {
            Profile::default_global().context("Failed to create default global profile")?
        };

        for profile in matching_profiles.iter().skip(1) {
            resolved = self.merge_profiles_deterministic(&resolved, profile)?;
        }

        if let Some(session) = session_overrides {
            resolved = self.merge_profiles_deterministic(&resolved, session)?;
        }

        debug!("Profile hierarchy resolved successfully");
        Ok(resolved)
    }

    /// Deterministic profile merge (other takes precedence)
    pub fn merge_profiles_deterministic(
        &self,
        base: &Profile,
        other: &Profile,
    ) -> anyhow::Result<Profile> {
        let mut merged = base.clone();

        merged.base_settings = other.base_settings.clone();

        if other.led_config.is_some() {
            merged.led_config = other.led_config.clone();
        }

        if other.haptics_config.is_some() {
            merged.haptics_config = other.haptics_config.clone();
        }

        merged.metadata.modified_at = chrono::Utc::now().to_rfc3339();
        merged.metadata.name = format!("Merged: {} + {}", base.metadata.name, other.metadata.name);

        Ok(merged)
    }

    /// Load all profiles from disk into cache
    async fn load_all_profiles(&self) -> anyhow::Result<()> {
        info!("Loading all profiles from disk");

        let mut entries = tokio::fs::read_dir(&self.config.profiles_dir)
            .await
            .with_context(|| {
                format!(
                    "Failed to read profiles directory: {:?}",
                    self.config.profiles_dir
                )
            })?;

        let mut loaded_count = 0;
        let mut error_count = 0;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && let Some(file_stem) = path.file_stem().and_then(|s| s.to_str())
            {
                match file_stem.parse::<ProfileId>() {
                    Ok(profile_id) => match self.load_profile(&profile_id).await {
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
                    },
                    Err(e) => {
                        warn!(path = ?path, error = %e, "Invalid profile ID in filename");
                        error_count += 1;
                    }
                }
            }
        }

        info!(
            loaded = loaded_count,
            errors = error_count,
            "Profile loading completed"
        );
        Ok(())
    }

    /// Get file path for a profile
    pub fn get_profile_file_path(&self, profile_id: &ProfileId) -> PathBuf {
        self.config
            .profiles_dir
            .join(format!("{}.json", profile_id))
    }

    /// Get the repository configuration
    pub fn config(&self) -> &ProfileRepositoryConfig {
        &self.config
    }

    /// Clear the in-memory cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        let mut sig_cache = self.signature_cache.write().await;
        sig_cache.clear();
    }

    /// Reload all profiles from disk
    pub async fn reload(&self) -> anyhow::Result<()> {
        self.clear_cache().await;
        self.load_all_profiles().await
    }

    /// Sign profile JSON with Ed25519 key
    fn sign_profile_json(
        &self,
        json: &str,
        signing_key: &SigningKey,
    ) -> anyhow::Result<ProfileSignature> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash = hasher.finalize();

        let signature = signing_key.sign(&hash);
        let public_key = signing_key.verifying_key();

        Ok(ProfileSignature {
            signature: BASE64.encode(signature.to_bytes()),
            public_key: BASE64.encode(public_key.to_bytes()),
            trust_state: TrustState::Trusted,
        })
    }

    /// Verify profile signature
    fn verify_profile_signature(
        &self,
        json: &str,
        signature_b64: &str,
    ) -> anyhow::Result<ProfileSignature> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        use sha2::{Digest, Sha256};

        let value: serde_json::Value = serde_json::from_str(json)
            .context("Failed to parse JSON for signature verification")?;

        let signature_bytes = BASE64
            .decode(signature_b64)
            .context("Failed to decode signature from base64")?;

        let signature = Signature::from_bytes(
            &signature_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
        );

        let public_key_b64 = value
            .get("publicKey")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if public_key_b64.is_empty() {
            return Ok(ProfileSignature {
                signature: signature_b64.to_string(),
                public_key: String::new(),
                trust_state: TrustState::Invalid,
            });
        }

        let public_key_bytes = BASE64
            .decode(public_key_b64)
            .context("Failed to decode public key from base64")?;

        let public_key = VerifyingKey::from_bytes(
            &public_key_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid public key length"))?,
        )
        .context("Failed to create verifying key")?;

        let mut json_for_verification = value.clone();
        let json_obj = json_for_verification
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Profile JSON is not an object"))?;
        json_obj.remove("signature");
        json_obj.remove("publicKey");

        let json_without_sig = serde_json::to_string(&json_for_verification)
            .context("Failed to serialize JSON for verification")?;

        let mut hasher = Sha256::new();
        hasher.update(json_without_sig.as_bytes());
        let hash = hasher.finalize();

        let trust_state = match public_key.verify(&hash, &signature) {
            Ok(()) => {
                if self
                    .config
                    .trusted_keys
                    .contains(&public_key_b64.to_string())
                {
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
    fn profile_to_schema(&self, profile: &Profile) -> anyhow::Result<ProfileSchema> {
        Ok(ProfileSchema {
            schema: "wheel.profile/1".to_string(),
            scope: racing_wheel_schemas::config::ProfileScope {
                game: profile.scope.game.clone(),
                car: profile.scope.car.clone(),
                track: profile.scope.track.clone(),
            },
            base: racing_wheel_schemas::config::BaseConfig {
                ffb_gain: profile.base_settings.ffb_gain.value(),
                dor_deg: profile.base_settings.degrees_of_rotation.value() as u16,
                torque_cap_nm: profile.base_settings.torque_cap.value(),
                filters: racing_wheel_schemas::config::FilterConfig {
                    reconstruction: profile.base_settings.filters.reconstruction,
                    friction: profile.base_settings.filters.friction.value(),
                    damper: profile.base_settings.filters.damper.value(),
                    inertia: profile.base_settings.filters.inertia.value(),
                    notch_filters: profile
                        .base_settings
                        .filters
                        .notch_filters
                        .iter()
                        .map(|nf| racing_wheel_schemas::config::NotchFilter {
                            hz: nf.frequency.value(),
                            q: nf.q_factor,
                            gain_db: nf.gain_db,
                        })
                        .collect(),
                    slew_rate: profile.base_settings.filters.slew_rate.value(),
                    curve_points: profile
                        .base_settings
                        .filters
                        .curve_points
                        .iter()
                        .map(|cp| racing_wheel_schemas::config::CurvePoint {
                            input: cp.input,
                            output: cp.output,
                        })
                        .collect(),
                    torque_cap: Some(1.0),
                    bumpstop: racing_wheel_schemas::config::BumpstopConfig::default(),
                    hands_off: racing_wheel_schemas::config::HandsOffConfig::default(),
                },
            },
            leds: profile
                .led_config
                .as_ref()
                .map(|led| racing_wheel_schemas::config::LedConfig {
                    rpm_bands: led.rpm_bands.clone(),
                    pattern: led.pattern.clone(),
                    brightness: led.brightness.value(),
                    colors: Some(led.colors.clone()),
                }),
            haptics: profile.haptics_config.as_ref().map(|haptics| {
                racing_wheel_schemas::config::HapticsConfig {
                    enabled: haptics.enabled,
                    intensity: haptics.intensity.value(),
                    frequency_hz: haptics.frequency.value(),
                    effects: Some(haptics.effects.clone()),
                }
            }),
            signature: None,
        })
    }

    /// Convert schema ProfileSchema to domain Profile
    fn schema_to_profile(
        &self,
        schema: &ProfileSchema,
        profile_id: &ProfileId,
    ) -> anyhow::Result<Profile> {
        use racing_wheel_schemas::prelude::{
            CurvePoint, Degrees, FrequencyHz, Gain, NotchFilter, TorqueNm,
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
                schema
                    .base
                    .filters
                    .notch_filters
                    .iter()
                    .map(|nf| {
                        let freq = FrequencyHz::new(nf.hz).map_err(|e| {
                            anyhow::anyhow!("Invalid notch filter frequency: {:?}", e)
                        })?;
                        NotchFilter::new(freq, nf.q, nf.gain_db)
                            .map_err(|e| anyhow::anyhow!("Invalid notch filter: {:?}", e))
                    })
                    .collect::<anyhow::Result<Vec<_>, anyhow::Error>>()?,
                Gain::new(schema.base.filters.slew_rate)
                    .map_err(|e| anyhow::anyhow!("Invalid slew rate value: {:?}", e))?,
                schema
                    .base
                    .filters
                    .curve_points
                    .iter()
                    .map(|cp| CurvePoint::new(cp.input, cp.output))
                    .collect::<Result<Vec<_>, racing_wheel_schemas::prelude::DomainError>>()
                    .map_err(|e| anyhow::anyhow!("Invalid curve points: {:?}", e))?,
            )
            .map_err(|e| anyhow::anyhow!("Invalid filter configuration: {:?}", e))?,
        );

        let led_config = if let Some(led) = &schema.leds {
            Some(
                LedConfig::new(
                    led.rpm_bands.clone(),
                    led.pattern.clone(),
                    Gain::new(led.brightness)
                        .map_err(|e| anyhow::anyhow!("Invalid LED brightness: {:?}", e))?,
                    led.colors.clone().unwrap_or_default(),
                )
                .map_err(|e| anyhow::anyhow!("Invalid LED configuration: {:?}", e))?,
            )
        } else {
            None
        };

        let haptics_config = if let Some(haptics) = &schema.haptics {
            Some(HapticsConfig::new(
                haptics.enabled,
                Gain::new(haptics.intensity)
                    .map_err(|e| anyhow::anyhow!("Invalid haptics intensity: {:?}", e))?,
                FrequencyHz::new(haptics.frequency_hz)
                    .map_err(|e| anyhow::anyhow!("Invalid haptics frequency: {:?}", e))?,
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
            parent: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::prelude::{Degrees, Gain, TorqueNm};
    use tempfile::TempDir;

    fn must<T, E: std::fmt::Debug>(r: std::result::Result<T, E>) -> T {
        r.expect("operation should succeed")
    }

    fn valid_profile_id(value: &str) -> ProfileId {
        must(ProfileId::new(value.to_string()))
    }

    fn valid_gain(value: f32) -> Gain {
        must(Gain::new(value))
    }

    fn valid_dor(value: f32) -> Degrees {
        must(Degrees::new_dor(value))
    }

    fn valid_torque(value: f32) -> TorqueNm {
        must(TorqueNm::new(value))
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

    #[tokio::test]
    async fn test_repository_creation() {
        let (_repo, _temp_dir) = create_test_repository().await;
    }

    #[tokio::test]
    async fn test_save_and_load_profile() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("test1");

        must(repo.save_profile(&profile, None).await);

        let loaded = must(repo.load_profile(&profile.id).await);
        assert!(loaded.is_some());
        let loaded = loaded.expect("profile should exist");

        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.scope, profile.scope);
    }

    #[tokio::test]
    async fn test_profile_deletion() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("delete_test");

        must(repo.save_profile(&profile, None).await);

        let loaded = must(repo.load_profile(&profile.id).await);
        assert!(loaded.is_some());

        must(repo.delete_profile(&profile.id).await);

        let loaded = must(repo.load_profile(&profile.id).await);
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_list_profiles() {
        let (repo, _temp_dir) = create_test_repository().await;

        let profile1 = create_test_profile("list1");
        let profile2 = create_test_profile("list2");
        let profile3 = create_test_profile("list3");

        must(repo.save_profile(&profile1, None).await);
        must(repo.save_profile(&profile2, None).await);
        must(repo.save_profile(&profile3, None).await);

        let profiles = must(repo.list_profiles().await);
        assert_eq!(profiles.len(), 3);
    }

    #[tokio::test]
    async fn test_profile_hierarchy_resolution() {
        let (repo, _temp_dir) = create_test_repository().await;

        let global_profile = Profile::new(
            valid_profile_id("global"),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: valid_gain(0.5),
                degrees_of_rotation: valid_dor(900.0),
                torque_cap: valid_torque(10.0),
                filters: FilterConfig::default(),
            },
            "Global Profile".to_string(),
        );

        let game_profile = Profile::new(
            valid_profile_id("iracing"),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings {
                ffb_gain: valid_gain(0.7),
                degrees_of_rotation: valid_dor(540.0),
                torque_cap: valid_torque(15.0),
                filters: FilterConfig::default(),
            },
            "iRacing Profile".to_string(),
        );

        must(repo.save_profile(&global_profile, None).await);
        must(repo.save_profile(&game_profile, None).await);

        let resolved = must(
            repo.resolve_profile_hierarchy(Some("iracing"), None, None, None)
                .await,
        );

        assert_eq!(resolved.base_settings.ffb_gain.value(), 0.7);
    }

    #[tokio::test]
    async fn test_deterministic_merge() {
        let (repo, _temp_dir) = create_test_repository().await;

        let base = Profile::new(
            valid_profile_id("base"),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: valid_gain(0.5),
                degrees_of_rotation: valid_dor(900.0),
                torque_cap: valid_torque(10.0),
                filters: FilterConfig::default(),
            },
            "Base".to_string(),
        );

        let override_profile = Profile::new(
            valid_profile_id("override"),
            ProfileScope::global(),
            BaseSettings {
                ffb_gain: valid_gain(0.8),
                degrees_of_rotation: valid_dor(540.0),
                torque_cap: valid_torque(15.0),
                filters: FilterConfig::default(),
            },
            "Override".to_string(),
        );

        let merged = must(repo.merge_profiles_deterministic(&base, &override_profile));

        assert_eq!(merged.base_settings.ffb_gain.value(), 0.8);
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let (repo, _temp_dir) = create_test_repository().await;
        let profile = create_test_profile("cache_test");

        must(repo.save_profile(&profile, None).await);

        let loaded = must(repo.load_profile(&profile.id).await);
        assert!(loaded.is_some());

        repo.clear_cache().await;

        let loaded_after_clear = must(repo.load_profile(&profile.id).await);
        assert!(loaded_after_clear.is_some());
    }
}
