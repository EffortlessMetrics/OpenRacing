//! Delta update implementation using binary diffs

use anyhow::{Context, Result};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Compress data for storage in update packages
pub fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .context("Failed to write data to compressor")?;

    encoder.finish().context("Failed to finish compression")
}

/// Decompress data from update packages
pub fn decompress_data(compressed_data: &[u8]) -> Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(compressed_data);
    let mut decompressed = Vec::new();

    decoder
        .read_to_end(&mut decompressed)
        .context("Failed to decompress data")?;

    Ok(decompressed)
}

/// Compute SHA256 hash of a file
pub async fn compute_file_hash(file_path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut file = tokio::fs::File::open(file_path)
        .await
        .context("Failed to open file for hashing")?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .await
            .context("Failed to read file for hashing")?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Create a binary delta patch between two files
pub async fn create_delta_patch(old_file: &Path, new_file: &Path) -> Result<Vec<u8>> {
    // Read both files
    let old_data = tokio::fs::read(old_file)
        .await
        .context("Failed to read old file")?;

    let new_data = tokio::fs::read(new_file)
        .await
        .context("Failed to read new file")?;

    // Create binary diff using a simple algorithm
    // In production, you might want to use a more sophisticated algorithm like bsdiff
    let patch = create_simple_patch(&old_data, &new_data)?;

    // Compress the patch
    compress_data(&patch)
}

/// Apply a binary delta patch to a file
pub async fn apply_delta_patch(target_file: &Path, compressed_patch: &[u8]) -> Result<()> {
    // Decompress patch
    let patch = decompress_data(compressed_patch).context("Failed to decompress delta patch")?;

    // Read current file
    let current_data = tokio::fs::read(target_file)
        .await
        .context("Failed to read target file")?;

    // Apply patch
    let new_data = apply_simple_patch(&current_data, &patch).context("Failed to apply patch")?;

    // Write result back to file
    tokio::fs::write(target_file, &new_data)
        .await
        .context("Failed to write patched file")?;

    Ok(())
}

/// Create a simple binary patch (for demonstration - use bsdiff in production)
fn create_simple_patch(old_data: &[u8], new_data: &[u8]) -> Result<Vec<u8>> {
    let mut patch = Vec::new();

    // Write patch header
    std::io::Write::write_all(&mut patch, b"WSPATCH1")?; // Magic number
    std::io::Write::write_all(&mut patch, &(old_data.len() as u64).to_le_bytes())?; // Old size
    std::io::Write::write_all(&mut patch, &(new_data.len() as u64).to_le_bytes())?; // New size

    // Simple algorithm: find common blocks and encode differences
    let mut old_pos = 0;
    let mut new_pos = 0;

    while new_pos < new_data.len() {
        // Find longest common substring starting at current positions
        let mut match_len = 0;
        let mut best_old_pos = old_pos;

        // Search for matches in old data
        for search_pos in 0..old_data.len() {
            let mut len = 0;
            while new_pos + len < new_data.len()
                && search_pos + len < old_data.len()
                && new_data[new_pos + len] == old_data[search_pos + len]
            {
                len += 1;
            }

            if len > match_len && len >= 4 {
                // Minimum match length
                match_len = len;
                best_old_pos = search_pos;
            }
        }

        if match_len >= 4 {
            // Encode copy operation: COPY old_pos len
            std::io::Write::write_all(&mut patch, &[0x01])?; // COPY command
            std::io::Write::write_all(&mut patch, &(best_old_pos as u64).to_le_bytes())?;
            std::io::Write::write_all(&mut patch, &(match_len as u64).to_le_bytes())?;

            new_pos += match_len;
            old_pos = best_old_pos + match_len;
        } else {
            // Encode insert operation: INSERT byte
            std::io::Write::write_all(&mut patch, &[0x02])?; // INSERT command
            std::io::Write::write_all(&mut patch, &[new_data[new_pos]])?;

            new_pos += 1;
        }
    }

    // End marker
    std::io::Write::write_all(&mut patch, &[0x00])?;

    Ok(patch)
}

/// Apply a simple binary patch
fn apply_simple_patch(old_data: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    use std::io::Cursor;

    let mut cursor = Cursor::new(patch);
    let mut result = Vec::new();

    // Read and verify header
    let mut magic = [0u8; 8];
    std::io::Read::read_exact(&mut cursor, &mut magic)?;
    if &magic != b"WSPATCH1" {
        return Err(anyhow::anyhow!("Invalid patch magic number"));
    }

    let mut size_bytes = [0u8; 8];
    std::io::Read::read_exact(&mut cursor, &mut size_bytes)?;
    let expected_old_size = u64::from_le_bytes(size_bytes) as usize;

    std::io::Read::read_exact(&mut cursor, &mut size_bytes)?;
    let expected_new_size = u64::from_le_bytes(size_bytes) as usize;

    if old_data.len() != expected_old_size {
        return Err(anyhow::anyhow!(
            "Old data size mismatch: expected {}, got {}",
            expected_old_size,
            old_data.len()
        ));
    }

    // Process commands
    loop {
        let mut cmd = [0u8; 1];
        if std::io::Read::read_exact(&mut cursor, &mut cmd).is_err() {
            break; // End of patch
        }

        match cmd[0] {
            0x00 => break, // End marker

            0x01 => {
                // COPY command
                std::io::Read::read_exact(&mut cursor, &mut size_bytes)?;
                let old_pos = u64::from_le_bytes(size_bytes) as usize;

                std::io::Read::read_exact(&mut cursor, &mut size_bytes)?;
                let len = u64::from_le_bytes(size_bytes) as usize;

                if old_pos + len > old_data.len() {
                    return Err(anyhow::anyhow!("Copy operation out of bounds"));
                }

                result.extend_from_slice(&old_data[old_pos..old_pos + len]);
            }

            0x02 => {
                // INSERT command
                let mut byte = [0u8; 1];
                std::io::Read::read_exact(&mut cursor, &mut byte)?;
                result.push(byte[0]);
            }

            _ => {
                return Err(anyhow::anyhow!("Unknown patch command: {}", cmd[0]));
            }
        }
    }

    if result.len() != expected_new_size {
        return Err(anyhow::anyhow!(
            "Result size mismatch: expected {}, got {}",
            expected_new_size,
            result.len()
        ));
    }

    Ok(result)
}

/// Utilities for creating update packages
pub mod package_builder {
    use super::*;
    use crate::update::{FileOperation, UpdateFile, UpdatePackage, UpdateType};

    /// Builder for creating update packages
    pub struct UpdatePackageBuilder {
        target_version: semver::Version,
        update_type: UpdateType,
        files: Vec<UpdateFile>,
    }

    impl UpdatePackageBuilder {
        /// Create a new package builder for a full update
        pub fn new_full(target_version: semver::Version) -> Self {
            Self {
                target_version,
                update_type: UpdateType::Full,
                files: Vec::new(),
            }
        }

        /// Create a new package builder for a delta update
        pub fn new_delta(target_version: semver::Version, from_version: semver::Version) -> Self {
            Self {
                target_version,
                update_type: UpdateType::Delta { from_version },
                files: Vec::new(),
            }
        }

        /// Add a file replacement to the package
        pub async fn add_file_replacement(
            &mut self,
            relative_path: &Path,
            new_file_path: &Path,
        ) -> Result<()> {
            let data = tokio::fs::read(new_file_path)
                .await
                .context("Failed to read new file")?;

            let compressed_data = compress_data(&data).context("Failed to compress file data")?;

            let expected_hash = compute_file_hash(new_file_path)
                .await
                .context("Failed to compute file hash")?;

            let file_metadata = tokio::fs::metadata(new_file_path)
                .await
                .context("Failed to get file metadata")?;

            self.files.push(UpdateFile {
                path: relative_path.to_path_buf(),
                operation: FileOperation::Replace {
                    data: compressed_data,
                },
                expected_hash,
                expected_size: file_metadata.len(),
                critical: is_critical_file(relative_path),
            });

            Ok(())
        }

        /// Add a delta patch to the package
        pub async fn add_delta_patch(
            &mut self,
            relative_path: &Path,
            old_file: &Path,
            new_file: &Path,
        ) -> Result<()> {
            let patch = create_delta_patch(old_file, new_file)
                .await
                .context("Failed to create delta patch")?;

            let expected_hash = compute_file_hash(new_file)
                .await
                .context("Failed to compute new file hash")?;

            let file_metadata = tokio::fs::metadata(new_file)
                .await
                .context("Failed to get file metadata")?;

            self.files.push(UpdateFile {
                path: relative_path.to_path_buf(),
                operation: FileOperation::Delta { patch },
                expected_hash,
                expected_size: file_metadata.len(),
                critical: is_critical_file(relative_path),
            });

            Ok(())
        }

        /// Build the update package
        pub fn build(self) -> UpdatePackage {
            let modified_files: Vec<_> = self.files.iter().map(|f| f.path.clone()).collect();
            UpdatePackage {
                version: "1.0".to_string(),
                target_version: self.target_version,
                min_version: None,
                update_type: self.update_type,
                files: self.files,
                pre_checks: vec![crate::update::HealthCheck {
                    id: "service_stop".to_string(),
                    description: "Stop racing wheel service".to_string(),
                    check_type: crate::update::HealthCheckType::Command {
                        command: "systemctl".to_string(),
                        args: vec![
                            "--user".to_string(),
                            "stop".to_string(),
                            "racing-wheel-suite.service".to_string(),
                        ],
                        expected_exit_code: 0,
                    },
                    timeout_seconds: 30,
                    critical: true,
                }],
                post_checks: vec![
                    crate::update::HealthCheck {
                        id: "service_start".to_string(),
                        description: "Start racing wheel service".to_string(),
                        check_type: crate::update::HealthCheckType::ServiceStart,
                        timeout_seconds: 30,
                        critical: true,
                    },
                    crate::update::HealthCheck {
                        id: "service_ping".to_string(),
                        description: "Verify service responds".to_string(),
                        check_type: crate::update::HealthCheckType::ServicePing,
                        timeout_seconds: 10,
                        critical: true,
                    },
                ],
                rollback_info: crate::update::RollbackInfo {
                    supported: true,
                    backup_path: None,
                    backup_retention_seconds: 7 * 24 * 3600, // 7 days
                    modified_files,
                },
                signature: None, // Will be added during signing
            }
        }
    }

    /// Check if a file is critical for operation
    fn is_critical_file(path: &Path) -> bool {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Core binaries are critical
        matches!(
            filename,
            "wheeld" | "wheeld.exe" | "wheelctl" | "wheelctl.exe"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_compression_roundtrip() -> Result<()> {
        let original_data = b"Hello, world! This is test data for compression.";

        let compressed = compress_data(original_data)?;
        let decompressed = decompress_data(&compressed)?;

        assert_eq!(original_data, decompressed.as_slice());
        Ok(())
    }

    #[tokio::test]
    async fn test_file_hash() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        tokio::fs::write(temp_file.path(), b"test data").await?;

        let hash = compute_file_hash(temp_file.path()).await?;

        // SHA256 of "test data"
        let expected = "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9";
        assert_eq!(hash, expected);
        Ok(())
    }

    #[test]
    fn test_simple_patch() -> Result<()> {
        let old_data = b"Hello, world!";
        let new_data = b"Hello, Rust world!";

        let patch = create_simple_patch(old_data, new_data)?;
        let result = apply_simple_patch(old_data, &patch)?;

        assert_eq!(result, new_data);
        Ok(())
    }
}
