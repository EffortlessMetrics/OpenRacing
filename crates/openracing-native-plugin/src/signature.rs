//! Signature verification for native plugins.

use std::path::Path;

use openracing_crypto::ed25519::{Ed25519Verifier, Signature};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::{SignatureMetadata, TrustLevel};

use crate::error::NativePluginError;

/// Configuration for signature verification.
#[derive(Debug, Clone)]
pub struct SignatureVerificationConfig {
    /// Whether to require signature verification for signed plugins.
    pub require_signatures: bool,
    /// Whether to allow loading unsigned plugins.
    pub allow_unsigned: bool,
}

impl Default for SignatureVerificationConfig {
    fn default() -> Self {
        Self {
            require_signatures: true,
            allow_unsigned: false,
        }
    }
}

impl SignatureVerificationConfig {
    /// Create a strict configuration (production mode).
    pub fn strict() -> Self {
        Self {
            require_signatures: true,
            allow_unsigned: false,
        }
    }

    /// Create a permissive configuration.
    pub fn permissive() -> Self {
        Self {
            require_signatures: true,
            allow_unsigned: true,
        }
    }

    /// Create a development configuration (no verification).
    pub fn development() -> Self {
        Self {
            require_signatures: false,
            allow_unsigned: true,
        }
    }
}

/// Result of signature verification.
#[derive(Debug, Clone)]
pub struct SignatureVerificationResult {
    /// Whether the plugin is signed.
    pub is_signed: bool,
    /// Signature metadata (if signed).
    pub metadata: Option<SignatureMetadata>,
    /// Trust level of the signer.
    pub trust_level: TrustLevel,
    /// Whether verification passed.
    pub verified: bool,
    /// Any warnings generated during verification.
    pub warnings: Vec<String>,
}

/// Signature verifier for native plugins.
pub struct SignatureVerifier<'a> {
    trust_store: &'a TrustStore,
    config: SignatureVerificationConfig,
}

impl<'a> SignatureVerifier<'a> {
    /// Create a new signature verifier.
    pub fn new(trust_store: &'a TrustStore, config: SignatureVerificationConfig) -> Self {
        Self {
            trust_store,
            config,
        }
    }

    /// Verify the plugin's signature.
    ///
    /// # Arguments
    ///
    /// * `library_path` - Path to the plugin shared library.
    ///
    /// # Returns
    ///
    /// * `Ok(SignatureVerificationResult)` - Verification result.
    /// * `Err(NativePluginError)` - Verification failed.
    pub fn verify(
        &self,
        library_path: &Path,
    ) -> Result<SignatureVerificationResult, NativePluginError> {
        let has_signature = self.signature_exists(library_path);

        if !has_signature {
            return self.handle_unsigned_plugin(library_path);
        }

        self.verify_signed_plugin(library_path)
    }

    /// Check if a signature file exists for the plugin.
    fn signature_exists(&self, library_path: &Path) -> bool {
        let ext = library_path
            .extension()
            .map_or(String::new(), |e| e.to_string_lossy().into_owned());
        let sig_path = library_path.with_extension(format!("{}.sig", ext));
        sig_path.exists()
    }

    /// Handle unsigned plugin verification.
    fn handle_unsigned_plugin(
        &self,
        library_path: &Path,
    ) -> Result<SignatureVerificationResult, NativePluginError> {
        if self.config.require_signatures && !self.config.allow_unsigned {
            tracing::warn!(
                path = %library_path.display(),
                "Rejecting unsigned native plugin"
            );
            return Err(NativePluginError::UnsignedPlugin {
                path: library_path.to_path_buf(),
            });
        }

        if self.config.allow_unsigned {
            tracing::warn!(
                path = %library_path.display(),
                "Loading unsigned native plugin (allow_unsigned=true)"
            );
            return Ok(SignatureVerificationResult {
                is_signed: false,
                metadata: None,
                trust_level: TrustLevel::Unknown,
                verified: true,
                warnings: vec!["Plugin is unsigned".to_string()],
            });
        }

        Err(NativePluginError::UnsignedPlugin {
            path: library_path.to_path_buf(),
        })
    }

    /// Verify a signed plugin.
    fn verify_signed_plugin(
        &self,
        library_path: &Path,
    ) -> Result<SignatureVerificationResult, NativePluginError> {
        let metadata = self.read_signature_metadata(library_path)?;

        let trust_level = self.trust_store.get_trust_level(&metadata.key_fingerprint);

        match trust_level {
            TrustLevel::Distrusted => {
                tracing::error!(
                    path = %library_path.display(),
                    fingerprint = %metadata.key_fingerprint,
                    "Rejecting plugin signed by distrusted key"
                );
                return Err(NativePluginError::DistrustedSigner {
                    fingerprint: metadata.key_fingerprint.clone(),
                });
            }
            TrustLevel::Unknown => {
                tracing::warn!(
                    path = %library_path.display(),
                    fingerprint = %metadata.key_fingerprint,
                    "Plugin signed by unknown key"
                );
            }
            TrustLevel::Trusted => {
                tracing::debug!(
                    path = %library_path.display(),
                    fingerprint = %metadata.key_fingerprint,
                    "Plugin signed by trusted key"
                );
            }
        }

        let public_key = match self.trust_store.get_public_key(&metadata.key_fingerprint) {
            Some(key) => key,
            None => {
                if trust_level == TrustLevel::Unknown && !self.config.allow_unsigned {
                    return Err(NativePluginError::UntrustedSigner {
                        fingerprint: metadata.key_fingerprint.clone(),
                    });
                }
                tracing::warn!(
                    path = %library_path.display(),
                    "Loading plugin with unverifiable signature (key not in trust store)"
                );
                return Ok(SignatureVerificationResult {
                    is_signed: true,
                    metadata: Some(metadata),
                    trust_level,
                    verified: true,
                    warnings: vec!["Key not in trust store".to_string()],
                });
            }
        };

        let content = std::fs::read(library_path)?;
        let signature = Signature::from_base64(&metadata.signature)
            .map_err(|e| NativePluginError::SignatureVerificationFailed(e.to_string()))?;

        let is_valid = Ed25519Verifier::verify(&content, &signature, &public_key)
            .map_err(|e| NativePluginError::SignatureVerificationFailed(e.to_string()))?;

        if !is_valid {
            tracing::error!(
                path = %library_path.display(),
                "Plugin signature verification failed"
            );
            return Err(NativePluginError::SignatureVerificationFailed(
                "Signature verification failed".to_string(),
            ));
        }

        tracing::info!(
            path = %library_path.display(),
            signer = %metadata.signer,
            "Plugin signature verified successfully"
        );

        Ok(SignatureVerificationResult {
            is_signed: true,
            metadata: Some(metadata),
            trust_level,
            verified: true,
            warnings: vec![],
        })
    }

    /// Read signature metadata from the signature file.
    fn read_signature_metadata(
        &self,
        library_path: &Path,
    ) -> Result<SignatureMetadata, NativePluginError> {
        let ext = library_path
            .extension()
            .map_or(String::new(), |e| e.to_string_lossy().into_owned());
        let sig_path = library_path.with_extension(format!("{}.sig", ext));

        let sig_content = std::fs::read_to_string(&sig_path)?;
        let metadata: SignatureMetadata = serde_json::from_str(&sig_content)?;

        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_strict() {
        let config = SignatureVerificationConfig::default();
        assert!(config.require_signatures);
        assert!(!config.allow_unsigned);
    }

    #[test]
    fn test_strict_config() {
        let config = SignatureVerificationConfig::strict();
        assert!(config.require_signatures);
        assert!(!config.allow_unsigned);
    }

    #[test]
    fn test_permissive_config() {
        let config = SignatureVerificationConfig::permissive();
        assert!(config.require_signatures);
        assert!(config.allow_unsigned);
    }

    #[test]
    fn test_development_config() {
        let config = SignatureVerificationConfig::development();
        assert!(!config.require_signatures);
        assert!(config.allow_unsigned);
    }
}
