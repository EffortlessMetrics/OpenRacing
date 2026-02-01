//! Firmware update system with A/B partition support
//!
//! Provides atomic firmware updates with automatic rollback capability.
//!
//! # Features
//! - A/B partition support with atomic swaps
//! - Automatic rollback on failure
//! - FFB blocking during updates (safety requirement)
//! - Firmware image caching for offline updates
//! - Ed25519 signature verification
//!
//! # Safety
//! During firmware updates, FFB operations are blocked to prevent
//! unsafe states. The system maintains mutual exclusion between
//! firmware updates and FFB operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{error, info, warn};

#[derive(Error, Debug)]
pub enum FirmwareUpdateError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Firmware verification failed: {0}")]
    VerificationFailed(String),

    #[error("Update transfer failed: {0}")]
    TransferFailed(String),

    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    #[error("Rollback failed: {0}")]
    RollbackFailed(String),

    #[error("Invalid firmware image: {0}")]
    InvalidFirmware(String),

    #[error("Device communication error: {0}")]
    DeviceError(String),

    #[error("Timeout during operation: {0}")]
    Timeout(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FFB operation blocked: firmware update in progress")]
    FfbBlocked,

    #[error("Update already in progress for device: {0}")]
    UpdateInProgress(String),

    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Firmware update state machine states
///
/// This enum represents the high-level state of the firmware update system,
/// as specified in the design document (Requirement 17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UpdateState {
    /// No update in progress, system is idle
    #[default]
    Idle,

    /// Downloading firmware image from remote source
    Downloading {
        /// Download progress as percentage (0-100)
        progress: u8,
    },

    /// Verifying firmware image signature and integrity
    Verifying,

    /// Writing firmware to device (flashing)
    Flashing {
        /// Flash progress as percentage (0-100)
        progress: u8,
    },

    /// Rebooting device to apply new firmware
    Rebooting,

    /// Update completed successfully
    Complete,

    /// Update failed with error information
    Failed {
        /// Error description
        error: String,
        /// Whether the error is recoverable (rollback possible)
        recoverable: bool,
    },
}

impl UpdateState {
    /// Check if an update is currently in progress
    pub fn is_in_progress(&self) -> bool {
        !matches!(
            self,
            UpdateState::Idle | UpdateState::Complete | UpdateState::Failed { .. }
        )
    }

    /// Check if FFB operations should be blocked
    ///
    /// FFB is blocked during all active update phases to ensure safety.
    /// **Validates: Requirements 17.4**
    pub fn should_block_ffb(&self) -> bool {
        self.is_in_progress()
    }
}

/// Firmware partition identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Partition {
    /// Partition A
    A,
    /// Partition B
    B,
}

impl Partition {
    /// Get the other partition
    pub fn other(self) -> Self {
        match self {
            Partition::A => Partition::B,
            Partition::B => Partition::A,
        }
    }
}

/// Firmware partition status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// Partition identifier
    pub partition: Partition,

    /// Whether this partition is currently active (booted)
    pub active: bool,

    /// Whether this partition is bootable
    pub bootable: bool,

    /// Firmware version in this partition
    pub version: Option<semver::Version>,

    /// Size of firmware in bytes
    pub size_bytes: u64,

    /// SHA256 hash of firmware
    pub hash: Option<String>,

    /// Last update timestamp
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Health status of this partition
    pub health: PartitionHealth,
}

/// Health status of a firmware partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartitionHealth {
    /// Partition is healthy and functional
    Healthy,

    /// Partition has minor issues but is functional
    Degraded { reason: String },

    /// Partition is corrupted or non-functional
    Corrupted { reason: String },

    /// Partition status is unknown
    Unknown,
}

/// Firmware image metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareImage {
    /// Target device model/type
    pub device_model: String,

    /// Firmware version
    pub version: semver::Version,

    /// Minimum compatible hardware version
    pub min_hardware_version: Option<String>,

    /// Maximum compatible hardware version
    pub max_hardware_version: Option<String>,

    /// Firmware binary data
    pub data: Vec<u8>,

    /// SHA256 hash of firmware data
    pub hash: String,

    /// Size in bytes
    pub size_bytes: u64,

    /// Build timestamp
    pub build_timestamp: chrono::DateTime<chrono::Utc>,

    /// Release notes or changelog
    pub release_notes: Option<String>,

    /// Signature metadata for verification
    pub signature: Option<crate::crypto::SignatureMetadata>,
}

/// Progress information for firmware update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgress {
    /// Current phase of the update
    pub phase: UpdatePhase,

    /// Progress percentage (0-100)
    pub progress_percent: u8,

    /// Bytes transferred so far
    pub bytes_transferred: u64,

    /// Total bytes to transfer
    pub total_bytes: u64,

    /// Transfer rate in bytes per second
    pub transfer_rate_bps: u64,

    /// Estimated time remaining
    pub eta_seconds: Option<u64>,

    /// Current status message
    pub status_message: String,

    /// Any warnings or non-fatal errors
    pub warnings: Vec<String>,
}

/// Phases of firmware update process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdatePhase {
    /// Initializing update process
    Initializing,

    /// Verifying firmware image
    Verifying,

    /// Preparing target partition
    Preparing,

    /// Transferring firmware data
    Transferring,

    /// Validating transferred data
    Validating,

    /// Activating new firmware
    Activating,

    /// Running health checks
    HealthCheck,

    /// Update completed successfully
    Completed,

    /// Update failed, rolling back
    RollingBack,

    /// Update failed completely
    Failed,
}

/// Configuration for staged rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedRolloutConfig {
    /// Enable staged rollout
    pub enabled: bool,

    /// Maximum number of devices to update in first stage
    pub stage1_max_devices: u32,

    /// Minimum success rate required to proceed to next stage
    pub min_success_rate: f64,

    /// Time to wait between stages
    pub stage_delay_minutes: u32,

    /// Maximum error rate before automatic rollback
    pub max_error_rate: f64,

    /// Time window for monitoring success rate
    pub monitoring_window_minutes: u32,
}

impl Default for StagedRolloutConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            stage1_max_devices: 10,
            min_success_rate: 0.95,
            stage_delay_minutes: 60,
            max_error_rate: 0.05,
            monitoring_window_minutes: 120,
        }
    }
}

/// Result of firmware update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    /// Device identifier
    pub device_id: String,

    /// Whether update was successful
    pub success: bool,

    /// Version before update
    pub old_version: Option<semver::Version>,

    /// Version after update
    pub new_version: Option<semver::Version>,

    /// Partition that was updated
    pub updated_partition: Option<Partition>,

    /// Whether rollback was performed
    pub rollback_performed: bool,

    /// Duration of update process
    pub duration: Duration,

    /// Error message if update failed
    pub error: Option<String>,

    /// Final partition states
    pub partition_states: Vec<PartitionInfo>,
}

/// Trait for device-specific firmware update operations
#[async_trait::async_trait]
pub trait FirmwareDevice: Send + Sync {
    /// Get device identifier
    fn device_id(&self) -> &str;

    /// Get device model/type
    fn device_model(&self) -> &str;

    /// Get current partition information
    async fn get_partition_info(&self) -> Result<Vec<PartitionInfo>>;

    /// Get currently active partition
    async fn get_active_partition(&self) -> Result<Partition>;

    /// Prepare a partition for firmware update
    async fn prepare_partition(&self, partition: Partition) -> Result<()>;

    /// Write firmware data to partition
    async fn write_firmware_chunk(
        &self,
        partition: Partition,
        offset: u64,
        data: &[u8],
    ) -> Result<()>;

    /// Validate firmware in partition
    async fn validate_partition(&self, partition: Partition, expected_hash: &str) -> Result<()>;

    /// Set partition as bootable
    async fn set_bootable(&self, partition: Partition, bootable: bool) -> Result<()>;

    /// Perform atomic swap to new partition
    async fn activate_partition(&self, partition: Partition) -> Result<()>;

    /// Reboot device to apply firmware change
    async fn reboot(&self) -> Result<()>;

    /// Check if device is responsive after reboot
    async fn health_check(&self) -> Result<()>;

    /// Get hardware version for compatibility checking
    async fn get_hardware_version(&self) -> Result<String>;
}

/// Firmware update manager
pub struct FirmwareUpdateManager {
    /// Verification service for checking signatures
    verifier: crate::crypto::verification::VerificationService,

    /// Configuration for staged rollout
    #[allow(dead_code)]
    rollout_config: StagedRolloutConfig,

    /// Progress broadcast channel
    progress_tx: broadcast::Sender<UpdateProgress>,

    /// Active update tracking
    active_updates:
        std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, UpdateHandle>>>,
}

/// Handle for tracking an active update
struct UpdateHandle {
    #[allow(dead_code)]
    device_id: String,
    cancel_tx: mpsc::Sender<()>,
    #[allow(dead_code)]
    progress_rx: mpsc::Receiver<UpdateProgress>,
}

impl FirmwareUpdateManager {
    /// Create a new firmware update manager
    pub fn new(
        verifier: crate::crypto::verification::VerificationService,
        rollout_config: StagedRolloutConfig,
    ) -> Self {
        let (progress_tx, _) = broadcast::channel(1000);

        Self {
            verifier,
            rollout_config,
            progress_tx,
            active_updates: std::sync::Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Load and verify firmware image from file
    pub async fn load_firmware_image(&self, firmware_path: &Path) -> Result<FirmwareImage> {
        info!("Loading firmware image: {}", firmware_path.display());

        // Verify firmware signature
        let verification_result = self
            .verifier
            .verify_firmware(firmware_path)
            .context("Firmware signature verification failed")?;

        if !verification_result.signature_valid {
            return Err(FirmwareUpdateError::VerificationFailed(
                "Firmware signature is invalid".to_string(),
            )
            .into());
        }

        // Read firmware file
        let firmware_data = tokio::fs::read(firmware_path)
            .await
            .context("Failed to read firmware file")?;

        // Parse firmware metadata (this would be device-specific)
        let firmware_image = self
            .parse_firmware_image(firmware_data, verification_result.metadata.clone())
            .context("Failed to parse firmware image")?;

        info!(
            "Loaded firmware image: {} v{}",
            firmware_image.device_model, firmware_image.version
        );
        Ok(firmware_image)
    }

    /// Update firmware on a single device
    pub async fn update_device_firmware(
        &self,
        device: Box<dyn FirmwareDevice>,
        firmware: &FirmwareImage,
    ) -> Result<UpdateResult> {
        let device_id = device.device_id().to_string();
        let start_time = Instant::now();

        info!("Starting firmware update for device: {}", device_id);

        // Check if update is already in progress
        {
            let active_updates = self.active_updates.lock().await;
            if active_updates.contains_key(&device_id) {
                return Err(anyhow::anyhow!(
                    "Update already in progress for device: {}",
                    device_id
                ));
            }
        }

        // Create progress tracking
        let (progress_tx, progress_rx) = mpsc::channel(100);
        let (cancel_tx, mut cancel_rx) = mpsc::channel(1);

        // Register active update
        {
            let mut active_updates = self.active_updates.lock().await;
            active_updates.insert(
                device_id.clone(),
                UpdateHandle {
                    device_id: device_id.clone(),
                    cancel_tx,
                    progress_rx,
                },
            );
        }

        // Perform the update
        let result = self
            .perform_device_update(device, firmware, progress_tx, &mut cancel_rx)
            .await;

        // Clean up active update tracking
        {
            let mut active_updates = self.active_updates.lock().await;
            active_updates.remove(&device_id);
        }

        // Create result
        let duration = start_time.elapsed();
        match result {
            Ok((old_version, new_version, updated_partition, partition_states)) => {
                info!(
                    "Firmware update completed successfully for device: {}",
                    device_id
                );
                Ok(UpdateResult {
                    device_id,
                    success: true,
                    old_version,
                    new_version: Some(new_version),
                    updated_partition: Some(updated_partition),
                    rollback_performed: false,
                    duration,
                    error: None,
                    partition_states,
                })
            }
            Err(e) => {
                error!("Firmware update failed for device {}: {}", device_id, e);
                Ok(UpdateResult {
                    device_id,
                    success: false,
                    old_version: None,
                    new_version: None,
                    updated_partition: None,
                    rollback_performed: false, // TODO: Track actual rollback
                    duration,
                    error: Some(e.to_string()),
                    partition_states: Vec::new(),
                })
            }
        }
    }

    /// Perform the actual firmware update process
    async fn perform_device_update(
        &self,
        device: Box<dyn FirmwareDevice>,
        firmware: &FirmwareImage,
        progress_tx: mpsc::Sender<UpdateProgress>,
        cancel_rx: &mut mpsc::Receiver<()>,
    ) -> Result<(
        Option<semver::Version>,
        semver::Version,
        Partition,
        Vec<PartitionInfo>,
    )> {
        // Phase 1: Initialize and verify compatibility
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Initializing,
                progress_percent: 0,
                bytes_transferred: 0,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Initializing firmware update".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        // Check device compatibility
        let hardware_version = device
            .get_hardware_version()
            .await
            .context("Failed to get hardware version")?;

        self.check_compatibility(firmware, &hardware_version)
            .context("Firmware compatibility check failed")?;

        // Get current partition info
        let partition_info = device
            .get_partition_info()
            .await
            .context("Failed to get partition information")?;

        let active_partition = device
            .get_active_partition()
            .await
            .context("Failed to get active partition")?;

        let target_partition = active_partition.other();
        let old_version = partition_info
            .iter()
            .find(|p| p.partition == active_partition)
            .and_then(|p| p.version.clone());

        info!(
            "Updating from partition {:?} to {:?}",
            active_partition, target_partition
        );

        // Phase 2: Verify firmware image
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Verifying,
                progress_percent: 5,
                bytes_transferred: 0,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Verifying firmware image".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        // Verify firmware hash
        let computed_hash = self
            .compute_firmware_hash(&firmware.data)
            .context("Failed to compute firmware hash")?;

        if computed_hash != firmware.hash {
            return Err(
                FirmwareUpdateError::InvalidFirmware("Firmware hash mismatch".to_string()).into(),
            );
        }

        // Phase 3: Prepare target partition
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Preparing,
                progress_percent: 10,
                bytes_transferred: 0,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Preparing target partition".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        device
            .prepare_partition(target_partition)
            .await
            .context("Failed to prepare target partition")?;

        // Phase 4: Transfer firmware data
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Transferring,
                progress_percent: 15,
                bytes_transferred: 0,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Transferring firmware data".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        let transfer_start = Instant::now();
        let chunk_size = 4096; // 4KB chunks
        let mut bytes_transferred = 0u64;

        for (i, chunk) in firmware.data.chunks(chunk_size).enumerate() {
            // Check for cancellation
            if cancel_rx.try_recv().is_ok() {
                return Err(anyhow::anyhow!("Update cancelled by user"));
            }

            let offset = i * chunk_size;
            device
                .write_firmware_chunk(target_partition, offset as u64, chunk)
                .await
                .with_context(|| format!("Failed to write firmware chunk at offset {}", offset))?;

            bytes_transferred += chunk.len() as u64;

            // Update progress every 64KB or at the end
            if bytes_transferred.is_multiple_of(64 * 1024)
                || bytes_transferred == firmware.size_bytes
            {
                let elapsed = transfer_start.elapsed();
                let transfer_rate = if elapsed.as_secs() > 0 {
                    bytes_transferred / elapsed.as_secs()
                } else {
                    0
                };

                let eta = (firmware.size_bytes - bytes_transferred).checked_div(transfer_rate);

                let progress_percent = 15 + ((bytes_transferred * 60) / firmware.size_bytes) as u8;

                self.send_progress(
                    &progress_tx,
                    UpdateProgress {
                        phase: UpdatePhase::Transferring,
                        progress_percent,
                        bytes_transferred,
                        total_bytes: firmware.size_bytes,
                        transfer_rate_bps: transfer_rate,
                        eta_seconds: eta,
                        status_message: format!(
                            "Transferred {} / {} bytes",
                            bytes_transferred, firmware.size_bytes
                        ),
                        warnings: Vec::new(),
                    },
                )
                .await;
            }
        }

        // Phase 5: Validate transferred data
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Validating,
                progress_percent: 75,
                bytes_transferred: firmware.size_bytes,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Validating transferred firmware".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        device
            .validate_partition(target_partition, &firmware.hash)
            .await
            .context("Firmware validation failed")?;

        // Phase 6: Activate new firmware
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Activating,
                progress_percent: 85,
                bytes_transferred: firmware.size_bytes,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Activating new firmware".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        // Set target partition as bootable
        device
            .set_bootable(target_partition, true)
            .await
            .context("Failed to set target partition as bootable")?;

        // Perform atomic swap
        device
            .activate_partition(target_partition)
            .await
            .context("Failed to activate target partition")?;

        // Reboot device
        device.reboot().await.context("Failed to reboot device")?;

        // Wait for device to come back online
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Phase 7: Health check
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::HealthCheck,
                progress_percent: 95,
                bytes_transferred: firmware.size_bytes,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: "Running health checks".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        // Perform health check with retries
        let mut health_check_attempts = 0;
        const MAX_HEALTH_CHECK_ATTEMPTS: u32 = 5;

        loop {
            match device.health_check().await {
                Ok(()) => break,
                Err(e) => {
                    health_check_attempts += 1;
                    if health_check_attempts >= MAX_HEALTH_CHECK_ATTEMPTS {
                        // Health check failed, attempt rollback
                        warn!(
                            "Health check failed after {} attempts, attempting rollback",
                            MAX_HEALTH_CHECK_ATTEMPTS
                        );

                        if let Err(rollback_error) =
                            self.perform_rollback(&*device, active_partition).await
                        {
                            error!("Rollback failed: {}", rollback_error);
                            return Err(FirmwareUpdateError::RollbackFailed(format!(
                                "Health check failed and rollback failed: {} -> {}",
                                e, rollback_error
                            ))
                            .into());
                        }

                        return Err(FirmwareUpdateError::HealthCheckFailed(format!(
                            "Health check failed after {} attempts, rolled back to previous firmware",
                            MAX_HEALTH_CHECK_ATTEMPTS
                        ))
                        .into());
                    }

                    warn!(
                        "Health check attempt {} failed: {}, retrying...",
                        health_check_attempts, e
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }

        // Phase 8: Complete
        self.send_progress(
            &progress_tx,
            UpdateProgress {
                phase: UpdatePhase::Completed,
                progress_percent: 100,
                bytes_transferred: firmware.size_bytes,
                total_bytes: firmware.size_bytes,
                transfer_rate_bps: 0,
                eta_seconds: Some(0),
                status_message: "Firmware update completed successfully".to_string(),
                warnings: Vec::new(),
            },
        )
        .await;

        // Get final partition states
        let final_partition_info = device
            .get_partition_info()
            .await
            .context("Failed to get final partition information")?;

        Ok((
            old_version,
            firmware.version.clone(),
            target_partition,
            final_partition_info,
        ))
    }

    /// Perform rollback to previous firmware
    async fn perform_rollback(
        &self,
        device: &dyn FirmwareDevice,
        rollback_partition: Partition,
    ) -> Result<()> {
        info!(
            "Performing firmware rollback to partition {:?}",
            rollback_partition
        );

        // Set rollback partition as bootable
        device
            .set_bootable(rollback_partition, true)
            .await
            .context("Failed to set rollback partition as bootable")?;

        // Activate rollback partition
        device
            .activate_partition(rollback_partition)
            .await
            .context("Failed to activate rollback partition")?;

        // Reboot device
        device
            .reboot()
            .await
            .context("Failed to reboot device for rollback")?;

        // Wait for device to come back online
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Verify rollback was successful
        device
            .health_check()
            .await
            .context("Health check failed after rollback")?;

        info!("Firmware rollback completed successfully");
        Ok(())
    }

    /// Check firmware compatibility with device
    fn check_compatibility(&self, firmware: &FirmwareImage, hardware_version: &str) -> Result<()> {
        // Check minimum hardware version
        if let Some(min_version) = &firmware.min_hardware_version
            && hardware_version < min_version.as_str()
        {
            return Err(FirmwareUpdateError::InvalidFirmware(format!(
                "Hardware version {} is below minimum required version {}",
                hardware_version, min_version
            ))
            .into());
        }

        // Check maximum hardware version
        if let Some(max_version) = &firmware.max_hardware_version
            && hardware_version > max_version.as_str()
        {
            return Err(FirmwareUpdateError::InvalidFirmware(format!(
                "Hardware version {} is above maximum supported version {}",
                hardware_version, max_version
            ))
            .into());
        }

        Ok(())
    }

    /// Compute SHA256 hash of firmware data
    fn compute_firmware_hash(&self, data: &[u8]) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Parse firmware image from binary data
    fn parse_firmware_image(
        &self,
        data: Vec<u8>,
        signature: crate::crypto::SignatureMetadata,
    ) -> Result<FirmwareImage> {
        // This is a simplified parser - in reality, this would parse
        // device-specific firmware formats (Intel HEX, binary, etc.)

        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        };

        // Extract version from signature comment or use default
        let version = if let Some(comment) = &signature.comment {
            if let Some(version_str) = comment.strip_prefix("v") {
                semver::Version::parse(version_str)
                    .unwrap_or_else(|_| semver::Version::new(1, 0, 0))
            } else {
                semver::Version::new(1, 0, 0)
            }
        } else {
            semver::Version::new(1, 0, 0)
        };

        Ok(FirmwareImage {
            device_model: "generic".to_string(), // Would be parsed from firmware header
            version,
            min_hardware_version: None,
            max_hardware_version: None,
            size_bytes: data.len() as u64,
            hash,
            data,
            build_timestamp: signature.timestamp,
            release_notes: signature.comment.clone(),
            signature: Some(signature),
        })
    }

    /// Send progress update
    async fn send_progress(
        &self,
        progress_tx: &mpsc::Sender<UpdateProgress>,
        progress: UpdateProgress,
    ) {
        // Send to local progress channel
        let _ = progress_tx.send(progress.clone()).await;

        // Broadcast to global progress channel
        let _ = self.progress_tx.send(progress);
    }

    /// Subscribe to progress updates
    pub fn subscribe_progress(&self) -> broadcast::Receiver<UpdateProgress> {
        self.progress_tx.subscribe()
    }

    /// Cancel an active update
    pub async fn cancel_update(&self, device_id: &str) -> Result<()> {
        let active_updates = self.active_updates.lock().await;
        if let Some(handle) = active_updates.get(device_id) {
            handle
                .cancel_tx
                .send(())
                .await
                .context("Failed to send cancel signal")?;
            info!("Sent cancel signal for device: {}", device_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "No active update found for device: {}",
                device_id
            ))
        }
    }

    /// Get list of devices with active updates
    pub async fn get_active_updates(&self) -> Vec<String> {
        let active_updates = self.active_updates.lock().await;
        active_updates.keys().cloned().collect()
    }

    /// Check if any firmware update is in progress
    pub async fn is_update_in_progress(&self) -> bool {
        let active_updates = self.active_updates.lock().await;
        !active_updates.is_empty()
    }
}

// ============================================================================
// FFB Blocking Mechanism
// ============================================================================

/// FFB blocker for mutual exclusion during firmware updates
///
/// This component ensures that FFB operations are blocked during firmware
/// updates to prevent unsafe states. It provides a thread-safe mechanism
/// for checking and setting the update-in-progress state.
///
/// **Validates: Requirements 17.4**
#[derive(Debug)]
pub struct FfbBlocker {
    /// Atomic flag indicating if an update is in progress
    update_in_progress: AtomicBool,

    /// Device ID currently being updated (if any)
    updating_device: RwLock<Option<String>>,

    /// Update state for detailed status
    update_state: RwLock<UpdateState>,
}

impl Default for FfbBlocker {
    fn default() -> Self {
        Self::new()
    }
}

impl FfbBlocker {
    /// Create a new FFB blocker
    pub fn new() -> Self {
        Self {
            update_in_progress: AtomicBool::new(false),
            updating_device: RwLock::new(None),
            update_state: RwLock::new(UpdateState::Idle),
        }
    }

    /// Check if FFB operations are currently blocked
    ///
    /// Returns `true` if a firmware update is in progress and FFB should be blocked.
    /// This method is designed to be called from the RT path and uses atomic operations.
    ///
    /// **Validates: Requirements 17.4**
    #[inline]
    pub fn is_ffb_blocked(&self) -> bool {
        self.update_in_progress.load(Ordering::Acquire)
    }

    /// Try to perform an FFB operation, returning an error if blocked
    ///
    /// This is a convenience method that checks the block state and returns
    /// an appropriate error if FFB is blocked.
    ///
    /// **Validates: Requirements 17.4**
    pub fn try_ffb_operation(&self) -> Result<(), FirmwareUpdateError> {
        if self.is_ffb_blocked() {
            Err(FirmwareUpdateError::FfbBlocked)
        } else {
            Ok(())
        }
    }

    /// Begin a firmware update, blocking FFB operations
    ///
    /// Returns an error if an update is already in progress.
    pub async fn begin_update(&self, device_id: &str) -> Result<(), FirmwareUpdateError> {
        // Try to set the update flag atomically
        if self
            .update_in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            let current_device = self.updating_device.read().await;
            let device_name = current_device.as_deref().unwrap_or("unknown");
            return Err(FirmwareUpdateError::UpdateInProgress(
                device_name.to_string(),
            ));
        }

        // Set the device ID and state
        {
            let mut device = self.updating_device.write().await;
            *device = Some(device_id.to_string());
        }
        {
            let mut state = self.update_state.write().await;
            *state = UpdateState::Verifying;
        }

        info!("FFB blocked for firmware update on device: {}", device_id);
        Ok(())
    }

    /// Update the current update state
    pub async fn set_state(&self, new_state: UpdateState) {
        let mut state = self.update_state.write().await;
        *state = new_state;
    }

    /// Get the current update state
    pub async fn get_state(&self) -> UpdateState {
        self.update_state.read().await.clone()
    }

    /// End a firmware update, unblocking FFB operations
    pub async fn end_update(&self) {
        // Clear the device ID and state
        {
            let mut device = self.updating_device.write().await;
            let device_id = device.take();
            if let Some(id) = device_id {
                info!("FFB unblocked after firmware update on device: {}", id);
            }
        }
        {
            let mut state = self.update_state.write().await;
            *state = UpdateState::Idle;
        }

        // Clear the update flag
        self.update_in_progress.store(false, Ordering::Release);
    }

    /// Get the device ID currently being updated
    pub async fn get_updating_device(&self) -> Option<String> {
        self.updating_device.read().await.clone()
    }
}

// ============================================================================
// Firmware Cache
// ============================================================================

/// Cached firmware image entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFirmware {
    /// Firmware image metadata (without data for serialization)
    pub device_model: String,

    /// Firmware version
    pub version: semver::Version,

    /// SHA256 hash of firmware data
    pub hash: String,

    /// Size in bytes
    pub size_bytes: u64,

    /// When the firmware was cached
    pub cached_at: chrono::DateTime<chrono::Utc>,

    /// Path to the cached firmware file
    pub cache_path: PathBuf,

    /// Original signature metadata
    pub signature: Option<crate::crypto::SignatureMetadata>,
}

/// Firmware cache for offline updates
///
/// This component caches downloaded firmware images to support offline updates.
/// Cached images are stored on disk and can be retrieved without network access.
///
/// **Validates: Requirements 17.6**
pub struct FirmwareCache {
    /// Cache directory path
    cache_dir: PathBuf,

    /// In-memory index of cached firmware
    index: RwLock<HashMap<String, CachedFirmware>>,

    /// Maximum cache size in bytes (0 = unlimited)
    max_cache_size: u64,
}

impl FirmwareCache {
    /// Create a new firmware cache
    ///
    /// # Arguments
    /// * `cache_dir` - Directory to store cached firmware images
    /// * `max_cache_size` - Maximum cache size in bytes (0 = unlimited)
    pub async fn new(cache_dir: PathBuf, max_cache_size: u64) -> Result<Self> {
        // Create cache directory if it doesn't exist
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .context("Failed to create firmware cache directory")?;

        let cache = Self {
            cache_dir,
            index: RwLock::new(HashMap::new()),
            max_cache_size,
        };

        // Load existing cache index
        cache.load_index().await?;

        Ok(cache)
    }

    /// Generate a cache key for a firmware image
    fn cache_key(device_model: &str, version: &semver::Version) -> String {
        format!("{}_{}", device_model, version)
    }

    /// Generate a filename for a cached firmware image
    fn cache_filename(device_model: &str, version: &semver::Version) -> String {
        format!("{}_{}.fw", device_model, version)
    }

    /// Load the cache index from disk
    async fn load_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");

        if index_path.exists() {
            let index_data = tokio::fs::read_to_string(&index_path)
                .await
                .context("Failed to read cache index")?;

            let loaded_index: HashMap<String, CachedFirmware> =
                serde_json::from_str(&index_data).context("Failed to parse cache index")?;

            // Verify cached files still exist
            let mut valid_entries = HashMap::new();
            for (key, entry) in loaded_index {
                if entry.cache_path.exists() {
                    valid_entries.insert(key, entry);
                } else {
                    warn!(
                        "Cached firmware file missing, removing from index: {}",
                        entry.cache_path.display()
                    );
                }
            }

            let mut index = self.index.write().await;
            *index = valid_entries;
        }

        Ok(())
    }

    /// Save the cache index to disk
    async fn save_index(&self) -> Result<()> {
        let index_path = self.cache_dir.join("index.json");
        let index = self.index.read().await;

        let index_data =
            serde_json::to_string_pretty(&*index).context("Failed to serialize cache index")?;

        tokio::fs::write(&index_path, index_data)
            .await
            .context("Failed to write cache index")?;

        Ok(())
    }

    /// Add a firmware image to the cache
    ///
    /// **Validates: Requirements 17.6**
    pub async fn add(&self, firmware: &FirmwareImage) -> Result<()> {
        let key = Self::cache_key(&firmware.device_model, &firmware.version);
        let filename = Self::cache_filename(&firmware.device_model, &firmware.version);
        let cache_path = self.cache_dir.join(&filename);

        // Check cache size limit
        if self.max_cache_size > 0 {
            let current_size = self.get_cache_size().await;
            if current_size + firmware.size_bytes > self.max_cache_size {
                // Try to evict old entries
                self.evict_oldest(firmware.size_bytes).await?;
            }
        }

        // Write firmware data to cache
        tokio::fs::write(&cache_path, &firmware.data)
            .await
            .context("Failed to write firmware to cache")?;

        // Create cache entry
        let entry = CachedFirmware {
            device_model: firmware.device_model.clone(),
            version: firmware.version.clone(),
            hash: firmware.hash.clone(),
            size_bytes: firmware.size_bytes,
            cached_at: chrono::Utc::now(),
            cache_path,
            signature: firmware.signature.clone(),
        };

        // Add to index
        {
            let mut index = self.index.write().await;
            index.insert(key, entry);
        }

        // Save index
        self.save_index().await?;

        info!(
            "Cached firmware: {} v{}",
            firmware.device_model, firmware.version
        );
        Ok(())
    }

    /// Get a firmware image from the cache
    ///
    /// Returns `None` if the firmware is not cached.
    ///
    /// **Validates: Requirements 17.6**
    pub async fn get(
        &self,
        device_model: &str,
        version: &semver::Version,
    ) -> Result<Option<FirmwareImage>> {
        let key = Self::cache_key(device_model, version);

        let entry = {
            let index = self.index.read().await;
            index.get(&key).cloned()
        };

        match entry {
            Some(cached) => {
                // Read firmware data from cache
                let data = tokio::fs::read(&cached.cache_path)
                    .await
                    .context("Failed to read cached firmware")?;

                // Verify hash
                let actual_hash = {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(&data);
                    hex::encode(hasher.finalize())
                };

                if actual_hash != cached.hash {
                    warn!(
                        "Cached firmware hash mismatch, removing: {} v{}",
                        device_model, version
                    );
                    self.remove(device_model, version).await?;
                    return Ok(None);
                }

                // Reconstruct firmware image
                let firmware = FirmwareImage {
                    device_model: cached.device_model,
                    version: cached.version,
                    min_hardware_version: None,
                    max_hardware_version: None,
                    data,
                    hash: cached.hash,
                    size_bytes: cached.size_bytes,
                    build_timestamp: cached.cached_at,
                    release_notes: None,
                    signature: cached.signature,
                };

                info!(
                    "Retrieved firmware from cache: {} v{}",
                    device_model, version
                );
                Ok(Some(firmware))
            }
            None => Ok(None),
        }
    }

    /// Check if a firmware image is in the cache
    pub async fn contains(&self, device_model: &str, version: &semver::Version) -> bool {
        let key = Self::cache_key(device_model, version);
        let index = self.index.read().await;
        index.contains_key(&key)
    }

    /// Remove a firmware image from the cache
    pub async fn remove(&self, device_model: &str, version: &semver::Version) -> Result<()> {
        let key = Self::cache_key(device_model, version);

        let entry = {
            let mut index = self.index.write().await;
            index.remove(&key)
        };

        if let Some(cached) = entry {
            // Delete the cached file
            if cached.cache_path.exists() {
                tokio::fs::remove_file(&cached.cache_path)
                    .await
                    .context("Failed to remove cached firmware file")?;
            }

            // Save index
            self.save_index().await?;

            info!("Removed firmware from cache: {} v{}", device_model, version);
        }

        Ok(())
    }

    /// Get the total size of cached firmware
    pub async fn get_cache_size(&self) -> u64 {
        let index = self.index.read().await;
        index.values().map(|e| e.size_bytes).sum()
    }

    /// Get the number of cached firmware images
    pub async fn get_cache_count(&self) -> usize {
        let index = self.index.read().await;
        index.len()
    }

    /// List all cached firmware
    pub async fn list(&self) -> Vec<CachedFirmware> {
        let index = self.index.read().await;
        index.values().cloned().collect()
    }

    /// Evict oldest entries to free up space
    async fn evict_oldest(&self, required_space: u64) -> Result<()> {
        let mut entries: Vec<_> = {
            let index = self.index.read().await;
            index.values().cloned().collect()
        };

        // Sort by cached_at (oldest first)
        entries.sort_by_key(|a| a.cached_at);

        let mut freed_space = 0u64;
        for entry in entries {
            if freed_space >= required_space {
                break;
            }

            self.remove(&entry.device_model, &entry.version).await?;
            freed_space += entry.size_bytes;
        }

        Ok(())
    }

    /// Clear all cached firmware
    pub async fn clear(&self) -> Result<()> {
        let entries: Vec<_> = {
            let index = self.index.read().await;
            index
                .values()
                .map(|e| (e.device_model.clone(), e.version.clone()))
                .collect()
        };

        for (device_model, version) in entries {
            self.remove(&device_model, &version).await?;
        }

        info!("Cleared firmware cache");
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    // Tests are in the separate firmware_tests.rs file in the parent module
}

#[cfg(test)]
mod firmware_tests_internal {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock firmware device for testing
    struct MockFirmwareDevice {
        device_id: String,
        device_model: String,
        hardware_version: String,
        partitions: Arc<Mutex<Vec<PartitionInfo>>>,
        active_partition: Arc<Mutex<Partition>>,
        firmware_data: Arc<Mutex<std::collections::HashMap<Partition, Vec<u8>>>>,
        should_fail_health_check: Arc<Mutex<bool>>,
    }

    impl MockFirmwareDevice {
        fn new(device_id: String) -> Self {
            let partitions = vec![
                PartitionInfo {
                    partition: Partition::A,
                    active: true,
                    bootable: true,
                    version: Some(semver::Version::new(1, 0, 0)),
                    size_bytes: 1024 * 1024, // 1MB
                    hash: Some("old_hash".to_string()),
                    updated_at: Some(chrono::Utc::now() - chrono::Duration::days(30)),
                    health: PartitionHealth::Healthy,
                },
                PartitionInfo {
                    partition: Partition::B,
                    active: false,
                    bootable: false,
                    version: None,
                    size_bytes: 0,
                    hash: None,
                    updated_at: None,
                    health: PartitionHealth::Unknown,
                },
            ];

            Self {
                device_id,
                device_model: "test_wheel".to_string(),
                hardware_version: "1.0".to_string(),
                partitions: Arc::new(Mutex::new(partitions)),
                active_partition: Arc::new(Mutex::new(Partition::A)),
                firmware_data: Arc::new(Mutex::new(std::collections::HashMap::new())),
                should_fail_health_check: Arc::new(Mutex::new(false)),
            }
        }

        async fn set_health_check_failure(&self, should_fail: bool) {
            *self.should_fail_health_check.lock().await = should_fail;
        }
    }

    #[async_trait::async_trait]
    impl FirmwareDevice for MockFirmwareDevice {
        fn device_id(&self) -> &str {
            &self.device_id
        }

        fn device_model(&self) -> &str {
            &self.device_model
        }

        async fn get_partition_info(&self) -> Result<Vec<PartitionInfo>> {
            Ok(self.partitions.lock().await.clone())
        }

        async fn get_active_partition(&self) -> Result<Partition> {
            Ok(*self.active_partition.lock().await)
        }

        async fn prepare_partition(&self, partition: Partition) -> Result<()> {
            let mut partitions = self.partitions.lock().await;
            if let Some(p) = partitions.iter_mut().find(|p| p.partition == partition) {
                p.bootable = false;
                p.version = None;
                p.size_bytes = 0;
                p.hash = None;
                p.health = PartitionHealth::Unknown;
            }
            Ok(())
        }

        async fn write_firmware_chunk(
            &self,
            partition: Partition,
            offset: u64,
            data: &[u8],
        ) -> Result<()> {
            let mut firmware_data = self.firmware_data.lock().await;
            let partition_data = firmware_data.entry(partition).or_insert_with(Vec::new);

            // Extend partition data if needed
            let required_size = offset as usize + data.len();
            if partition_data.len() < required_size {
                partition_data.resize(required_size, 0);
            }

            // Write chunk
            partition_data[offset as usize..offset as usize + data.len()].copy_from_slice(data);

            Ok(())
        }

        async fn validate_partition(
            &self,
            partition: Partition,
            expected_hash: &str,
        ) -> Result<()> {
            let firmware_data = self.firmware_data.lock().await;
            if let Some(data) = firmware_data.get(&partition) {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(data);
                let actual_hash = hex::encode(hasher.finalize());

                if actual_hash == expected_hash {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "Hash mismatch: expected {}, got {}",
                        expected_hash,
                        actual_hash
                    ))
                }
            } else {
                Err(anyhow::anyhow!(
                    "No firmware data found for partition {:?}",
                    partition
                ))
            }
        }

        async fn set_bootable(&self, partition: Partition, bootable: bool) -> Result<()> {
            let mut partitions = self.partitions.lock().await;
            if let Some(p) = partitions.iter_mut().find(|p| p.partition == partition) {
                p.bootable = bootable;
            }
            Ok(())
        }

        async fn activate_partition(&self, partition: Partition) -> Result<()> {
            *self.active_partition.lock().await = partition;

            let mut partitions = self.partitions.lock().await;
            for p in partitions.iter_mut() {
                p.active = p.partition == partition;
            }

            Ok(())
        }

        async fn reboot(&self) -> Result<()> {
            // Simulate reboot delay
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }

        async fn health_check(&self) -> Result<()> {
            if *self.should_fail_health_check.lock().await {
                Err(anyhow::anyhow!("Mock health check failure"))
            } else {
                Ok(())
            }
        }

        async fn get_hardware_version(&self) -> Result<String> {
            Ok(self.hardware_version.clone())
        }
    }

    fn create_test_firmware() -> FirmwareImage {
        let data = b"test firmware data".to_vec();
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        };

        FirmwareImage {
            device_model: "test_wheel".to_string(),
            version: semver::Version::new(2, 0, 0),
            min_hardware_version: Some("1.0".to_string()),
            max_hardware_version: None,
            data,
            hash,
            size_bytes: 18,
            build_timestamp: chrono::Utc::now(),
            release_notes: Some("Test firmware".to_string()),
            signature: None,
        }
    }

    #[tokio::test]
    async fn test_successful_firmware_update() -> Result<()> {
        // Create mock device and firmware
        let device = Box::new(MockFirmwareDevice::new("test_device".to_string()));
        let firmware = create_test_firmware();

        // Create update manager with mock verifier
        let temp_dir = tempfile::TempDir::new()?;
        let config = crate::crypto::VerificationConfig {
            trust_store_path: temp_dir.path().join("trust_store.json"),
            require_firmware_signatures: false, // Disable for test
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;
        let rollout_config = StagedRolloutConfig::default();

        let manager = FirmwareUpdateManager::new(verifier, rollout_config);

        // Perform update
        let result = manager.update_device_firmware(device, &firmware).await?;

        // Verify result
        assert!(result.success);
        assert_eq!(result.new_version, Some(firmware.version));
        assert_eq!(result.updated_partition, Some(Partition::B));
        assert!(!result.rollback_performed);
        Ok(())
    }

    #[tokio::test]
    async fn test_firmware_update_with_health_check_failure() -> Result<()> {
        // Create mock device that will fail health check
        let device = MockFirmwareDevice::new("test_device".to_string());
        device.set_health_check_failure(true).await;
        let device = Box::new(device);
        let firmware = create_test_firmware();

        // Create update manager
        let temp_dir = tempfile::TempDir::new()?;
        let config = crate::crypto::VerificationConfig {
            trust_store_path: temp_dir.path().join("trust_store.json"),
            require_firmware_signatures: false,
            ..Default::default()
        };
        let verifier = crate::crypto::verification::VerificationService::new(config)?;
        let rollout_config = StagedRolloutConfig::default();

        let manager = FirmwareUpdateManager::new(verifier, rollout_config);

        // Perform update (should fail and rollback)
        let result = manager.update_device_firmware(device, &firmware).await?;

        // Verify result shows failure
        assert!(!result.success);
        assert!(result.error.is_some());
        let error_msg = result.error.as_ref().expect("Expected error message");
        assert!(error_msg.contains("Health check failed"));
        Ok(())
    }

    #[test]
    fn test_partition_other() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
    }

    #[test]
    fn test_firmware_image_serialization() -> Result<(), serde_json::Error> {
        let firmware = create_test_firmware();
        let json = serde_json::to_string(&firmware)?;
        let deserialized: FirmwareImage = serde_json::from_str(&json)?;

        assert_eq!(firmware.device_model, deserialized.device_model);
        assert_eq!(firmware.version, deserialized.version);
        assert_eq!(firmware.hash, deserialized.hash);
        Ok(())
    }
}
