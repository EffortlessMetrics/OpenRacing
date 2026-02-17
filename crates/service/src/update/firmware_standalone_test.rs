//! Standalone tests for firmware update system
//! These tests don't depend on other modules to avoid compilation issues

use super::firmware::*;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Minimal mock device for testing
struct SimpleMockDevice {
    device_id: String,
    partitions: Arc<Mutex<Vec<PartitionInfo>>>,
    active_partition: Arc<Mutex<Partition>>,
    firmware_data: Arc<Mutex<HashMap<Partition, Vec<u8>>>>,
}

impl SimpleMockDevice {
    fn new(device_id: String) -> Self {
        let partitions = vec![
            PartitionInfo {
                partition: Partition::A,
                active: true,
                bootable: true,
                version: Some(semver::Version::new(1, 0, 0)),
                size_bytes: 1024,
                hash: Some("old_hash".to_string()),
                updated_at: Some(chrono::Utc::now()),
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
            partitions: Arc::new(Mutex::new(partitions)),
            active_partition: Arc::new(Mutex::new(Partition::A)),
            firmware_data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl FirmwareDevice for SimpleMockDevice {
    fn device_id(&self) -> &str {
        &self.device_id
    }
    
    fn device_model(&self) -> &str {
        "test_device"
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
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(data);
            let actual_hash = hex::encode(hasher.finalize());
            
            if actual_hash == expected_hash {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Hash mismatch"))
            }
        } else {
            Err(anyhow::anyhow!("No firmware data"))
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
        Ok(())
    }
    
    async fn reboot(&self) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
    
    async fn health_check(&self) -> Result<()> {
        Ok(())
    }
    
    async fn get_hardware_version(&self) -> Result<String> {
        Ok("1.0".to_string())
    }
}

fn create_test_firmware() -> FirmwareImage {
    let data = b"test firmware data".to_vec();
    let hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        hex::encode(hasher.finalize())
    };
    
    FirmwareImage {
        device_model: "test_device".to_string(),
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

fn create_minimal_manager() -> Result<FirmwareUpdateManager> {
    // Create a minimal verification service that doesn't require signatures
    let temp_dir = tempfile::TempDir::new()?;
    let config = crate::crypto::VerificationConfig {
        trust_store_path: temp_dir.path().join("trust_store.json"),
        require_firmware_signatures: false,
        ..Default::default()
    };

    let verifier = crate::crypto::verification::VerificationService::new(config)?;
    let rollout_config = StagedRolloutConfig::default();

    Ok(FirmwareUpdateManager::new(verifier, rollout_config))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_partition_enum() -> Result<()> {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_firmware_image_creation() -> Result<()> {
        let firmware = create_test_firmware();
        
        assert_eq!(firmware.device_model, "test_device");
        assert_eq!(firmware.version, semver::Version::new(2, 0, 0));
        assert!(!firmware.data.is_empty());
        assert!(!firmware.hash.is_empty());
        assert_eq!(firmware.size_bytes, 18);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_mock_device_basic_operations() -> Result<()> {
        let device = SimpleMockDevice::new("test_device".to_string());
        
        // Test basic getters
        assert_eq!(device.device_id(), "test_device");
        assert_eq!(device.device_model(), "test_device");
        
        // Test partition info
        let partitions = device.get_partition_info().await?;
        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[0].partition, Partition::A);
        assert!(partitions[0].active);
        
        // Test active partition
        let active = device.get_active_partition().await?;
        assert_eq!(active, Partition::A);
        
        // Test hardware version
        let hw_version = device.get_hardware_version().await?;
        assert_eq!(hw_version, "1.0");
        Ok(())
    }
    
    #[tokio::test]
    async fn test_mock_device_firmware_operations() -> Result<()> {
        let device = SimpleMockDevice::new("test_device".to_string());
        let test_data = b"hello world";
        
        // Prepare partition
        device.prepare_partition(Partition::B).await?;
        
        // Write firmware chunk
        device
            .write_firmware_chunk(Partition::B, 0, test_data)
            .await?;
        
        // Calculate expected hash
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(test_data);
        let expected_hash = hex::encode(hasher.finalize());
        
        // Validate partition
        device.validate_partition(Partition::B, &expected_hash).await?;
        
        // Set bootable
        device.set_bootable(Partition::B, true).await?;
        
        // Activate partition
        device.activate_partition(Partition::B).await?;
        
        // Verify active partition changed
        let active = device.get_active_partition().await?;
        assert_eq!(active, Partition::B);
        
        // Test reboot
        device.reboot().await?;
        
        // Test health check
        device.health_check().await?;
        Ok(())
    }
    
    #[tokio::test]
    async fn test_firmware_update_manager_creation() -> Result<()> {
        let _manager = create_minimal_manager()?;
        // If we get here without panicking, the manager was created successfully
        Ok(())
    }
    
    #[tokio::test]
    async fn test_successful_firmware_update() -> Result<()> {
        let device = Box::new(SimpleMockDevice::new("test_device".to_string()));
        let firmware = create_test_firmware();
        let manager = create_minimal_manager()?;
        
        let result = manager.update_device_firmware(device, &firmware).await?;
        
        assert!(result.success, "Update should succeed");
        assert_eq!(result.device_id, "test_device");
        assert_eq!(result.new_version, Some(firmware.version));
        assert_eq!(result.updated_partition, Some(Partition::B));
        assert!(!result.rollback_performed);
        assert!(result.error.is_none());
        Ok(())
    }
    
    #[tokio::test]
    async fn test_firmware_serialization() -> Result<()> {
        let firmware = create_test_firmware();
        
        // Test that firmware can be serialized and deserialized
        let json = serde_json::to_string(&firmware)?;
        let deserialized: FirmwareImage = serde_json::from_str(&json)?;
        
        assert_eq!(firmware.device_model, deserialized.device_model);
        assert_eq!(firmware.version, deserialized.version);
        assert_eq!(firmware.hash, deserialized.hash);
        assert_eq!(firmware.data, deserialized.data);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_partition_info_serialization() -> Result<()> {
        let partition_info = PartitionInfo {
            partition: Partition::A,
            active: true,
            bootable: true,
            version: Some(semver::Version::new(1, 0, 0)),
            size_bytes: 1024,
            hash: Some("test_hash".to_string()),
            updated_at: Some(chrono::Utc::now()),
            health: PartitionHealth::Healthy,
        };
        
        let json = serde_json::to_string(&partition_info)?;
        let deserialized: PartitionInfo = serde_json::from_str(&json)?;
        
        assert_eq!(partition_info.partition, deserialized.partition);
        assert_eq!(partition_info.active, deserialized.active);
        assert_eq!(partition_info.version, deserialized.version);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_update_progress_serialization() -> Result<()> {
        let progress = UpdateProgress {
            phase: UpdatePhase::Transferring,
            progress_percent: 50,
            bytes_transferred: 1024,
            total_bytes: 2048,
            transfer_rate_bps: 1000,
            eta_seconds: Some(1),
            status_message: "Transferring firmware".to_string(),
            warnings: vec!["Test warning".to_string()],
        };
        
        let json = serde_json::to_string(&progress)?;
        let deserialized: UpdateProgress = serde_json::from_str(&json)?;
        
        assert_eq!(progress.progress_percent, deserialized.progress_percent);
        assert_eq!(progress.bytes_transferred, deserialized.bytes_transferred);
        assert_eq!(progress.status_message, deserialized.status_message);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_staged_rollout_config() -> Result<()> {
        let config = StagedRolloutConfig::default();
        
        assert!(config.enabled);
        assert_eq!(config.stage1_max_devices, 10);
        assert_eq!(config.min_success_rate, 0.95);
        assert_eq!(config.max_error_rate, 0.05);
        
        // Test serialization
        let json = serde_json::to_string(&config)?;
        let deserialized: StagedRolloutConfig = serde_json::from_str(&json)?;
        
        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.stage1_max_devices, deserialized.stage1_max_devices);
        Ok(())
    }
}
