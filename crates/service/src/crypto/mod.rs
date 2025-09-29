//! Cryptographic signature verification for Racing Wheel Suite
//! 
//! This module provides Ed25519 signature verification for:
//! - Application binaries and updates
//! - Firmware images
//! - Plugin packages
//! - Configuration profiles (optional)

use std::path::Path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod ed25519;
pub mod verification;
pub mod trust_store;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Untrusted signer: {0}")]
    UntrustedSigner(String),
    
    #[error("Signature verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Key format error: {0}")]
    KeyFormatError(String),
    
    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Signature metadata attached to signed content
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub content_type: ContentType,
    
    /// Optional comment or description
    pub comment: Option<String>,
}

/// Types of content that can be signed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
    /// Application binary (wheeld, wheelctl, wheel-ui)
    Binary,
    
    /// Firmware image for racing wheel hardware
    Firmware,
    
    /// Plugin package (WASM or native)
    Plugin,
    
    /// Configuration profile
    Profile,
    
    /// Update package
    Update,
}

/// Trust level for a signature
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    /// Explicitly trusted (user or system trust store)
    Trusted,
    
    /// Unknown signer (not in trust store)
    Unknown,
    
    /// Explicitly distrusted
    Distrusted,
}

/// Result of signature verification
#[derive(Debug)]
pub struct VerificationResult {
    /// Whether the signature is cryptographically valid
    pub signature_valid: bool,
    
    /// Trust level of the signer
    pub trust_level: TrustLevel,
    
    /// Signature metadata
    pub metadata: SignatureMetadata,
    
    /// Any warnings or additional information
    pub warnings: Vec<String>,
}

/// Main signature verification interface
pub trait SignatureVerifier {
    /// Verify a signature for the given content
    fn verify_content(&self, content: &[u8], metadata: &SignatureMetadata) -> Result<VerificationResult>;
    
    /// Verify a signed file
    fn verify_file(&self, file_path: &Path) -> Result<VerificationResult>;
    
    /// Check if a signer is trusted
    fn is_trusted_signer(&self, key_fingerprint: &str) -> TrustLevel;
}

/// Configuration for signature verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Whether to require signatures for binaries
    pub require_binary_signatures: bool,
    
    /// Whether to require signatures for firmware
    pub require_firmware_signatures: bool,
    
    /// Whether to require signatures for plugins
    pub require_plugin_signatures: bool,
    
    /// Whether to allow unknown signers (not in trust store)
    pub allow_unknown_signers: bool,
    
    /// Path to trust store directory
    pub trust_store_path: std::path::PathBuf,
    
    /// Maximum age for signatures (in seconds)
    pub max_signature_age_seconds: Option<u64>,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            require_binary_signatures: true,
            require_firmware_signatures: true,
            require_plugin_signatures: false, // Allow unsigned plugins by default
            allow_unknown_signers: true,      // Allow but warn
            trust_store_path: std::path::PathBuf::from("trust_store"),
            max_signature_age_seconds: Some(365 * 24 * 3600), // 1 year
        }
    }
}

/// Utility functions for signature handling
pub mod utils {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    
    /// Extract signature metadata from a signed file
    /// 
    /// Looks for signature in:
    /// 1. Separate .sig file
    /// 2. Embedded signature section
    /// 3. Extended attributes (Linux/macOS)
    pub fn extract_signature_metadata(file_path: &Path) -> Result<Option<SignatureMetadata>> {
        // Try separate .sig file first
        let sig_path = file_path.with_extension(
            format!("{}.sig", file_path.extension().unwrap_or_default().to_string_lossy())
        );
        
        if sig_path.exists() {
            let sig_content = std::fs::read_to_string(&sig_path)
                .context("Failed to read signature file")?;
            
            let metadata: SignatureMetadata = serde_json::from_str(&sig_content)
                .context("Failed to parse signature metadata")?;
            
            return Ok(Some(metadata));
        }
        
        // TODO: Check for embedded signatures in PE/ELF sections
        // TODO: Check extended attributes on Unix systems
        
        Ok(None)
    }
    
    /// Create a detached signature file for content
    pub fn create_detached_signature(
        content_path: &Path,
        signature_metadata: &SignatureMetadata,
    ) -> Result<()> {
        let sig_path = content_path.with_extension(
            format!("{}.sig", content_path.extension().unwrap_or_default().to_string_lossy())
        );
        
        let sig_json = serde_json::to_string_pretty(signature_metadata)
            .context("Failed to serialize signature metadata")?;
        
        std::fs::write(&sig_path, sig_json)
            .context("Failed to write signature file")?;
        
        Ok(())
    }
    
    /// Compute SHA256 fingerprint of a public key
    pub fn compute_key_fingerprint(public_key: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(public_key);
        hex::encode(hasher.finalize())
    }
    
    /// Encode bytes as base64
    pub fn encode_base64(data: &[u8]) -> String {
        BASE64.encode(data)
    }
    
    /// Decode base64 to bytes
    pub fn decode_base64(data: &str) -> Result<Vec<u8>> {
        BASE64.decode(data)
            .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))
    }
}