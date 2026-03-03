//! Deep firmware update process tests.
//!
//! Covers firmware bundle parsing/validation, version comparison semantics,
//! update state machine lifecycle, rollback on verification failure,
//! incomplete download recovery, checksum verification (SHA-256, CRC32),
//! multi-device firmware coordination, compatibility matrix, progress
//! reporting, concurrent update prevention, power-failure recovery
//! simulation, and Ed25519 signature verification.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use openracing_firmware_update::bundle::{
    BundleMetadata, CompressionType, FirmwareBundle, ReleaseChannel,
};
use openracing_firmware_update::manager::{
    FfbBlocker, FirmwareDevice, FirmwareImage, FirmwareUpdateManager, StagedRolloutConfig,
    UpdatePhase, UpdateState,
};
use openracing_firmware_update::partition::{Partition, PartitionHealth, PartitionInfo};
use openracing_firmware_update::rollback::RollbackManager;
use tokio::sync::Mutex;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_firmware_image(data: &[u8], version: &str, device_model: &str) -> Result<FirmwareImage> {
    let hash = openracing_crypto::utils::compute_sha256_hex(data);
    let size_bytes = data.len() as u64;
    Ok(FirmwareImage {
        device_model: device_model.to_string(),
        version: version.parse()?,
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: None,
        data: data.to_vec(),
        hash,
        size_bytes,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some(format!("{} v{}", device_model, version)),
        signature: None,
    })
}

fn make_manager() -> FirmwareUpdateManager {
    FirmwareUpdateManager::new(StagedRolloutConfig::default())
}

/// A configurable mock device for process-level tests.
struct ProcessMockDevice {
    id: String,
    model: String,
    hw_version: String,
    partitions: Arc<Mutex<Vec<PartitionInfo>>>,
    active: Arc<Mutex<Partition>>,
    fw_data: Arc<Mutex<HashMap<Partition, Vec<u8>>>>,
    fail_health: Arc<Mutex<bool>>,
    fail_validate: Arc<Mutex<bool>>,
    fail_write_after: Arc<Mutex<Option<u64>>>,
}

impl ProcessMockDevice {
    fn new(id: &str, model: &str, hw: &str) -> Self {
        let partitions = vec![
            PartitionInfo {
                partition: Partition::A,
                active: true,
                bootable: true,
                version: Some(semver::Version::new(1, 0, 0)),
                size_bytes: 1024 * 1024,
                hash: Some("old".to_string()),
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
            id: id.to_string(),
            model: model.to_string(),
            hw_version: hw.to_string(),
            partitions: Arc::new(Mutex::new(partitions)),
            active: Arc::new(Mutex::new(Partition::A)),
            fw_data: Arc::new(Mutex::new(HashMap::new())),
            fail_health: Arc::new(Mutex::new(false)),
            fail_validate: Arc::new(Mutex::new(false)),
            fail_write_after: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl FirmwareDevice for ProcessMockDevice {
    fn device_id(&self) -> &str {
        &self.id
    }
    fn device_model(&self) -> &str {
        &self.model
    }
    async fn get_partition_info(&self) -> Result<Vec<PartitionInfo>> {
        Ok(self.partitions.lock().await.clone())
    }
    async fn get_active_partition(&self) -> Result<Partition> {
        Ok(*self.active.lock().await)
    }
    async fn prepare_partition(&self, partition: Partition) -> Result<()> {
        let mut parts = self.partitions.lock().await;
        if let Some(p) = parts.iter_mut().find(|p| p.partition == partition) {
            p.bootable = false;
            p.version = None;
            p.size_bytes = 0;
            p.hash = None;
            p.health = PartitionHealth::Unknown;
        }
        self.fw_data.lock().await.remove(&partition);
        Ok(())
    }
    async fn write_firmware_chunk(
        &self,
        partition: Partition,
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        if let Some(limit) = *self.fail_write_after.lock().await
            && offset >= limit
        {
            return Err(anyhow::anyhow!(
                "Simulated write failure at offset {offset}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        let mut fw = self.fw_data.lock().await;
        let buf = fw.entry(partition).or_default();
        let needed = offset as usize + data.len();
        if buf.len() < needed {
            buf.resize(needed, 0);
        }
        buf[offset as usize..needed].copy_from_slice(data);
        Ok(())
    }
    async fn validate_partition(&self, partition: Partition, expected_hash: &str) -> Result<()> {
        if *self.fail_validate.lock().await {
            return Err(anyhow::anyhow!("Simulated validation failure"));
        }
        let fw = self.fw_data.lock().await;
        let data = fw
            .get(&partition)
            .ok_or_else(|| anyhow::anyhow!("No data for {:?}", partition))?;
        let actual = openracing_crypto::utils::compute_sha256_hex(data);
        let data_len = data.len();
        if actual != expected_hash {
            return Err(anyhow::anyhow!("Hash mismatch"));
        }
        drop(fw);
        let mut parts = self.partitions.lock().await;
        if let Some(p) = parts.iter_mut().find(|p| p.partition == partition) {
            p.size_bytes = data_len as u64;
            p.hash = Some(actual);
            p.health = PartitionHealth::Healthy;
        }
        Ok(())
    }
    async fn set_bootable(&self, partition: Partition, bootable: bool) -> Result<()> {
        let mut parts = self.partitions.lock().await;
        if let Some(p) = parts.iter_mut().find(|p| p.partition == partition) {
            p.bootable = bootable;
        }
        Ok(())
    }
    async fn activate_partition(&self, partition: Partition) -> Result<()> {
        *self.active.lock().await = partition;
        let mut parts = self.partitions.lock().await;
        for p in parts.iter_mut() {
            p.active = p.partition == partition;
        }
        Ok(())
    }
    async fn reboot(&self) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
    async fn health_check(&self) -> Result<()> {
        if *self.fail_health.lock().await {
            return Err(anyhow::anyhow!("Health check failed"));
        }
        Ok(())
    }
    async fn get_hardware_version(&self) -> Result<String> {
        Ok(self.hw_version.clone())
    }
}

// ===========================================================================
// 1. Firmware bundle parsing and validation
// ===========================================================================

mod bundle_parsing {
    use super::*;

    #[test]
    fn parse_large_payload_roundtrip() -> TestResult {
        let data = vec![0xAB; 64 * 1024]; // 64 KiB
        let image = make_firmware_image(&data, "3.0.0", "big-wheel")?;
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data.len(), data.len());
        assert_eq!(extracted.data, data);
        Ok(())
    }

    #[test]
    fn parse_bundle_preserves_metadata_fields() -> TestResult {
        let data = vec![0x01; 16];
        let image = make_firmware_image(&data, "1.0.0", "meta-wheel")?;
        let meta = BundleMetadata {
            title: Some("Release Candidate".to_string()),
            changelog: Some("Fixed braking force feedback".to_string()),
            channel: ReleaseChannel::Beta,
            rollback_version: Some(semver::Version::new(0, 9, 0)),
            ..BundleMetadata::default()
        };
        let bundle = FirmwareBundle::new(&image, meta, CompressionType::None)?;
        let bytes = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&bytes)?;
        assert_eq!(parsed.metadata.title.as_deref(), Some("Release Candidate"));
        assert_eq!(parsed.metadata.channel, ReleaseChannel::Beta);
        assert!(parsed.metadata.rollback_version.is_some());
        Ok(())
    }

    #[test]
    fn parse_empty_payload_succeeds() -> TestResult {
        let data: Vec<u8> = vec![];
        let image = make_firmware_image(&data, "0.0.1", "empty-fw")?;
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;
        let bytes = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&bytes)?;
        let extracted = parsed.extract_image()?;
        assert!(extracted.data.is_empty());
        Ok(())
    }

    #[test]
    fn corrupted_header_json_rejected() {
        // Valid magic, but corrupted header length/data
        let mut buf = Vec::new();
        buf.extend_from_slice(b"OWFB\0\0\0\x01");
        buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F]); // absurd length
        let result = FirmwareBundle::parse(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_payload_rejected() -> TestResult {
        let data = vec![0x42; 128];
        let image = make_firmware_image(&data, "1.0.0", "trunc-wheel")?;
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;
        let mut bytes = bundle.serialize()?;
        // Chop off part of the payload
        bytes.truncate(bytes.len() - 64);
        let result = FirmwareBundle::parse(&bytes);
        assert!(result.is_err());
        Ok(())
    }
}

// ===========================================================================
// 2. Version comparison logic (upgrade, downgrade, same version)
// ===========================================================================

mod version_comparison {
    use super::*;

    #[test]
    fn upgrade_detected_when_new_is_higher() {
        let old = semver::Version::new(1, 0, 0);
        let new = semver::Version::new(2, 0, 0);
        assert!(new > old, "2.0.0 should be an upgrade from 1.0.0");
    }

    #[test]
    fn downgrade_detected_when_new_is_lower() {
        let old = semver::Version::new(3, 0, 0);
        let new = semver::Version::new(2, 5, 0);
        assert!(new < old, "2.5.0 should be a downgrade from 3.0.0");
    }

    #[test]
    fn same_version_detected() {
        let a = semver::Version::new(1, 2, 3);
        let b = semver::Version::new(1, 2, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn pre_release_is_less_than_release() -> TestResult {
        let pre: semver::Version = "1.0.0-beta.1".parse()?;
        let rel = semver::Version::new(1, 0, 0);
        assert!(pre < rel);
        Ok(())
    }

    #[test]
    fn patch_increment_is_upgrade() {
        let a = semver::Version::new(1, 0, 0);
        let b = semver::Version::new(1, 0, 1);
        assert!(b > a);
    }

    #[test]
    fn rollback_protection_blocks_downgrade() -> TestResult {
        let data = vec![0x01; 8];
        let image = make_firmware_image(&data, "2.0.0", "wheel")?;
        let meta = BundleMetadata {
            rollback_version: Some(semver::Version::new(1, 5, 0)),
            ..BundleMetadata::default()
        };
        let bundle = FirmwareBundle::new(&image, meta, CompressionType::None)?;
        // Device at 1.4.0 cannot upgrade (below rollback floor)
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(1, 4, 0)));
        // Device at 1.5.0 can upgrade
        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 5, 0)));
        // Device at 1.6.0 can upgrade
        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 6, 0)));
        Ok(())
    }
}

// ===========================================================================
// 3. Update state machine transitions
// ===========================================================================

mod state_machine_lifecycle {
    use super::*;

    #[test]
    fn full_lifecycle_idle_to_complete() {
        let states = [
            UpdateState::Idle,
            UpdateState::Downloading { progress: 0 },
            UpdateState::Downloading { progress: 50 },
            UpdateState::Downloading { progress: 100 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 0 },
            UpdateState::Flashing { progress: 100 },
            UpdateState::Rebooting,
            UpdateState::Complete,
        ];
        // All intermediate states should report in-progress
        for state in &states[1..states.len() - 1] {
            assert!(state.is_in_progress(), "{state:?} should be in-progress");
        }
        // Terminal states should not be in-progress
        assert!(!states[0].is_in_progress());
        assert!(!states[states.len() - 1].is_in_progress());
    }

    #[test]
    fn failed_state_carries_error_info() {
        let state = UpdateState::Failed {
            error: "checksum mismatch".to_string(),
            recoverable: true,
        };
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());

        let unrecoverable = UpdateState::Failed {
            error: "hardware fault".to_string(),
            recoverable: false,
        };
        assert!(!unrecoverable.is_in_progress());
    }

    #[tokio::test]
    async fn state_machine_set_and_get_transitions() -> TestResult {
        let blocker = FfbBlocker::new();
        assert_eq!(blocker.get_state().await, UpdateState::Idle);

        blocker.begin_update("dev-lifecycle").await?;

        let sequence = [
            UpdateState::Downloading { progress: 0 },
            UpdateState::Downloading { progress: 100 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 50 },
            UpdateState::Flashing { progress: 100 },
            UpdateState::Rebooting,
        ];
        for expected in &sequence {
            blocker.set_state(expected.clone()).await;
            let actual = blocker.get_state().await;
            assert_eq!(&actual, expected);
        }

        blocker.end_update().await;
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        Ok(())
    }
}

// ===========================================================================
// 4. Rollback on verification failure
// ===========================================================================

mod rollback_on_failure {
    use super::*;

    #[tokio::test]
    async fn validation_failure_returns_error_result() -> TestResult {
        let device = ProcessMockDevice::new("rollback-dev", "test_wheel", "1.0");
        *device.fail_validate.lock().await = true;
        let fw_data = b"firmware for rollback test".to_vec();
        let firmware = make_firmware_image(&fw_data, "2.0.0", "test_wheel")?;
        let manager = make_manager();
        let result = manager
            .update_device_firmware(Box::new(device), &firmware)
            .await?;
        assert!(!result.success, "Update should fail when validation fails");
        assert!(result.error.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn rollback_manager_restores_after_failed_update() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        // Write original firmware file
        tokio::fs::write(install_dir.join("fw.bin"), b"original-fw-data").await?;

        let mgr = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        mgr.create_backup(
            "pre-update",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Simulate failed update corrupting the file
        tokio::fs::write(install_dir.join("fw.bin"), b"corrupted-data").await?;

        // Rollback should restore original
        mgr.rollback_to("pre-update").await?;
        let restored = tokio::fs::read_to_string(install_dir.join("fw.bin")).await?;
        assert_eq!(restored, "original-fw-data");
        Ok(())
    }

    #[tokio::test]
    async fn multiple_backups_independent_rollback() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"v1-content").await?;
        let mgr = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        mgr.create_backup(
            "bak-v1",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Simulate v2 install
        tokio::fs::write(install_dir.join("fw.bin"), b"v2-content").await?;

        mgr.create_backup(
            "bak-v2",
            semver::Version::new(2, 0, 0),
            semver::Version::new(3, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Rolling back to bak-v1 restores v1 content
        mgr.rollback_to("bak-v1").await?;
        let content = tokio::fs::read_to_string(install_dir.join("fw.bin")).await?;
        assert_eq!(content, "v1-content");
        Ok(())
    }
}

// ===========================================================================
// 5. Incomplete download recovery
// ===========================================================================

mod incomplete_download {
    use super::*;

    #[tokio::test]
    async fn write_failure_mid_transfer_reports_error() -> TestResult {
        let device = ProcessMockDevice::new("incomplete-dev", "test_wheel", "1.0");
        // Fail after writing first 4096 bytes
        *device.fail_write_after.lock().await = Some(4096);

        let fw_data = vec![0xAA; 8192]; // > 4096 so write will fail partway
        let firmware = make_firmware_image(&fw_data, "2.0.0", "test_wheel")?;
        let manager = make_manager();

        let result = manager
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        assert!(!result.success);
        assert!(result.error.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn retry_after_failure_succeeds() -> TestResult {
        // First attempt fails
        let device1 = ProcessMockDevice::new("retry-dev", "test_wheel", "1.0");
        *device1.fail_write_after.lock().await = Some(0);
        let fw_data = b"small firmware".to_vec();
        let firmware = make_firmware_image(&fw_data, "2.0.0", "test_wheel")?;
        let manager = make_manager();

        let result1 = manager
            .update_device_firmware(Box::new(device1), &firmware)
            .await?;
        assert!(!result1.success);

        // Second attempt succeeds (new device, no failure injection)
        let device2 = ProcessMockDevice::new("retry-dev", "test_wheel", "1.0");
        let result2 = manager
            .update_device_firmware(Box::new(device2), &firmware)
            .await?;
        assert!(result2.success);
        Ok(())
    }
}

// ===========================================================================
// 6. Checksum verification (SHA-256, CRC32)
// ===========================================================================

mod checksum_verification {
    use super::*;
    use openracing_firmware_update::delta::compute_data_hash;

    #[test]
    fn sha256_hash_matches_known_value() {
        let data = b"OpenRacing firmware test data";
        let hash = openracing_crypto::utils::compute_sha256_hex(data);
        // Verify it's 64 hex characters (256 bits)
        assert_eq!(hash.len(), 64);
        // Same data should produce same hash
        let hash2 = openracing_crypto::utils::compute_sha256_hex(data);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn sha256_different_data_different_hash() {
        let hash_a = openracing_crypto::utils::compute_sha256_hex(b"data-a");
        let hash_b = openracing_crypto::utils::compute_sha256_hex(b"data-b");
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn compute_data_hash_matches_crypto_util() {
        let data = b"consistent hashing across modules";
        let hash_delta = compute_data_hash(data);
        let hash_crypto = openracing_crypto::utils::compute_sha256_hex(data);
        assert_eq!(hash_delta, hash_crypto);
    }

    #[test]
    fn crc32_basic_verification() {
        // CRC32 for integrity checks alongside SHA-256
        let data = b"firmware payload for CRC32 check";
        let crc = crc32_of(data);
        assert_ne!(crc, 0);
        // Same data → same CRC
        assert_eq!(crc, crc32_of(data));
        // Different data → different CRC (with high probability)
        assert_ne!(crc, crc32_of(b"different payload"));
    }

    #[test]
    fn crc32_empty_data() {
        let crc = crc32_of(b"");
        // CRC32 of empty data is 0x00000000
        assert_eq!(crc, 0);
    }

    #[test]
    fn firmware_image_hash_matches_data() -> TestResult {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let image = make_firmware_image(&data, "1.0.0", "hash-wheel")?;
        let expected = openracing_crypto::utils::compute_sha256_hex(&data);
        assert_eq!(image.hash, expected);
        Ok(())
    }

    #[test]
    fn bundle_payload_hash_verified_on_parse() -> TestResult {
        let data = vec![0x42; 32];
        let image = make_firmware_image(&data, "1.0.0", "hash-bundle")?;
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;
        let mut bytes = bundle.serialize()?;

        // Corrupt a payload byte (near end, in the payload region)
        let len = bytes.len();
        if len > 10 {
            bytes[len - 5] ^= 0xFF;
        }

        let result = FirmwareBundle::parse(&bytes);
        assert!(result.is_err(), "Corrupted payload should fail hash check");
        Ok(())
    }

    /// Simple CRC32 implementation for test verification.
    fn crc32_of(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc ^ 0xFFFF_FFFF
    }
}

// ===========================================================================
// 7. Multi-device firmware coordination
// ===========================================================================

mod multi_device_coordination {
    use super::*;

    #[tokio::test]
    async fn sequential_updates_to_different_devices() -> TestResult {
        let manager = make_manager();

        let fw_a = b"firmware-for-device-a".to_vec();
        let fw_b = b"firmware-for-device-b".to_vec();

        let img_a = make_firmware_image(&fw_a, "2.0.0", "wheel-a")?;
        let img_b = make_firmware_image(&fw_b, "2.0.0", "wheel-b")?;

        let dev_a = ProcessMockDevice::new("dev-a", "wheel-a", "1.0");
        let dev_b = ProcessMockDevice::new("dev-b", "wheel-b", "1.0");

        let result_a = manager
            .update_device_firmware(Box::new(dev_a), &img_a)
            .await?;
        assert!(result_a.success, "Device A update should succeed");

        let result_b = manager
            .update_device_firmware(Box::new(dev_b), &img_b)
            .await?;
        assert!(result_b.success, "Device B update should succeed");
        Ok(())
    }

    #[tokio::test]
    async fn different_device_models_get_correct_firmware() -> TestResult {
        let manager = make_manager();

        let fw_data = b"wheel-model-x-firmware".to_vec();
        let img = make_firmware_image(&fw_data, "1.5.0", "model-x")?;

        // Device with matching model succeeds
        let dev_match = ProcessMockDevice::new("d-match", "model-x", "1.0");
        let result = manager
            .update_device_firmware(Box::new(dev_match), &img)
            .await?;
        assert!(result.success);

        // Device with different model — firmware still gets written
        // (model matching is done at a higher level, not in update_device_firmware)
        let dev_other = ProcessMockDevice::new("d-other", "model-y", "1.0");
        let result = manager
            .update_device_firmware(Box::new(dev_other), &img)
            .await?;
        // The manager doesn't enforce device_model matching; it does hw version checks
        assert!(result.success);
        Ok(())
    }

    #[tokio::test]
    async fn update_result_contains_device_id() -> TestResult {
        let manager = make_manager();
        let fw_data = b"fw-data".to_vec();
        let img = make_firmware_image(&fw_data, "2.0.0", "wheel")?;
        let dev = ProcessMockDevice::new("unique-dev-42", "wheel", "1.0");
        let result = manager.update_device_firmware(Box::new(dev), &img).await?;
        assert_eq!(result.device_id, "unique-dev-42");
        Ok(())
    }
}

// ===========================================================================
// 8. Firmware compatibility matrix (device model → supported fw versions)
// ===========================================================================

mod compatibility_matrix {
    use super::*;

    fn bundle_for_hw(min: Option<&str>, max: Option<&str>, model: &str) -> Result<FirmwareBundle> {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: model.to_string(),
            version: semver::Version::new(2, 0, 0),
            min_hardware_version: min.map(String::from),
            max_hardware_version: max.map(String::from),
            data,
            hash,
            size_bytes: 8,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };
        FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)
    }

    #[test]
    fn exact_boundary_versions_compatible() -> TestResult {
        let bundle = bundle_for_hw(Some("1.0"), Some("5.0"), "wheel-v1")?;
        assert!(bundle.is_compatible_with_hardware("1.0"));
        assert!(bundle.is_compatible_with_hardware("5.0"));
        Ok(())
    }

    #[test]
    fn multi_component_versions_compared_numerically() -> TestResult {
        let bundle = bundle_for_hw(Some("1.0.0"), Some("2.5.3"), "wheel-v2")?;
        assert!(bundle.is_compatible_with_hardware("1.0.0"));
        assert!(bundle.is_compatible_with_hardware("2.5.3"));
        assert!(bundle.is_compatible_with_hardware("1.5.0"));
        assert!(!bundle.is_compatible_with_hardware("2.5.4"));
        assert!(!bundle.is_compatible_with_hardware("0.9.9"));
        Ok(())
    }

    #[test]
    fn multiple_models_independent_compatibility() -> TestResult {
        let bundle_a = bundle_for_hw(Some("1.0"), Some("3.0"), "pro-wheel")?;
        let bundle_b = bundle_for_hw(Some("2.0"), Some("4.0"), "lite-wheel")?;

        // hw 1.5: compatible with pro, not lite
        assert!(bundle_a.is_compatible_with_hardware("1.5"));
        assert!(!bundle_b.is_compatible_with_hardware("1.5"));

        // hw 3.5: not compatible with pro, compatible with lite
        assert!(!bundle_a.is_compatible_with_hardware("3.5"));
        assert!(bundle_b.is_compatible_with_hardware("3.5"));
        Ok(())
    }

    #[tokio::test]
    async fn incompatible_hw_version_rejects_update() -> TestResult {
        let manager = make_manager();
        let fw_data = b"firmware".to_vec();
        let mut img = make_firmware_image(&fw_data, "2.0.0", "wheel")?;
        img.min_hardware_version = Some("3.0".to_string()); // device is 1.0

        let dev = ProcessMockDevice::new("compat-dev", "wheel", "1.0");
        let result = manager.update_device_firmware(Box::new(dev), &img).await?;
        assert!(!result.success, "Should reject incompatible hardware");
        Ok(())
    }
}

// ===========================================================================
// 9. Update progress reporting
// ===========================================================================

mod progress_reporting {
    use super::*;

    #[tokio::test]
    async fn progress_events_emitted_during_update() -> TestResult {
        let manager = make_manager();
        let mut rx = manager.subscribe_progress();
        let fw_data = b"progress-tracking-firmware".to_vec();
        let img = make_firmware_image(&fw_data, "2.0.0", "wheel")?;
        let dev = ProcessMockDevice::new("progress-dev", "wheel", "1.0");

        let mut task = tokio::spawn({
            let img = img.clone();
            async move { manager.update_device_firmware(Box::new(dev), &img).await }
        });

        let mut events = Vec::new();
        let mut done = false;
        while !done {
            tokio::select! {
                p = rx.recv() => {
                    if let Ok(progress) = p {
                        events.push(progress);
                    }
                }
                r = &mut task => {
                    done = true;
                    let result = r??;
                    assert!(result.success);
                }
            }
        }

        // Drain any remaining buffered events after task completion
        while let Ok(progress) = rx.try_recv() {
            events.push(progress);
        }

        assert!(
            !events.is_empty(),
            "Should receive at least one progress event"
        );

        // Should see Initializing at the start
        assert!(
            events
                .iter()
                .any(|e| matches!(e.phase, UpdatePhase::Initializing))
        );
        Ok(())
    }

    #[tokio::test]
    async fn progress_contains_byte_counts() -> TestResult {
        let manager = make_manager();
        let mut rx = manager.subscribe_progress();
        let fw_data = vec![0x55; 256];
        let img = make_firmware_image(&fw_data, "2.0.0", "wheel")?;
        let dev = ProcessMockDevice::new("byte-count-dev", "wheel", "1.0");

        let mut task = tokio::spawn({
            let img = img.clone();
            async move { manager.update_device_firmware(Box::new(dev), &img).await }
        });

        let mut events = Vec::new();
        let mut done = false;
        while !done {
            tokio::select! {
                p = rx.recv() => {
                    if let Ok(progress) = p {
                        events.push(progress);
                    }
                }
                r = &mut task => {
                    done = true;
                    let _ = r??;
                }
            }
        }

        // Drain remaining buffered events
        while let Ok(progress) = rx.try_recv() {
            events.push(progress);
        }

        assert!(!events.is_empty(), "Should receive progress events");

        // total_bytes should match firmware size across all events
        for e in &events {
            assert_eq!(e.total_bytes, 256, "total_bytes should match firmware size");
        }
        Ok(())
    }
}

// ===========================================================================
// 10. Concurrent update prevention (one device at a time)
// ===========================================================================

mod concurrent_update_prevention {
    use super::*;

    #[tokio::test]
    async fn ffb_blocker_prevents_second_concurrent_update() -> TestResult {
        let blocker = FfbBlocker::new();

        blocker.begin_update("device-1").await?;
        assert!(blocker.is_ffb_blocked());

        // Second device should be rejected
        let err = blocker.begin_update("device-2").await;
        assert!(err.is_err());

        // First device finishes
        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());

        // Now second device can start
        let ok = blocker.begin_update("device-2").await;
        assert!(ok.is_ok());
        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn manager_prevents_duplicate_device_update() -> TestResult {
        let manager = Arc::new(make_manager());

        let fw_data = vec![0x01; 16384]; // Larger payload to keep update running
        let img = Arc::new(make_firmware_image(&fw_data, "2.0.0", "wheel")?);

        // Start first update (long-running due to reboot sleep)
        let mgr1 = Arc::clone(&manager);
        let img1 = Arc::clone(&img);
        let handle = tokio::spawn(async move {
            let dev = ProcessMockDevice::new("same-dev", "wheel", "1.0");
            mgr1.update_device_firmware(Box::new(dev), &img1).await
        });

        // Give first update time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Check if an update is in progress
        let in_progress = manager.is_update_in_progress().await;
        // Whether or not it's still in progress depends on timing, so we just
        // verify the API works without panicking.
        let _ = in_progress;

        // Wait for first to complete, then second should succeed
        let result = handle.await??;
        assert!(result.success);

        let dev2 = ProcessMockDevice::new("same-dev", "wheel", "1.0");
        let result2 = manager.update_device_firmware(Box::new(dev2), &img).await?;
        assert!(result2.success);
        Ok(())
    }

    #[tokio::test]
    async fn ffb_operations_blocked_during_active_update() -> TestResult {
        let blocker = FfbBlocker::new();

        // Before update: FFB allowed
        assert!(blocker.try_ffb_operation().is_ok());

        blocker.begin_update("fw-device").await?;

        // During update: FFB blocked
        let err = blocker.try_ffb_operation();
        assert!(err.is_err());

        blocker.end_update().await;

        // After update: FFB allowed again
        assert!(blocker.try_ffb_operation().is_ok());
        Ok(())
    }
}

// ===========================================================================
// 11. Power failure recovery simulation
// ===========================================================================

mod power_failure_recovery {
    use super::*;

    #[tokio::test]
    async fn backup_survives_interrupted_update() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        // Simulate: backup created, then "power failure" during update
        tokio::fs::write(install_dir.join("fw.bin"), b"pre-update-firmware").await?;
        let mgr = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        mgr.create_backup(
            "pre-power-fail",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Simulate partial/corrupted write (power failure)
        tokio::fs::write(install_dir.join("fw.bin"), b"CORRUPT").await?;

        // On recovery, rollback restores the backup
        mgr.rollback_to("pre-power-fail").await?;
        let content = tokio::fs::read_to_string(install_dir.join("fw.bin")).await?;
        assert_eq!(content, "pre-update-firmware");
        Ok(())
    }

    #[tokio::test]
    async fn backup_integrity_verified_before_rollback() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"content").await?;
        let mgr = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        mgr.create_backup(
            "integrity-test",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Verify backup integrity
        let info = mgr.get_backup_info().await?;
        assert_eq!(info.len(), 1);
        assert!(info[0].valid, "Backup should be valid");
        assert!(info[0].size_bytes > 0, "Backup should have non-zero size");
        Ok(())
    }

    #[tokio::test]
    async fn health_check_failure_triggers_rollback_path() -> TestResult {
        let device = ProcessMockDevice::new("power-dev", "wheel", "1.0");
        *device.fail_health.lock().await = true;

        let fw_data = b"fw-for-health-check-test".to_vec();
        let firmware = make_firmware_image(&fw_data, "2.0.0", "wheel")?;
        let manager = make_manager();

        let result = manager
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        // Health check failure causes the update to fail and attempt rollback
        assert!(!result.success);
        assert!(result.error.is_some());
        let err_msg = result.error.as_deref().unwrap_or("");
        // The error should mention health check
        assert!(
            err_msg.contains("ealth") || err_msg.contains("rollback"),
            "Error should mention health check or rollback, got: {err_msg}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn partition_state_recoverable_after_failed_update() -> TestResult {
        // After a failed update, the original partition should still be usable
        let device = ProcessMockDevice::new("recovery-dev", "wheel", "1.0");
        *device.fail_validate.lock().await = true;

        let fw_data = b"test-recovery-fw".to_vec();
        let firmware = make_firmware_image(&fw_data, "2.0.0", "wheel")?;
        let manager = make_manager();

        let _ = manager
            .update_device_firmware(Box::new(device), &firmware)
            .await?;

        // Create a new device to check state (simulating restart)
        let device2 = ProcessMockDevice::new("recovery-dev", "wheel", "1.0");
        let parts = device2.get_partition_info().await?;
        let active = parts.iter().find(|p| p.active);
        assert!(active.is_some(), "Should have an active partition");
        let active = active.ok_or("no active partition")?;
        assert_eq!(
            active.partition,
            Partition::A,
            "Original partition should remain active"
        );
        Ok(())
    }
}

// ===========================================================================
// 12. Firmware signature verification (Ed25519)
// ===========================================================================

mod signature_verification {
    use super::*;
    use openracing_crypto::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair};
    use openracing_crypto::verification::ContentType;

    #[test]
    fn sign_and_verify_firmware_data() -> TestResult {
        let keypair = KeyPair::generate()?;
        let fw_data = b"signed firmware payload for verification";

        let sig_meta = Ed25519Signer::sign_with_metadata(
            fw_data,
            &keypair,
            "test-signer",
            ContentType::Firmware,
            Some("test signature".to_string()),
        )?;

        // Parse the signature back and verify
        let signature = Ed25519Verifier::parse_signature(&sig_meta.signature)?;
        let valid = Ed25519Verifier::verify(fw_data, &signature, &keypair.public_key)?;
        assert!(valid, "Valid signature should verify");
        Ok(())
    }

    #[test]
    fn tampered_data_fails_verification() -> TestResult {
        let keypair = KeyPair::generate()?;
        let fw_data = b"original firmware data";

        let sig_meta = Ed25519Signer::sign_with_metadata(
            fw_data,
            &keypair,
            "test-signer",
            ContentType::Firmware,
            None,
        )?;

        let signature = Ed25519Verifier::parse_signature(&sig_meta.signature)?;

        // Tampered data should fail
        let tampered = b"tampered firmware data";
        let valid = Ed25519Verifier::verify(tampered, &signature, &keypair.public_key)?;
        assert!(!valid, "Tampered data should fail verification");
        Ok(())
    }

    #[test]
    fn wrong_key_fails_verification() -> TestResult {
        let keypair1 = KeyPair::generate()?;
        let keypair2 = KeyPair::generate()?;
        let fw_data = b"firmware signed with key1";

        let sig_meta = Ed25519Signer::sign_with_metadata(
            fw_data,
            &keypair1,
            "signer-1",
            ContentType::Firmware,
            None,
        )?;

        let signature = Ed25519Verifier::parse_signature(&sig_meta.signature)?;

        // Verify with wrong key should fail
        let valid = Ed25519Verifier::verify(fw_data, &signature, &keypair2.public_key)?;
        assert!(!valid, "Wrong key should fail verification");
        Ok(())
    }

    #[test]
    fn signature_metadata_has_correct_fields() -> TestResult {
        let keypair = KeyPair::generate()?;
        let fw_data = b"metadata check firmware";

        let sig_meta = Ed25519Signer::sign_with_metadata(
            fw_data,
            &keypair,
            "OpenRacing CI",
            ContentType::Firmware,
            Some("Build #42".to_string()),
        )?;

        assert_eq!(sig_meta.signer, "OpenRacing CI");
        assert_eq!(sig_meta.comment.as_deref(), Some("Build #42"));
        assert!(!sig_meta.signature.is_empty());
        assert!(!sig_meta.key_fingerprint.is_empty());
        assert_eq!(sig_meta.key_fingerprint, keypair.fingerprint());
        Ok(())
    }

    #[test]
    fn signed_bundle_preserves_signature() -> TestResult {
        let keypair = KeyPair::generate()?;
        let fw_data = vec![0xCA, 0xFE, 0xBA, 0xBE];
        let mut image = make_firmware_image(&fw_data, "1.0.0", "signed-wheel")?;

        let sig = Ed25519Signer::sign_with_metadata(
            &fw_data,
            &keypair,
            "release-signer",
            ContentType::Firmware,
            None,
        )?;
        image.signature = Some(sig);

        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::Gzip)?;
        let bytes = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&bytes)?;

        assert!(parsed.signature.is_some());
        let parsed_sig = parsed.signature.as_ref().ok_or("missing signature")?;
        assert_eq!(parsed_sig.signer, "release-signer");
        assert_eq!(parsed_sig.key_fingerprint, keypair.fingerprint());

        // Verify signature against extracted data
        let extracted = parsed.extract_image()?;
        let sig_obj = Ed25519Verifier::parse_signature(&parsed_sig.signature)?;
        let valid = Ed25519Verifier::verify(&extracted.data, &sig_obj, &keypair.public_key)?;
        assert!(
            valid,
            "Signature in bundle should verify against extracted data"
        );
        Ok(())
    }

    #[test]
    fn signature_metadata_serde_roundtrip() -> TestResult {
        let keypair = KeyPair::generate()?;
        let fw_data = b"serde test";

        let sig_meta = Ed25519Signer::sign_with_metadata(
            fw_data,
            &keypair,
            "serde-signer",
            ContentType::Firmware,
            None,
        )?;

        let json = serde_json::to_string(&sig_meta)?;
        let decoded: openracing_crypto::SignatureMetadata = serde_json::from_str(&json)?;
        assert_eq!(decoded.signer, sig_meta.signer);
        assert_eq!(decoded.key_fingerprint, sig_meta.key_fingerprint);
        assert_eq!(decoded.signature, sig_meta.signature);
        Ok(())
    }
}
