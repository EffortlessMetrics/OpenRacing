//! PE Binary Embedded Signature Extraction for Windows
//!
//! This module provides functionality to extract Ed25519 signatures embedded
//! in PE (Portable Executable) binary files for native plugin verification
//! on Windows platforms.
//!
//! # Signature Section
//!
//! Signatures are stored in a custom `.orsig` section within the PE binary.
//! This allows for tamper-evident signing where the signature is part of the
//! binary itself, without relying on external signature files.
//!
//! # PE Format Support
//!
//! - PE32 (32-bit Windows executables/DLLs)
//! - PE32+ (64-bit Windows executables/DLLs)

use std::fs;
use std::io;
use std::path::Path;

use goblin::pe::PE;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Section name for OpenRacing signatures in PE binaries
pub const PE_SIGNATURE_SECTION: &str = ".orsig";

/// Magic bytes at the start of the signature section to identify valid signatures
pub const SIGNATURE_MAGIC: &[u8; 8] = b"ORSIG\x00\x01\x00";

/// Version of the signature format
pub const SIGNATURE_FORMAT_VERSION: u8 = 1;

/// Errors that can occur during PE signature extraction
#[derive(Error, Debug)]
pub enum PeSigError {
    /// I/O error while reading the binary file
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// The file is not a valid PE binary
    #[error("Invalid PE binary: {0}")]
    InvalidPe(String),

    /// No signature section found in the binary
    #[error("No signature section found in binary")]
    NoSignatureSection,

    /// The signature section exists but contains malformed data
    #[error("Malformed signature: {0}")]
    MalformedSignature(String),
}

/// Embedded signature extracted from a PE binary
///
/// This structure contains the Ed25519 signature and associated metadata
/// that was embedded in the `.orsig` section of a PE binary.
#[derive(Debug, Clone)]
pub struct EmbeddedSignature {
    /// The Ed25519 signature bytes (64 bytes)
    pub signature: Vec<u8>,

    /// SHA256 hash of the signed data (32 bytes)
    pub signed_data_hash: [u8; 32],

    /// Optional fingerprint of the signing key
    pub signer_fingerprint: Option<String>,

    /// Optional timestamp when the signature was created (Unix epoch seconds)
    pub timestamp: Option<u64>,
}

/// Extractor for PE binary embedded signatures
///
/// This struct provides methods to extract and verify signatures
/// embedded in PE (Windows) binary files.
///
/// # Example
///
/// ```ignore
/// use racing_wheel_plugins::pe_sig::PeSignatureExtractor;
/// use std::path::Path;
///
/// let path = Path::new("my_plugin.dll");
/// let extractor = PeSignatureExtractor::new();
///
/// if let Ok(Some(signature)) = extractor.extract_signature(&path) {
///     println!("Found signature with hash: {:?}", signature.signed_data_hash);
/// }
/// ```
#[derive(Debug, Default)]
pub struct PeSignatureExtractor {
    _private: (),
}

impl PeSignatureExtractor {
    /// Create a new PE signature extractor
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Extract an embedded signature from a PE binary
    ///
    /// This method reads the PE binary at the given path and attempts to
    /// extract signature data from the `.orsig` section.
    ///
    /// # Arguments
    ///
    /// * `binary_path` - Path to the PE binary file (.dll or .exe)
    ///
    /// # Returns
    ///
    /// * `Ok(Some(signature))` - A valid embedded signature was found
    /// * `Ok(None)` - The binary is valid PE but has no signature section
    /// * `Err(PeSigError)` - An error occurred during extraction
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The file cannot be read (`IoError`)
    /// * The file is not a valid PE binary (`InvalidPe`)
    /// * The signature section exists but is malformed (`MalformedSignature`)
    pub fn extract_signature(
        &self,
        binary_path: &Path,
    ) -> Result<Option<EmbeddedSignature>, PeSigError> {
        let file_data = fs::read(binary_path)?;

        let pe = match PE::parse(&file_data) {
            Ok(pe) => pe,
            Err(e) => {
                return Err(PeSigError::InvalidPe(format!(
                    "Failed to parse PE: {}",
                    e
                )));
            }
        };

        // Find the .orsig section
        let section_data = self.find_signature_section(&pe, &file_data)?;

        match section_data {
            Some(data) => {
                let signature = self.parse_signature_data(data)?;
                Ok(Some(signature))
            }
            None => Ok(None),
        }
    }

    /// Check if a PE binary has a signature section
    ///
    /// This is a faster check than full extraction when you only need
    /// to know if a signature exists.
    ///
    /// # Arguments
    ///
    /// * `binary_path` - Path to the PE binary file
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - The binary has a `.orsig` section
    /// * `Ok(false)` - The binary has no `.orsig` section
    /// * `Err(PeSigError)` - An error occurred reading the binary
    pub fn has_signature_section(&self, binary_path: &Path) -> Result<bool, PeSigError> {
        let file_data = fs::read(binary_path)?;

        let pe = match PE::parse(&file_data) {
            Ok(pe) => pe,
            Err(e) => {
                return Err(PeSigError::InvalidPe(format!(
                    "Failed to parse PE: {}",
                    e
                )));
            }
        };

        for section in &pe.sections {
            let name = String::from_utf8_lossy(&section.name);
            let name = name.trim_end_matches('\0');
            if name == PE_SIGNATURE_SECTION {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Find and return the signature section data from a PE binary
    fn find_signature_section<'a>(
        &self,
        pe: &PE<'_>,
        file_data: &'a [u8],
    ) -> Result<Option<&'a [u8]>, PeSigError> {
        for section in &pe.sections {
            let name = String::from_utf8_lossy(&section.name);
            let name = name.trim_end_matches('\0');

            if name == PE_SIGNATURE_SECTION {
                let start = section.pointer_to_raw_data as usize;
                let size = section.size_of_raw_data as usize;

                // Validate section bounds
                if start > file_data.len() {
                    return Err(PeSigError::MalformedSignature(format!(
                        "Section offset {} exceeds file size {}",
                        start,
                        file_data.len()
                    )));
                }

                if start.saturating_add(size) > file_data.len() {
                    return Err(PeSigError::MalformedSignature(format!(
                        "Section extends beyond file: offset {} + size {} > file size {}",
                        start,
                        size,
                        file_data.len()
                    )));
                }

                let data = &file_data[start..start + size];
                // Trim null padding from the section data
                let trimmed = trim_null_bytes(data);

                if trimmed.is_empty() {
                    return Err(PeSigError::MalformedSignature(
                        "Signature section is empty".to_string(),
                    ));
                }

                return Ok(Some(trimmed));
            }
        }

        Ok(None)
    }

    /// Parse signature data from the raw section bytes
    ///
    /// # Signature Section Format
    ///
    /// The `.orsig` section contains:
    /// - 8 bytes: Magic header "ORSIG\0\x01\0"
    /// - 64 bytes: Ed25519 signature
    /// - 32 bytes: SHA256 hash of signed content
    /// - 8 bytes: Timestamp (little-endian u64, 0 if not present)
    /// - 2 bytes: Fingerprint length (little-endian u16)
    /// - N bytes: Fingerprint string (UTF-8, optional)
    fn parse_signature_data(&self, data: &[u8]) -> Result<EmbeddedSignature, PeSigError> {
        // Minimum size: magic (8) + signature (64) + hash (32) = 104 bytes
        const MIN_SIZE: usize = 8 + 64 + 32;

        if data.len() < MIN_SIZE {
            return Err(PeSigError::MalformedSignature(format!(
                "Signature section too small: {} bytes (minimum {})",
                data.len(),
                MIN_SIZE
            )));
        }

        // Check magic header
        let magic = &data[0..8];
        if magic != SIGNATURE_MAGIC {
            // Try parsing as JSON format (legacy/alternative format)
            return self.parse_json_signature(data);
        }

        // Extract signature (64 bytes)
        let signature = data[8..72].to_vec();

        // Extract signed data hash (32 bytes)
        let mut signed_data_hash = [0u8; 32];
        signed_data_hash.copy_from_slice(&data[72..104]);

        // Extract optional timestamp (8 bytes) if present
        let timestamp = if data.len() >= MIN_SIZE + 8 {
            let ts_bytes: [u8; 8] = data[104..112]
                .try_into()
                .map_err(|_| PeSigError::MalformedSignature("Invalid timestamp bytes".to_string()))?;
            let ts = u64::from_le_bytes(ts_bytes);
            if ts > 0 {
                Some(ts)
            } else {
                None
            }
        } else {
            None
        };

        // Extract optional fingerprint if present
        let signer_fingerprint = if data.len() >= MIN_SIZE + 8 + 2 {
            let fp_len_bytes: [u8; 2] = data[112..114].try_into().map_err(|_| {
                PeSigError::MalformedSignature("Invalid fingerprint length bytes".to_string())
            })?;
            let fp_len = u16::from_le_bytes(fp_len_bytes) as usize;

            if fp_len > 0 && data.len() >= MIN_SIZE + 8 + 2 + fp_len {
                let fp_bytes = &data[114..114 + fp_len];
                Some(String::from_utf8_lossy(fp_bytes).to_string())
            } else {
                None
            }
        } else {
            None
        };

        Ok(EmbeddedSignature {
            signature,
            signed_data_hash,
            signer_fingerprint,
            timestamp,
        })
    }

    /// Parse signature data from JSON format (alternative/legacy format)
    ///
    /// This supports a JSON-based signature format for compatibility with
    /// tools that embed JSON metadata in the signature section.
    fn parse_json_signature(&self, data: &[u8]) -> Result<EmbeddedSignature, PeSigError> {
        // Try to parse as JSON
        let json_str = std::str::from_utf8(data).map_err(|_| {
            PeSigError::MalformedSignature("Signature section is not valid UTF-8 or binary format".to_string())
        })?;

        #[derive(serde::Deserialize)]
        struct JsonSignature {
            signature: String,
            #[serde(default)]
            key_fingerprint: Option<String>,
            #[serde(default)]
            timestamp: Option<String>,
        }

        let json_sig: JsonSignature = serde_json::from_str(json_str).map_err(|e| {
            PeSigError::MalformedSignature(format!("Failed to parse JSON signature: {}", e))
        })?;

        // Decode base64 signature
        let signature_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &json_sig.signature)
                .map_err(|e| {
                    PeSigError::MalformedSignature(format!("Invalid base64 signature: {}", e))
                })?;

        // Compute hash of the signature itself as a placeholder
        // (actual signed data hash should be computed separately)
        let mut hasher = Sha256::new();
        hasher.update(&signature_bytes);
        let hash_result = hasher.finalize();
        let mut signed_data_hash = [0u8; 32];
        signed_data_hash.copy_from_slice(&hash_result);

        // Parse timestamp if present
        let timestamp = json_sig.timestamp.as_ref().and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.timestamp() as u64)
        });

        Ok(EmbeddedSignature {
            signature: signature_bytes,
            signed_data_hash,
            signer_fingerprint: json_sig.key_fingerprint,
            timestamp,
        })
    }

    /// Compute the SHA256 hash of the PE binary excluding the signature section
    ///
    /// This can be used to verify that the signature matches the binary content.
    ///
    /// # Arguments
    ///
    /// * `binary_path` - Path to the PE binary file
    ///
    /// # Returns
    ///
    /// A 32-byte SHA256 hash of the binary content (excluding signature section)
    pub fn compute_signed_data_hash(&self, binary_path: &Path) -> Result<[u8; 32], PeSigError> {
        let file_data = fs::read(binary_path)?;

        let pe = match PE::parse(&file_data) {
            Ok(pe) => pe,
            Err(e) => {
                return Err(PeSigError::InvalidPe(format!(
                    "Failed to parse PE: {}",
                    e
                )));
            }
        };

        let mut hasher = Sha256::new();

        // Find signature section bounds
        let mut sig_start: Option<usize> = None;
        let mut sig_end: Option<usize> = None;

        for section in &pe.sections {
            let name = String::from_utf8_lossy(&section.name);
            let name = name.trim_end_matches('\0');
            if name == PE_SIGNATURE_SECTION {
                sig_start = Some(section.pointer_to_raw_data as usize);
                sig_end = Some(
                    section.pointer_to_raw_data as usize + section.size_of_raw_data as usize,
                );
                break;
            }
        }

        // Hash everything except the signature section
        match (sig_start, sig_end) {
            (Some(start), Some(end)) => {
                // Hash data before signature section
                if start > 0 {
                    hasher.update(&file_data[..start]);
                }
                // Hash data after signature section
                if end < file_data.len() {
                    hasher.update(&file_data[end..]);
                }
            }
            _ => {
                // No signature section, hash entire file
                hasher.update(&file_data);
            }
        }

        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Ok(hash)
    }
}

/// Trim null bytes from the end of section data
fn trim_null_bytes(data: &[u8]) -> &[u8] {
    let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &data[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Helper to create a minimal PE binary for testing
    /// This creates a valid PE32 structure with optional .orsig section
    fn create_mock_pe_binary(signature_data: Option<&[u8]>) -> Vec<u8> {
        // DOS Header (64 bytes minimum)
        let mut pe_data = vec![0u8; 512]; // Pre-allocate with zeros

        // DOS Magic "MZ"
        pe_data[0] = 0x4D; // 'M'
        pe_data[1] = 0x5A; // 'Z'

        // e_lfanew - PE header offset at 0x3C (points to PE signature)
        pe_data[0x3C] = 0x80; // PE header at offset 0x80

        // PE Signature "PE\0\0" at offset 0x80
        pe_data[0x80] = 0x50; // 'P'
        pe_data[0x81] = 0x45; // 'E'
        pe_data[0x82] = 0x00;
        pe_data[0x83] = 0x00;

        // COFF Header (20 bytes) starting at 0x84
        // Machine: 0x8664 for AMD64
        pe_data[0x84] = 0x64;
        pe_data[0x85] = 0x86;

        // NumberOfSections: 1 or 2 depending on signature
        let num_sections: u16 = if signature_data.is_some() { 2 } else { 1 };
        pe_data[0x86] = (num_sections & 0xFF) as u8;
        pe_data[0x87] = ((num_sections >> 8) & 0xFF) as u8;

        // TimeDateStamp (4 bytes)
        pe_data[0x88..0x8C].copy_from_slice(&[0x00; 4]);

        // PointerToSymbolTable (4 bytes)
        pe_data[0x8C..0x90].copy_from_slice(&[0x00; 4]);

        // NumberOfSymbols (4 bytes)
        pe_data[0x90..0x94].copy_from_slice(&[0x00; 4]);

        // SizeOfOptionalHeader: 240 bytes for PE32+
        pe_data[0x94] = 0xF0;
        pe_data[0x95] = 0x00;

        // Characteristics: 0x22 (Executable, No relocations)
        pe_data[0x96] = 0x22;
        pe_data[0x97] = 0x00;

        // Optional Header (PE32+) starting at 0x98
        // Magic: 0x20B for PE32+
        pe_data[0x98] = 0x0B;
        pe_data[0x99] = 0x02;

        // Fill rest of optional header with minimal values
        // ... (keeping mostly zeros for this mock)

        // Section headers start at 0x98 + 240 = 0x188
        let section_header_start = 0x188;

        // First section: .text
        let text_section = section_header_start;
        pe_data[text_section..text_section + 8].copy_from_slice(b".text\0\0\0");
        // VirtualSize
        pe_data[text_section + 8..text_section + 12].copy_from_slice(&[0x00, 0x10, 0x00, 0x00]);
        // VirtualAddress
        pe_data[text_section + 12..text_section + 16].copy_from_slice(&[0x00, 0x10, 0x00, 0x00]);
        // SizeOfRawData
        pe_data[text_section + 16..text_section + 20].copy_from_slice(&[0x00, 0x02, 0x00, 0x00]);
        // PointerToRawData
        pe_data[text_section + 20..text_section + 24].copy_from_slice(&[0x00, 0x02, 0x00, 0x00]);

        // Section raw data starts at 0x200
        pe_data.resize(0x400, 0);

        // Add signature section if provided
        if let Some(sig_data) = signature_data {
            let orsig_section = section_header_start + 40; // Second section header

            // Section name: .orsig
            pe_data[orsig_section..orsig_section + 8].copy_from_slice(b".orsig\0\0");

            // VirtualSize
            let sig_size = sig_data.len() as u32;
            pe_data[orsig_section + 8..orsig_section + 12].copy_from_slice(&sig_size.to_le_bytes());

            // VirtualAddress
            pe_data[orsig_section + 12..orsig_section + 16]
                .copy_from_slice(&[0x00, 0x20, 0x00, 0x00]);

            // SizeOfRawData (aligned to 512 bytes)
            let aligned_size = ((sig_data.len() + 511) / 512) * 512;
            pe_data[orsig_section + 16..orsig_section + 20]
                .copy_from_slice(&(aligned_size as u32).to_le_bytes());

            // PointerToRawData - signature data starts at 0x400
            pe_data[orsig_section + 20..orsig_section + 24]
                .copy_from_slice(&[0x00, 0x04, 0x00, 0x00]);

            // Extend the file and add signature data at offset 0x400
            pe_data.resize(0x400 + aligned_size, 0);
            pe_data[0x400..0x400 + sig_data.len()].copy_from_slice(sig_data);
        }

        pe_data
    }

    /// Create a binary format signature for testing
    fn create_binary_signature(
        signature: &[u8; 64],
        hash: &[u8; 32],
        timestamp: Option<u64>,
        fingerprint: Option<&str>,
    ) -> Vec<u8> {
        let mut data = Vec::new();

        // Magic header
        data.extend_from_slice(SIGNATURE_MAGIC);

        // Signature (64 bytes)
        data.extend_from_slice(signature);

        // Hash (32 bytes)
        data.extend_from_slice(hash);

        // Timestamp (8 bytes)
        let ts = timestamp.unwrap_or(0);
        data.extend_from_slice(&ts.to_le_bytes());

        // Fingerprint length and data
        if let Some(fp) = fingerprint {
            let fp_bytes = fp.as_bytes();
            data.extend_from_slice(&(fp_bytes.len() as u16).to_le_bytes());
            data.extend_from_slice(fp_bytes);
        } else {
            data.extend_from_slice(&0u16.to_le_bytes());
        }

        data
    }

    #[test]
    fn test_extract_signature_no_section() -> Result<(), Box<dyn std::error::Error>> {
        let pe_data = create_mock_pe_binary(None);

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(temp_file.path())?;

        assert!(result.is_none(), "Expected no signature for PE without .orsig section");

        Ok(())
    }

    #[test]
    fn test_has_signature_section_false() -> Result<(), Box<dyn std::error::Error>> {
        let pe_data = create_mock_pe_binary(None);

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let has_sig = extractor.has_signature_section(temp_file.path())?;

        assert!(!has_sig, "Expected no signature section");

        Ok(())
    }

    #[test]
    fn test_has_signature_section_true() -> Result<(), Box<dyn std::error::Error>> {
        let signature = [0xABu8; 64];
        let hash = [0xCDu8; 32];
        let sig_data = create_binary_signature(&signature, &hash, None, None);
        let pe_data = create_mock_pe_binary(Some(&sig_data));

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let has_sig = extractor.has_signature_section(temp_file.path())?;

        assert!(has_sig, "Expected signature section to be found");

        Ok(())
    }

    #[test]
    fn test_extract_binary_signature() -> Result<(), Box<dyn std::error::Error>> {
        let signature = [0x42u8; 64];
        let hash = [0x24u8; 32];
        let timestamp = 1700000000u64;
        let fingerprint = "abc123def456";

        let sig_data = create_binary_signature(&signature, &hash, Some(timestamp), Some(fingerprint));
        let pe_data = create_mock_pe_binary(Some(&sig_data));

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(temp_file.path())?;

        assert!(result.is_some(), "Expected signature to be extracted");

        let embedded_sig = result.ok_or("Expected Some")?;
        assert_eq!(embedded_sig.signature, signature.to_vec());
        assert_eq!(embedded_sig.signed_data_hash, hash);
        assert_eq!(embedded_sig.timestamp, Some(timestamp));
        assert_eq!(
            embedded_sig.signer_fingerprint,
            Some(fingerprint.to_string())
        );

        Ok(())
    }

    #[test]
    fn test_extract_json_signature() -> Result<(), Box<dyn std::error::Error>> {
        let json_sig = r#"{
            "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "key_fingerprint": "sha256:abcd1234",
            "timestamp": "2024-01-01T00:00:00Z"
        }"#;

        let pe_data = create_mock_pe_binary(Some(json_sig.as_bytes()));

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(temp_file.path())?;

        assert!(result.is_some(), "Expected JSON signature to be parsed");

        let embedded_sig = result.ok_or("Expected Some")?;
        assert!(!embedded_sig.signature.is_empty());
        assert_eq!(
            embedded_sig.signer_fingerprint,
            Some("sha256:abcd1234".to_string())
        );
        assert!(embedded_sig.timestamp.is_some());

        Ok(())
    }

    #[test]
    fn test_invalid_pe_error() -> Result<(), Box<dyn std::error::Error>> {
        let invalid_data = b"This is not a valid PE file";

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(invalid_data)?;

        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(temp_file.path());

        assert!(result.is_err(), "Expected error for invalid PE");

        match result {
            Err(PeSigError::InvalidPe(_)) => {} // Expected
            Err(other) => {
                return Err(format!("Expected InvalidPe error, got: {:?}", other).into());
            }
            Ok(_) => {
                return Err("Expected error, got Ok".into());
            }
        }

        Ok(())
    }

    #[test]
    fn test_malformed_signature_too_small() -> Result<(), Box<dyn std::error::Error>> {
        // Signature section with only magic header (too small)
        let sig_data = SIGNATURE_MAGIC.to_vec();
        let pe_data = create_mock_pe_binary(Some(&sig_data));

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(temp_file.path());

        assert!(result.is_err(), "Expected error for too-small signature");

        match result {
            Err(PeSigError::MalformedSignature(msg)) => {
                assert!(
                    msg.contains("too small"),
                    "Error should mention size: {}",
                    msg
                );
            }
            Err(other) => {
                return Err(format!("Expected MalformedSignature error, got: {:?}", other).into());
            }
            Ok(_) => {
                return Err("Expected error, got Ok".into());
            }
        }

        Ok(())
    }

    #[test]
    fn test_io_error_nonexistent_file() -> Result<(), Box<dyn std::error::Error>> {
        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(Path::new("/nonexistent/path/to/file.dll"));

        assert!(result.is_err(), "Expected error for nonexistent file");

        match result {
            Err(PeSigError::IoError(_)) => {} // Expected
            Err(other) => {
                return Err(format!("Expected IoError, got: {:?}", other).into());
            }
            Ok(_) => {
                return Err("Expected error, got Ok".into());
            }
        }

        Ok(())
    }

    #[test]
    fn test_extract_signature_no_timestamp_or_fingerprint() -> Result<(), Box<dyn std::error::Error>>
    {
        let signature = [0x11u8; 64];
        let hash = [0x22u8; 32];

        // Create signature with only required fields (no timestamp or fingerprint)
        let sig_data = create_binary_signature(&signature, &hash, None, None);
        let pe_data = create_mock_pe_binary(Some(&sig_data));

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let result = extractor.extract_signature(temp_file.path())?;

        assert!(result.is_some(), "Expected signature to be extracted");

        let embedded_sig = result.ok_or("Expected Some")?;
        assert_eq!(embedded_sig.signature, signature.to_vec());
        assert_eq!(embedded_sig.signed_data_hash, hash);
        assert!(
            embedded_sig.timestamp.is_none(),
            "Expected no timestamp"
        );
        assert!(
            embedded_sig.signer_fingerprint.is_none(),
            "Expected no fingerprint"
        );

        Ok(())
    }

    #[test]
    fn test_compute_signed_data_hash() -> Result<(), Box<dyn std::error::Error>> {
        let signature = [0x33u8; 64];
        let hash = [0x44u8; 32];
        let sig_data = create_binary_signature(&signature, &hash, None, None);
        let pe_data = create_mock_pe_binary(Some(&sig_data));

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pe_data)?;

        let extractor = PeSignatureExtractor::new();
        let computed_hash = extractor.compute_signed_data_hash(temp_file.path())?;

        // Hash should be 32 bytes and non-zero
        assert_eq!(computed_hash.len(), 32);
        assert!(
            computed_hash.iter().any(|&b| b != 0),
            "Hash should not be all zeros"
        );

        Ok(())
    }

    #[test]
    fn test_embedded_signature_clone() -> Result<(), Box<dyn std::error::Error>> {
        let sig = EmbeddedSignature {
            signature: vec![1, 2, 3, 4],
            signed_data_hash: [5u8; 32],
            signer_fingerprint: Some("test-fp".to_string()),
            timestamp: Some(12345),
        };

        let cloned = sig.clone();

        assert_eq!(sig.signature, cloned.signature);
        assert_eq!(sig.signed_data_hash, cloned.signed_data_hash);
        assert_eq!(sig.signer_fingerprint, cloned.signer_fingerprint);
        assert_eq!(sig.timestamp, cloned.timestamp);

        Ok(())
    }

    #[test]
    fn test_pe_sig_error_display() -> Result<(), Box<dyn std::error::Error>> {
        let io_err = PeSigError::IoError(io::Error::new(io::ErrorKind::NotFound, "test"));
        assert!(io_err.to_string().contains("I/O error"));

        let invalid_pe = PeSigError::InvalidPe("bad magic".to_string());
        assert!(invalid_pe.to_string().contains("Invalid PE"));
        assert!(invalid_pe.to_string().contains("bad magic"));

        let no_section = PeSigError::NoSignatureSection;
        assert!(no_section.to_string().contains("No signature section"));

        let malformed = PeSigError::MalformedSignature("corrupt data".to_string());
        assert!(malformed.to_string().contains("Malformed signature"));
        assert!(malformed.to_string().contains("corrupt data"));

        Ok(())
    }

    #[test]
    fn test_trim_null_bytes() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(trim_null_bytes(&[1, 2, 3, 0, 0, 0]), &[1, 2, 3]);
        assert_eq!(trim_null_bytes(&[0, 0, 0]), &[] as &[u8]);
        assert_eq!(trim_null_bytes(&[1, 2, 3]), &[1, 2, 3]);
        assert_eq!(trim_null_bytes(&[]), &[] as &[u8]);
        assert_eq!(trim_null_bytes(&[0, 1, 2, 0]), &[0, 1, 2]);

        Ok(())
    }

    #[test]
    fn test_extractor_default() -> Result<(), Box<dyn std::error::Error>> {
        let extractor = PeSignatureExtractor::default();
        // Just verify it can be created via Default
        let _ = format!("{:?}", extractor);

        Ok(())
    }
}
