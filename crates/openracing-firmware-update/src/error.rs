//! Error types for firmware update operations

use thiserror::Error;

/// Errors that can occur during firmware update operations
#[derive(Error, Debug)]
pub enum FirmwareUpdateError {
    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Firmware verification failed
    #[error("Firmware verification failed: {0}")]
    VerificationFailed(String),

    /// Update transfer failed
    #[error("Update transfer failed: {0}")]
    TransferFailed(String),

    /// Health check failed
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    /// Rollback failed
    #[error("Rollback failed: {0}")]
    RollbackFailed(String),

    /// Invalid firmware image
    #[error("Invalid firmware image: {0}")]
    InvalidFirmware(String),

    /// Device communication error
    #[error("Device communication error: {0}")]
    DeviceError(String),

    /// Timeout during operation
    #[error("Timeout during operation: {0}")]
    Timeout(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// FFB operation blocked due to firmware update in progress
    #[error("FFB operation blocked: firmware update in progress")]
    FfbBlocked,

    /// Update already in progress
    #[error("Update already in progress for device: {0}")]
    UpdateInProgress(String),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Bundle error
    #[error("Bundle error: {0}")]
    BundleError(String),

    /// Partition error
    #[error("Partition error: {0}")]
    PartitionError(String),

    /// Compatibility error
    #[error("Compatibility error: {0}")]
    CompatibilityError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Invalid state for operation
    #[error("Invalid state for operation: {0}")]
    InvalidState(String),

    /// Operation cancelled
    #[error("Operation cancelled: {0}")]
    Cancelled(String),

    /// Rollout error
    #[error("Rollout error: {0}")]
    RolloutError(String),
}

impl From<serde_json::Error> for FirmwareUpdateError {
    fn from(e: serde_json::Error) -> Self {
        FirmwareUpdateError::SerializationError(e.to_string())
    }
}
