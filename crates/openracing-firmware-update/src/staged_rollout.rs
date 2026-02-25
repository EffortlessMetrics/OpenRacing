//! Staged rollout system for firmware updates
//!
//! Provides controlled deployment with automatic rollback on error thresholds.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};
use tracing::info;

use crate::manager::{
    FirmwareDevice, FirmwareImage, FirmwareUpdateManager, StagedRolloutConfig, UpdateResult,
};

/// A stage in the rollout process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutStage {
    /// Stage number (1, 2, 3, etc.)
    pub stage_number: u32,

    /// Maximum number of devices to update in this stage
    pub max_devices: u32,

    /// Devices included in this stage
    pub device_ids: Vec<String>,

    /// When this stage started
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,

    /// When this stage completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Stage status
    pub status: StageStatus,

    /// Results from devices in this stage
    pub results: Vec<UpdateResult>,
}

/// Status of a rollout stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StageStatus {
    /// Stage is waiting to start
    Pending,

    /// Stage is currently running
    Running,

    /// Stage completed successfully
    Completed,

    /// Stage failed and rollout was paused
    Failed {
        /// Reason for failure
        reason: String,
    },

    /// Stage was cancelled
    Cancelled,
}

/// A complete rollout plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutPlan {
    /// Unique identifier for this rollout
    pub rollout_id: String,

    /// Firmware being deployed
    pub firmware_version: semver::Version,

    /// Target device model
    pub device_model: String,

    /// All devices eligible for this rollout
    pub target_devices: Vec<String>,

    /// Rollout configuration
    pub config: StagedRolloutConfig,

    /// Stages in the rollout plan
    pub stages: Vec<RolloutStage>,

    /// When rollout was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// When rollout started
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,

    /// When rollout completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Overall rollout status
    pub status: RolloutStatus,
}

/// Overall status of a rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RolloutStatus {
    /// Rollout plan created but not started
    Created,

    /// Rollout is in progress
    InProgress,

    /// Rollout completed successfully
    Completed,

    /// Rollout was paused due to errors
    Paused {
        /// Reason for pause
        reason: String,
    },

    /// Rollout was cancelled
    Cancelled,

    /// Rollout failed and automatic rollback was triggered
    RolledBack {
        /// Reason for rollback
        reason: String,
    },
}

/// Metrics for a rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutMetrics {
    /// Total devices in rollout
    pub total_devices: u32,

    /// Devices successfully updated
    pub successful_updates: u32,

    /// Devices that failed to update
    pub failed_updates: u32,

    /// Devices not yet attempted
    pub pending_updates: u32,

    /// Current success rate (0.0 - 1.0)
    pub success_rate: f64,

    /// Current error rate (0.0 - 1.0)
    pub error_rate: f64,

    /// Average update duration
    #[serde(with = "duration_serde")]
    pub avg_update_duration: Duration,

    /// Time since rollout started
    #[serde(with = "duration_serde")]
    pub elapsed_time: Duration,
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

/// Progress information for staged rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutProgress {
    /// Rollout identifier
    pub rollout_id: String,

    /// Current stage being executed
    pub current_stage: u32,

    /// Total number of stages
    pub total_stages: u32,

    /// Overall progress percentage
    pub overall_progress_percent: u8,

    /// Current stage progress percentage
    pub stage_progress_percent: u8,

    /// Current metrics
    pub metrics: RolloutMetrics,

    /// Status message
    pub status_message: String,

    /// Any warnings or issues
    pub warnings: Vec<String>,
}

/// Trait for device registry to get device handles
#[async_trait::async_trait]
pub trait DeviceRegistry: Send + Sync {
    /// Get a device handle by ID
    async fn get_device(&self, device_id: &str) -> Result<Box<dyn FirmwareDevice>>;

    /// List all available devices of a specific model
    async fn list_devices(&self, device_model: &str) -> Result<Vec<String>>;

    /// Check if a device is online and available
    async fn is_device_available(&self, device_id: &str) -> bool;
}

/// Staged rollout manager
pub struct StagedRolloutManager {
    #[allow(dead_code)]
    firmware_manager: FirmwareUpdateManager,
    active_rollouts: RwLock<HashMap<String, RolloutPlan>>,
    #[allow(dead_code)]
    device_registry: Box<dyn DeviceRegistry>,
    progress_tx: broadcast::Sender<RolloutProgress>,
}

impl StagedRolloutManager {
    /// Create a new staged rollout manager
    pub fn new(
        firmware_manager: FirmwareUpdateManager,
        device_registry: Box<dyn DeviceRegistry>,
    ) -> Self {
        let (progress_tx, _) = broadcast::channel(1000);

        Self {
            firmware_manager,
            active_rollouts: RwLock::new(HashMap::new()),
            device_registry,
            progress_tx,
        }
    }

    /// Create a rollout plan for firmware deployment
    pub async fn create_rollout_plan(
        &self,
        firmware: &FirmwareImage,
        target_devices: Vec<String>,
        config: StagedRolloutConfig,
    ) -> Result<RolloutPlan> {
        let rollout_id = format!("rollout_{}", uuid::Uuid::new_v4());

        info!(
            "Creating rollout plan {} for {} devices",
            rollout_id,
            target_devices.len()
        );

        let stages = self.create_stages(&target_devices, &config)?;

        let plan = RolloutPlan {
            rollout_id: rollout_id.clone(),
            firmware_version: firmware.version.clone(),
            device_model: firmware.device_model.clone(),
            target_devices,
            config,
            stages,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            status: RolloutStatus::Created,
        };

        {
            let mut active_rollouts = self.active_rollouts.write().await;
            active_rollouts.insert(rollout_id.clone(), plan.clone());
        }

        info!(
            "Created rollout plan {} with {} stages",
            rollout_id,
            plan.stages.len()
        );
        Ok(plan)
    }

    fn create_stages(
        &self,
        devices: &[String],
        config: &StagedRolloutConfig,
    ) -> Result<Vec<RolloutStage>> {
        if !config.enabled {
            return Ok(vec![RolloutStage {
                stage_number: 1,
                max_devices: devices.len() as u32,
                device_ids: devices.to_vec(),
                started_at: None,
                completed_at: None,
                status: StageStatus::Pending,
                results: Vec::new(),
            }]);
        }

        let mut stages = Vec::new();
        let mut remaining_devices = devices.to_vec();
        let mut stage_number = 1u32;

        if !remaining_devices.is_empty() {
            let stage1_count = config
                .stage1_max_devices
                .min(remaining_devices.len() as u32) as usize;
            let stage1_devices: Vec<_> = remaining_devices.drain(..stage1_count).collect();

            stages.push(RolloutStage {
                stage_number,
                max_devices: stage1_count as u32,
                device_ids: stage1_devices,
                started_at: None,
                completed_at: None,
                status: StageStatus::Pending,
                results: Vec::new(),
            });

            stage_number += 1;
        }

        let mut stage_size = config.stage1_max_devices * 2;

        while !remaining_devices.is_empty() {
            let current_stage_size = stage_size.min(remaining_devices.len() as u32) as usize;
            let stage_devices: Vec<_> = remaining_devices.drain(..current_stage_size).collect();

            stages.push(RolloutStage {
                stage_number,
                max_devices: current_stage_size as u32,
                device_ids: stage_devices,
                started_at: None,
                completed_at: None,
                status: StageStatus::Pending,
                results: Vec::new(),
            });

            stage_number += 1;
            stage_size *= 2;
        }

        Ok(stages)
    }

    #[allow(dead_code)]
    fn calculate_metrics(&self, plan: &RolloutPlan) -> RolloutMetrics {
        let mut total_devices = 0u32;
        let mut successful_updates = 0u32;
        let mut failed_updates = 0u32;
        let mut total_duration = Duration::from_secs(0);
        let mut update_count = 0u32;

        for stage in &plan.stages {
            total_devices += stage.device_ids.len() as u32;

            for result in &stage.results {
                if result.success {
                    successful_updates += 1;
                } else {
                    failed_updates += 1;
                }
                total_duration += result.duration;
                update_count += 1;
            }
        }

        let pending_updates = total_devices - successful_updates - failed_updates;
        let attempted_updates = successful_updates + failed_updates;

        let success_rate = if attempted_updates > 0 {
            successful_updates as f64 / attempted_updates as f64
        } else {
            0.0
        };

        let error_rate = if attempted_updates > 0 {
            failed_updates as f64 / attempted_updates as f64
        } else {
            0.0
        };

        let avg_update_duration = if update_count > 0 {
            total_duration / update_count
        } else {
            Duration::from_secs(0)
        };

        let elapsed_time = plan
            .started_at
            .map(|start| chrono::Utc::now().signed_duration_since(start))
            .and_then(|d| d.to_std().ok())
            .unwrap_or_else(|| Duration::from_secs(0));

        RolloutMetrics {
            total_devices,
            successful_updates,
            failed_updates,
            pending_updates,
            success_rate,
            error_rate,
            avg_update_duration,
            elapsed_time,
        }
    }

    /// Subscribe to rollout progress updates
    pub fn subscribe_progress(&self) -> broadcast::Receiver<RolloutProgress> {
        self.progress_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staged_rollout_config_default() {
        let config = StagedRolloutConfig::default();
        assert!(config.enabled);
        assert_eq!(config.stage1_max_devices, 10);
    }

    #[test]
    fn test_create_stages_single_stage() -> Result<()> {
        let config = StagedRolloutConfig {
            enabled: false,
            ..Default::default()
        };

        let manager = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let registry = MockDeviceRegistry;
        let rollout = StagedRolloutManager::new(manager, Box::new(registry));

        let devices: Vec<String> = (0..100).map(|i| format!("device_{}", i)).collect();
        let stages = rollout.create_stages(&devices, &config)?;

        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].device_ids.len(), 100);
        Ok(())
    }

    #[test]
    fn test_create_stages_multiple_stages() -> Result<()> {
        let config = StagedRolloutConfig {
            enabled: true,
            stage1_max_devices: 10,
            ..Default::default()
        };

        let manager = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let registry = MockDeviceRegistry;
        let rollout = StagedRolloutManager::new(manager, Box::new(registry));

        let devices: Vec<String> = (0..100).map(|i| format!("device_{}", i)).collect();
        let stages = rollout.create_stages(&devices, &config)?;

        assert!(stages.len() > 1, "Should have multiple stages");
        assert_eq!(
            stages[0].max_devices, 10,
            "First stage should have 10 devices"
        );

        let total_devices: usize = stages.iter().map(|s| s.device_ids.len()).sum();
        assert_eq!(
            total_devices, 100,
            "All devices should be assigned to stages"
        );

        Ok(())
    }

    struct MockDeviceRegistry;

    #[async_trait::async_trait]
    impl DeviceRegistry for MockDeviceRegistry {
        async fn get_device(&self, _device_id: &str) -> Result<Box<dyn FirmwareDevice>> {
            Err(anyhow::anyhow!("Mock device registry not implemented"))
        }

        async fn list_devices(&self, _device_model: &str) -> Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn is_device_available(&self, _device_id: &str) -> bool {
            false
        }
    }
}
