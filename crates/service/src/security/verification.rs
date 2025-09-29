//! Verification service for checking signatures on various components

use crate::security::{
    signature::{PublicKey, Signature, SignedContent},
    trust::{TrustStore, TrustLevel, Operation},
    SecurityConfig, SecurityError,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// Result of signature verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the signature is cryptographically valid
    pub signature_valid: bool,
    /// Trust level of the signing key
    pub trust_level: TrustLevel,
    /// Whether the operation is allowed for this key
    pub operation_allowed: bool,
    /// Public key that signed the content
    pub public_key: Option<PublicKey>,
    /// Error message if verification failed
    pub error: Option<String>,
}

impl VerificationResult {
    /// Check if verification passed all checks
    pub fn is_trusted(&self) -> bool {
        self.signature_valid && self.operation_allowed && self.trust_level >= TrustLevel::Trusted
    }

    /// Check if verification passed with minimum trust level
    pub fn is_trusted_with_level(&self, min_level: TrustLevel) -> bool {
        self.signature_valid && self.operation_allowed && self.trust_level >= min_level
    }
}

/// Verifier for checking signatures on different types of content
pub struct Verifier {
    trust_store: TrustStore,
    config: SecurityConfig,
}

impl Verifier {
    /// Create a new verifier
    pub fn new(trust_store: TrustStore, config: SecurityConfig) -> Self {
        Self {
            trust_store,
            config,
        }
    }

    /// Verify an application update package
    pub async fn verify_update(&self, package_path: &Path) -> Result<VerificationResult> {
        self.verify_file(package_path, &Operation::SignUpdates).await
    }

    /// Verify a firmware image
    pub async fn verify_firmware(&self, firmware_path: &Path) -> Result<VerificationResult> {
        self.verify_file(firmware_path, &Operation::SignFirmware).await
    }

    /// Verify a plugin package
    pub async fn verify_plugin(&self, plugin_path: &Path) -> Result<VerificationResult> {
        if self.config.allow_unsigned_plugins {
            // In development mode, allow unsigned plugins
            return Ok(VerificationResult {
                signature_valid: true,
                trust_level: TrustLevel::Trusted,
                operation_allowed: true,
                public_key: None,
                error: None,
            });
        }

        self.verify_file(plugin_path, &Operation::SignPlugins).await
    }

    /// Verify a profile file
    pub async fn verify_profile(&self, profile_path: &Path) -> Result<VerificationResult> {
        if self.config.allow_unsigned_profiles {
            // Profiles are optional by default
            return Ok(VerificationResult {
                signature_valid: true,
                trust_level: TrustLevel::Trusted,
                operation_allowed: true,
                public_key: None,
                error: None,
            });
        }

        self.verify_file(profile_path, &Operation::SignProfiles).await
    }

    /// Verify signed content directly
    pub fn verify_signed_content<T>(&self, content: &SignedContent<T>, operation: &Operation) -> VerificationResult
    where
        T: Serialize,
    {
        // Check signature validity
        let signature_valid = match content.verify() {
            Ok(()) => true,
            Err(e) => {
                return VerificationResult {
                    signature_valid: false,
                    trust_level: TrustLevel::Untrusted,
                    operation_allowed: false,
                    public_key: Some(content.public_key.clone()),
                    error: Some(format!("Signature verification failed: {}", e)),
                };
            }
        };

        // Check trust level and operation permission
        let trust_level = self.trust_store.get_trust_level(&content.public_key);
        let operation_allowed = self.trust_store.is_operation_allowed(&content.public_key, operation);

        VerificationResult {
            signature_valid,
            trust_level,
            operation_allowed,
            public_key: Some(content.public_key.clone()),
            error: None,
        }
    }

    /// Verify a file with embedded signature
    async fn verify_file(&self, file_path: &Path, operation: &Operation) -> Result<VerificationResult> {
        // Read the file
        let content = fs::read(file_path).await
            .context("Failed to read file for verification")?;

        // Look for signature metadata
        match self.extract_signature_metadata(&content).await {
            Ok(Some(metadata)) => {
                // Verify the signature
                let signature = Signature::from_base64(&metadata.signature)
                    .map_err(|e| SecurityError::Signature(e))?;

                let public_key = PublicKey::from_base64(&metadata.public_key)
                    .map_err(|e| SecurityError::Signature(e))?;

                // Verify signature against file content (excluding signature metadata)
                let content_to_verify = self.extract_content_without_signature(&content);
                
                let signature_valid = match public_key.verify(&content_to_verify, &signature) {
                    Ok(()) => true,
                    Err(e) => {
                        return Ok(VerificationResult {
                            signature_valid: false,
                            trust_level: TrustLevel::Untrusted,
                            operation_allowed: false,
                            public_key: Some(public_key),
                            error: Some(format!("Signature verification failed: {}", e)),
                        });
                    }
                };

                // Check trust and permissions
                let trust_level = self.trust_store.get_trust_level(&public_key);
                let operation_allowed = self.trust_store.is_operation_allowed(&public_key, operation);

                Ok(VerificationResult {
                    signature_valid,
                    trust_level,
                    operation_allowed,
                    public_key: Some(public_key),
                    error: None,
                })
            }
            Ok(None) => {
                // No signature found
                if self.config.require_signatures {
                    Ok(VerificationResult {
                        signature_valid: false,
                        trust_level: TrustLevel::Untrusted,
                        operation_allowed: false,
                        public_key: None,
                        error: Some("No signature found and signatures are required".to_string()),
                    })
                } else {
                    // Allow unsigned content in development mode
                    Ok(VerificationResult {
                        signature_valid: true,
                        trust_level: TrustLevel::Unknown,
                        operation_allowed: true,
                        public_key: None,
                        error: None,
                    })
                }
            }
            Err(e) => {
                Ok(VerificationResult {
                    signature_valid: false,
                    trust_level: TrustLevel::Untrusted,
                    operation_allowed: false,
                    public_key: None,
                    error: Some(format!("Failed to extract signature: {}", e)),
                })
            }
        }
    }

    /// Extract signature metadata from file content
    async fn extract_signature_metadata(&self, content: &[u8]) -> Result<Option<SignatureMetadata>> {
        // Look for signature block at the end of the file
        // Format: -----BEGIN WHEEL SIGNATURE-----\n{json}\n-----END WHEEL SIGNATURE-----
        
        let content_str = String::from_utf8_lossy(content);
        
        if let Some(start_pos) = content_str.rfind("-----BEGIN WHEEL SIGNATURE-----") {
            if let Some(end_pos) = content_str.rfind("-----END WHEEL SIGNATURE-----") {
                if end_pos > start_pos {
                    let signature_block = &content_str[start_pos..end_pos + "-----END WHEEL SIGNATURE-----".len()];
                    
                    // Extract JSON between markers
                    let json_start = start_pos + "-----BEGIN WHEEL SIGNATURE-----\n".len();
                    let json_end = end_pos;
                    
                    if json_end > json_start {
                        let json_str = &content_str[json_start..json_end];
                        
                        match serde_json::from_str::<SignatureMetadata>(json_str) {
                            Ok(metadata) => return Ok(Some(metadata)),
                            Err(e) => {
                                tracing::warn!("Failed to parse signature metadata: {}", e);
                            }
                        }
                    }
                }
            }
        }
        
        Ok(None)
    }

    /// Extract content without signature metadata for verification
    fn extract_content_without_signature(&self, content: &[u8]) -> Vec<u8> {
        let content_str = String::from_utf8_lossy(content);
        
        if let Some(start_pos) = content_str.rfind("-----BEGIN WHEEL SIGNATURE-----") {
            // Return content up to signature block
            content[..start_pos].to_vec()
        } else {
            // No signature block found, return full content
            content.to_vec()
        }
    }

    /// Update the trust store
    pub fn update_trust_store(&mut self, trust_store: TrustStore) {
        self.trust_store = trust_store;
    }

    /// Get a reference to the trust store
    pub fn trust_store(&self) -> &TrustStore {
        &self.trust_store
    }

    /// Get a mutable reference to the trust store
    pub fn trust_store_mut(&mut self) -> &mut TrustStore {
        &mut self.trust_store
    }
}

/// Signature metadata embedded in files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureMetadata {
    /// Version of the signature format
    pub version: String,
    /// Base64-encoded signature
    pub signature: String,
    /// Base64-encoded public key
    pub public_key: String,
    /// Timestamp when signed
    pub timestamp: u64,
    /// Algorithm used for signing
    pub algorithm: String,
    /// Optional signer information
    pub signer: Option<String>,
}

impl SignatureMetadata {
    /// Create new signature metadata
    pub fn new(
        signature: &Signature,
        public_key: &PublicKey,
        signer: Option<String>,
    ) -> Self {
        Self {
            version: "1.0".to_string(),
            signature: signature.to_base64(),
            public_key: public_key.to_base64(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            algorithm: "Ed25519".to_string(),
            signer,
        }
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize signature metadata")
    }

    /// Create signature block for embedding in files
    pub fn to_signature_block(&self) -> Result<String> {
        let json = self.to_json()?;
        Ok(format!(
            "-----BEGIN WHEEL SIGNATURE-----\n{}\n-----END WHEEL SIGNATURE-----",
            json
        ))
    }
}

/// Utilities for signing files
pub mod signing {
    use super::*;

    /// Sign a file and embed the signature
    pub async fn sign_file(
        file_path: &Path,
        signature: &Signature,
        public_key: &PublicKey,
        signer: Option<String>,
    ) -> Result<()> {
        // Read original content
        let mut content = fs::read(file_path).await
            .context("Failed to read file for signing")?;

        // Create signature metadata
        let metadata = SignatureMetadata::new(signature, public_key, signer);
        let signature_block = metadata.to_signature_block()?;

        // Append signature block
        content.extend_from_slice(signature_block.as_bytes());

        // Write back to file
        fs::write(file_path, content).await
            .context("Failed to write signed file")?;

        Ok(())
    }

    /// Remove signature from a file
    pub async fn unsign_file(file_path: &Path) -> Result<()> {
        let content = fs::read(file_path).await
            .context("Failed to read file for unsigning")?;

        let content_str = String::from_utf8_lossy(&content);
        
        if let Some(start_pos) = content_str.rfind("-----BEGIN WHEEL SIGNATURE-----") {
            // Remove signature block
            let unsigned_content = &content[..start_pos];
            
            fs::write(file_path, unsigned_content).await
                .context("Failed to write unsigned file")?;
        }

        Ok(())
    }

    /// Check if a file has a signature
    pub async fn is_file_signed(file_path: &Path) -> Result<bool> {
        let content = fs::read_to_string(file_path).await
            .context("Failed to read file")?;

        Ok(content.contains("-----BEGIN WHEEL SIGNATURE-----"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::trust::TrustStore;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_signature_metadata() {
        let signature = Signature::from_bytes([1u8; 64]);
        let public_key = PublicKey::from_bytes([2u8; 32]);
        
        let metadata = SignatureMetadata::new(&signature, &public_key, Some("Test Signer".to_string()));
        
        let json = metadata.to_json().unwrap();
        let parsed: SignatureMetadata = serde_json::from_str(&json).unwrap();
        
        assert_eq!(metadata.signature, parsed.signature);
        assert_eq!(metadata.public_key, parsed.public_key);
        assert_eq!(metadata.signer, parsed.signer);
    }

    #[tokio::test]
    async fn test_file_signing() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let original_content = b"Hello, world!";
        
        fs::write(temp_file.path(), original_content).await.unwrap();
        
        let signature = Signature::from_bytes([1u8; 64]);
        let public_key = PublicKey::from_bytes([2u8; 32]);
        
        // Sign the file
        signing::sign_file(
            temp_file.path(),
            &signature,
            &public_key,
            Some("Test".to_string()),
        ).await.unwrap();
        
        // Check if signed
        assert!(signing::is_file_signed(temp_file.path()).await.unwrap());
        
        // Unsign the file
        signing::unsign_file(temp_file.path()).await.unwrap();
        
        // Check content is restored
        let restored_content = fs::read(temp_file.path()).await.unwrap();
        assert_eq!(restored_content, original_content);
        
        // Check if unsigned
        assert!(!signing::is_file_signed(temp_file.path()).await.unwrap());
    }

    #[tokio::test]
    async fn test_verifier() {
        let mut trust_store = TrustStore::new();
        let public_key = PublicKey::from_bytes([2u8; 32]);
        
        trust_store.add_key(
            public_key.clone(),
            TrustLevel::Trusted,
            "Test Key".to_string(),
            vec![Operation::SignUpdates],
            None,
        );
        
        let config = SecurityConfig::default();
        let verifier = Verifier::new(trust_store, config);
        
        // Test signed content verification
        let content = "test content";
        let signature = Signature::from_bytes([1u8; 64]);
        let signed_content = SignedContent::new(content, signature, public_key);
        
        let result = verifier.verify_signed_content(&signed_content, &Operation::SignUpdates);
        
        assert!(result.signature_valid);
        assert_eq!(result.trust_level, TrustLevel::Trusted);
        assert!(result.operation_allowed);
    }
}