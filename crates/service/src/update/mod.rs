//! Delta update system for Racing Wheel Suite
//! 
//! Provides secure, atomic updates with rollback capability

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod delta;
pub mod rollback;
pub mod health;
pub mod firmware;
pub mod staged_rollout;

#[cfg(test)]
pub mod firmware_tests;

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Update verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Update application failed: {0}")]
    ApplicationFailed(String),
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    
    #[error("Rollback failed: {0}")]
    RollbackFailed(String),
    
    #[error("Invalid update package: {0}")]
    InvalidPackage(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Update package metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePackage {
    /// Package format version
    pub version: String,
    
    /// Target application version
    pub target_version: semver::Version,
    
    /// Minimum compatible version for delta updates
    pub min_version: Option<semver::Version>,
    
    /// Update type (full or delta)
    pub update_type: UpdateType,
    
    /// List of files to be updated
    pub files: Vec<UpdateFile>,
    
    /// Pre-update health checks to perform
    pub pre_checks: Vec<HealthCheck>,
    
    /// Post-update health checks to perform
    pub post_checks: Vec<HealthCheck>,
    
    /// Rollback information
    pub rollback_info: RollbackInfo,
    
    /// Package signature metadata
    pub signature: Option<crate::crypto::SignatureMetadata>,
}

/// Type of update package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateType {
    /// Full replacement of all files
    Full,
    
    /// Delta update with binary diffs
    Delta { from_version: semver::Version },
}

/// Information about a file in the update package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFile {
    /// Relative path from installation root
    pub path: PathBuf,
    
    /// File operation to perform
    pub operation: FileOperation,
    
    /// Expected SHA256 hash after operation
    pub expected_hash: String,
    
    /// File size after operation
    pub expected_size: u64,
    
    /// Whether this file is critical for operation
    pub critical: bool,
}

/// File operation to perform during update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileOperation {
    /// Replace file with new content
    Replace { 
        /// Compressed file data
        data: Vec<u8>,
    },
    
    /// Apply binary delta patch
    Delta { 
        /// Binary delta data
        patch: Vec<u8>,
    },
    
    /// Delete file
    Delete,
    
    /// Create directory
    CreateDir,
}

/// Health check to perform during update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Check identifier
    pub id: String,
    
    /// Human-readable description
    pub description: String,
    
    /// Check type
    pub check_type: HealthCheckType,
    
    /// Timeout for the check
    pub timeout_seconds: u32,
    
    /// Whether failure should abort the update
    pub critical: bool,
}

/// Type of health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckType {
    /// Check if service starts successfully
    ServiceStart,
    
    /// Check if service responds to ping
    ServicePing,
    
    /// Check if device enumeration works
    DeviceEnumeration,
    
    /// Run custom command and check exit code
    Command { 
        command: String, 
        args: Vec<String>,
        expected_exit_code: i32,
    },
    
    /// Check if file exists and has expected properties
    FileCheck {
        path: PathBuf,
        expected_hash: Option<String>,
        expected_size: Option<u64>,
    },
}

/// Rollback information for the update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackInfo {
    /// Whether rollback is supported
    pub supported: bool,
    
    /// Backup location for original files
    pub backup_path: Option<PathBuf>,
    
    /// Maximum time to keep backup (seconds)
    pub backup_retention_seconds: u64,
    
    /// Files that were modified and can be rolled back
    pub modified_files: Vec<PathBuf>,
}

/// Update manager for handling the update process
pub struct UpdateManager {
    /// Installation directory
    install_dir: PathBuf,
    
    /// Backup directory for rollbacks
    backup_dir: PathBuf,
    
    /// Current application version
    current_version: semver::Version,
    
    /// Verification service for checking signatures
    verifier: crate::crypto::verification::VerificationService,
}

impl UpdateManager {
    /// Create a new update manager
    pub fn new(
        install_dir: PathBuf,
        backup_dir: PathBuf,
        current_version: semver::Version,
        verifier: crate::crypto::verification::VerificationService,
    ) -> Self {
        Self {
            install_dir,
            backup_dir,
            current_version,
            verifier,
        }
    }
    
    /// Apply an update package
    pub async fn apply_update(&mut self, package_path: &Path) -> Result<UpdateResult> {
        tracing::info!("Starting update from package: {}", package_path.display());
        
        // Step 1: Load and verify the update package
        let package = self.load_and_verify_package(package_path).await
            .context("Failed to load update package")?;
        
        tracing::info!("Loaded update package for version {}", package.target_version);
        
        // Step 2: Check compatibility
        self.check_compatibility(&package)
            .context("Compatibility check failed")?;
        
        // Step 3: Run pre-update health checks
        self.run_health_checks(&package.pre_checks, "pre-update").await
            .context("Pre-update health checks failed")?;
        
        // Step 4: Create backup for rollback
        let backup_id = self.create_backup(&package).await
            .context("Failed to create backup")?;
        
        // Step 5: Apply the update
        let apply_result = self.apply_package_files(&package).await;
        
        match apply_result {
            Ok(()) => {
                tracing::info!("Update files applied successfully");
                
                // Step 6: Run post-update health checks
                match self.run_health_checks(&package.post_checks, "post-update").await {
                    Ok(()) => {
                        tracing::info!("Update completed successfully");
                        self.current_version = package.target_version.clone();
                        
                        // Clean up old backups
                        let _ = self.cleanup_old_backups().await;
                        
                        Ok(UpdateResult {
                            success: true,
                            new_version: package.target_version,
                            backup_id: Some(backup_id),
                            rollback_performed: false,
                            error: None,
                        })
                    }
                    Err(health_error) => {
                        tracing::error!("Post-update health checks failed: {}", health_error);
                        
                        // Attempt rollback
                        match self.rollback_update(&backup_id).await {
                            Ok(()) => {
                                tracing::info!("Rollback completed successfully");
                                Ok(UpdateResult {
                                    success: false,
                                    new_version: self.current_version.clone(),
                                    backup_id: Some(backup_id),
                                    rollback_performed: true,
                                    error: Some(format!("Health check failed, rolled back: {}", health_error)),
                                })
                            }
                            Err(rollback_error) => {
                                tracing::error!("Rollback failed: {}", rollback_error);
                                Err(UpdateError::RollbackFailed(format!(
                                    "Health check failed and rollback failed: {} -> {}",
                                    health_error, rollback_error
                                )).into())
                            }
                        }
                    }
                }
            }
            Err(apply_error) => {
                tracing::error!("Failed to apply update: {}", apply_error);
                
                // Attempt rollback
                match self.rollback_update(&backup_id).await {
                    Ok(()) => {
                        tracing::info!("Rollback completed successfully");
                        Ok(UpdateResult {
                            success: false,
                            new_version: self.current_version.clone(),
                            backup_id: Some(backup_id),
                            rollback_performed: true,
                            error: Some(format!("Update failed, rolled back: {}", apply_error)),
                        })
                    }
                    Err(rollback_error) => {
                        tracing::error!("Rollback failed: {}", rollback_error);
                        Err(UpdateError::RollbackFailed(format!(
                            "Update failed and rollback failed: {} -> {}",
                            apply_error, rollback_error
                        )).into())
                    }
                }
            }
        }
    }
    
    /// Load and verify an update package
    async fn load_and_verify_package(&self, package_path: &Path) -> Result<UpdatePackage> {
        // Read package file
        let package_data = tokio::fs::read(package_path).await
            .context("Failed to read update package")?;
        
        // Verify signature
        let verification_result = self.verifier.verify_update(package_path)
            .context("Update package signature verification failed")?;
        
        if !verification_result.signature_valid {
            return Err(UpdateError::VerificationFailed(
                "Invalid update package signature".to_string()
            ).into());
        }
        
        // Parse package metadata
        // In a real implementation, this would extract from a structured format (ZIP, tar, etc.)
        let package: UpdatePackage = serde_json::from_slice(&package_data)
            .context("Failed to parse update package metadata")?;
        
        Ok(package)
    }
    
    /// Check if the update is compatible with current version
    fn check_compatibility(&self, package: &UpdatePackage) -> Result<()> {
        match &package.update_type {
            UpdateType::Full => {
                // Full updates are always compatible
                Ok(())
            }
            UpdateType::Delta { from_version } => {
                if *from_version != self.current_version {
                    return Err(UpdateError::InvalidPackage(format!(
                        "Delta update requires version {}, but current version is {}",
                        from_version, self.current_version
                    )).into());
                }
                Ok(())
            }
        }
    }
    
    /// Run health checks
    async fn run_health_checks(&self, checks: &[HealthCheck], phase: &str) -> Result<()> {
        tracing::info!("Running {} health checks ({} checks)", phase, checks.len());
        
        for check in checks {
            tracing::debug!("Running health check: {}", check.description);
            
            let result = health::run_health_check(check).await;
            
            match result {
                Ok(()) => {
                    tracing::debug!("Health check passed: {}", check.id);
                }
                Err(e) => {
                    tracing::error!("Health check failed: {} - {}", check.id, e);
                    
                    if check.critical {
                        return Err(UpdateError::HealthCheckFailed(format!(
                            "Critical health check '{}' failed: {}",
                            check.id, e
                        )).into());
                    } else {
                        tracing::warn!("Non-critical health check failed, continuing: {}", e);
                    }
                }
            }
        }
        
        tracing::info!("All {} health checks passed", phase);
        Ok(())
    }
    
    /// Create backup for rollback
    async fn create_backup(&self, package: &UpdatePackage) -> Result<String> {
        let backup_id = format!("backup_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
        let backup_path = self.backup_dir.join(&backup_id);
        
        tokio::fs::create_dir_all(&backup_path).await
            .context("Failed to create backup directory")?;
        
        // Backup files that will be modified
        for file in &package.files {
            if matches!(file.operation, FileOperation::Replace { .. } | FileOperation::Delta { .. }) {
                let source_path = self.install_dir.join(&file.path);
                let backup_file_path = backup_path.join(&file.path);
                
                if source_path.exists() {
                    // Create parent directories
                    if let Some(parent) = backup_file_path.parent() {
                        tokio::fs::create_dir_all(parent).await
                            .context("Failed to create backup subdirectory")?;
                    }
                    
                    // Copy file to backup
                    tokio::fs::copy(&source_path, &backup_file_path).await
                        .context("Failed to backup file")?;
                    
                    tracing::debug!("Backed up: {} -> {}", source_path.display(), backup_file_path.display());
                }
            }
        }
        
        // Save backup metadata
        let backup_metadata = rollback::BackupMetadata {
            backup_id: backup_id.clone(),
            created_at: chrono::Utc::now(),
            original_version: self.current_version.clone(),
            target_version: package.target_version.clone(),
            files: package.files.iter().map(|f| f.path.clone()).collect(),
        };
        
        let metadata_path = backup_path.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&backup_metadata)
            .context("Failed to serialize backup metadata")?;
        
        tokio::fs::write(&metadata_path, metadata_json).await
            .context("Failed to write backup metadata")?;
        
        tracing::info!("Created backup: {}", backup_id);
        Ok(backup_id)
    }
    
    /// Apply package files
    async fn apply_package_files(&self, package: &UpdatePackage) -> Result<()> {
        tracing::info!("Applying {} file operations", package.files.len());
        
        for file in &package.files {
            self.apply_file_operation(file).await
                .with_context(|| format!("Failed to apply operation for file: {}", file.path.display()))?;
        }
        
        Ok(())
    }
    
    /// Apply a single file operation
    async fn apply_file_operation(&self, file: &UpdateFile) -> Result<()> {
        let target_path = self.install_dir.join(&file.path);
        
        match &file.operation {
            FileOperation::Replace { data } => {
                // Create parent directories
                if let Some(parent) = target_path.parent() {
                    tokio::fs::create_dir_all(parent).await
                        .context("Failed to create parent directory")?;
                }
                
                // Decompress and write file
                let decompressed_data = delta::decompress_data(data)
                    .context("Failed to decompress file data")?;
                
                tokio::fs::write(&target_path, &decompressed_data).await
                    .context("Failed to write file")?;
                
                // Verify file hash
                let actual_hash = delta::compute_file_hash(&target_path).await
                    .context("Failed to compute file hash")?;
                
                if actual_hash != file.expected_hash {
                    return Err(UpdateError::ApplicationFailed(format!(
                        "File hash mismatch for {}: expected {}, got {}",
                        file.path.display(), file.expected_hash, actual_hash
                    )).into());
                }
                
                tracing::debug!("Replaced file: {}", target_path.display());
            }
            
            FileOperation::Delta { patch } => {
                // Apply binary delta patch
                delta::apply_delta_patch(&target_path, patch).await
                    .context("Failed to apply delta patch")?;
                
                // Verify file hash
                let actual_hash = delta::compute_file_hash(&target_path).await
                    .context("Failed to compute file hash")?;
                
                if actual_hash != file.expected_hash {
                    return Err(UpdateError::ApplicationFailed(format!(
                        "File hash mismatch after delta for {}: expected {}, got {}",
                        file.path.display(), file.expected_hash, actual_hash
                    )).into());
                }
                
                tracing::debug!("Applied delta patch: {}", target_path.display());
            }
            
            FileOperation::Delete => {
                if target_path.exists() {
                    tokio::fs::remove_file(&target_path).await
                        .context("Failed to delete file")?;
                    
                    tracing::debug!("Deleted file: {}", target_path.display());
                }
            }
            
            FileOperation::CreateDir => {
                tokio::fs::create_dir_all(&target_path).await
                    .context("Failed to create directory")?;
                
                tracing::debug!("Created directory: {}", target_path.display());
            }
        }
        
        Ok(())
    }
    
    /// Rollback an update
    async fn rollback_update(&self, backup_id: &str) -> Result<()> {
        tracing::info!("Rolling back update using backup: {}", backup_id);
        
        rollback::perform_rollback(&self.backup_dir, &self.install_dir, backup_id).await
            .context("Rollback operation failed")?;
        
        tracing::info!("Rollback completed successfully");
        Ok(())
    }
    
    /// Clean up old backups
    async fn cleanup_old_backups(&self) -> Result<()> {
        rollback::cleanup_old_backups(&self.backup_dir, 7).await // Keep 7 days of backups
            .context("Failed to clean up old backups")?;
        
        Ok(())
    }
}

/// Result of an update operation
#[derive(Debug)]
pub struct UpdateResult {
    /// Whether the update was successful
    pub success: bool,
    
    /// Version after update (may be original if rolled back)
    pub new_version: semver::Version,
    
    /// Backup ID created for this update
    pub backup_id: Option<String>,
    
    /// Whether a rollback was performed
    pub rollback_performed: bool,
    
    /// Error message if update failed
    pub error: Option<String>,
}