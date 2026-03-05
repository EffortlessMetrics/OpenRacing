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
                // TODO(security): Fail-closed — cannot verify signature without
                // the public key in the trust store. Mark as unverified so callers
                // never treat an unchecked signature as valid.
                tracing::warn!(
                    path = %library_path.display(),
                    "Plugin signature NOT verified (signing key not in trust store)"
                );
                return Ok(SignatureVerificationResult {
                    is_signed: true,
                    metadata: Some(metadata),
                    trust_level,
                    verified: false,
                    warnings: vec![
                        "Signature present but NOT cryptographically verified: \
                         signing key not found in trust store"
                            .to_string(),
                    ],
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

    #[test]
    fn test_verify_signed_plugin_with_trusted_key() -> Result<(), Box<dyn std::error::Error>> {
        use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
        use openracing_crypto::verification::ContentType;

        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("test_plugin.dll");
        let plugin_data = b"fake plugin binary content";
        std::fs::write(&plugin_path, plugin_data)?;

        // Generate keypair and sign the plugin
        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_data,
            &keypair,
            "Trusted Author",
            ContentType::Plugin,
            None,
        )?;

        // Write the .dll.sig file
        let sig_path = plugin_path.with_extension("dll.sig");
        let sig_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&sig_path, sig_json)?;

        // Build trust store with the keypair's public key
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test trusted key".to_string()),
        )?;

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&plugin_path)?;

        assert!(result.is_signed);
        assert!(result.verified);
        assert_eq!(result.trust_level, TrustLevel::Trusted);
        assert!(result.warnings.is_empty());

        Ok(())
    }

    #[test]
    fn test_reject_unsigned_plugin_in_strict_mode() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("unsigned_plugin.dll");
        std::fs::write(&plugin_path, b"unsigned plugin")?;

        let trust_store = TrustStore::new_in_memory();
        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&plugin_path);

        assert!(result.is_err(), "strict mode must reject unsigned plugins");

        Ok(())
    }

    #[test]
    fn test_allow_unsigned_plugin_in_dev_mode() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("dev_plugin.dll");
        std::fs::write(&plugin_path, b"dev plugin")?;

        let trust_store = TrustStore::new_in_memory();
        let verifier =
            SignatureVerifier::new(&trust_store, SignatureVerificationConfig::development());
        let result = verifier.verify(&plugin_path)?;

        assert!(!result.is_signed);
        assert!(result.verified);
        assert_eq!(result.trust_level, TrustLevel::Unknown);

        Ok(())
    }

    #[test]
    fn test_reject_plugin_signed_by_distrusted_key() -> Result<(), Box<dyn std::error::Error>> {
        use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
        use openracing_crypto::verification::ContentType;

        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("distrusted_plugin.so");
        let plugin_data = b"plugin from distrusted source";
        std::fs::write(&plugin_path, plugin_data)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_data,
            &keypair,
            "Evil Author",
            ContentType::Plugin,
            None,
        )?;

        let sig_path = plugin_path.with_extension("so.sig");
        let sig_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&sig_path, sig_json)?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Distrusted,
            Some("Compromised key".to_string()),
        )?;

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&plugin_path);

        assert!(
            result.is_err(),
            "must reject plugins signed by distrusted key"
        );

        Ok(())
    }

    #[test]
    fn test_reject_plugin_with_tampered_content() -> Result<(), Box<dyn std::error::Error>> {
        use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
        use openracing_crypto::verification::ContentType;

        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("tampered_plugin.dll");
        let original_data = b"original plugin binary";
        std::fs::write(&plugin_path, original_data)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            original_data,
            &keypair,
            "Author",
            ContentType::Plugin,
            None,
        )?;

        let sig_path = plugin_path.with_extension("dll.sig");
        let sig_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&sig_path, sig_json)?;

        // Tamper with the plugin file after signing
        std::fs::write(&plugin_path, b"TAMPERED plugin binary")?;

        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&plugin_path);

        assert!(result.is_err(), "tampered content must fail verification");

        Ok(())
    }

    #[test]
    fn test_fail_closed_trust_store_rejects_signed_plugin() -> Result<(), Box<dyn std::error::Error>>
    {
        use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
        use openracing_crypto::verification::ContentType;

        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("fc_plugin.dll");
        let plugin_data = b"plugin for fail-closed test";
        std::fs::write(&plugin_path, plugin_data)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_data,
            &keypair,
            "Author",
            ContentType::Plugin,
            None,
        )?;

        let sig_path = plugin_path.with_extension("dll.sig");
        let sig_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&sig_path, sig_json)?;

        // Use fail-closed trust store — no key will be found
        let trust_store = TrustStore::new_fail_closed("simulated failure");

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&plugin_path);

        // The verifier must either error or return verified=false
        match result {
            Err(_) => { /* expected: fail-closed store causes rejection */ }
            Ok(r) => {
                assert!(
                    !r.verified || r.trust_level == TrustLevel::Distrusted,
                    "fail-closed store must not produce a trusted+verified result"
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_unknown_key_strict_mode_rejects() -> Result<(), Box<dyn std::error::Error>> {
        use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
        use openracing_crypto::verification::ContentType;

        let temp_dir = tempfile::TempDir::new()?;
        let plugin_path = temp_dir.path().join("unknown_key_plugin.dll");
        let plugin_data = b"plugin from unknown signer";
        std::fs::write(&plugin_path, plugin_data)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            plugin_data,
            &keypair,
            "Unknown Author",
            ContentType::Plugin,
            None,
        )?;

        let sig_path = plugin_path.with_extension("dll.sig");
        let sig_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&sig_path, sig_json)?;

        // Empty trust store (except default placeholder) — key is unknown
        let trust_store = TrustStore::new_in_memory();

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&plugin_path);

        // Unknown key + strict + allow_unsigned=false → should reject or mark unverified
        match result {
            Err(_) => { /* rejection is correct */ }
            Ok(r) => {
                assert!(
                    !r.verified,
                    "unknown key in strict mode must not be marked verified"
                );
            }
        }

        Ok(())
    }
}
