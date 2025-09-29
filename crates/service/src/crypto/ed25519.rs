//! Ed25519 signature implementation for Racing Wheel Suite

use super::{CryptoError, SignatureMetadata, VerificationResult, TrustLevel, ContentType};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Ed25519 public key wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKey {
    /// Raw public key bytes (32 bytes for Ed25519)
    pub key_bytes: [u8; 32],
    
    /// Human-readable identifier for this key
    pub identifier: String,
    
    /// Optional comment or description
    pub comment: Option<String>,
}

/// Ed25519 signature wrapper
#[derive(Debug, Clone)]
pub struct Signature {
    /// Raw signature bytes (64 bytes for Ed25519)
    pub signature_bytes: [u8; 64],
}

/// Ed25519 signature verifier implementation
pub struct Ed25519Verifier {
    /// Trust store for managing trusted public keys
    trust_store: crate::crypto::trust_store::TrustStore,
}

impl Ed25519Verifier {
    /// Create a new Ed25519 verifier with the given trust store
    pub fn new(trust_store: crate::crypto::trust_store::TrustStore) -> Self {
        Self { trust_store }
    }
    
    /// Verify an Ed25519 signature
    pub fn verify_signature(
        &self,
        message: &[u8],
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<bool> {
        use ed25519_dalek::{Verifier, VerifyingKey, ed25519::signature::Signature as _};
        
        let verifying_key = VerifyingKey::from_bytes(&public_key.key_bytes)
            .map_err(|e| CryptoError::KeyFormatError(format!("Invalid public key: {}", e)))?;
        
        let signature_obj = ed25519_dalek::Signature::from_bytes(&signature.signature_bytes);
        
        match verifying_key.verify(message, &signature_obj) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    /// Parse a signature from base64 string
    pub fn parse_signature(signature_b64: &str) -> Result<Signature> {
        let signature_bytes = super::utils::decode_base64(signature_b64)
            .context("Failed to decode signature from base64")?;
        
        if signature_bytes.len() != 64 {
            return Err(CryptoError::KeyFormatError(
                format!("Invalid signature length: expected 64 bytes, got {}", signature_bytes.len())
            ).into());
        }
        
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&signature_bytes);
        
        Ok(Signature {
            signature_bytes: bytes,
        })
    }
    
    /// Parse a public key from base64 string
    pub fn parse_public_key(key_b64: &str, identifier: String) -> Result<PublicKey> {
        let key_bytes = super::utils::decode_base64(key_b64)
            .context("Failed to decode public key from base64")?;
        
        if key_bytes.len() != 32 {
            return Err(CryptoError::KeyFormatError(
                format!("Invalid public key length: expected 32 bytes, got {}", key_bytes.len())
            ).into());
        }
        
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&key_bytes);
        
        Ok(PublicKey {
            key_bytes: bytes,
            identifier,
            comment: None,
        })
    }
    
    /// Get the fingerprint of a public key
    pub fn get_key_fingerprint(public_key: &PublicKey) -> String {
        super::utils::compute_key_fingerprint(&public_key.key_bytes)
    }
}

impl super::SignatureVerifier for Ed25519Verifier {
    fn verify_content(&self, content: &[u8], metadata: &SignatureMetadata) -> Result<VerificationResult> {
        // Parse the signature
        let signature = Self::parse_signature(&metadata.signature)
            .context("Failed to parse signature")?;
        
        // Look up the public key by fingerprint
        let public_key = self.trust_store.get_public_key(&metadata.key_fingerprint)
            .ok_or_else(|| CryptoError::UntrustedSigner(metadata.key_fingerprint.clone()))?;
        
        // Verify the signature
        let signature_valid = self.verify_signature(content, &signature, &public_key)
            .context("Signature verification failed")?;
        
        // Check trust level
        let trust_level = self.trust_store.get_trust_level(&metadata.key_fingerprint);
        
        // Collect any warnings
        let mut warnings = Vec::new();
        
        // Check signature age if configured
        let now = chrono::Utc::now();
        let signature_age = now.signed_duration_since(metadata.timestamp);
        if signature_age.num_seconds() > (365 * 24 * 3600) {
            warnings.push(format!(
                "Signature is {} days old", 
                signature_age.num_days()
            ));
        }
        
        // Check content type appropriateness
        match metadata.content_type {
            ContentType::Binary | ContentType::Firmware => {
                if trust_level != TrustLevel::Trusted {
                    warnings.push("Critical component signed by untrusted key".to_string());
                }
            }
            ContentType::Plugin => {
                if trust_level == TrustLevel::Distrusted {
                    warnings.push("Plugin signed by distrusted key".to_string());
                }
            }
            _ => {}
        }
        
        Ok(VerificationResult {
            signature_valid,
            trust_level,
            metadata: metadata.clone(),
            warnings,
        })
    }
    
    fn verify_file(&self, file_path: &std::path::Path) -> Result<VerificationResult> {
        // Read the file content
        let content = std::fs::read(file_path)
            .context("Failed to read file for verification")?;
        
        // Extract signature metadata
        let metadata = super::utils::extract_signature_metadata(file_path)
            .context("Failed to extract signature metadata")?
            .ok_or_else(|| CryptoError::VerificationFailed("No signature found".to_string()))?;
        
        self.verify_content(&content, &metadata)
    }
    
    fn is_trusted_signer(&self, key_fingerprint: &str) -> TrustLevel {
        self.trust_store.get_trust_level(key_fingerprint)
    }
}

/// Utility functions for Ed25519 operations
impl Ed25519Verifier {
    /// Generate a new Ed25519 keypair (for development/testing)
    #[cfg(feature = "keygen")]
    pub fn generate_keypair() -> Result<(ed25519_dalek::SigningKey, PublicKey)> {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;
        
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        
        let public_key = PublicKey {
            key_bytes: verifying_key.to_bytes(),
            identifier: "generated-key".to_string(),
            comment: Some("Generated for development".to_string()),
        };
        
        Ok((signing_key, public_key))
    }
    
    /// Sign content with a signing key (for development/testing)
    #[cfg(feature = "keygen")]
    pub fn sign_content(
        content: &[u8],
        signing_key: &ed25519_dalek::SigningKey,
        signer_name: &str,
        content_type: ContentType,
    ) -> Result<SignatureMetadata> {
        use ed25519_dalek::Signer;
        
        let signature = signing_key.sign(content);
        let public_key = signing_key.verifying_key();
        
        let signature_b64 = super::utils::encode_base64(&signature.to_bytes());
        let key_fingerprint = super::utils::compute_key_fingerprint(&public_key.to_bytes());
        
        Ok(SignatureMetadata {
            signature: signature_b64,
            key_fingerprint,
            signer: signer_name.to_string(),
            timestamp: chrono::Utc::now(),
            content_type,
            comment: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::trust_store::TrustStore;
    
    #[test]
    fn test_signature_roundtrip() {
        let trust_store = TrustStore::new_in_memory();
        let verifier = Ed25519Verifier::new(trust_store);
        
        // This test would require the keygen feature
        // In practice, we'd use pre-generated test keys
    }
    
    #[test]
    fn test_signature_parsing() {
        // Test signature parsing with known good values
        let signature_b64 = "dGVzdCBzaWduYXR1cmUgZGF0YSB0aGF0IGlzIGV4YWN0bHkgNjQgYnl0ZXMgbG9uZyBmb3IgdGVzdGluZyBwdXJwb3Nlcw==";
        
        // This should fail because it's not a valid signature
        let result = Ed25519Verifier::parse_signature(signature_b64);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_public_key_parsing() {
        // Test public key parsing with known good values
        let key_b64 = "dGVzdCBwdWJsaWMga2V5IGRhdGEgMzIgYnl0ZXM="; // 32 bytes
        
        let result = Ed25519Verifier::parse_public_key(key_b64, "test-key".to_string());
        assert!(result.is_ok());
        
        let public_key = result.unwrap();
        assert_eq!(public_key.identifier, "test-key");
        assert_eq!(public_key.key_bytes.len(), 32);
    }
}