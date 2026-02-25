//! Firmware update management for OpenRacing
//!
//! This crate provides secure, atomic firmware updates with:
//! - A/B partition support with atomic swaps
//! - Automatic rollback on failure
//! - FFB blocking during updates (safety requirement)
//! - Firmware image caching for offline updates
//! - Ed25519 signature verification
//! - Binary delta patching
//! - Staged rollout for gradual deployments
//!
//! # Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`manager`]: Core firmware update manager
//! - [`bundle`]: Firmware bundle format (.owfb) handling
//! - [`partition`]: A/B partition management
//! - [`delta`]: Binary delta patching
//! - [`rollback`]: Rollback functionality
//! - [`health`]: Health check system
//! - [`staged_rollout`]: Gradual deployment support
//! - [`hardware_version`]: Hardware version parsing and comparison
//! - [`error`]: Error types
//!
//! # Safety
//!
//! During firmware updates, FFB operations are blocked to prevent unsafe states.
//! The system maintains mutual exclusion between firmware updates and FFB operations.
//!
//! # Example
//!
//! ```ignore
//! use openracing_firmware_update::prelude::*;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a firmware update manager
//! let config = FirmwareUpdateConfig::default();
//! let manager = FirmwareUpdateManager::new(config);
//!
//! // Load a firmware bundle
//! let bundle = FirmwareBundle::load("firmware.owfb")?;
//! let image = bundle.extract_image()?;
//!
//! // Update a device
//! let result = manager.update_device(device, &image).await?;
//!
//! if result.success {
//!     println!("Update successful!");
//! } else {
//!     println!("Update failed: {:?}", result.error);
//! }
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![warn(missing_docs, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod bundle;
pub mod delta;
pub mod error;
pub mod hardware_version;
pub mod health;
pub mod manager;
pub mod partition;
pub mod prelude;
pub mod rollback;
pub mod staged_rollout;

pub use bundle::{
    BundleError, BundleHeader, BundleMetadata, CompressionType, FirmwareBundle, ReleaseChannel,
};
pub use delta::{
    apply_delta_patch, apply_simple_patch, compress_data, compute_data_hash, compute_file_hash,
    create_delta_patch, create_simple_patch, decompress_data,
};
pub use error::FirmwareUpdateError;
pub use hardware_version::{HardwareVersion, HardwareVersionError};
pub use health::{HealthCheckResult, HealthCheckRunner, HealthCheckSummary, run_health_check};
pub use manager::{
    CachedFirmware, FfbBlocker, FirmwareCache, FirmwareDevice, FirmwareImage,
    FirmwareUpdateManager, StagedRolloutConfig, UpdatePhase, UpdateProgress, UpdateResult,
    UpdateState,
};
pub use partition::{Partition, PartitionHealth, PartitionInfo};
pub use rollback::{BackupInfo, BackupMetadata, BackupVerificationResult, RollbackManager};
pub use staged_rollout::{
    DeviceRegistry, RolloutMetrics, RolloutPlan, RolloutProgress, RolloutStage, RolloutStatus,
    StageStatus, StagedRolloutManager,
};
