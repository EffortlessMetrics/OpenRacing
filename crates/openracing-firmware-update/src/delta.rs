//! Delta update implementation using binary diffs
//!
//! Provides compression, decompression, and binary patching utilities for firmware updates.

use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::path::Path;

/// Compress data for storage in update packages
pub fn compress_data(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::{Compression, write::GzEncoder};

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .context("Failed to write data to compressor")?;

    encoder.finish().context("Failed to finish compression")
}

/// Decompress data from update packages
pub fn decompress_data(compressed_data: &[u8]) -> Result<Vec<u8>> {
    use flate2::read::GzDecoder;

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
    use tokio::io::AsyncReadExt;

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

/// Compute SHA256 hash of data
pub fn compute_data_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Create a binary delta patch between two files
pub async fn create_delta_patch(old_file: &Path, new_file: &Path) -> Result<Vec<u8>> {
    let old_data = tokio::fs::read(old_file)
        .await
        .context("Failed to read old file")?;

    let new_data = tokio::fs::read(new_file)
        .await
        .context("Failed to read new file")?;

    let patch = create_simple_patch(&old_data, &new_data)?;

    compress_data(&patch)
}

/// Apply a binary delta patch to a file
pub async fn apply_delta_patch(target_file: &Path, compressed_patch: &[u8]) -> Result<()> {
    let patch = decompress_data(compressed_patch).context("Failed to decompress delta patch")?;

    let current_data = tokio::fs::read(target_file)
        .await
        .context("Failed to read target file")?;

    let new_data = apply_simple_patch(&current_data, &patch).context("Failed to apply patch")?;

    tokio::fs::write(target_file, &new_data)
        .await
        .context("Failed to write patched file")?;

    Ok(())
}

/// Create a simple binary patch
pub fn create_simple_patch(old_data: &[u8], new_data: &[u8]) -> Result<Vec<u8>> {
    let mut patch = Vec::new();

    patch.write_all(b"WSPATCH1")?;
    patch.write_all(&(old_data.len() as u64).to_le_bytes())?;
    patch.write_all(&(new_data.len() as u64).to_le_bytes())?;

    let mut new_pos = 0;

    while new_pos < new_data.len() {
        let mut match_len = 0;
        let mut best_old_pos = 0;

        for search_pos in 0..old_data.len() {
            let mut len = 0;
            while new_pos + len < new_data.len()
                && search_pos + len < old_data.len()
                && new_data[new_pos + len] == old_data[search_pos + len]
            {
                len += 1;
            }

            if len > match_len && len >= 4 {
                match_len = len;
                best_old_pos = search_pos;
            }
        }

        if match_len >= 4 {
            patch.write_all(&[0x01])?;
            patch.write_all(&(best_old_pos as u64).to_le_bytes())?;
            patch.write_all(&(match_len as u64).to_le_bytes())?;

            new_pos += match_len;
        } else {
            patch.write_all(&[0x02])?;
            patch.write_all(&[new_data[new_pos]])?;

            new_pos += 1;
        }
    }

    patch.write_all(&[0x00])?;

    Ok(patch)
}

/// Apply a simple binary patch
pub fn apply_simple_patch(old_data: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    use std::io::Cursor;

    let mut cursor = Cursor::new(patch);
    let mut result = Vec::new();

    let mut magic = [0u8; 8];
    cursor.read_exact(&mut magic)?;
    if &magic != b"WSPATCH1" {
        return Err(anyhow::anyhow!("Invalid patch magic number"));
    }

    let mut size_bytes = [0u8; 8];
    cursor.read_exact(&mut size_bytes)?;
    let expected_old_size = u64::from_le_bytes(size_bytes) as usize;

    cursor.read_exact(&mut size_bytes)?;
    let expected_new_size = u64::from_le_bytes(size_bytes) as usize;

    if old_data.len() != expected_old_size {
        return Err(anyhow::anyhow!(
            "Old data size mismatch: expected {}, got {}",
            expected_old_size,
            old_data.len()
        ));
    }

    loop {
        let mut cmd = [0u8; 1];
        if cursor.read_exact(&mut cmd).is_err() {
            break;
        }

        match cmd[0] {
            0x00 => break,

            0x01 => {
                cursor.read_exact(&mut size_bytes)?;
                let old_pos = u64::from_le_bytes(size_bytes) as usize;

                cursor.read_exact(&mut size_bytes)?;
                let len = u64::from_le_bytes(size_bytes) as usize;

                if old_pos + len > old_data.len() {
                    return Err(anyhow::anyhow!("Copy operation out of bounds"));
                }

                result.extend_from_slice(&old_data[old_pos..old_pos + len]);
            }

            0x02 => {
                let mut byte = [0u8; 1];
                cursor.read_exact(&mut byte)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compression_roundtrip() -> Result<()> {
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

    #[test]
    fn test_data_hash() {
        let hash = compute_data_hash(b"test data");
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }
}
