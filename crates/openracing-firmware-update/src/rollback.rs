//! Rollback functionality for failed updates
//!
//! Provides backup management and rollback operations for firmware updates.

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

/// Information about a backup including metadata and status
#[derive(Debug)]
pub struct BackupInfo {
    /// Backup metadata
    pub metadata: BackupMetadata,
    /// Size of the backup in bytes
    pub size_bytes: u64,
    /// Whether the backup is valid
    pub valid: bool,
}

/// Result of backup verification
#[derive(Debug)]
pub struct BackupVerificationResult {
    /// Backup identifier
    pub backup_id: String,
    /// Whether the backup is valid
    pub valid: bool,
    /// Files that are missing from the backup
    pub missing_files: Vec<PathBuf>,
    /// Files that are corrupted
    pub corrupted_files: Vec<PathBuf>,
    /// Extra files not in metadata
    pub extra_files: Vec<PathBuf>,
    /// Total number of files expected
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
        Ok(backups.into_iter().next())
    }

    /// Create a backup before update
    pub async fn create_backup(
        &self,
        backup_id: &str,
        original_version: semver::Version,
        target_version: semver::Version,
        files: &[PathBuf],
    ) -> Result<()> {
        let backup_path = self.backup_dir.join(backup_id);

        fs::create_dir_all(&backup_path)
            .await
            .context("Failed to create backup directory")?;

        for file_path in files {
            let source_path = self.install_dir.join(file_path);
            let backup_file_path = backup_path.join(file_path);

            if source_path.exists() {
                if let Some(parent) = backup_file_path.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .context("Failed to create backup subdirectory")?;
                }

                fs::copy(&source_path, &backup_file_path)
                    .await
                    .context("Failed to backup file")?;
            }
        }

        let metadata = BackupMetadata {
            backup_id: backup_id.to_string(),
            created_at: chrono::Utc::now(),
            original_version,
            target_version,
            files: files.to_vec(),
        };

        let metadata_path = backup_path.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .context("Failed to serialize backup metadata")?;

        fs::write(&metadata_path, metadata_json)
            .await
            .context("Failed to write backup metadata")?;

        Ok(())
    }
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

    for file_path in &metadata.files {
        let backup_file_path = backup_path.join(file_path);
        let target_file_path = install_dir.join(file_path);

        if backup_file_path.exists() {
            if let Some(parent) = target_file_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .context("Failed to create parent directory during rollback")?;
            }

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
        } else if target_file_path.exists() {
            fs::remove_file(&target_file_path).await.with_context(|| {
                format!(
                    "Failed to remove file during rollback: {}",
                    target_file_path.display()
                )
            })?;

            tracing::debug!("Removed file: {}", file_path.display());
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

    let metadata = load_backup_metadata(&metadata_path)
        .await
        .context("Failed to load backup metadata")?;

    result.total_files = metadata.files.len();

    for file_path in &metadata.files {
        let backup_file_path = backup_path.join(file_path);

        if !backup_file_path.exists() {
            result.missing_files.push(file_path.clone());
            result.valid = false;
        } else {
            match fs::metadata(&backup_file_path).await {
                Ok(_) => {}
                Err(_) => {
                    result.corrupted_files.push(file_path.clone());
                    result.valid = false;
                }
            }
        }
    }

    if let Ok(mut entries) = fs::read_dir(&backup_path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let entry_path = entry.path();

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_backup_metadata_serialization() -> Result<()> {
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
    async fn test_rollback_manager() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let backup_dir = temp_dir.path().join("backups");
        let install_dir = temp_dir.path().join("install");

        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        let files = vec![PathBuf::from("test.txt")];
        fs::write(install_dir.join("test.txt"), b"original content").await?;

        manager
            .create_backup(
                "backup_001",
                semver::Version::new(1, 0, 0),
                semver::Version::new(1, 1, 0),
                &files,
            )
            .await?;

        let backups = manager.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert!(backups[0].valid);

        Ok(())
    }
}
