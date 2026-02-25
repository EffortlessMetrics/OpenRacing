//! Convenience re-exports for common firmware update types

pub use crate::bundle::{
    BundleError, BundleHeader, BundleMetadata, CompressionType, FirmwareBundle, ReleaseChannel,
};
pub use crate::delta::{
    apply_delta_patch, compress_data, compute_file_hash, create_delta_patch, decompress_data,
};
pub use crate::error::FirmwareUpdateError;
pub use crate::hardware_version::{HardwareVersion, HardwareVersionError};
pub use crate::health::{
    HealthCheckResult, HealthCheckRunner, HealthCheckSummary, run_health_check,
};
pub use crate::manager::{
    CachedFirmware, FfbBlocker, FirmwareCache, FirmwareDevice, FirmwareImage,
    FirmwareUpdateManager, StagedRolloutConfig, UpdatePhase, UpdateProgress, UpdateResult,
    UpdateState,
};
pub use crate::partition::{Partition, PartitionHealth, PartitionInfo};
pub use crate::rollback::{BackupInfo, BackupMetadata, BackupVerificationResult, RollbackManager};
pub use crate::staged_rollout::{
    DeviceRegistry, RolloutMetrics, RolloutPlan, RolloutProgress, RolloutStage, RolloutStatus,
    StageStatus, StagedRolloutManager,
};
