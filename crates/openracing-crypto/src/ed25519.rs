//! Ed25519 signature implementation for OpenRacing
//!
//! This module provides Ed25519 digital signature functionality for:
//! - Key pair generation
//! - Signing arbitrary data
//! - Signature verification
//! - Detached signature file support (.sig files)
//!
//! # Security Considerations
//!
//! - Key generation uses `OsRng` for cryptographically secure randomness
//! - Signature verification uses constant-time comparison
//! - Private keys should be securely stored and zeroized when no longer needed

#![deny(clippy::unwrap_used)]

use crate::error::CryptoError;
use crate::utils;
use crate::verification::ContentType;
use crate::{SignatureMetadata, TrustLevel, VerificationResult};
use crate::{SignatureVerifier, TrustStore};
use anyhow::Context;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::Path;
use subtle::ConstantTimeEq;

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

impl PublicKey {
    /// Create a new public key from raw bytes
    pub fn from_bytes(bytes: [u8; 32], identifier: String) -> Self {
        Self {
            key_bytes: bytes,
            identifier,
            comment: None,
        }
    }

    /// Create a public key with a comment
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Get the fingerprint of this public key (SHA256 hash in hex)
    pub fn fingerprint(&self) -> String {
        utils::compute_key_fingerprint(&self.key_bytes)
    }

    /// Convert to ed25519_dalek VerifyingKey
    pub fn to_verifying_key(&self) -> Result<VerifyingKey, CryptoError> {
        VerifyingKey::from_bytes(&self.key_bytes)
            .map_err(|e| CryptoError::KeyFormatError(format!("Invalid public key: {}", e)))
    }

    /// Compare two public keys in constant time
    pub fn ct_eq(&self, other: &PublicKey) -> bool {
        self.key_bytes.ct_eq(&other.key_bytes).into()
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.ct_eq(other)
    }
}

impl Eq for PublicKey {}

/// Ed25519 signature wrapper
#[derive(Debug, Clone)]
pub struct Signature {
    /// Raw signature bytes (64 bytes for Ed25519)
    pub signature_bytes: [u8; 64],
}

impl Signature {
    /// Create a new signature from raw bytes
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self {
            signature_bytes: bytes,
        }
    }

    /// Encode signature as base64 string
    pub fn to_base64(&self) -> String {
        utils::encode_base64(&self.signature_bytes)
    }

    /// Parse signature from base64 string
    pub fn from_base64(encoded: &str) -> Result<Self, CryptoError> {
        let bytes = utils::decode_base64(encoded)?;

        if bytes.len() != 64 {
            return Err(CryptoError::InvalidSignatureLength {
                expected: 64,
                actual: bytes.len(),
            });
        }

        let mut signature_bytes = [0u8; 64];
        signature_bytes.copy_from_slice(&bytes);

        Ok(Self { signature_bytes })
    }

    /// Convert to ed25519_dalek Signature
    pub fn to_dalek_signature(&self) -> ed25519_dalek::Signature {
        ed25519_dalek::Signature::from_bytes(&self.signature_bytes)
    }

    /// Compare two signatures in constant time
    pub fn ct_eq(&self, other: &Signature) -> bool {
        self.signature_bytes.ct_eq(&other.signature_bytes).into()
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.ct_eq(other)
    }
}

impl Eq for Signature {}

/// Ed25519 key pair for signing and verification
#[derive(Debug)]
pub struct KeyPair {
    /// The signing (private) key
    pub signing_key: SigningKey,
    /// The public key derived from the signing key
    pub public_key: PublicKey,
}

impl KeyPair {
    /// Generate a new random Ed25519 key pair
    ///
    /// Uses the operating system's cryptographically secure random number generator.
    pub fn generate() -> Result<Self, CryptoError> {
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let public_key = PublicKey {
            key_bytes: verifying_key.to_bytes(),
            identifier: format!("generated-{}", chrono::Utc::now().timestamp()),
            comment: Some("Generated key pair".to_string()),
        };

        Ok(Self {
            signing_key,
            public_key,
        })
    }

    /// Create a key pair from an existing signing key
    pub fn from_signing_key(signing_key: SigningKey, identifier: String) -> Self {
        let verifying_key = signing_key.verifying_key();

        let public_key = PublicKey {
            key_bytes: verifying_key.to_bytes(),
            identifier,
            comment: None,
        };

        Self {
            signing_key,
            public_key,
        }
    }

    /// Load a key pair from raw signing key bytes
    pub fn from_bytes(
        signing_key_bytes: &[u8; 32],
        identifier: String,
    ) -> Result<Self, CryptoError> {
        let signing_key = SigningKey::from_bytes(signing_key_bytes);
        Ok(Self::from_signing_key(signing_key, identifier))
    }

    /// Get the signing key bytes (for secure storage)
    pub fn signing_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Get the public key fingerprint
    pub fn fingerprint(&self) -> String {
        self.public_key.fingerprint()
    }
}

/// Ed25519 signer for creating signatures
pub struct Ed25519Signer;

impl Ed25519Signer {
    /// Sign arbitrary data with a signing key
    pub fn sign(data: &[u8], signing_key: &SigningKey) -> Result<Signature, CryptoError> {
        let signature = signing_key.sign(data);
        Ok(Signature::from_bytes(signature.to_bytes()))
    }

    /// Sign data and create signature metadata
    pub fn sign_with_metadata(
        data: &[u8],
        keypair: &KeyPair,
        signer_name: &str,
        content_type: ContentType,
        comment: Option<String>,
    ) -> Result<SignatureMetadata, CryptoError> {
        let signature = Self::sign(data, &keypair.signing_key)?;

        Ok(SignatureMetadata {
            signature: signature.to_base64(),
            key_fingerprint: keypair.fingerprint(),
            signer: signer_name.to_string(),
            timestamp: chrono::Utc::now(),
            content_type,
            comment,
        })
    }

    /// Sign a file and create a detached signature file (.sig)
    pub fn sign_file(
        file_path: &Path,
        keypair: &KeyPair,
        signer_name: &str,
        content_type: ContentType,
        comment: Option<String>,
    ) -> Result<SignatureMetadata, CryptoError> {
        let content = std::fs::read(file_path).map_err(CryptoError::IoError)?;

        let metadata =
            Self::sign_with_metadata(&content, keypair, signer_name, content_type, comment)?;

        utils::create_detached_signature(file_path, &metadata).map_err(|e| {
            CryptoError::VerificationFailed(format!("Failed to write signature file: {}", e))
        })?;

        Ok(metadata)
    }
}

/// Ed25519 signature verifier implementation
pub struct Ed25519Verifier {
    /// Trust store for managing trusted public keys
    trust_store: TrustStore,
}

impl Ed25519Verifier {
    /// Create a new Ed25519 verifier with the given trust store
    pub fn new(trust_store: TrustStore) -> Self {
        Self { trust_store }
    }

    /// Verify an Ed25519 signature against data and public key
    pub fn verify(
        data: &[u8],
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<bool, CryptoError> {
        let verifying_key = public_key.to_verifying_key()?;
        let dalek_signature = signature.to_dalek_signature();

        match verifying_key.verify(data, &dalek_signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify a signature using the instance's trust store
    pub fn verify_with_trust_store(
        &self,
        data: &[u8],
        signature: &Signature,
        key_fingerprint: &str,
    ) -> Result<bool, CryptoError> {
        let public_key = self
            .trust_store
            .get_public_key(key_fingerprint)
            .ok_or_else(|| CryptoError::UntrustedSigner(key_fingerprint.to_string()))?;

        Self::verify(data, signature, &public_key)
    }

    /// Parse a signature from base64 string
    pub fn parse_signature(signature_b64: &str) -> anyhow::Result<Signature> {
        Signature::from_base64(signature_b64).map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Parse a public key from base64 string
    pub fn parse_public_key(key_b64: &str, identifier: String) -> anyhow::Result<PublicKey> {
        let key_bytes = utils::decode_base64(key_b64)
            .map_err(|e| anyhow::anyhow!("Failed to decode public key: {}", e))?;

        if key_bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 32,
                actual: key_bytes.len(),
            }
            .into());
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
        public_key.fingerprint()
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

impl SignatureVerifier for Ed25519Verifier {
    fn verify_content(
        &self,
        content: &[u8],
        metadata: &SignatureMetadata,
    ) -> anyhow::Result<VerificationResult> {
        let signature = Signature::from_base64(&metadata.signature)
            .map_err(|e| anyhow::anyhow!("Failed to parse signature: {}", e))?;

        let public_key = self
            .trust_store
            .get_public_key(&metadata.key_fingerprint)
            .ok_or_else(|| CryptoError::UntrustedSigner(metadata.key_fingerprint.clone()))?;

        let signature_valid = Self::verify(content, &signature, &public_key)
            .context("Signature verification failed")?;

        let trust_level = self.trust_store.get_trust_level(&metadata.key_fingerprint);

        let mut warnings = Vec::new();

        let now = chrono::Utc::now();
        let signature_age = now.signed_duration_since(metadata.timestamp);
        if signature_age.num_seconds() > (365 * 24 * 3600) {
            warnings.push(format!(
                "Signature is {} days old",
                signature_age.num_days()
            ));
        }

        match metadata.content_type {
            ContentType::Binary | ContentType::Firmware if trust_level != TrustLevel::Trusted => {
                warnings.push("Critical component signed by untrusted key".to_string());
            }
            ContentType::Plugin if trust_level == TrustLevel::Distrusted => {
                warnings.push("Plugin signed by distrusted key".to_string());
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

    fn verify_file(&self, file_path: &Path) -> anyhow::Result<VerificationResult> {
        let content = std::fs::read(file_path).context("Failed to read file for verification")?;

        let metadata = utils::extract_signature_metadata(file_path)
            .context("Failed to extract signature metadata")?
            .ok_or_else(|| CryptoError::VerificationFailed("No signature found".to_string()))?;

        self.verify_content(&content, &metadata)
    }

    fn is_trusted_signer(&self, key_fingerprint: &str) -> TrustLevel {
        self.trust_store.get_trust_level(key_fingerprint)
    }
}

/// Detached signature file operations
pub mod detached {
    use super::*;
    use crate::SignatureMetadata;

    /// Signature file extension
    pub const SIGNATURE_EXTENSION: &str = "sig";

    /// Get the signature file path for a given content file
    pub fn get_signature_path(content_path: &Path) -> std::path::PathBuf {
        let mut sig_path = content_path.to_path_buf();
        let new_extension = match content_path.extension() {
            Some(ext) => format!("{}.{}", ext.to_string_lossy(), SIGNATURE_EXTENSION),
            None => SIGNATURE_EXTENSION.to_string(),
        };
        sig_path.set_extension(new_extension);
        sig_path
    }

    /// Check if a signature file exists for the given content file
    pub fn signature_exists(content_path: &Path) -> bool {
        get_signature_path(content_path).exists()
    }

    /// Read signature metadata from a detached signature file
    pub fn read_signature(content_path: &Path) -> Result<SignatureMetadata, CryptoError> {
        let sig_path = get_signature_path(content_path);

        if !sig_path.exists() {
            return Err(CryptoError::VerificationFailed(format!(
                "Signature file not found: {}",
                sig_path.display()
            )));
        }

        let sig_content = std::fs::read_to_string(&sig_path).map_err(CryptoError::IoError)?;

        serde_json::from_str(&sig_content).map_err(|e| {
            CryptoError::VerificationFailed(format!("Invalid signature file format: {}", e))
        })
    }

    /// Write signature metadata to a detached signature file
    pub fn write_signature(
        content_path: &Path,
        metadata: &SignatureMetadata,
    ) -> Result<(), CryptoError> {
        let sig_path = get_signature_path(content_path);

        let sig_json = serde_json::to_string_pretty(metadata).map_err(|e| {
            CryptoError::VerificationFailed(format!("Failed to serialize signature: {}", e))
        })?;

        std::fs::write(&sig_path, sig_json).map_err(CryptoError::IoError)?;

        Ok(())
    }

    /// Delete a detached signature file
    pub fn delete_signature(content_path: &Path) -> Result<bool, CryptoError> {
        let sig_path = get_signature_path(content_path);

        if sig_path.exists() {
            std::fs::remove_file(&sig_path).map_err(CryptoError::IoError)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Verify a file using its detached signature
    pub fn verify_file_with_detached_signature(
        content_path: &Path,
        public_key: &PublicKey,
    ) -> Result<bool, CryptoError> {
        let metadata = read_signature(content_path)?;

        let content = std::fs::read(content_path).map_err(CryptoError::IoError)?;

        let signature = Signature::from_base64(&metadata.signature)?;
        Ed25519Verifier::verify(&content, &signature, public_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_keypair_generation() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;

        assert_eq!(keypair.public_key.key_bytes.len(), 32);
        assert_eq!(keypair.signing_key_bytes().len(), 32);

        let fingerprint = keypair.fingerprint();
        assert_eq!(fingerprint.len(), 64);

        Ok(())
    }

    #[test]
    fn test_sign_and_verify() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let data = b"Hello, World!";

        let signature = Ed25519Signer::sign(data, &keypair.signing_key)?;

        let is_valid = Ed25519Verifier::verify(data, &signature, &keypair.public_key)?;
        assert!(is_valid, "Signature should be valid");

        let wrong_data = b"Wrong data";
        let is_valid = Ed25519Verifier::verify(wrong_data, &signature, &keypair.public_key)?;
        assert!(!is_valid, "Signature should be invalid for wrong data");

        Ok(())
    }

    #[test]
    fn test_sign_with_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let data = b"Test content for signing";

        let metadata = Ed25519Signer::sign_with_metadata(
            data,
            &keypair,
            "Test Signer",
            ContentType::Plugin,
            Some("Test signature".to_string()),
        )?;

        assert_eq!(metadata.signer, "Test Signer");
        assert_eq!(metadata.key_fingerprint, keypair.fingerprint());
        assert!(matches!(metadata.content_type, ContentType::Plugin));
        assert_eq!(metadata.comment, Some("Test signature".to_string()));

        let signature = Signature::from_base64(&metadata.signature)?;
        let is_valid = Ed25519Verifier::verify(data, &signature, &keypair.public_key)?;
        assert!(is_valid);

        Ok(())
    }

    #[test]
    fn test_signature_base64_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let data = b"Test data";

        let signature = Ed25519Signer::sign(data, &keypair.signing_key)?;
        let base64 = signature.to_base64();
        let parsed = Signature::from_base64(&base64)?;

        assert!(signature.ct_eq(&parsed));

        Ok(())
    }

    #[test]
    fn test_public_key_fingerprint() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;

        let fingerprint1 = keypair.public_key.fingerprint();
        let fingerprint2 = Ed25519Verifier::get_key_fingerprint(&keypair.public_key);

        assert_eq!(fingerprint1, fingerprint2);
        assert_eq!(fingerprint1.len(), 64);

        Ok(())
    }

    #[test]
    fn test_detached_signature_path() {
        let content_path = Path::new("/path/to/plugin.wasm");
        let sig_path = detached::get_signature_path(content_path);
        assert_eq!(sig_path.to_string_lossy(), "/path/to/plugin.wasm.sig");

        let content_path = Path::new("/path/to/firmware");
        let sig_path = detached::get_signature_path(content_path);
        assert_eq!(sig_path.to_string_lossy(), "/path/to/firmware.sig");
    }

    #[test]
    fn test_detached_signature_file_operations() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let content_path = temp_dir.path().join("test_file.bin");
        let content = b"Test file content";

        std::fs::write(&content_path, content)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_with_metadata(
            content,
            &keypair,
            "Test Signer",
            ContentType::Binary,
            None,
        )?;

        detached::write_signature(&content_path, &metadata)?;

        assert!(detached::signature_exists(&content_path));

        let read_metadata = detached::read_signature(&content_path)?;
        let original_sig = Signature::from_base64(&metadata.signature)?;
        let read_sig = Signature::from_base64(&read_metadata.signature)?;
        assert!(original_sig.ct_eq(&read_sig));

        let is_valid =
            detached::verify_file_with_detached_signature(&content_path, &keypair.public_key)?;
        assert!(is_valid);

        let deleted = detached::delete_signature(&content_path)?;
        assert!(deleted);
        assert!(!detached::signature_exists(&content_path));

        Ok(())
    }

    #[test]
    fn test_keypair_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let original = KeyPair::generate()?;
        let bytes = original.signing_key_bytes();

        let restored = KeyPair::from_bytes(&bytes, "restored-key".to_string())?;

        assert!(original.public_key.ct_eq(&restored.public_key));

        let data = b"Test data";
        let sig1 = Ed25519Signer::sign(data, &original.signing_key)?;
        let sig2 = Ed25519Signer::sign(data, &restored.signing_key)?;
        assert!(sig1.ct_eq(&sig2));

        Ok(())
    }

    #[test]
    fn test_sign_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test_plugin.wasm");
        let content = b"Fake WASM content for testing";

        std::fs::write(&file_path, content)?;

        let keypair = KeyPair::generate()?;
        let metadata = Ed25519Signer::sign_file(
            &file_path,
            &keypair,
            "Plugin Author",
            ContentType::Plugin,
            Some("Test plugin signature".to_string()),
        )?;

        assert!(detached::signature_exists(&file_path));

        let is_valid =
            detached::verify_file_with_detached_signature(&file_path, &keypair.public_key)?;
        assert!(is_valid);

        assert_eq!(metadata.signer, "Plugin Author");
        assert!(matches!(metadata.content_type, ContentType::Plugin));

        Ok(())
    }

    #[test]
    fn test_invalid_signature_length() {
        let invalid_base64 = utils::encode_base64(&[0u8; 32]);
        let result = Signature::from_base64(&invalid_base64);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_public_key_length() {
        let invalid_base64 = utils::encode_base64(&[0u8; 16]);
        let result = Ed25519Verifier::parse_public_key(&invalid_base64, "test".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_constant_time_equality() -> Result<(), Box<dyn std::error::Error>> {
        let keypair1 = KeyPair::generate()?;
        let keypair2 = KeyPair::generate()?;

        assert!(keypair1.public_key.ct_eq(&keypair1.public_key));
        assert!(!keypair1.public_key.ct_eq(&keypair2.public_key));

        let data = b"test";
        let sig1 = Ed25519Signer::sign(data, &keypair1.signing_key)?;
        let sig2 = Ed25519Signer::sign(data, &keypair1.signing_key)?;

        assert!(sig1.ct_eq(&sig2));

        Ok(())
    }
}
