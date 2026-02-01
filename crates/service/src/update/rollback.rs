//! Rollback functionality for failed updates

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Metadata for a backup created during update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// Unique identifier for this backup
    pub backup_id: String,

    /// When the backup was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Version before the update
    pub original_version: semver::Version,

    /// Target version of the update
    pub target_version: semver::Version,

    /// List of files that were backed up
    pub files: Vec<PathBuf>,
}

/// Perform a rollback using a specific backup
pub async fn perform_rollback(
    backup_dir: &Path,
    install_dir: &Path,
    backup_id: &str,
) -> Result<()> {
    let backup_path = backup_dir.join(backup_id);

    if !backup_path.exists() {
        return Err(anyhow::anyhow!("Backup not found: {}", backup_id));
    }

    // Load backup metadata
    let metadata_path = backup_path.join("metadata.json");
    let metadata_content = fs::read_to_string(&metadata_path)
        .await
        .context("Failed to read backup metadata")?;

    let metadata: BackupMetadata =
        serde_json::from_str(&metadata_content).context("Failed to parse backup metadata")?;

    tracing::info!(
        "Rolling back from version {} to version {}",
        metadata.target_version,
        metadata.original_version
    );

    // Restore each backed up file
    for file_path in &metadata.files {
        let backup_file_path = backup_path.join(file_path);
        let target_file_path = install_dir.join(file_path);

        if backup_file_path.exists() {
            // Create parent directories if needed
            if let Some(parent) = target_file_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .context("Failed to create parent directory during rollback")?;
            }

            // Restore the file
            fs::copy(&backup_file_path, &target_file_path)
                .await
                .with_context(|| {
                    format!(
                        "Failed to restore file: {} -> {}",
                        backup_file_path.display(),
                        target_file_path.display()
                    )
                })?;

            tracing::debug!("Restored file: {}", file_path.display());
        } else {
            // File was newly created in the update, so delete it
            if target_file_path.exists() {
                fs::remove_file(&target_file_path).await.with_context(|| {
                    format!(
                        "Failed to remove file during rollback: {}",
                        target_file_path.display()
                    )
                })?;

                tracing::debug!("Removed file: {}", file_path.display());
            }
        }
    }

    tracing::info!("Rollback completed successfully");
    Ok(())
}

/// List available backups
pub async fn list_backups(backup_dir: &Path) -> Result<Vec<BackupMetadata>> {
    let mut backups = Vec::new();

    if !backup_dir.exists() {
        return Ok(backups);
    }

    let mut entries = fs::read_dir(backup_dir)
        .await
        .context("Failed to read backup directory")?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .context("Failed to read backup directory entry")?
    {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let metadata_path = entry_path.join("metadata.json");

            if metadata_path.exists() {
                match load_backup_metadata(&metadata_path).await {
                    Ok(metadata) => backups.push(metadata),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load backup metadata from {}: {}",
                            metadata_path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    // Sort by creation time (newest first)
    backups.sort_by_key(|b| std::cmp::Reverse(b.created_at));

    Ok(backups)
}

/// Load backup metadata from a file
async fn load_backup_metadata(metadata_path: &Path) -> Result<BackupMetadata> {
    let content = fs::read_to_string(metadata_path)
        .await
        .context("Failed to read backup metadata file")?;

    serde_json::from_str(&content).context("Failed to parse backup metadata JSON")
}

/// Clean up old backups, keeping only the most recent ones
pub async fn cleanup_old_backups(backup_dir: &Path, keep_days: u32) -> Result<()> {
    let cutoff_time = chrono::Utc::now() - chrono::Duration::days(keep_days as i64);

    let backups = list_backups(backup_dir)
        .await
        .context("Failed to list backups for cleanup")?;

    let mut removed_count = 0;

    for backup in backups {
        if backup.created_at < cutoff_time {
            let backup_path = backup_dir.join(&backup.backup_id);

            match fs::remove_dir_all(&backup_path).await {
                Ok(()) => {
                    tracing::info!("Removed old backup: {}", backup.backup_id);
                    removed_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to remove old backup {}: {}", backup.backup_id, e);
                }
            }
        }
    }

    if removed_count > 0 {
        tracing::info!("Cleaned up {} old backups", removed_count);
    }

    Ok(())
}

/// Get the size of a backup directory
pub async fn get_backup_size(backup_dir: &Path, backup_id: &str) -> Result<u64> {
    let backup_path = backup_dir.join(backup_id);
    calculate_directory_size(&backup_path).await
}

/// Calculate the total size of a directory recursively
fn calculate_directory_size(
    dir_path: &Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64>> + Send + '_>> {
    Box::pin(async move {
        let mut total_size = 0u64;

        let mut entries = fs::read_dir(dir_path)
            .await
            .context("Failed to read directory for size calculation")?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .context("Failed to read directory entry")?
        {
            let entry_path = entry.path();
            let metadata = entry
                .metadata()
                .await
                .context("Failed to get entry metadata")?;

            if metadata.is_file() {
                total_size += metadata.len();
            } else if metadata.is_dir() {
                total_size += calculate_directory_size(&entry_path)
                    .await
                    .context("Failed to calculate subdirectory size")?;
            }
        }

        Ok(total_size)
    })
}

/// Verify the integrity of a backup
pub async fn verify_backup_integrity(
    backup_dir: &Path,
    backup_id: &str,
) -> Result<BackupVerificationResult> {
    let backup_path = backup_dir.join(backup_id);
    let metadata_path = backup_path.join("metadata.json");

    let mut result = BackupVerificationResult {
        backup_id: backup_id.to_string(),
        valid: true,
        missing_files: Vec::new(),
        corrupted_files: Vec::new(),
        extra_files: Vec::new(),
        total_files: 0,
    };

    // Load metadata
    let metadata = load_backup_metadata(&metadata_path)
        .await
        .context("Failed to load backup metadata")?;

    result.total_files = metadata.files.len();

    // Check each file in the metadata
    for file_path in &metadata.files {
        let backup_file_path = backup_path.join(file_path);

        if !backup_file_path.exists() {
            result.missing_files.push(file_path.clone());
            result.valid = false;
        } else {
            // Could add checksum verification here if we stored checksums
            // For now, just check that the file exists and is readable
            match fs::metadata(&backup_file_path).await {
                Ok(_) => {
                    // File exists and is readable
                }
                Err(_) => {
                    result.corrupted_files.push(file_path.clone());
                    result.valid = false;
                }
            }
        }
    }

    // Check for extra files (files in backup that aren't in metadata)
    if let Ok(mut entries) = fs::read_dir(&backup_path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let entry_path = entry.path();

            // Skip metadata file
            if entry_path.file_name() == Some(std::ffi::OsStr::new("metadata.json")) {
                continue;
            }

            if entry_path.is_file() {
                let relative_path = entry_path
                    .strip_prefix(&backup_path)
                    .unwrap_or(&entry_path)
                    .to_path_buf();

                if !metadata.files.contains(&relative_path) {
                    result.extra_files.push(relative_path);
                }
            }
        }
    }

    Ok(result)
}

/// Result of backup verification
#[derive(Debug)]
pub struct BackupVerificationResult {
    pub backup_id: String,
    pub valid: bool,
    pub missing_files: Vec<PathBuf>,
    pub corrupted_files: Vec<PathBuf>,
    pub extra_files: Vec<PathBuf>,
    pub total_files: usize,
}

/// Rollback manager for handling multiple backups
pub struct RollbackManager {
    backup_dir: PathBuf,
    install_dir: PathBuf,
}

impl RollbackManager {
    /// Create a new rollback manager
    pub fn new(backup_dir: PathBuf, install_dir: PathBuf) -> Self {
        Self {
            backup_dir,
            install_dir,
        }
    }

    /// Get information about all available backups
    pub async fn get_backup_info(&self) -> Result<Vec<BackupInfo>> {
        let backups = list_backups(&self.backup_dir)
            .await
            .context("Failed to list backups")?;

        let mut backup_info = Vec::new();

        for backup in backups {
            let size = get_backup_size(&self.backup_dir, &backup.backup_id)
                .await
                .unwrap_or(0);

            let verification = verify_backup_integrity(&self.backup_dir, &backup.backup_id)
                .await
                .unwrap_or_else(|_| BackupVerificationResult {
                    backup_id: backup.backup_id.clone(),
                    valid: false,
                    missing_files: Vec::new(),
                    corrupted_files: Vec::new(),
                    extra_files: Vec::new(),
                    total_files: 0,
                });

            backup_info.push(BackupInfo {
                metadata: backup,
                size_bytes: size,
                valid: verification.valid,
            });
        }

        Ok(backup_info)
    }

    /// Perform rollback to a specific backup
    pub async fn rollback_to(&self, backup_id: &str) -> Result<()> {
        perform_rollback(&self.backup_dir, &self.install_dir, backup_id).await
    }

    /// Clean up old backups
    pub async fn cleanup_old(&self, keep_days: u32) -> Result<()> {
        cleanup_old_backups(&self.backup_dir, keep_days).await
    }

    /// Get the most recent backup
    pub async fn get_latest_backup(&self) -> Result<Option<BackupMetadata>> {
        let backups = list_backups(&self.backup_dir).await?;
        Ok(backups.into_iter().next()) // Already sorted by creation time
    }
}

/// Information about a backup including metadata and status
#[derive(Debug)]
pub struct BackupInfo {
    pub metadata: BackupMetadata,
    pub size_bytes: u64,
    pub valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_backup_metadata_serialization() -> Result<()> {
        let metadata = BackupMetadata {
            backup_id: "test_backup".to_string(),
            created_at: chrono::Utc::now(),
            original_version: semver::Version::new(1, 0, 0),
            target_version: semver::Version::new(1, 1, 0),
            files: vec![PathBuf::from("test.txt")],
        };

        let json = serde_json::to_string(&metadata)?;
        let deserialized: BackupMetadata = serde_json::from_str(&json)?;

        assert_eq!(metadata.backup_id, deserialized.backup_id);
        assert_eq!(metadata.original_version, deserialized.original_version);
        assert_eq!(metadata.target_version, deserialized.target_version);
        assert_eq!(metadata.files, deserialized.files);
        Ok(())
    }

    #[tokio::test]
    async fn test_directory_size_calculation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        fs::write(&test_file, b"Hello, world!").await?;

        let size = calculate_directory_size(temp_dir.path()).await?;
        assert_eq!(size, 13); // Length of "Hello, world!"
        Ok(())
    }
}
