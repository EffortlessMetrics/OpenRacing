//! Cryptographic utilities for OpenRacing signature verification
//!
//! This crate provides Ed25519 signature verification for:
//! - Application binaries and updates
//! - Firmware images
//! - Plugin packages
//! - Configuration profiles (optional)
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`ed25519`]: Ed25519 signing and verification operations
//! - [`trust_store`]: Trust store management for public keys
//! - [`verification`]: High-level verification service
//! - [`error`]: Error types for cryptographic operations
//!
//! # Security Considerations
//!
//! - All signature comparisons use constant-time operations via the `subtle` crate
//! - Private keys are zeroized on drop when possible
//! - The trust store protects system keys from modification
//!
//! # Example
//!
//! ```
//! use openracing_crypto::prelude::*;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Generate a new key pair
//! let keypair = KeyPair::generate()?;
//!
//! // Sign some data
//! let data = b"Hello, World!";
//! let signature = Ed25519Signer::sign(data, &keypair.signing_key)?;
//!
//! // Verify the signature
//! let is_valid = Ed25519Verifier::verify(data, &signature, &keypair.public_key)?;
//! assert!(is_valid);
//! # Ok(())
//! # }
//! ```

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod ed25519;
pub mod error;
pub mod prelude;
pub mod trust_store;
pub mod verification;

pub use ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair, PublicKey, Signature};
pub use error::CryptoError;
pub use trust_store::{ImportResult, TrustEntry, TrustStore, TrustStoreStats};
pub use verification::{
    ContentType, VerificationConfig, VerificationReport, VerificationResult, VerificationService,
};

/// Trust level for a signature
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrustLevel {
    /// Explicitly trusted (user or system trust store)
    Trusted,
    /// Unknown signer (not in trust store)
    Unknown,
    /// Explicitly distrusted
    Distrusted,
}

/// Signature metadata attached to signed content
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureMetadata {
    /// Ed25519 signature in base64 format
    pub signature: String,
    /// Public key fingerprint (SHA256 of public key)
    pub key_fingerprint: String,
    /// Signer identity (human-readable)
    pub signer: String,
    /// Timestamp when signature was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Content type being signed
    pub content_type: verification::ContentType,
    /// Optional comment or description
    pub comment: Option<String>,
}

/// Main signature verification interface
pub trait SignatureVerifier {
    /// Verify a signature for the given content
    fn verify_content(
        &self,
        content: &[u8],
        metadata: &SignatureMetadata,
    ) -> anyhow::Result<VerificationResult>;

    /// Verify a signed file
    fn verify_file(&self, file_path: &std::path::Path) -> anyhow::Result<VerificationResult>;

    /// Check if a signer is trusted
    fn is_trusted_signer(&self, key_fingerprint: &str) -> TrustLevel;
}

/// Utility functions for signature handling
pub mod utils {
    use crate::SignatureMetadata;
    use crate::error::CryptoError;
    use anyhow::Context;
    use sha2::{Digest, Sha256};
    use std::path::Path;

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

    /// Compute SHA256 fingerprint of a public key
    pub fn compute_key_fingerprint(public_key: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(public_key);
        hex::encode(hasher.finalize())
    }

    /// Compute SHA256 hash of data and return as hex string
    pub fn compute_sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Compute SHA256 hash of a file and return as hex string
    pub fn compute_file_sha256_hex(file_path: &Path) -> anyhow::Result<String> {
        let content = std::fs::read(file_path).context("Failed to read file for hashing")?;
        Ok(compute_sha256_hex(&content))
    }

    /// Encode bytes as base64 (Standard alphabet with padding)
    pub fn encode_base64(data: &[u8]) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.encode(data)
    }

    /// Decode base64 to bytes
    pub fn decode_base64(data: &str) -> Result<Vec<u8>, CryptoError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.decode(data).map_err(CryptoError::from)
    }

    /// Extract signature metadata from a signed file
    pub fn extract_signature_metadata(
        file_path: &Path,
    ) -> anyhow::Result<Option<SignatureMetadata>> {
        let sig_path = get_signature_path(file_path);

        if sig_path.exists() {
            let sig_content =
                std::fs::read_to_string(&sig_path).context("Failed to read signature file")?;

            let metadata: SignatureMetadata =
                serde_json::from_str(&sig_content).context("Failed to parse signature metadata")?;

            return Ok(Some(metadata));
        }

        if let Some(metadata) = extract_embedded_signature(file_path)? {
            return Ok(Some(metadata));
        }

        Ok(None)
    }

    /// Check if a signature file exists for the given content file
    pub fn signature_exists(content_path: &Path) -> bool {
        get_signature_path(content_path).exists()
    }

    /// Create a detached signature file for content
    pub fn create_detached_signature(
        content_path: &Path,
        signature_metadata: &SignatureMetadata,
    ) -> anyhow::Result<()> {
        let sig_path = get_signature_path(content_path);

        let sig_json = serde_json::to_string_pretty(signature_metadata)
            .context("Failed to serialize signature metadata")?;

        std::fs::write(&sig_path, sig_json).context("Failed to write signature file")?;

        Ok(())
    }

    /// Delete a detached signature file
    pub fn delete_detached_signature(content_path: &Path) -> anyhow::Result<bool> {
        let sig_path = get_signature_path(content_path);

        if sig_path.exists() {
            std::fs::remove_file(&sig_path).context("Failed to delete signature file")?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Extract embedded signature from a binary file (PE/ELF/Mach-O)
    pub fn extract_embedded_signature(
        file_path: &Path,
    ) -> anyhow::Result<Option<SignatureMetadata>> {
        use openracing_binary_signature::extract_embedded_signature_payload;
        use tracing::debug;

        match extract_embedded_signature_payload(file_path)? {
            Some(data) => {
                let metadata: SignatureMetadata = serde_json::from_slice(&data)
                    .context("Failed to parse embedded signature metadata")?;
                debug!("Found embedded signature from signer: {}", metadata.signer);
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }
}
