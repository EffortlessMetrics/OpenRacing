//! Firmware Bundle Format (.owfb)
//!
//! Defines the OpenRacing Wheel Firmware Bundle format for packaging and
//! distributing firmware updates with signatures and metadata.
//!
//! # Format Overview
//!
//! The `.owfb` file format consists of:
//! 1. Magic number (8 bytes): "OWFB\0\0\0\1" (version 1)
//! 2. Header (JSON, length-prefixed)
//! 3. Metadata (JSON, length-prefixed)
//! 4. Compressed payload (zstd or gzip)
//! 5. Signature block (JSON, length-prefixed)
//!
//! All length prefixes are 4-byte little-endian unsigned integers.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};

use super::FirmwareImage;
use crate::crypto::verification::VerificationService;
use crate::crypto::{SignatureMetadata, TrustLevel};

/// Errors that can occur during firmware bundle operations
#[derive(Error, Debug)]
pub enum BundleError {
    /// Bundle signature is required but not present
    #[error("firmware signature required but bundle is unsigned")]
    SignatureRequired,

    /// Signature verification failed cryptographically
    #[error("firmware signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// Signer is not in trust store or is distrusted
    #[error("firmware signed by untrusted key: {0}")]
    UntrustedSigner(String),

    /// Payload hash does not match header
    #[error("payload hash mismatch: expected {expected}, got {actual}")]
    PayloadHashMismatch { expected: String, actual: String },

    /// Invalid bundle format
    #[error("invalid bundle format: {0}")]
    InvalidFormat(String),

    /// I/O error during bundle operations
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Magic bytes for the OWFB format (version 1)
pub const OWFB_MAGIC: &[u8; 8] = b"OWFB\0\0\0\x01";

/// Current bundle format version
pub const BUNDLE_FORMAT_VERSION: u32 = 1;

/// Bundle header containing identification and compatibility info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleHeader {
    /// Format version number
    pub format_version: u32,

    /// Target device model identifier
    pub device_model: String,

    /// Firmware version contained in this bundle
    pub firmware_version: semver::Version,

    /// Minimum compatible hardware version (inclusive)
    pub min_hw_version: Option<String>,

    /// Maximum compatible hardware version (inclusive)
    pub max_hw_version: Option<String>,

    /// Compression algorithm used for payload
    pub compression: CompressionType,

    /// Size of the uncompressed payload in bytes
    pub uncompressed_size: u64,

    /// Size of the compressed payload in bytes
    pub compressed_size: u64,

    /// SHA256 hash of the uncompressed payload
    pub payload_hash: String,
}

/// Compression types supported by the bundle format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression
    None,
    /// Gzip compression
    Gzip,
}

impl Default for CompressionType {
    fn default() -> Self {
        Self::Gzip
    }
}

/// Bundle metadata containing release information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleMetadata {
    /// Human-readable release title
    pub title: Option<String>,

    /// Release notes / changelog (markdown)
    pub changelog: Option<String>,

    /// Signing key fingerprint
    pub signing_key: Option<String>,

    /// Minimum version that can be upgraded from (for rollback protection)
    pub rollback_version: Option<semver::Version>,

    /// Release channel (stable, beta, nightly)
    pub channel: ReleaseChannel,

    /// Build timestamp
    pub build_timestamp: chrono::DateTime<chrono::Utc>,

    /// Additional custom metadata
    #[serde(default)]
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

impl Default for BundleMetadata {
    fn default() -> Self {
        Self {
            title: None,
            changelog: None,
            signing_key: None,
            rollback_version: None,
            channel: ReleaseChannel::Stable,
            build_timestamp: chrono::Utc::now(),
            custom: std::collections::HashMap::new(),
        }
    }
}

/// Release channel for firmware updates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReleaseChannel {
    /// Stable release for general use
    #[default]
    Stable,
    /// Beta release for testing
    Beta,
    /// Nightly/development builds
    Nightly,
}

/// OpenRacing Wheel Firmware Bundle
///
/// A complete firmware package with headers, metadata, payload, and signature.
#[derive(Debug, Clone)]
pub struct FirmwareBundle {
    /// Bundle header with identification and compatibility info
    pub header: BundleHeader,

    /// Release metadata
    pub metadata: BundleMetadata,

    /// Compressed firmware payload
    payload: Vec<u8>,

    /// Optional cryptographic signature
    pub signature: Option<SignatureMetadata>,
}

impl FirmwareBundle {
    /// Create a new firmware bundle from a firmware image
    ///
    /// This compresses the firmware data and computes necessary hashes.
    pub fn new(
        image: &FirmwareImage,
        metadata: BundleMetadata,
        compression: CompressionType,
    ) -> Result<Self> {
        let uncompressed_size = image.data.len() as u64;

        // Compute payload hash before compression
        let payload_hash = crate::crypto::utils::compute_sha256_hex(&image.data);

        // Compress payload
        let payload = match compression {
            CompressionType::None => image.data.clone(),
            CompressionType::Gzip => {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder
                    .write_all(&image.data)
                    .context("Failed to compress payload")?;
                encoder.finish().context("Failed to finish compression")?
            }
        };

        let compressed_size = payload.len() as u64;

        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            device_model: image.device_model.clone(),
            firmware_version: image.version.clone(),
            min_hw_version: image.min_hardware_version.clone(),
            max_hw_version: image.max_hardware_version.clone(),
            compression,
            uncompressed_size,
            compressed_size,
            payload_hash,
        };

        Ok(Self {
            header,
            metadata,
            payload,
            signature: image.signature.clone(),
        })
    }

    /// Load a firmware bundle from a file
    ///
    /// Optionally verifies the bundle signature using the provided verification service.
    pub fn load(path: &Path, verifier: Option<&VerificationService>) -> Result<Self> {
        info!("Loading firmware bundle from: {}", path.display());

        let data = std::fs::read(path).context("Failed to read bundle file")?;
        Self::parse(&data, verifier)
    }

    /// Parse a firmware bundle from bytes
    pub fn parse(data: &[u8], verifier: Option<&VerificationService>) -> Result<Self> {
        let mut cursor = std::io::Cursor::new(data);

        // Read and verify magic
        let mut magic = [0u8; 8];
        cursor
            .read_exact(&mut magic)
            .context("Failed to read magic")?;
        if &magic != OWFB_MAGIC {
            bail!("Invalid bundle magic: expected OWFB format");
        }

        // Read header - keep raw bytes for signature verification
        let (header, header_bytes): (BundleHeader, Vec<u8>) =
            read_json_block_with_bytes(&mut cursor).context("Failed to read bundle header")?;

        debug!(
            "Bundle header: device={}, version={}",
            header.device_model, header.firmware_version
        );

        // Verify format version
        if header.format_version > BUNDLE_FORMAT_VERSION {
            bail!(
                "Unsupported bundle format version: {} (max supported: {})",
                header.format_version,
                BUNDLE_FORMAT_VERSION
            );
        }

        // Read metadata
        let metadata: BundleMetadata =
            read_json_block(&mut cursor).context("Failed to read bundle metadata")?;

        // Read compressed payload
        let mut payload = vec![0u8; header.compressed_size as usize];
        cursor
            .read_exact(&mut payload)
            .context("Failed to read payload")?;

        // Read signature (optional - may not be present)
        let signature: Option<SignatureMetadata> = read_json_block(&mut cursor).ok();

        // Verify signature if verifier is provided
        if let Some(verifier) = verifier {
            let require_signatures = verifier.get_config().require_firmware_signatures;

            match &signature {
                Some(sig) => {
                    debug!("Verifying bundle signature from: {}", sig.signer);

                    // Cryptographically verify the signature against the header bytes
                    let verification_result = verifier
                        .verify_content(&header_bytes, sig)
                        .context("Failed to verify bundle signature")?;

                    if !verification_result.signature_valid {
                        return Err(BundleError::SignatureVerificationFailed(
                            "cryptographic signature verification failed".to_string(),
                        )
                        .into());
                    }

                    // Check trust level - warn but don't fail for unknown signers
                    // unless they are explicitly distrusted
                    match verification_result.trust_level {
                        TrustLevel::Trusted => {
                            info!("Bundle signature verified: trusted signer '{}'", sig.signer);
                        }
                        TrustLevel::Unknown => {
                            warn!(
                                "Bundle signed by unknown key (fingerprint: {}). \
                                 Consider adding to trust store.",
                                sig.key_fingerprint
                            );
                        }
                        TrustLevel::Distrusted => {
                            return Err(BundleError::UntrustedSigner(format!(
                                "key '{}' is explicitly distrusted",
                                sig.key_fingerprint
                            ))
                            .into());
                        }
                    }
                }
                None => {
                    if require_signatures {
                        return Err(BundleError::SignatureRequired.into());
                    }
                    warn!("Bundle is unsigned - firmware signature verification skipped");
                }
            }
        }

        let bundle = Self {
            header,
            metadata,
            payload,
            signature,
        };

        // Verify payload hash
        let decompressed = bundle.extract_payload()?;
        let computed_hash = crate::crypto::utils::compute_sha256_hex(&decompressed);
        if computed_hash != bundle.header.payload_hash {
            return Err(BundleError::PayloadHashMismatch {
                expected: bundle.header.payload_hash.clone(),
                actual: computed_hash,
            }
            .into());
        }

        info!(
            "Bundle loaded successfully: {} v{}",
            bundle.header.device_model, bundle.header.firmware_version
        );

        Ok(bundle)
    }

    /// Extract and decompress the firmware payload
    fn extract_payload(&self) -> Result<Vec<u8>> {
        match self.header.compression {
            CompressionType::None => Ok(self.payload.clone()),
            CompressionType::Gzip => {
                let mut decoder = GzDecoder::new(&self.payload[..]);
                let mut decompressed = Vec::with_capacity(self.header.uncompressed_size as usize);
                decoder
                    .read_to_end(&mut decompressed)
                    .context("Failed to decompress payload")?;
                Ok(decompressed)
            }
        }
    }

    /// Extract the firmware image from this bundle
    pub fn extract_image(&self) -> Result<FirmwareImage> {
        let data = self.extract_payload()?;

        Ok(FirmwareImage {
            device_model: self.header.device_model.clone(),
            version: self.header.firmware_version.clone(),
            min_hardware_version: self.header.min_hw_version.clone(),
            max_hardware_version: self.header.max_hw_version.clone(),
            data: data.clone(),
            hash: self.header.payload_hash.clone(),
            size_bytes: data.len() as u64,
            build_timestamp: self.metadata.build_timestamp,
            release_notes: self.metadata.changelog.clone(),
            signature: self.signature.clone(),
        })
    }

    /// Write the bundle to a file
    pub fn write(&self, path: &Path) -> Result<()> {
        let data = self.serialize()?;
        std::fs::write(path, data).context("Failed to write bundle file")?;
        info!("Bundle written to: {}", path.display());
        Ok(())
    }

    /// Serialize the bundle to bytes
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut output = Vec::new();

        // Write magic
        output.extend_from_slice(OWFB_MAGIC);

        // Write header
        write_json_block(&mut output, &self.header)?;

        // Write metadata
        write_json_block(&mut output, &self.metadata)?;

        // Write payload
        output.extend_from_slice(&self.payload);

        // Write signature if present
        if let Some(ref sig) = self.signature {
            write_json_block(&mut output, sig)?;
        }

        Ok(output)
    }

    /// Check if this bundle is compatible with the given hardware version
    ///
    /// Uses proper numeric version comparison so that "2.0" < "10.0" works correctly.
    /// If version parsing fails, returns false (fail closed for safety).
    pub fn is_compatible_with_hardware(&self, hw_version: &str) -> bool {
        use super::hardware_version::HardwareVersion;
        use std::cmp::Ordering;

        // Check minimum version
        if let Some(ref min) = self.header.min_hw_version {
            match HardwareVersion::try_compare(hw_version, min) {
                Some(Ordering::Less) => return false,
                None => return false, // Parse error = fail closed for safety
                _ => {}
            }
        }

        // Check maximum version
        if let Some(ref max) = self.header.max_hw_version {
            match HardwareVersion::try_compare(hw_version, max) {
                Some(Ordering::Greater) => return false,
                None => return false, // Parse error = fail closed for safety
                _ => {}
            }
        }

        true
    }

    /// Check if upgrading from the given version is allowed (rollback protection)
    pub fn allows_upgrade_from(&self, current_version: &semver::Version) -> bool {
        if let Some(ref rollback) = self.metadata.rollback_version {
            current_version >= rollback
        } else {
            true
        }
    }

    /// Get the total size of the bundle in bytes
    pub fn bundle_size(&self) -> usize {
        8 // magic
            + 4 + serde_json::to_vec(&self.header).map(|v| v.len()).unwrap_or(0)
            + 4 + serde_json::to_vec(&self.metadata).map(|v| v.len()).unwrap_or(0)
            + self.payload.len()
            + self.signature.as_ref()
                .and_then(|s| serde_json::to_vec(s).ok())
                .map(|v| 4 + v.len())
                .unwrap_or(0)
    }
}

/// Read a length-prefixed JSON block from a reader
fn read_json_block<R: Read, T: serde::de::DeserializeOwned>(reader: &mut R) -> Result<T> {
    let (value, _bytes) = read_json_block_with_bytes(reader)?;
    Ok(value)
}

/// Read a length-prefixed JSON block from a reader, returning both the parsed value and raw bytes
///
/// This is useful when we need to verify signatures against the raw JSON bytes.
fn read_json_block_with_bytes<R: Read, T: serde::de::DeserializeOwned>(
    reader: &mut R,
) -> Result<(T, Vec<u8>)> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .context("Failed to read block length")?;
    let len = u32::from_le_bytes(len_buf) as usize;

    let mut json_buf = vec![0u8; len];
    reader
        .read_exact(&mut json_buf)
        .context("Failed to read block data")?;

    let value = serde_json::from_slice(&json_buf).context("Failed to parse JSON block")?;
    Ok((value, json_buf))
}

/// Write a length-prefixed JSON block to a writer
fn write_json_block<W: Write, T: serde::Serialize>(writer: &mut W, value: &T) -> Result<()> {
    let json = serde_json::to_vec(value).context("Failed to serialize JSON block")?;
    let len = json.len() as u32;

    writer
        .write_all(&len.to_le_bytes())
        .context("Failed to write block length")?;
    writer
        .write_all(&json)
        .context("Failed to write block data")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_image() -> FirmwareImage {
        FirmwareImage {
            device_model: "test-wheel".to_string(),
            version: semver::Version::new(1, 2, 3),
            min_hardware_version: Some("1.0".to_string()),
            max_hardware_version: Some("2.0".to_string()),
            data: vec![0x01, 0x02, 0x03, 0x04, 0x05],
            hash: crate::crypto::utils::compute_sha256_hex(&[0x01, 0x02, 0x03, 0x04, 0x05]),
            size_bytes: 5,
            build_timestamp: chrono::Utc::now(),
            release_notes: Some("Test release".to_string()),
            signature: None,
        }
    }

    #[test]
    fn test_bundle_roundtrip_no_compression() -> Result<()> {
        let image = create_test_image();
        let metadata = BundleMetadata {
            title: Some("Test Bundle".to_string()),
            changelog: Some("Initial release".to_string()),
            ..Default::default()
        };

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized, None)?;

        assert_eq!(parsed.header.device_model, "test-wheel");
        assert_eq!(
            parsed.header.firmware_version,
            semver::Version::new(1, 2, 3)
        );
        assert_eq!(parsed.header.compression, CompressionType::None);

        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, image.data);

        Ok(())
    }

    #[test]
    fn test_bundle_roundtrip_gzip() -> Result<()> {
        let image = create_test_image();
        let metadata = BundleMetadata::default();

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;

        // Compressed size should be different (might be larger for tiny payloads)
        assert_eq!(bundle.header.uncompressed_size, 5);

        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized, None)?;

        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, image.data);

        Ok(())
    }

    #[test]
    fn test_bundle_write_and_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let bundle_path = temp_dir.path().join("test.owfb");

        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;

        bundle.write(&bundle_path)?;
        assert!(bundle_path.exists());

        let loaded = FirmwareBundle::load(&bundle_path, None)?;
        assert_eq!(loaded.header.device_model, "test-wheel");

        Ok(())
    }

    #[test]
    fn test_hardware_compatibility() -> Result<()> {
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // Within range
        assert!(bundle.is_compatible_with_hardware("1.5"));
        assert!(bundle.is_compatible_with_hardware("1.0"));
        assert!(bundle.is_compatible_with_hardware("2.0"));

        // Below minimum
        assert!(!bundle.is_compatible_with_hardware("0.9"));

        // Above maximum
        assert!(!bundle.is_compatible_with_hardware("2.1"));

        Ok(())
    }

    #[test]
    fn test_rollback_protection() -> Result<()> {
        let image = create_test_image();
        let metadata = BundleMetadata {
            rollback_version: Some(semver::Version::new(1, 0, 0)),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // Can upgrade from 1.0.0 or higher
        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 0, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 1, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 0, 0)));

        // Cannot upgrade from 0.x (rollback protection)
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(0, 9, 0)));

        Ok(())
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let data = b"INVALID\x01header";
        let result = FirmwareBundle::parse(data, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_bundle_size() -> Result<()> {
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        let serialized = bundle.serialize()?;
        assert_eq!(bundle.bundle_size(), serialized.len());

        Ok(())
    }

    // =========================================================================
    // Security Fix Tests: Hardware Version Comparison
    // =========================================================================

    #[test]
    fn test_hardware_version_numeric_comparison_10_vs_2() -> Result<()> {
        // This is the critical bug fix test
        // String comparison: "10.0" < "2.0" is TRUE (wrong!)
        // Numeric comparison: 10.0 > 2.0 (correct!)
        let mut image = create_test_image();
        image.min_hardware_version = Some("2.0".to_string());
        image.max_hardware_version = Some("10.0".to_string());

        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // Hardware version 5.0 should be compatible (2.0 <= 5.0 <= 10.0)
        assert!(
            bundle.is_compatible_with_hardware("5.0"),
            "5.0 should be between 2.0 and 10.0"
        );

        // Hardware version 10.0 should be compatible (boundary)
        assert!(
            bundle.is_compatible_with_hardware("10.0"),
            "10.0 should equal max version"
        );

        // Hardware version 2.0 should be compatible (boundary)
        assert!(
            bundle.is_compatible_with_hardware("2.0"),
            "2.0 should equal min version"
        );

        // Hardware version 1.0 should NOT be compatible (below min)
        assert!(
            !bundle.is_compatible_with_hardware("1.0"),
            "1.0 should be below 2.0 minimum"
        );

        // Hardware version 11.0 should NOT be compatible (above max)
        assert!(
            !bundle.is_compatible_with_hardware("11.0"),
            "11.0 should be above 10.0 maximum"
        );

        Ok(())
    }

    #[test]
    fn test_hardware_version_invalid_fails_closed() -> Result<()> {
        // Security: Invalid version strings should fail closed (return false)
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // Invalid hardware version should be rejected
        assert!(
            !bundle.is_compatible_with_hardware("invalid"),
            "Invalid version should fail closed"
        );
        assert!(
            !bundle.is_compatible_with_hardware(""),
            "Empty version should fail closed"
        );
        assert!(
            !bundle.is_compatible_with_hardware("1.x.2"),
            "Non-numeric version should fail closed"
        );

        Ok(())
    }

    // =========================================================================
    // Security Fix Tests: Signature Verification
    // =========================================================================

    #[test]
    fn test_missing_signature_with_require_false_passes() -> Result<()> {
        // When require_firmware_signatures is false, unsigned bundles should pass
        let temp_dir = TempDir::new()?;
        let config = crate::crypto::VerificationConfig {
            trust_store_path: temp_dir.path().join("trust_store.json"),
            require_firmware_signatures: false,
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;

        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;

        // Should succeed - signature not required
        let result = FirmwareBundle::parse(&serialized, Some(&verifier));
        assert!(
            result.is_ok(),
            "Unsigned bundle should pass when signatures not required"
        );

        Ok(())
    }

    #[test]
    fn test_missing_signature_with_require_true_fails() -> Result<()> {
        // When require_firmware_signatures is true, unsigned bundles should fail
        let temp_dir = TempDir::new()?;
        let config = crate::crypto::VerificationConfig {
            trust_store_path: temp_dir.path().join("trust_store.json"),
            require_firmware_signatures: true,
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;

        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;

        // Should fail - signature required but missing
        let result = FirmwareBundle::parse(&serialized, Some(&verifier));
        assert!(
            result.is_err(),
            "Unsigned bundle should fail when signatures required"
        );

        // Verify the error type
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("signature required") || err_msg.contains("SignatureRequired"),
            "Error should indicate signature is required: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn test_tampered_payload_fails_hash_verification() -> Result<()> {
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let mut serialized = bundle.serialize()?;

        // Find the payload in the serialized data and tamper with it
        // The payload is after magic (8) + header block + metadata block
        // For simplicity, just flip a byte near the end
        let len = serialized.len();
        if len > 10 {
            serialized[len - 5] ^= 0xFF; // Flip bits in payload area
        }

        // Should fail due to hash mismatch
        let result = FirmwareBundle::parse(&serialized, None);
        assert!(
            result.is_err(),
            "Tampered bundle should fail hash verification"
        );

        Ok(())
    }

    // =========================================================================
    // Security Test: Real Ed25519 Signature Round-Trip
    //
    // This test verifies that the ACTUAL cryptographic signature verification
    // is wired up correctly:
    // 1. Generate a real Ed25519 key pair
    // 2. Create a trust store containing the public key
    // 3. Sign a bundle with the real signature
    // 4. Verify the signed bundle passes verification
    // 5. Tamper with the signed bytes and verify it FAILS
    // =========================================================================

    #[test]
    fn test_real_ed25519_signature_roundtrip_pass() -> Result<()> {
        use crate::crypto::ed25519::{Ed25519Signer, KeyPair};
        use crate::crypto::trust_store::TrustStore;
        use crate::crypto::{ContentType, TrustLevel, VerificationConfig};

        // 1. Generate a real Ed25519 key pair
        let keypair = KeyPair::generate().map_err(|e| anyhow::anyhow!("{}", e))?;
        let fingerprint = keypair.fingerprint();

        // 2. Create a trust store with the public key
        let temp_dir = TempDir::new()?;
        let trust_store_path = temp_dir.path().join("trust_store.json");
        let mut trust_store = TrustStore::new(trust_store_path.clone())?;
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test signing key".to_string()),
        )?;
        trust_store.save_to_file()?;

        // 3. Create a bundle and sign it with the real key
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let mut bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // Serialize just the header to get the bytes we need to sign
        let header_json = serde_json::to_vec(&bundle.header)?;

        // Create a real signature over the header bytes
        let signature_metadata = Ed25519Signer::sign_with_metadata(
            &header_json,
            &keypair,
            "Test Firmware Signer",
            ContentType::Firmware,
            Some("Test bundle signature".to_string()),
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Attach the signature to the bundle
        bundle.signature = Some(signature_metadata);

        // Serialize the complete bundle with signature
        let serialized = bundle.serialize()?;

        // 4. Create verification service and verify the bundle
        let config = VerificationConfig {
            trust_store_path,
            require_firmware_signatures: true,
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;

        // Parse and verify - this should PASS
        let parsed = FirmwareBundle::parse(&serialized, Some(&verifier))?;

        // Verify the signature is present and matches
        assert!(parsed.signature.is_some(), "Signature should be present");
        assert_eq!(
            parsed.signature.as_ref().map(|s| &s.key_fingerprint),
            Some(&fingerprint),
            "Fingerprint should match"
        );

        Ok(())
    }

    #[test]
    fn test_real_ed25519_signature_tamper_detection() -> Result<()> {
        use crate::crypto::ed25519::{Ed25519Signer, KeyPair};
        use crate::crypto::trust_store::TrustStore;
        use crate::crypto::{ContentType, TrustLevel, VerificationConfig};

        // 1. Generate a real Ed25519 key pair
        let keypair = KeyPair::generate().map_err(|e| anyhow::anyhow!("{}", e))?;

        // 2. Create a trust store with the public key
        let temp_dir = TempDir::new()?;
        let trust_store_path = temp_dir.path().join("trust_store.json");
        let mut trust_store = TrustStore::new(trust_store_path.clone())?;
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test signing key".to_string()),
        )?;
        trust_store.save_to_file()?;

        // 3. Create and sign a bundle
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let mut bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // Get the original header JSON bytes
        let header_json = serde_json::to_vec(&bundle.header)?;

        // Sign the header
        let signature_metadata = Ed25519Signer::sign_with_metadata(
            &header_json,
            &keypair,
            "Test Firmware Signer",
            ContentType::Firmware,
            Some("Test bundle signature".to_string()),
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        bundle.signature = Some(signature_metadata);

        // Serialize the bundle
        let mut serialized = bundle.serialize()?;

        // 5. TAMPER with the header bytes in the serialized bundle
        // The header starts after the 8-byte magic and 4-byte length prefix (12 bytes total)
        // We need to flip a bit in the JSON data itself, not in the length prefix
        // Let's flip a byte further into the header JSON to avoid corrupting the length
        if serialized.len() > 50 {
            // Tamper with a byte well into the header JSON region
            serialized[40] ^= 0x01; // Flip one bit in the header JSON
        }

        // 6. Create verification service
        let config = VerificationConfig {
            trust_store_path,
            require_firmware_signatures: true,
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;

        // 7. Parse and verify - this should FAIL due to tampered header
        // The failure could manifest as:
        // - JSON parse error (if we corrupted the JSON structure)
        // - Signature verification failure (if the JSON is still valid but content changed)
        // - Hash mismatch (if the payload hash check fails)
        let result = FirmwareBundle::parse(&serialized, Some(&verifier));

        assert!(result.is_err(), "Tampered bundle should fail verification");

        // Any error is acceptable here - the point is the bundle is rejected
        // The specific error depends on where in the header the tampering occurred
        let err_msg = result
            .err()
            .map(|e| e.to_string().to_lowercase())
            .unwrap_or_default();
        assert!(!err_msg.is_empty(), "Should have an error message");
        // Log the error for debugging
        eprintln!("Tamper detection error (expected): {}", err_msg);

        Ok(())
    }

    #[test]
    fn test_signature_from_wrong_key_fails() -> Result<()> {
        use crate::crypto::ed25519::{Ed25519Signer, KeyPair};
        use crate::crypto::trust_store::TrustStore;
        use crate::crypto::{ContentType, TrustLevel, VerificationConfig};

        // 1. Generate TWO key pairs - one trusted, one used for signing
        let trusted_keypair = KeyPair::generate().map_err(|e| anyhow::anyhow!("{}", e))?;
        let attacker_keypair = KeyPair::generate().map_err(|e| anyhow::anyhow!("{}", e))?;

        // 2. Create a trust store with ONLY the trusted key
        let temp_dir = TempDir::new()?;
        let trust_store_path = temp_dir.path().join("trust_store.json");
        let mut trust_store = TrustStore::new(trust_store_path.clone())?;
        trust_store.add_key(
            trusted_keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Legitimate key".to_string()),
        )?;
        trust_store.save_to_file()?;

        // 3. Create a bundle and sign it with the ATTACKER's key
        let image = create_test_image();
        let metadata = BundleMetadata::default();
        let mut bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        let header_json = serde_json::to_vec(&bundle.header)?;

        // Sign with attacker's key (not in trust store)
        let signature_metadata = Ed25519Signer::sign_with_metadata(
            &header_json,
            &attacker_keypair, // Wrong key!
            "Attacker",
            ContentType::Firmware,
            None,
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;

        bundle.signature = Some(signature_metadata);
        let serialized = bundle.serialize()?;

        // 4. Create verification service
        let config = VerificationConfig {
            trust_store_path,
            require_firmware_signatures: true,
            allow_unknown_signers: false,
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;

        // 5. Parse and verify - should FAIL because attacker's key is not trusted
        let result = FirmwareBundle::parse(&serialized, Some(&verifier));

        assert!(
            result.is_err(),
            "Bundle signed by untrusted key should fail"
        );

        // The error could be:
        // - "untrusted signer" if the key is explicitly distrusted
        // - "unknown" if the key is not in the trust store
        // - "failed to verify" if signature verification fails
        let err_msg = result
            .err()
            .map(|e| e.to_string().to_lowercase())
            .unwrap_or_default();
        assert!(
            err_msg.contains("untrust")
                || err_msg.contains("unknown")
                || err_msg.contains("signer")
                || err_msg.contains("failed")
                || err_msg.contains("verify"),
            "Error should indicate verification failure: {}",
            err_msg
        );
        // Log the error for debugging
        eprintln!("Wrong key error (expected): {}", err_msg);

        Ok(())
    }
}
