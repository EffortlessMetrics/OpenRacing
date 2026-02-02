//! Cryptographic signature verification for Racing Wheel Suite
//!
//! This module provides Ed25519 signature verification for:
//! - Application binaries and updates
//! - Firmware images
//! - Plugin packages
//! - Configuration profiles (optional)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

pub mod ed25519;
pub mod trust_store;
pub mod verification;

#[cfg(test)]
mod signature_properties;

#[cfg(test)]
mod trust_store_properties;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    fn verify_content(
        &self,
        content: &[u8],
        metadata: &SignatureMetadata,
    ) -> Result<VerificationResult>;

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

    /// Signature file extension
    pub const SIGNATURE_EXTENSION: &str = "sig";

    /// Get the signature file path for a given content file
    ///
    /// For a file like `plugin.wasm`, returns `plugin.wasm.sig`
    pub fn get_signature_path(content_path: &Path) -> std::path::PathBuf {
        let mut sig_path = content_path.to_path_buf();
        let new_extension = match content_path.extension() {
            Some(ext) => format!("{}.{}", ext.to_string_lossy(), SIGNATURE_EXTENSION),
            None => SIGNATURE_EXTENSION.to_string(),
        };
        sig_path.set_extension(new_extension);
        sig_path
    }

    /// Extract signature metadata from a signed file
    ///
    /// Looks for signature in:
    /// 1. Separate .sig file (e.g., plugin.wasm.sig for plugin.wasm)
    /// 2. Embedded signature section (future)
    /// 3. Extended attributes (future, Linux/macOS)
    pub fn extract_signature_metadata(file_path: &Path) -> Result<Option<SignatureMetadata>> {
        // Try separate .sig file first
        let sig_path = get_signature_path(file_path);

        if sig_path.exists() {
            let sig_content =
                std::fs::read_to_string(&sig_path).context("Failed to read signature file")?;

            let metadata: SignatureMetadata =
                serde_json::from_str(&sig_content).context("Failed to parse signature metadata")?;

            return Ok(Some(metadata));
        }

        // Try embedded signature in binary sections
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
    ///
    /// Creates a `.sig` file alongside the content file containing
    /// the signature metadata in JSON format.
    pub fn create_detached_signature(
        content_path: &Path,
        signature_metadata: &SignatureMetadata,
    ) -> Result<()> {
        let sig_path = get_signature_path(content_path);

        let sig_json = serde_json::to_string_pretty(signature_metadata)
            .context("Failed to serialize signature metadata")?;

        std::fs::write(&sig_path, sig_json).context("Failed to write signature file")?;

        Ok(())
    }

    /// Delete a detached signature file
    pub fn delete_detached_signature(content_path: &Path) -> Result<bool> {
        let sig_path = get_signature_path(content_path);

        if sig_path.exists() {
            std::fs::remove_file(&sig_path).context("Failed to delete signature file")?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Compute SHA256 fingerprint of a public key
    ///
    /// Returns a 64-character lowercase hex string representing
    /// the SHA256 hash of the public key bytes.
    pub fn compute_key_fingerprint(public_key: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(public_key);
        hex::encode(hasher.finalize())
    }

    /// Encode bytes as base64 (standard alphabet with padding)
    pub fn encode_base64(data: &[u8]) -> String {
        BASE64.encode(data)
    }

    /// Decode base64 to bytes
    pub fn decode_base64(data: &str) -> Result<Vec<u8>> {
        BASE64
            .decode(data)
            .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))
    }

    /// Compute SHA256 hash of data and return as hex string
    pub fn compute_sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Compute SHA256 hash of a file and return as hex string
    pub fn compute_file_sha256_hex(file_path: &Path) -> Result<String> {
        let content = std::fs::read(file_path).context("Failed to read file for hashing")?;
        Ok(compute_sha256_hex(&content))
    }

    /// Section name for embedded signatures in PE binaries (Windows)
    pub const PE_SIGNATURE_SECTION: &str = ".orsig";

    /// Section name for embedded signatures in ELF binaries (Linux)
    pub const ELF_SIGNATURE_SECTION: &str = ".note.openracing.sig";

    /// Section name for embedded signatures in Mach-O binaries (macOS)
    pub const MACHO_SIGNATURE_SECTION: &str = "__orsig";

    /// Mach-O segment containing the signature section
    pub const MACHO_SIGNATURE_SEGMENT: &str = "__DATA";

    /// Extract embedded signature from a binary file (PE/ELF/Mach-O)
    ///
    /// This function attempts to find and extract signature metadata that has been
    /// embedded directly in the binary file's sections. This is useful for native
    /// plugins and executables where a detached .sig file may not be desirable.
    ///
    /// # Section Names
    /// - PE (Windows): `.orsig` section
    /// - ELF (Linux): `.note.openracing.sig` section
    /// - Mach-O (macOS): `__orsig` section in `__DATA` segment
    ///
    /// # Returns
    /// - `Ok(Some(metadata))` if a valid embedded signature was found
    /// - `Ok(None)` if no embedded signature exists
    /// - `Err(_)` if the file could not be read or parsed
    pub fn extract_embedded_signature(file_path: &Path) -> Result<Option<SignatureMetadata>> {
        use goblin::Object;
        use tracing::debug;

        let file_data = std::fs::read(file_path).context("Failed to read file for signature extraction")?;

        let object = match Object::parse(&file_data) {
            Ok(obj) => obj,
            Err(_) => {
                // Not a recognized binary format, no embedded signature possible
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
            _ => {
                // Unknown or archive format
                None
            }
        };

        match section_data {
            Some(data) => {
                // Parse the section content as JSON SignatureMetadata
                let metadata: SignatureMetadata = serde_json::from_slice(data)
                    .context("Failed to parse embedded signature metadata")?;
                debug!("Found embedded signature from signer: {}", metadata.signer);
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    /// Extract signature section from a PE binary
    fn extract_pe_signature_section<'a>(pe: &goblin::pe::PE<'_>, file_data: &'a [u8]) -> Option<&'a [u8]> {
        for section in &pe.sections {
            let name = String::from_utf8_lossy(&section.name);
            let name = name.trim_end_matches('\0');
            if name == PE_SIGNATURE_SECTION {
                let start = section.pointer_to_raw_data as usize;
                let size = section.size_of_raw_data as usize;
                if start + size <= file_data.len() {
                    // Trim null padding from the section data
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

    /// Extract signature section from an ELF binary
    fn extract_elf_signature_section<'a>(elf: &goblin::elf::Elf<'_>, file_data: &'a [u8]) -> Option<&'a [u8]> {
        for section in &elf.section_headers {
            if let Some(name) = elf.shdr_strtab.get_at(section.sh_name) {
                if name == ELF_SIGNATURE_SECTION {
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
        }
        None
    }

    /// Extract signature section from a Mach-O binary
    fn extract_macho_signature_section<'a>(mach: &goblin::mach::Mach<'_>, file_data: &'a [u8]) -> Option<&'a [u8]> {
        match mach {
            goblin::mach::Mach::Binary(macho) => {
                extract_macho_binary_signature(macho, file_data)
            }
            goblin::mach::Mach::Fat(fat) => {
                // For fat binaries, check each architecture
                for arch in fat.iter_arches().flatten() {
                    if let Ok(macho) = goblin::mach::MachO::parse(file_data, arch.offset as usize) {
                        if let Some(data) = extract_macho_binary_signature(&macho, file_data) {
                            return Some(data);
                        }
                    }
                }
                None
            }
        }
    }

    /// Extract signature from a single Mach-O binary
    fn extract_macho_binary_signature<'a>(macho: &goblin::mach::MachO<'_>, file_data: &'a [u8]) -> Option<&'a [u8]> {
        for segment in &macho.segments {
            if let Ok(name) = segment.name() {
                if name == MACHO_SIGNATURE_SEGMENT {
                    if let Ok(sections) = segment.sections() {
                        for (section, _data) in sections {
                            if let Ok(sect_name) = section.name() {
                                if sect_name == MACHO_SIGNATURE_SECTION {
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
                }
            }
        }
        None
    }

    /// Trim null bytes from the end of section data
    fn trim_null_bytes(data: &[u8]) -> &[u8] {
        let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        &data[..end]
    }
}
