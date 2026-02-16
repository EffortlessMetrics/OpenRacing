//! High-level verification interface for Racing Wheel Suite

use super::{
    ContentType, CryptoError, SignatureVerifier, VerificationConfig, VerificationResult,
    ed25519::Ed25519Verifier, trust_store::TrustStore,
};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{error, info, warn};

/// Main verification service for the Racing Wheel Suite
pub struct VerificationService {
    /// Ed25519 verifier implementation
    verifier: Ed25519Verifier,

    /// Configuration for verification behavior
    config: VerificationConfig,
}

impl VerificationService {
    /// Create a new verification service
    pub fn new(config: VerificationConfig) -> Result<Self> {
        let trust_store = TrustStore::new(config.trust_store_path.clone())
            .context("Failed to initialize trust store")?;

        let verifier = Ed25519Verifier::new(trust_store);

        Ok(Self { verifier, config })
    }

    /// Verify a binary file (wheeld, wheelctl, wheel-ui)
    pub fn verify_binary(&self, binary_path: &Path) -> Result<VerificationResult> {
        info!("Verifying binary: {}", binary_path.display());

        let result = self
            .verifier
            .verify_file(binary_path)
            .context("Binary verification failed")?;

        // Check if signature is required for binaries
        if self.config.require_binary_signatures && !result.signature_valid {
            return Err(CryptoError::VerificationFailed(
                "Binary signature required but verification failed".to_string(),
            )
            .into());
        }

        // Log verification result
        match result.signature_valid {
            true => info!("Binary signature verification: PASS"),
            false => warn!("Binary signature verification: FAIL"),
        }

        for warning in &result.warnings {
            warn!("Binary verification warning: {}", warning);
        }

        Ok(result)
    }

    /// Verify a firmware file
    pub fn verify_firmware(&self, firmware_path: &Path) -> Result<VerificationResult> {
        info!("Verifying firmware: {}", firmware_path.display());

        let result = self
            .verifier
            .verify_file(firmware_path)
            .context("Firmware verification failed")?;

        // Firmware signatures are always required for safety
        if self.config.require_firmware_signatures && !result.signature_valid {
            return Err(CryptoError::VerificationFailed(
                "Firmware signature required but verification failed".to_string(),
            )
            .into());
        }

        // Log verification result
        match result.signature_valid {
            true => info!("Firmware signature verification: PASS"),
            false => error!("Firmware signature verification: FAIL"),
        }

        Ok(result)
    }

    /// Verify a plugin file
    pub fn verify_plugin(&self, plugin_path: &Path) -> Result<VerificationResult> {
        info!("Verifying plugin: {}", plugin_path.display());

        // Try to verify signature if present
        match self.verifier.verify_file(plugin_path) {
            Ok(result) => {
                // Check signature requirement
                if self.config.require_plugin_signatures && !result.signature_valid {
                    return Err(CryptoError::VerificationFailed(
                        "Plugin signature required but verification failed".to_string(),
                    )
                    .into());
                }

                // Log result
                match result.signature_valid {
                    true => info!("Plugin signature verification: PASS"),
                    false => warn!("Plugin signature verification: FAIL (unsigned plugin)"),
                }

                Ok(result)
            }
            Err(e) => {
                // If no signature found and signatures are not required, allow it
                if !self.config.require_plugin_signatures {
                    warn!("Plugin has no signature, allowing unsigned plugin: {}", e);

                    // Create a result indicating no signature
                    Ok(VerificationResult {
                        signature_valid: false,
                        trust_level: super::TrustLevel::Unknown,
                        metadata: super::SignatureMetadata {
                            signature: String::new(),
                            key_fingerprint: String::new(),
                            signer: "unsigned".to_string(),
                            timestamp: chrono::Utc::now(),
                            content_type: ContentType::Plugin,
                            comment: Some("Unsigned plugin".to_string()),
                        },
                        warnings: vec!["Plugin is not signed".to_string()],
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Verify a profile file
    pub fn verify_profile(&self, profile_path: &Path) -> Result<Option<VerificationResult>> {
        info!("Verifying profile: {}", profile_path.display());

        // Profiles are optionally signed
        match self.verifier.verify_file(profile_path) {
            Ok(result) => {
                info!("Profile signature verification: PASS");
                Ok(Some(result))
            }
            Err(_) => {
                // No signature found - this is OK for profiles
                info!("Profile has no signature (unsigned profile)");
                Ok(None)
            }
        }
    }

    /// Verify an update package
    pub fn verify_update(&self, update_path: &Path) -> Result<VerificationResult> {
        info!("Verifying update package: {}", update_path.display());

        let result = self
            .verifier
            .verify_file(update_path)
            .context("Update package verification failed")?;

        // Updates must be signed
        if !result.signature_valid {
            return Err(CryptoError::VerificationFailed(
                "Update package signature verification failed".to_string(),
            )
            .into());
        }

        info!("Update package signature verification: PASS");
        Ok(result)
    }

    /// Batch verify multiple files
    pub fn verify_batch(
        &self,
        files: &[(PathBuf, ContentType)],
    ) -> Result<Vec<(PathBuf, Result<VerificationResult>)>> {
        let mut results = Vec::new();

        for (path, content_type) in files {
            let result = match content_type {
                ContentType::Binary => self.verify_binary(path),
                ContentType::Firmware => self.verify_firmware(path),
                ContentType::Plugin => self.verify_plugin(path),
                ContentType::Profile => match self.verify_profile(path)? {
                    Some(result) => Ok(result),
                    None => Err(anyhow::anyhow!("No signature found")),
                },
                ContentType::Update => self.verify_update(path),
            };

            results.push((path.clone(), result));
        }

        Ok(results)
    }

    /// Verify content directly with signature metadata
    ///
    /// This is useful for verifying embedded signatures (e.g., in firmware bundles)
    /// where the content and signature metadata are both in memory.
    pub fn verify_content(
        &self,
        content: &[u8],
        metadata: &super::SignatureMetadata,
    ) -> Result<VerificationResult> {
        use super::SignatureVerifier;
        self.verifier.verify_content(content, metadata)
    }

    /// Get verification configuration
    pub fn get_config(&self) -> &VerificationConfig {
        &self.config
    }

    /// Update verification configuration
    pub fn update_config(&mut self, new_config: VerificationConfig) -> Result<()> {
        // If trust store path changed, reload trust store
        if new_config.trust_store_path != self.config.trust_store_path {
            let trust_store = TrustStore::new(new_config.trust_store_path.clone())
                .context("Failed to load new trust store")?;

            self.verifier = Ed25519Verifier::new(trust_store);
        }

        self.config = new_config;
        Ok(())
    }
}

/// Utility functions for verification
pub mod utils {
    use super::*;

    /// Check if a file should be verified based on its extension and content type
    pub fn should_verify_file(
        _path: &Path,
        content_type: &ContentType,
        config: &VerificationConfig,
    ) -> bool {
        match content_type {
            ContentType::Binary => config.require_binary_signatures,
            ContentType::Firmware => config.require_firmware_signatures,
            ContentType::Plugin => config.require_plugin_signatures,
            ContentType::Profile => false, // Profiles are optionally signed
            ContentType::Update => true,   // Updates are always verified
        }
    }

    /// Determine content type from file path
    pub fn detect_content_type(path: &Path) -> Option<ContentType> {
        let filename = path.file_name()?.to_string_lossy().to_lowercase();
        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase());

        // Check by filename
        if filename.starts_with("wheeld")
            || filename.starts_with("wheelctl")
            || filename.starts_with("wheel-ui")
        {
            return Some(ContentType::Binary);
        }

        // Check by extension
        match extension.as_deref() {
            Some("exe") => Some(ContentType::Binary),
            Some("fw") | Some("hex") => Some(ContentType::Firmware),
            Some("bin") => Some(ContentType::Binary), // Could be either, default to binary
            Some("wasm") | Some("so") | Some("dll") | Some("dylib") => Some(ContentType::Plugin),
            Some("json") if filename.contains("profile") => Some(ContentType::Profile),
            Some("wup") | Some("update") => Some(ContentType::Update), // Wheel Update Package
            _ => None,
        }
    }

    /// Create a verification report for a directory
    pub fn create_verification_report(
        service: &VerificationService,
        directory: &Path,
    ) -> Result<VerificationReport> {
        let mut report = VerificationReport::default();

        // Walk directory and find files to verify
        for entry in walkdir::WalkDir::new(directory) {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if path.is_file()
                && let Some(content_type) = detect_content_type(path)
            {
                let should_verify = should_verify_file(path, &content_type, service.get_config());

                if should_verify {
                    let result = match content_type {
                        ContentType::Binary => service.verify_binary(path),
                        ContentType::Firmware => service.verify_firmware(path),
                        ContentType::Plugin => service.verify_plugin(path),
                        ContentType::Profile => match service.verify_profile(path)? {
                            Some(result) => Ok(result),
                            None => continue, // Skip unsigned profiles
                        },
                        ContentType::Update => service.verify_update(path),
                    };

                    match result {
                        Ok(verification_result) => {
                            if verification_result.signature_valid {
                                report.verified_files.push(path.to_path_buf());
                            } else {
                                report
                                    .failed_files
                                    .push((path.to_path_buf(), "Invalid signature".to_string()));
                            }
                        }
                        Err(e) => {
                            report
                                .failed_files
                                .push((path.to_path_buf(), e.to_string()));
                        }
                    }
                }
            }
        }

        Ok(report)
    }
}

/// Report of verification results for a directory
#[derive(Debug, Default)]
pub struct VerificationReport {
    pub verified_files: Vec<PathBuf>,
    pub failed_files: Vec<(PathBuf, String)>,
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_content_type_detection() -> Result<(), Box<dyn std::error::Error>> {
        // Test binary detection
        let result = utils::detect_content_type(Path::new("wheeld.exe"));
        assert!(matches!(result, Some(ContentType::Binary)));

        // Test firmware detection
        let result = utils::detect_content_type(Path::new("firmware.fw"));
        assert!(matches!(result, Some(ContentType::Firmware)));

        // Test plugin detection
        let result = utils::detect_content_type(Path::new("plugin.wasm"));
        assert!(matches!(result, Some(ContentType::Plugin)));

        // Test profile detection
        let result = utils::detect_content_type(Path::new("car.profile.json"));
        assert!(matches!(result, Some(ContentType::Profile)));

        // Test update detection
        let result = utils::detect_content_type(Path::new("v1.0.0.wup"));
        assert!(matches!(result, Some(ContentType::Update)));

        Ok(())
    }

    #[test]
    fn test_verification_service_creation() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let config = VerificationConfig {
            trust_store_path: temp_dir.path().join("trust_store.json"),
            ..Default::default()
        };

        let service = VerificationService::new(config)?;
        assert!(service.get_config().require_binary_signatures);

        Ok(())
    }

    #[test]
    fn test_should_verify_file() -> Result<(), Box<dyn std::error::Error>> {
        let config = VerificationConfig::default();

        // Binary should be verified by default
        assert!(utils::should_verify_file(
            Path::new("test.exe"),
            &ContentType::Binary,
            &config
        ));

        // Firmware should be verified by default
        assert!(utils::should_verify_file(
            Path::new("test.fw"),
            &ContentType::Firmware,
            &config
        ));

        // Plugin should be verified by default (secure-by-default)
        assert!(utils::should_verify_file(
            Path::new("test.wasm"),
            &ContentType::Plugin,
            &config
        ));

        // Profile should NOT be verified (optional)
        assert!(!utils::should_verify_file(
            Path::new("test.profile.json"),
            &ContentType::Profile,
            &config
        ));

        // Update should always be verified
        assert!(utils::should_verify_file(
            Path::new("test.wup"),
            &ContentType::Update,
            &config
        ));

        Ok(())
    }
}
