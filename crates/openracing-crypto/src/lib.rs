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

    /// Section name for embedded signatures in PE binaries (Windows)
    pub const PE_SIGNATURE_SECTION: &str = ".orsig";

    /// Section name for embedded signatures in ELF binaries (Linux)
    pub const ELF_SIGNATURE_SECTION: &str = ".note.openracing.sig";

    /// Section name for embedded signatures in Mach-O binaries (macOS)
    pub const MACHO_SIGNATURE_SECTION: &str = "__orsig";

    /// Mach-O segment containing the signature section
    pub const MACHO_SIGNATURE_SEGMENT: &str = "__DATA";

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
        use goblin::Object;
        use tracing::debug;

        let file_data =
            std::fs::read(file_path).context("Failed to read file for signature extraction")?;

        let object = match Object::parse(&file_data) {
            Ok(obj) => obj,
            Err(_) => {
                return Ok(None);
            }
        };

        let section_data = match object {
            Object::PE(pe) => {
                debug!("Checking PE binary for embedded signature");
                extract_pe_signature_section(&pe, &file_data)
            }
            Object::Elf(elf) => {
                debug!("Checking ELF binary for embedded signature");
                extract_elf_signature_section(&elf, &file_data)
            }
            Object::Mach(mach) => {
                debug!("Checking Mach-O binary for embedded signature");
                extract_macho_signature_section(&mach, &file_data)
            }
            _ => None,
        };

        match section_data {
            Some(data) => {
                let metadata: SignatureMetadata = serde_json::from_slice(data)
                    .context("Failed to parse embedded signature metadata")?;
                debug!("Found embedded signature from signer: {}", metadata.signer);
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    fn extract_pe_signature_section<'a>(
        pe: &goblin::pe::PE<'_>,
        file_data: &'a [u8],
    ) -> Option<&'a [u8]> {
        for section in &pe.sections {
            let name = String::from_utf8_lossy(&section.name);
            let name = name.trim_end_matches('\0');
            if name == PE_SIGNATURE_SECTION {
                let start = section.pointer_to_raw_data as usize;
                let size = section.size_of_raw_data as usize;
                if start + size <= file_data.len() {
                    let data = &file_data[start..start + size];
                    let trimmed = trim_null_bytes(data);
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
        None
    }

    fn extract_elf_signature_section<'a>(
        elf: &goblin::elf::Elf<'_>,
        file_data: &'a [u8],
    ) -> Option<&'a [u8]> {
        for section in &elf.section_headers {
            if let Some(name) = elf.shdr_strtab.get_at(section.sh_name)
                && name == ELF_SIGNATURE_SECTION
            {
                let start = section.sh_offset as usize;
                let size = section.sh_size as usize;
                if start + size <= file_data.len() {
                    let data = &file_data[start..start + size];
                    let trimmed = trim_null_bytes(data);
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
        None
    }

    fn extract_macho_signature_section<'a>(
        mach: &goblin::mach::Mach<'_>,
        file_data: &'a [u8],
    ) -> Option<&'a [u8]> {
        match mach {
            goblin::mach::Mach::Binary(macho) => extract_macho_binary_signature(macho, file_data),
            goblin::mach::Mach::Fat(fat) => {
                for arch in fat.iter_arches().flatten() {
                    if let Ok(macho) = goblin::mach::MachO::parse(file_data, arch.offset as usize)
                        && let Some(data) = extract_macho_binary_signature(&macho, file_data)
                    {
                        return Some(data);
                    }
                }
                None
            }
        }
    }

    fn extract_macho_binary_signature<'a>(
        macho: &goblin::mach::MachO<'_>,
        file_data: &'a [u8],
    ) -> Option<&'a [u8]> {
        for segment in &macho.segments {
            if let Ok(name) = segment.name()
                && name == MACHO_SIGNATURE_SEGMENT
                && let Ok(sections) = segment.sections()
            {
                for (section, _data) in sections {
                    if let Ok(sect_name) = section.name()
                        && sect_name == MACHO_SIGNATURE_SECTION
                    {
                        let start = section.offset as usize;
                        let size = section.size as usize;
                        if start + size <= file_data.len() {
                            let data = &file_data[start..start + size];
                            let trimmed = trim_null_bytes(data);
                            if !trimmed.is_empty() {
                                return Some(trimmed);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn trim_null_bytes(data: &[u8]) -> &[u8] {
        let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        &data[..end]
    }
}
