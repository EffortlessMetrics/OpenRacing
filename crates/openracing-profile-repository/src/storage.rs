//! File-based storage operations for profiles

use anyhow::Context;
use std::path::{Path, PathBuf};
use tokio::fs as async_fs;
use tracing::debug;

/// File storage configuration
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Base directory for file storage
    pub base_dir: PathBuf,
    /// File extension for profile files
    pub extension: String,
    /// Enable atomic writes (write to temp, then rename)
    pub atomic_writes: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from("profiles"),
            extension: "json".to_string(),
            atomic_writes: true,
        }
    }
}

impl StorageConfig {
    /// Create a new storage configuration with the specified base directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            ..Default::default()
        }
    }
}

/// File-based storage for profiles with atomic write support
#[derive(Debug)]
pub struct FileStorage {
    config: StorageConfig,
}

impl FileStorage {
    /// Create a new file storage instance
    ///
    /// # Error Recovery
    ///
    /// Creates the base directory if it doesn't exist.
    /// Returns an error if directory creation fails.
    pub async fn new(base_dir: &Path) -> anyhow::Result<Self> {
        async_fs::create_dir_all(base_dir)
            .await
            .with_context(|| format!("Failed to create storage directory: {:?}", base_dir))?;

        Ok(Self {
            config: StorageConfig::new(base_dir),
        })
    }

    /// Create with custom configuration
    pub fn with_config(config: StorageConfig) -> Self {
        Self { config }
    }

    /// Write content to a file atomically
    ///
    /// # Error Recovery
    ///
    /// Uses atomic write pattern:
    /// 1. Write to temporary file
    /// 2. Rename temp file to target
    /// 3. Original file is preserved if write fails
    pub async fn write_atomic(&self, path: &Path, content: &str) -> anyhow::Result<()> {
        debug!(path = ?path, "Writing file atomically");

        let temp_path = path.with_extension("tmp");

        async_fs::write(&temp_path, content)
            .await
            .with_context(|| format!("Failed to write temp file: {:?}", temp_path))?;

        async_fs::rename(&temp_path, path)
            .await
            .with_context(|| format!("Failed to rename temp file to target: {:?}", path))?;

        debug!(path = ?path, "File written successfully");
        Ok(())
    }

    /// Read file content as string
    pub async fn read_to_string(&self, path: &Path) -> anyhow::Result<String> {
        debug!(path = ?path, "Reading file");

        let content = async_fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read file: {:?}", path))?;

        Ok(content)
    }

    /// Check if a file exists
    pub fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    /// Delete a file
    pub async fn delete(&self, path: &Path) -> anyhow::Result<()> {
        if path.exists() {
            async_fs::remove_file(path)
                .await
                .with_context(|| format!("Failed to delete file: {:?}", path))?;
        }
        Ok(())
    }

    /// List all profile files in the storage directory
    pub async fn list_profile_files(&self) -> anyhow::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let extension = &self.config.extension;

        let mut entries = async_fs::read_dir(&self.config.base_dir)
            .await
            .with_context(|| format!("Failed to read directory: {:?}", self.config.base_dir))?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some(extension) {
                files.push(path);
            }
        }

        Ok(files)
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.config.base_dir
    }

    /// Create a backup of a file
    pub async fn create_backup(&self, source: &Path) -> anyhow::Result<PathBuf> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("profile");
        let backup_name = format!("{}_{}.json.bak", stem, timestamp);
        let backup_path = self.config.base_dir.join("backups").join(backup_name);

        let backup_dir = backup_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("backup path has no parent: {:?}", backup_path))?;
        async_fs::create_dir_all(backup_dir)
            .await
            .with_context(|| format!("Failed to create backup directory: {:?}", backup_dir))?;

        async_fs::copy(source, &backup_path)
            .await
            .with_context(|| format!("Failed to create backup: {:?}", backup_path))?;

        debug!(source = ?source, backup = ?backup_path, "Backup created");
        Ok(backup_path)
    }
}

/// Represents a profile file on disk
#[derive(Debug, Clone)]
pub struct ProfileFile {
    /// Path to the profile file
    pub path: PathBuf,
    /// Profile ID derived from filename
    pub id: String,
    /// Last modified timestamp
    pub modified: Option<std::time::SystemTime>,
}

impl ProfileFile {
    /// Create a new profile file reference
    pub fn new(path: PathBuf, id: String) -> Self {
        Self {
            path,
            id,
            modified: None,
        }
    }

    /// Create from a path, extracting ID from filename
    pub fn from_path(path: PathBuf) -> Option<Self> {
        let id = path.file_stem().and_then(|s| s.to_str())?.to_string();

        let modified = path.metadata().ok().and_then(|m| m.modified().ok());

        Some(Self { path, id, modified })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_storage() -> (FileStorage, TempDir) {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let storage = FileStorage::new(temp_dir.path())
            .await
            .expect("storage should be created");
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_storage_creation() {
        let (_storage, _temp_dir) = create_test_storage().await;
    }

    #[tokio::test]
    async fn test_atomic_write() {
        let (storage, temp_dir) = create_test_storage().await;
        let file_path = temp_dir.path().join("test.json");

        storage
            .write_atomic(&file_path, r#"{"test": "data"}"#)
            .await
            .expect("write should succeed");

        assert!(file_path.exists());

        let content = storage
            .read_to_string(&file_path)
            .await
            .expect("read should succeed");

        assert_eq!(content, r#"{"test": "data"}"#);
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let (storage, temp_dir) = create_test_storage().await;
        let file_path = temp_dir.path().join("nonexistent.json");

        let result = storage.read_to_string(&file_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_file() {
        let (storage, temp_dir) = create_test_storage().await;
        let file_path = temp_dir.path().join("delete_me.json");

        storage
            .write_atomic(&file_path, r#"{"test": "data"}"#)
            .await
            .expect("write should succeed");

        assert!(file_path.exists());

        storage
            .delete(&file_path)
            .await
            .expect("delete should succeed");

        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_file() {
        let (storage, temp_dir) = create_test_storage().await;
        let file_path = temp_dir.path().join("nonexistent.json");

        let result = storage.delete(&file_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_profile_files() {
        let (storage, temp_dir) = create_test_storage().await;

        storage
            .write_atomic(&temp_dir.path().join("profile1.json"), "{}")
            .await
            .expect("write should succeed");
        storage
            .write_atomic(&temp_dir.path().join("profile2.json"), "{}")
            .await
            .expect("write should succeed");
        storage
            .write_atomic(&temp_dir.path().join("not_a_profile.txt"), "text")
            .await
            .expect("write should succeed");

        let files = storage
            .list_profile_files()
            .await
            .expect("list should succeed");

        assert_eq!(files.len(), 2);
    }

    #[tokio::test]
    async fn test_create_backup() {
        let (storage, temp_dir) = create_test_storage().await;
        let file_path = temp_dir.path().join("profile.json");

        storage
            .write_atomic(&file_path, r#"{"test": "backup"}"#)
            .await
            .expect("write should succeed");

        let backup_path = storage
            .create_backup(&file_path)
            .await
            .expect("backup should succeed");

        assert!(backup_path.exists());
        assert!(backup_path.to_str().expect("path").contains("backups"));
    }

    #[test]
    fn test_profile_file_from_path() {
        let path = PathBuf::from("/profiles/test_profile.json");
        let profile_file = ProfileFile::from_path(path).expect("should parse");

        assert_eq!(profile_file.id, "test_profile");
    }
}
