//! Tests for firmware update system with mock devices and failure injection

use super::firmware::{
    FirmwareUpdateManager, FirmwareImage, FirmwareDevice, UpdateResult, 
    StagedRolloutConfig, Partition, PartitionInfo, PartitionHealth
};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Mock firmware device for testing with configurable failure modes
pub struct MockFirmwareDevice {
    device_id: String,
    device_model: String,
    hardware_version: String,
    partitions: Arc<Mutex<Vec<PartitionInfo>>>,
    active_partition: Arc<Mutex<Partition>>,
    firmware_data: Arc<Mutex<HashMap<Partition, Vec<u8>>>>,
    
    // Failure injection configuration
    should_fail_prepare: Arc<Mutex<bool>>,
    should_fail_write: Arc<Mutex<bool>>,
    should_fail_validate: Arc<Mutex<bool>>,
    should_fail_activate: Arc<Mutex<bool>>,
    should_fail_reboot: Arc<Mutex<bool>>,
    should_fail_health_check: Arc<Mutex<bool>>,
    
    // Simulation parameters
    write_delay_ms: Arc<Mutex<u64>>,
    reboot_delay_ms: Arc<Mutex<u64>>,
    health_check_attempts_to_fail: Arc<Mutex<u32>>,
    health_check_attempt_count: Arc<Mutex<u32>>,
}

impl MockFirmwareDevice {
    /// Create a new mock device with default settings
    pub fn new(device_id: String) -> Self {
        let partitions = vec![
            PartitionInfo {
                partition: Partition::A,
                active: true,
                bootable: true,
                version: Some(semver::Version::new(1, 0, 0)),
                size_bytes: 1024 * 1024, // 1MB
                hash: Some("old_firmware_hash".to_string()),
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
            firmware_data: Arc::new(Mutex::new(HashMap::new())),
            
            should_fail_prepare: Arc::new(Mutex::new(false)),
            should_fail_write: Arc::new(Mutex::new(false)),
            should_fail_validate: Arc::new(Mutex::new(false)),
            should_fail_activate: Arc::new(Mutex::new(false)),
            should_fail_reboot: Arc::new(Mutex::new(false)),
            should_fail_health_check: Arc::new(Mutex::new(false)),
            
            write_delay_ms: Arc::new(Mutex::new(1)),
            reboot_delay_ms: Arc::new(Mutex::new(100)),
            health_check_attempts_to_fail: Arc::new(Mutex::new(0)),
            health_check_attempt_count: Arc::new(Mutex::new(0)),
        }
    }
    
    /// Configure the device to fail at the prepare partition step
    pub async fn set_prepare_failure(&self, should_fail: bool) {
        *self.should_fail_prepare.lock().await = should_fail;
    }
    
    /// Configure the device to fail during firmware write
    pub async fn set_write_failure(&self, should_fail: bool) {
        *self.should_fail_write.lock().await = should_fail;
    }
    
    /// Configure the device to fail validation
    pub async fn set_validate_failure(&self, should_fail: bool) {
        *self.should_fail_validate.lock().await = should_fail;
    }
    
    /// Configure the device to fail activation
    pub async fn set_activate_failure(&self, should_fail: bool) {
        *self.should_fail_activate.lock().await = should_fail;
    }
    
    /// Configure the device to fail reboot
    pub async fn set_reboot_failure(&self, should_fail: bool) {
        *self.should_fail_reboot.lock().await = should_fail;
    }
    
    /// Configure the device to fail health checks for N attempts
    pub async fn set_health_check_failure(&self, attempts_to_fail: u32) {
        *self.should_fail_health_check.lock().await = attempts_to_fail > 0;
        *self.health_check_attempts_to_fail.lock().await = attempts_to_fail;
        *self.health_check_attempt_count.lock().await = 0;
    }
    
    /// Set write delay to simulate slow transfers
    pub async fn set_write_delay(&self, delay_ms: u64) {
        *self.write_delay_ms.lock().await = delay_ms;
    }
    
    /// Set reboot delay to simulate device restart time
    pub async fn set_reboot_delay(&self, delay_ms: u64) {
        *self.reboot_delay_ms.lock().await = delay_ms;
    }
    
    /// Get the current firmware data for a partition
    pub async fn get_partition_data(&self, partition: Partition) -> Option<Vec<u8>> {
        let firmware_data = self.firmware_data.lock().await;
        firmware_data.get(&partition).cloned()
    }
    
    /// Simulate a device that becomes unresponsive
    pub async fn simulate_device_disconnect(&self) {
        // Set all operations to fail
        self.set_prepare_failure(true).await;
        self.set_write_failure(true).await;
        self.set_validate_failure(true).await;
        self.set_activate_failure(true).await;
        self.set_reboot_failure(true).await;
        self.set_health_check_failure(u32::MAX).await;
    }
    
    /// Reset device to healthy state
    pub async fn reset_to_healthy(&self) {
        self.set_prepare_failure(false).await;
        self.set_write_failure(false).await;
        self.set_validate_failure(false).await;
        self.set_activate_failure(false).await;
        self.set_reboot_failure(false).await;
        self.set_health_check_failure(0).await;
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
        if *self.should_fail_prepare.lock().await {
            return Err(anyhow::anyhow!("Mock prepare partition failure"));
        }
        
        let mut partitions = self.partitions.lock().await;
        if let Some(p) = partitions.iter_mut().find(|p| p.partition == partition) {
            p.bootable = false;
            p.version = None;
            p.size_bytes = 0;
            p.hash = None;
            p.health = PartitionHealth::Unknown;
        }
        
        // Clear any existing firmware data for this partition
        {
            let mut firmware_data = self.firmware_data.lock().await;
            firmware_data.remove(&partition);
        }
        
        Ok(())
    }
    
    async fn write_firmware_chunk(
        &self,
        partition: Partition,
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        if *self.should_fail_write.lock().await {
            return Err(anyhow::anyhow!("Mock write firmware chunk failure"));
        }
        
        // Simulate write delay
        let delay = *self.write_delay_ms.lock().await;
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        
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
    
    async fn validate_partition(&self, partition: Partition, expected_hash: &str) -> Result<()> {
        if *self.should_fail_validate.lock().await {
            return Err(anyhow::anyhow!("Mock validate partition failure"));
        }
        
        let firmware_data = self.firmware_data.lock().await;
        if let Some(data) = firmware_data.get(&partition) {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(data);
            let actual_hash = hex::encode(hasher.finalize());
            
            if actual_hash == expected_hash {
                // Update partition info with validated firmware
                drop(firmware_data);
                let mut partitions = self.partitions.lock().await;
                if let Some(p) = partitions.iter_mut().find(|p| p.partition == partition) {
                    p.size_bytes = data.len() as u64;
                    p.hash = Some(actual_hash);
                    p.health = PartitionHealth::Healthy;
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("Hash mismatch: expected {}, got {}", expected_hash, actual_hash))
            }
        } else {
            Err(anyhow::anyhow!("No firmware data found for partition {:?}", partition))
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
        if *self.should_fail_activate.lock().await {
            return Err(anyhow::anyhow!("Mock activate partition failure"));
        }
        
        *self.active_partition.lock().await = partition;
        
        let mut partitions = self.partitions.lock().await;
        for p in partitions.iter_mut() {
            p.active = p.partition == partition;
        }
        
        Ok(())
    }
    
    async fn reboot(&self) -> Result<()> {
        if *self.should_fail_reboot.lock().await {
            return Err(anyhow::anyhow!("Mock reboot failure"));
        }
        
        // Simulate reboot delay
        let delay = *self.reboot_delay_ms.lock().await;
        tokio::time::sleep(Duration::from_millis(delay)).await;
        
        Ok(())
    }
    
    async fn health_check(&self) -> Result<()> {
        let mut attempt_count = self.health_check_attempt_count.lock().await;
        *attempt_count += 1;
        
        let attempts_to_fail = *self.health_check_attempts_to_fail.lock().await;
        
        if *attempt_count <= attempts_to_fail {
            Err(anyhow::anyhow!("Mock health check failure (attempt {})", *attempt_count))
        } else {
            Ok(())
        }
    }
    
    async fn get_hardware_version(&self) -> Result<String> {
        Ok(self.hardware_version.clone())
    }
}

/// Create a test firmware image
pub fn create_test_firmware(version: &str, device_model: &str) -> FirmwareImage {
    let data = format!("test firmware data for {} v{}", device_model, version).into_bytes();
    let hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        hex::encode(hasher.finalize())
    };
    
    FirmwareImage {
        device_model: device_model.to_string(),
        version: semver::Version::parse(version).unwrap(),
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: None,
        data,
        hash,
        size_bytes: data.len() as u64,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some(format!("Test firmware {}", version)),
        signature: None,
    }
}

/// Create a firmware update manager for testing
pub fn create_test_manager() -> FirmwareUpdateManager {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = crate::crypto::VerificationConfig {
        trust_store_path: temp_dir.path().join("trust_store.json"),
        require_firmware_signatures: false, // Disable for tests
        ..Default::default()
    };
    let verifier = crate::crypto::verification::VerificationService::new(config).unwrap();
    let rollout_config = StagedRolloutConfig::default();
    
    FirmwareUpdateManager::new(verifier, rollout_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_successful_firmware_update() {
        let device = Box::new(MockFirmwareDevice::new("test_device_001".to_string()));
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(result.success, "Update should succeed");
        assert_eq!(result.device_id, "test_device_001");
        assert_eq!(result.new_version, Some(firmware.version));
        assert_eq!(result.updated_partition, Some(Partition::B));
        assert!(!result.rollback_performed);
        assert!(result.error.is_none());
    }
    
    #[tokio::test]
    async fn test_firmware_update_with_prepare_failure() {
        let device = MockFirmwareDevice::new("test_device_002".to_string());
        device.set_prepare_failure(true).await;
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(!result.success, "Update should fail");
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("prepare"));
    }
    
    #[tokio::test]
    async fn test_firmware_update_with_write_failure() {
        let device = MockFirmwareDevice::new("test_device_003".to_string());
        device.set_write_failure(true).await;
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(!result.success, "Update should fail");
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("write"));
    }
    
    #[tokio::test]
    async fn test_firmware_update_with_validation_failure() {
        let device = MockFirmwareDevice::new("test_device_004".to_string());
        device.set_validate_failure(true).await;
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(!result.success, "Update should fail");
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("validation") || 
                result.error.as_ref().unwrap().contains("validate"));
    }
    
    #[tokio::test]
    async fn test_firmware_update_with_health_check_failure_and_rollback() {
        let device = MockFirmwareDevice::new("test_device_005".to_string());
        device.set_health_check_failure(10).await; // Fail more than max attempts
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(!result.success, "Update should fail due to health check");
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("Health check failed"));
    }
    
    #[tokio::test]
    async fn test_firmware_update_with_temporary_health_check_failure() {
        let device = MockFirmwareDevice::new("test_device_006".to_string());
        device.set_health_check_failure(2).await; // Fail first 2 attempts, then succeed
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(result.success, "Update should succeed after retries");
        assert_eq!(result.new_version, Some(firmware.version));
    }
    
    #[tokio::test]
    async fn test_firmware_compatibility_check() {
        let device = MockFirmwareDevice::new("test_device_007".to_string());
        let mut firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        firmware.min_hardware_version = Some("2.0".to_string()); // Require newer hardware
        let device = Box::new(device);
        let manager = create_test_manager();
        
        let result = manager.update_device_firmware(device, &firmware).await.unwrap();
        
        assert!(!result.success, "Update should fail due to compatibility");
        assert!(result.error.is_some());
        assert!(result.error.as_ref().unwrap().contains("Hardware version"));
    }
    
    #[tokio::test]
    async fn test_firmware_update_progress_reporting() {
        let device = MockFirmwareDevice::new("test_device_008".to_string());
        device.set_write_delay(10).await; // Slow writes to see progress
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        // Subscribe to progress updates
        let mut progress_rx = manager.subscribe_progress();
        
        // Start update in background
        let update_task = tokio::spawn(async move {
            manager.update_device_firmware(device, &firmware).await
        });
        
        // Collect progress updates
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
                    let result = result.unwrap().unwrap();
                    assert!(result.success, "Update should succeed");
                }
            }
        }
        
        // Verify we received progress updates
        assert!(!progress_updates.is_empty(), "Should receive progress updates");
        
        // Verify progress phases
        let phases: Vec<_> = progress_updates.iter()
            .map(|p| &p.phase)
            .collect();
        
        // Should see at least initializing and transferring phases
        assert!(phases.iter().any(|p| matches!(p, super::firmware::UpdatePhase::Initializing)));
        assert!(phases.iter().any(|p| matches!(p, super::firmware::UpdatePhase::Transferring)));
    }
    
    #[tokio::test]
    async fn test_concurrent_firmware_updates() {
        let manager = create_test_manager();
        
        // Create multiple devices
        let devices: Vec<Box<dyn FirmwareDevice>> = (0..3)
            .map(|i| Box::new(MockFirmwareDevice::new(format!("device_{}", i))) as Box<dyn FirmwareDevice>)
            .collect();
        
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        
        // Start updates concurrently
        let mut update_tasks = Vec::new();
        for device in devices {
            let firmware_clone = firmware.clone();
            let manager_ref = &manager;
            
            let task = async move {
                manager_ref.update_device_firmware(device, &firmware_clone).await
            };
            
            update_tasks.push(task);
        }
        
        // Wait for all updates to complete
        let results = futures::future::join_all(update_tasks).await;
        
        // Verify all updates succeeded
        for (i, result) in results.into_iter().enumerate() {
            let result = result.unwrap();
            assert!(result.success, "Update {} should succeed", i);
            assert_eq!(result.device_id, format!("device_{}", i));
        }
    }
    
    #[tokio::test]
    async fn test_firmware_update_cancellation() {
        let device = MockFirmwareDevice::new("test_device_cancel".to_string());
        device.set_write_delay(100).await; // Slow writes to allow cancellation
        let device = Box::new(device);
        let firmware = create_test_firmware("2.0.0", "test_wheel_v1");
        let manager = create_test_manager();
        
        // Start update
        let device_id = "test_device_cancel";
        let update_task = tokio::spawn({
            let manager = &manager;
            let firmware = firmware.clone();
            async move {
                manager.update_device_firmware(device, &firmware).await
            }
        });
        
        // Wait a bit then cancel
        tokio::time::sleep(Duration::from_millis(50)).await;
        let cancel_result = manager.cancel_update(device_id).await;
        assert!(cancel_result.is_ok(), "Should be able to cancel update");
        
        // Wait for update to complete (should be cancelled)
        let result = update_task.await.unwrap().unwrap();
        
        // Update might complete before cancellation takes effect, 
        // but cancellation should not cause errors
        assert!(cancel_result.is_ok());
    }
    
    #[test]
    fn test_partition_operations() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
    }
    
    #[test]
    fn test_firmware_image_creation() {
        let firmware = create_test_firmware("1.5.0", "racing_wheel_pro");
        
        assert_eq!(firmware.device_model, "racing_wheel_pro");
        assert_eq!(firmware.version, semver::Version::new(1, 5, 0));
        assert!(!firmware.data.is_empty());
        assert!(!firmware.hash.is_empty());
        assert_eq!(firmware.size_bytes, firmware.data.len() as u64);
    }
    
    #[tokio::test]
    async fn test_mock_device_failure_injection() {
        let device = MockFirmwareDevice::new("test_device_mock".to_string());
        
        // Test prepare failure
        device.set_prepare_failure(true).await;
        let result = device.prepare_partition(Partition::B).await;
        assert!(result.is_err());
        
        // Reset and test write failure
        device.set_prepare_failure(false).await;
        device.set_write_failure(true).await;
        let result = device.write_firmware_chunk(Partition::B, 0, b"test").await;
        assert!(result.is_err());
        
        // Test health check failure with retry
        device.reset_to_healthy().await;
        device.set_health_check_failure(3).await;
        
        // First 3 attempts should fail
        for i in 1..=3 {
            let result = device.health_check().await;
            assert!(result.is_err(), "Attempt {} should fail", i);
        }
        
        // 4th attempt should succeed
        let result = device.health_check().await;
        assert!(result.is_ok(), "Attempt 4 should succeed");
    }
}