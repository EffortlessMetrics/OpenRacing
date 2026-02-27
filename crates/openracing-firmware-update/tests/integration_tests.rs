//! Integration tests for firmware update lifecycle

use anyhow::Result;
use openracing_firmware_update::prelude::*;
use std::sync::Arc;
use std::time::Duration;
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
                size_bytes: 1024 * 1024,
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
            device_model: "test_wheel_v1".to_string(),
            hardware_version: "1.0".to_string(),
            partitions: Arc::new(Mutex::new(partitions)),
            active_partition: Arc::new(Mutex::new(Partition::A)),
            firmware_data: Arc::new(Mutex::new(std::collections::HashMap::new())),
            should_fail_health_check: Arc::new(Mutex::new(false)),
        }
    }

    #[allow(dead_code)]
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

        let mut firmware_data = self.firmware_data.lock().await;
        firmware_data.remove(&partition);

        Ok(())
    }

    async fn write_firmware_chunk(
        &self,
        partition: Partition,
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        let delay = 1;
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }

        let mut firmware_data = self.firmware_data.lock().await;
        let partition_data = firmware_data.entry(partition).or_insert_with(Vec::new);

        let required_size = offset as usize + data.len();
        if partition_data.len() < required_size {
            partition_data.resize(required_size, 0);
        }

        partition_data[offset as usize..offset as usize + data.len()].copy_from_slice(data);

        Ok(())
    }

    async fn validate_partition(&self, partition: Partition, expected_hash: &str) -> Result<()> {
        let firmware_data = self.firmware_data.lock().await;
        if let Some(data) = firmware_data.get(&partition) {
            let actual_hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(data);
                hex::encode(hasher.finalize())
            };
            let data_len = data.len();

            if actual_hash == expected_hash {
                drop(firmware_data);
                let mut partitions = self.partitions.lock().await;
                if let Some(p) = partitions.iter_mut().find(|p| p.partition == partition) {
                    p.size_bytes = data_len as u64;
                    p.hash = Some(actual_hash);
                    p.health = PartitionHealth::Healthy;
                }
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
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        let attempt_count = 0;
        let attempts_to_fail = if *self.should_fail_health_check.lock().await {
            u32::MAX
        } else {
            0
        };

        if attempt_count < attempts_to_fail {
            Err(anyhow::anyhow!("Mock health check failure"))
        } else {
            Ok(())
        }
    }

    async fn get_hardware_version(&self) -> Result<String> {
        Ok(self.hardware_version.clone())
    }
}

fn create_test_firmware(version: &str, device_model: &str) -> FirmwareImage {
    let data = format!("test firmware data for {} v{}", device_model, version).into_bytes();
    let size_bytes = data.len() as u64;
    let hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        hex::encode(hasher.finalize())
    };

    FirmwareImage {
        device_model: device_model.to_string(),
        version: version.parse().expect("Invalid version"),
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: None,
        data,
        hash,
        size_bytes,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some(format!("Test firmware {}", version)),
        signature: None,
    }
}

fn create_test_manager() -> FirmwareUpdateManager {
    let rollout_config = StagedRolloutConfig::default();
    FirmwareUpdateManager::new(rollout_config)
}

#[tokio::test]
async fn test_successful_firmware_update() {
    let device = Box::new(MockFirmwareDevice::new("test_device_001".to_string()));
    let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
    let manager = create_test_manager();

    let result = manager
        .update_device_firmware(device, &firmware)
        .await
        .expect("Update failed");

    assert!(result.success, "Update should succeed");
    assert_eq!(result.device_id, "test_device_001");
    assert_eq!(result.new_version, Some(firmware.version));
    assert_eq!(result.updated_partition, Some(Partition::B));
    assert!(!result.rollback_performed);
    assert!(result.error.is_none());
}

#[tokio::test]
async fn test_firmware_update_progress_reporting() {
    let device = Box::new(MockFirmwareDevice::new("test_device_008".to_string()));
    let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
    let manager = create_test_manager();

    let mut progress_rx = manager.subscribe_progress();

    let manager_clone = manager;
    let device_clone = device;
    let firmware_clone = firmware.clone();

    let mut update_task = tokio::spawn(async move {
        manager_clone
            .update_device_firmware(device_clone, &firmware_clone)
            .await
    });

    let mut progress_updates = Vec::new();
    let mut update_completed = false;

    while !update_completed {
        tokio::select! {
            progress = progress_rx.recv() => {
                if let Ok(progress) = progress {
                    progress_updates.push(progress);
                }
            }
            result = &mut update_task => {
                update_completed = true;
                let result = result.expect("Task panicked").expect("Update failed");
                assert!(result.success, "Update should succeed");
            }
        }
    }

    assert!(
        !progress_updates.is_empty(),
        "Should receive progress updates"
    );

    let phases: Vec<_> = progress_updates.iter().map(|p| &p.phase).collect();
    assert!(
        phases
            .iter()
            .any(|p| matches!(p, UpdatePhase::Initializing))
    );
    assert!(
        phases
            .iter()
            .any(|p| matches!(p, UpdatePhase::Transferring))
    );
}

#[tokio::test]
async fn test_firmware_compatibility_check() {
    let device = Box::new(MockFirmwareDevice::new("test_device_007".to_string()));
    let mut firmware = create_test_firmware("2.0.0", "test_wheel_v1");
    firmware.min_hardware_version = Some("2.0".to_string());
    let manager = create_test_manager();

    let result = manager
        .update_device_firmware(device, &firmware)
        .await
        .expect("Update should complete");

    assert!(!result.success, "Update should fail due to compatibility");
    assert!(result.error.is_some());
}

#[tokio::test]
async fn test_partition_operations() {
    assert_eq!(Partition::A.other(), Partition::B);
    assert_eq!(Partition::B.other(), Partition::A);
}

#[tokio::test]
async fn test_firmware_cache_basic() -> Result<()> {
    let temp_dir = tempfile::TempDir::new()?;
    let cache = FirmwareCache::new(temp_dir.path().to_path_buf(), 0).await?;

    let firmware = FirmwareImage {
        device_model: "test_wheel".to_string(),
        version: semver::Version::new(1, 0, 0),
        min_hardware_version: None,
        max_hardware_version: None,
        data: vec![1, 2, 3, 4, 5],
        hash: {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update([1, 2, 3, 4, 5]);
            hex::encode(hasher.finalize())
        },
        size_bytes: 5,
        build_timestamp: chrono::Utc::now(),
        release_notes: None,
        signature: None,
    };

    cache.add(&firmware).await?;

    assert!(
        cache
            .contains(&firmware.device_model, &firmware.version)
            .await
    );

    let cached = cache.get(&firmware.device_model, &firmware.version).await?;
    assert!(cached.is_some());

    let cached_fw = cached.expect("Expected cached firmware");
    assert_eq!(cached_fw.device_model, firmware.device_model);
    assert_eq!(cached_fw.version, firmware.version);
    assert_eq!(cached_fw.data, firmware.data);

    Ok(())
}

#[tokio::test]
async fn test_ffb_blocker_basic() -> Result<()> {
    let blocker = FfbBlocker::new();

    assert!(!blocker.is_ffb_blocked());

    blocker.begin_update("test-device").await?;
    assert!(blocker.is_ffb_blocked());

    blocker.end_update().await;
    assert!(!blocker.is_ffb_blocked());

    Ok(())
}

#[tokio::test]
async fn test_ffb_blocker_mutual_exclusion() -> Result<()> {
    let blocker = FfbBlocker::new();

    blocker.begin_update("device1").await?;

    let result = blocker.begin_update("device2").await;
    assert!(result.is_err());

    blocker.end_update().await;

    let result = blocker.begin_update("device2").await;
    assert!(result.is_ok());

    blocker.end_update().await;
    Ok(())
}

#[tokio::test]
async fn test_bundle_roundtrip() -> Result<()> {
    let image = create_test_firmware("1.2.3", "test-wheel");
    let metadata = BundleMetadata {
        title: Some("Test Bundle".to_string()),
        changelog: Some("Initial release".to_string()),
        ..Default::default()
    };

    let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
    let serialized = bundle.serialize()?;
    let parsed = FirmwareBundle::parse(&serialized)?;

    assert_eq!(parsed.header.device_model, "test-wheel");
    assert_eq!(
        parsed.header.firmware_version,
        semver::Version::new(1, 2, 3)
    );

    let extracted = parsed.extract_image()?;
    assert_eq!(extracted.data, image.data);

    Ok(())
}
