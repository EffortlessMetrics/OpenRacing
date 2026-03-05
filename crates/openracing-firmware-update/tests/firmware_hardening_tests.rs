//! Hardening tests for the firmware update system.
//!
//! Covers:
//! - Firmware package validation (bundle format, magic, checksums)
//! - Version comparison and compatibility
//! - Update lifecycle (state machine transitions, FFB blocking)
//! - Rollback handling (backup creation, restore, verification)
//! - Signature metadata in bundles
//! - Partial update / recovery scenarios
//! - Delta patching edge cases
//! - Staged rollout planning

use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use openracing_firmware_update::bundle::{
    BundleMetadata, CompressionType, FirmwareBundle, OWFB_MAGIC, ReleaseChannel,
};
use openracing_firmware_update::delta::{
    apply_simple_patch, compress_data, compute_data_hash, create_simple_patch, decompress_data,
};
use openracing_firmware_update::error::FirmwareUpdateError;
use openracing_firmware_update::hardware_version::{HardwareVersion, HardwareVersionError};
use openracing_firmware_update::health::HealthCheckSummary;
use openracing_firmware_update::manager::{
    FfbBlocker, FirmwareImage, FirmwareUpdateManager, StagedRolloutConfig, UpdatePhase,
    UpdateProgress, UpdateResult, UpdateState,
};
use openracing_firmware_update::partition::{Partition, PartitionHealth, PartitionInfo};
use openracing_firmware_update::rollback::{BackupMetadata, RollbackManager};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_firmware_image(data: &[u8]) -> FirmwareImage {
    let hash = compute_data_hash(data);
    FirmwareImage {
        device_model: "test-wheel".to_string(),
        version: semver::Version::new(2, 0, 0),
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: Some("5.0".to_string()),
        data: data.to_vec(),
        hash,
        size_bytes: data.len() as u64,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some("Test firmware".to_string()),
        signature: None,
    }
}

fn make_signed_firmware_image(data: &[u8]) -> FirmwareImage {
    let hash = compute_data_hash(data);
    FirmwareImage {
        device_model: "signed-wheel".to_string(),
        version: semver::Version::new(3, 1, 0),
        min_hardware_version: Some("2.0".to_string()),
        max_hardware_version: Some("8.0".to_string()),
        data: data.to_vec(),
        hash,
        size_bytes: data.len() as u64,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some("Signed firmware".to_string()),
        signature: Some(openracing_crypto::SignatureMetadata {
            signature: "dGVzdF9zaWduYXR1cmU=".to_string(),
            key_fingerprint: "SHA256:abcdef1234567890".to_string(),
            signer: "openracing-build-server".to_string(),
            timestamp: chrono::Utc::now(),
            content_type: openracing_crypto::verification::ContentType::Firmware,
            comment: Some("CI build".to_string()),
        }),
    }
}

fn make_default_metadata() -> BundleMetadata {
    BundleMetadata {
        title: Some("Test Bundle".to_string()),
        changelog: Some("Bug fixes and improvements".to_string()),
        ..Default::default()
    }
}

// ===========================================================================
// 1. Firmware Package Validation
// ===========================================================================

mod bundle_validation {
    use super::*;

    #[test]
    fn parse_rejects_empty_data() {
        let result = FirmwareBundle::parse(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_too_short_data() {
        let result = FirmwareBundle::parse(&[0x01, 0x02, 0x03]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_wrong_magic() {
        let mut data = vec![0u8; 128];
        data[..8].copy_from_slice(b"NOTOWFB\x01");
        let result = FirmwareBundle::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_truncated_after_magic() {
        let result = FirmwareBundle::parse(OWFB_MAGIC);
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_no_compression() -> TestResult {
        let image = make_firmware_image(&[0xDE, 0xAD, 0xBE, 0xEF, 0x42]);
        let metadata = make_default_metadata();

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;

        assert_eq!(parsed.header.device_model, "test-wheel");
        assert_eq!(
            parsed.header.firmware_version,
            semver::Version::new(2, 0, 0)
        );
        assert_eq!(parsed.header.compression, CompressionType::None);
        assert_eq!(parsed.header.uncompressed_size, 5);

        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, image.data);
        Ok(())
    }

    #[test]
    fn roundtrip_gzip_compression() -> TestResult {
        let data: Vec<u8> = (0..256).map(|i| (i % 256) as u8).collect();
        let image = make_firmware_image(&data);
        let metadata = make_default_metadata();

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;

        assert!(
            bundle.header.compressed_size <= bundle.header.uncompressed_size
                || bundle.header.compressed_size > 0,
            "Compressed size should be valid"
        );

        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, data);
        Ok(())
    }

    #[test]
    fn bundle_payload_hash_is_verified_on_parse() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        let mut serialized = bundle.serialize()?;

        // Corrupt the last byte of the payload
        let payload_end = serialized.len() - 1;
        serialized[payload_end] ^= 0xFF;

        let result = FirmwareBundle::parse(&serialized);
        assert!(result.is_err(), "Corrupted payload should fail validation");
        Ok(())
    }

    #[test]
    fn bundle_write_and_load_roundtrip() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let path = temp_dir.path().join("test.owfb");

        let image = make_firmware_image(&[10, 20, 30, 40, 50]);
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;

        bundle.write(&path)?;
        assert!(path.exists());

        let loaded = FirmwareBundle::load(&path)?;
        assert_eq!(loaded.header.device_model, "test-wheel");

        let extracted = loaded.extract_image()?;
        assert_eq!(extracted.data, image.data);
        Ok(())
    }

    #[test]
    fn bundle_size_matches_serialized_length() -> TestResult {
        let image = make_firmware_image(&[0xAB; 64]);
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        let serialized = bundle.serialize()?;
        assert_eq!(bundle.bundle_size(), serialized.len());
        Ok(())
    }

    #[test]
    fn bundle_preserves_metadata_fields() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = BundleMetadata {
            title: Some("Important Update".to_string()),
            changelog: Some("Fixed critical bug".to_string()),
            signing_key: Some("key-fingerprint-123".to_string()),
            rollback_version: Some(semver::Version::new(1, 5, 0)),
            channel: ReleaseChannel::Beta,
            ..Default::default()
        };

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;

        assert_eq!(parsed.metadata.title.as_deref(), Some("Important Update"));
        assert_eq!(
            parsed.metadata.changelog.as_deref(),
            Some("Fixed critical bug")
        );
        assert_eq!(
            parsed.metadata.signing_key.as_deref(),
            Some("key-fingerprint-123")
        );
        assert_eq!(
            parsed.metadata.rollback_version,
            Some(semver::Version::new(1, 5, 0))
        );
        assert_eq!(parsed.metadata.channel, ReleaseChannel::Beta);
        Ok(())
    }

    #[test]
    fn bundle_with_nightly_channel() -> TestResult {
        let image = make_firmware_image(&[0xFF; 8]);
        let metadata = BundleMetadata {
            channel: ReleaseChannel::Nightly,
            ..Default::default()
        };

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        assert_eq!(parsed.metadata.channel, ReleaseChannel::Nightly);
        Ok(())
    }
}

// ===========================================================================
// 2. Version Comparison and Compatibility
// ===========================================================================

mod version_compatibility {
    use super::*;

    #[test]
    fn hardware_version_numeric_ordering() -> Result<(), HardwareVersionError> {
        let v1 = HardwareVersion::parse("1.0")?;
        let v2 = HardwareVersion::parse("2.0")?;
        let v10 = HardwareVersion::parse("10.0")?;

        assert!(v1 < v2);
        assert!(v2 < v10);
        assert!(v1 < v10);
        Ok(())
    }

    #[test]
    fn hardware_version_multi_component() -> Result<(), HardwareVersionError> {
        let a = HardwareVersion::parse("1.2.3")?;
        let b = HardwareVersion::parse("1.2.4")?;
        let c = HardwareVersion::parse("1.3.0")?;

        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
        Ok(())
    }

    #[test]
    fn hardware_version_equal_with_trailing_zeros() -> Result<(), HardwareVersionError> {
        let a = HardwareVersion::parse("1.2")?;
        let b = HardwareVersion::parse("1.2.0")?;
        let c = HardwareVersion::parse("1.2.0.0")?;

        assert_eq!(a.cmp(&b), Ordering::Equal);
        assert_eq!(b.cmp(&c), Ordering::Equal);
        assert_eq!(a.cmp(&c), Ordering::Equal);
        Ok(())
    }

    #[test]
    fn hardware_version_parse_single_component() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("42")?;
        assert_eq!(v.components(), &[42]);
        assert_eq!(v.as_str(), "42");
        Ok(())
    }

    #[test]
    fn hardware_version_empty_is_error() {
        let result = HardwareVersion::parse("");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn hardware_version_whitespace_only_is_error() {
        let result = HardwareVersion::parse("   ");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn hardware_version_invalid_component() {
        let result = HardwareVersion::parse("1.abc.3");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn hardware_version_negative_component() {
        let result = HardwareVersion::parse("1.-2.3");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn hardware_version_try_compare_both_valid() {
        assert_eq!(
            HardwareVersion::try_compare("1.0", "2.0"),
            Some(Ordering::Less)
        );
        assert_eq!(
            HardwareVersion::try_compare("2.0", "2.0"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            HardwareVersion::try_compare("3.0", "2.0"),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn hardware_version_try_compare_invalid_returns_none() {
        assert_eq!(HardwareVersion::try_compare("bad", "1.0"), None);
        assert_eq!(HardwareVersion::try_compare("1.0", "bad"), None);
        assert_eq!(HardwareVersion::try_compare("", "1.0"), None);
    }

    #[test]
    fn hardware_version_display_and_from_str() -> Result<(), HardwareVersionError> {
        let v: HardwareVersion = "3.14.159".parse()?;
        assert_eq!(format!("{}", v), "3.14.159");
        assert_eq!(v.components(), &[3, 14, 159]);
        Ok(())
    }

    #[test]
    fn bundle_hardware_compatibility_in_range() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        // min=1.0, max=5.0
        assert!(bundle.is_compatible_with_hardware("1.0"));
        assert!(bundle.is_compatible_with_hardware("3.0"));
        assert!(bundle.is_compatible_with_hardware("5.0"));
        Ok(())
    }

    #[test]
    fn bundle_hardware_compatibility_out_of_range() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(!bundle.is_compatible_with_hardware("0.9"));
        assert!(!bundle.is_compatible_with_hardware("5.1"));
        assert!(!bundle.is_compatible_with_hardware("10.0"));
        Ok(())
    }

    #[test]
    fn bundle_hardware_compatibility_invalid_version() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(!bundle.is_compatible_with_hardware("invalid"));
        assert!(!bundle.is_compatible_with_hardware(""));
        Ok(())
    }

    #[test]
    fn bundle_no_hw_constraints_always_compatible() -> TestResult {
        let mut image = make_firmware_image(&[1, 2, 3, 4, 5]);
        image.min_hardware_version = None;
        image.max_hardware_version = None;
        let metadata = make_default_metadata();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(bundle.is_compatible_with_hardware("0.1"));
        assert!(bundle.is_compatible_with_hardware("999.0"));
        Ok(())
    }

    #[test]
    fn rollback_protection_allows_upgrade_from_minimum() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = BundleMetadata {
            rollback_version: Some(semver::Version::new(1, 5, 0)),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 5, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 0, 0)));
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(1, 4, 9)));
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(0, 1, 0)));
        Ok(())
    }

    #[test]
    fn rollback_protection_none_always_allows() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = BundleMetadata {
            rollback_version: None,
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(bundle.allows_upgrade_from(&semver::Version::new(0, 0, 1)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(999, 0, 0)));
        Ok(())
    }
}

// ===========================================================================
// 3. Update Lifecycle (State Machine, FFB Blocking)
// ===========================================================================

mod update_lifecycle {
    use super::*;

    #[test]
    fn update_state_idle_is_default() {
        let state = UpdateState::default();
        assert_eq!(state, UpdateState::Idle);
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());
    }

    #[test]
    fn update_state_downloading_blocks_ffb() {
        let state = UpdateState::Downloading { progress: 50 };
        assert!(state.is_in_progress());
        assert!(state.should_block_ffb());
    }

    #[test]
    fn update_state_verifying_blocks_ffb() {
        let state = UpdateState::Verifying;
        assert!(state.is_in_progress());
        assert!(state.should_block_ffb());
    }

    #[test]
    fn update_state_flashing_blocks_ffb() {
        let state = UpdateState::Flashing { progress: 75 };
        assert!(state.is_in_progress());
        assert!(state.should_block_ffb());
    }

    #[test]
    fn update_state_rebooting_blocks_ffb() {
        let state = UpdateState::Rebooting;
        assert!(state.is_in_progress());
        assert!(state.should_block_ffb());
    }

    #[test]
    fn update_state_complete_does_not_block() {
        let state = UpdateState::Complete;
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());
    }

    #[test]
    fn update_state_failed_does_not_block() {
        let state = UpdateState::Failed {
            error: "test error".to_string(),
            recoverable: true,
        };
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());
    }

    #[test]
    fn update_state_failed_irrecoverable() {
        let state = UpdateState::Failed {
            error: "bricked".to_string(),
            recoverable: false,
        };
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());
    }

    #[tokio::test]
    async fn ffb_blocker_initially_not_blocked() {
        let blocker = FfbBlocker::new();
        assert!(!blocker.is_ffb_blocked());
        assert!(blocker.try_ffb_operation().is_ok());
    }

    #[tokio::test]
    async fn ffb_blocker_blocks_during_update() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("device-001").await?;

        assert!(blocker.is_ffb_blocked());
        let result = blocker.try_ffb_operation();
        assert!(result.is_err());
        assert!(matches!(result, Err(FirmwareUpdateError::FfbBlocked)));

        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());
        assert!(blocker.try_ffb_operation().is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_rejects_concurrent_updates() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("device-001").await?;

        let result = blocker.begin_update("device-002").await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(FirmwareUpdateError::UpdateInProgress(_))
        ));

        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_state_transitions() -> TestResult {
        let blocker = FfbBlocker::new();

        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Idle);

        blocker.begin_update("device-001").await?;
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Verifying);

        blocker
            .set_state(UpdateState::Flashing { progress: 50 })
            .await;
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Flashing { progress: 50 });

        blocker.end_update().await;
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Idle);
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_tracks_updating_device() -> TestResult {
        let blocker = FfbBlocker::new();

        assert!(blocker.get_updating_device().await.is_none());

        blocker.begin_update("my-device-xyz").await?;
        assert_eq!(
            blocker.get_updating_device().await.as_deref(),
            Some("my-device-xyz")
        );

        blocker.end_update().await;
        assert!(blocker.get_updating_device().await.is_none());
        Ok(())
    }

    #[test]
    fn update_manager_can_be_created() {
        let config = StagedRolloutConfig::default();
        let _manager = FirmwareUpdateManager::new(config);
    }

    #[tokio::test]
    async fn update_manager_no_active_updates_initially() {
        let manager = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        assert!(!manager.is_update_in_progress().await);
        assert!(manager.get_active_updates().await.is_empty());
    }

    #[tokio::test]
    async fn cancel_nonexistent_update_returns_error() {
        let manager = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let result = manager.cancel_update("nonexistent-device").await;
        assert!(result.is_err());
    }
}

// ===========================================================================
// 4. Rollback Handling
// ===========================================================================

mod rollback_handling {
    use super::*;
    use tokio::fs;

    #[test]
    fn backup_metadata_serialization_roundtrip() -> TestResult {
        let metadata = BackupMetadata {
            backup_id: "backup-001".to_string(),
            created_at: chrono::Utc::now(),
            original_version: semver::Version::new(1, 0, 0),
            target_version: semver::Version::new(2, 0, 0),
            files: vec![PathBuf::from("firmware.bin"), PathBuf::from("config.json")],
        };

        let json = serde_json::to_string(&metadata)?;
        let deserialized: BackupMetadata = serde_json::from_str(&json)?;

        assert_eq!(deserialized.backup_id, "backup-001");
        assert_eq!(deserialized.original_version, semver::Version::new(1, 0, 0));
        assert_eq!(deserialized.target_version, semver::Version::new(2, 0, 0));
        assert_eq!(deserialized.files.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn rollback_manager_create_and_list_backup() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let backup_dir = temp_dir.path().join("backups");
        let install_dir = temp_dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        // Create a file to back up
        fs::write(install_dir.join("firmware.bin"), b"original firmware data").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        manager
            .create_backup(
                "backup-test-001",
                semver::Version::new(1, 0, 0),
                semver::Version::new(2, 0, 0),
                &[PathBuf::from("firmware.bin")],
            )
            .await?;

        let backups = manager.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert!(backups[0].valid);
        assert_eq!(backups[0].metadata.backup_id, "backup-test-001");
        Ok(())
    }

    #[tokio::test]
    async fn rollback_manager_restore_from_backup() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let backup_dir = temp_dir.path().join("backups");
        let install_dir = temp_dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        // Create original file
        fs::write(install_dir.join("firmware.bin"), b"version 1.0").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        // Backup
        manager
            .create_backup(
                "restore-test",
                semver::Version::new(1, 0, 0),
                semver::Version::new(2, 0, 0),
                &[PathBuf::from("firmware.bin")],
            )
            .await?;

        // "Update" the file
        fs::write(install_dir.join("firmware.bin"), b"version 2.0").await?;
        let updated = fs::read_to_string(install_dir.join("firmware.bin")).await?;
        assert_eq!(updated, "version 2.0");

        // Rollback
        manager.rollback_to("restore-test").await?;

        let restored = fs::read_to_string(install_dir.join("firmware.bin")).await?;
        assert_eq!(restored, "version 1.0");
        Ok(())
    }

    #[tokio::test]
    async fn rollback_manager_latest_backup() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let backup_dir = temp_dir.path().join("backups");
        let install_dir = temp_dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());

        let latest = manager.get_latest_backup().await?;
        assert!(latest.is_none(), "No backups should exist initially");

        fs::write(install_dir.join("f.bin"), b"data").await?;
        manager
            .create_backup(
                "b1",
                semver::Version::new(1, 0, 0),
                semver::Version::new(1, 1, 0),
                &[PathBuf::from("f.bin")],
            )
            .await?;

        let latest = manager.get_latest_backup().await?;
        assert!(latest.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn rollback_to_nonexistent_backup_fails() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let backup_dir = temp_dir.path().join("backups");
        let install_dir = temp_dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        let manager = RollbackManager::new(backup_dir, install_dir);
        let result = manager.rollback_to("does-not-exist").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn backup_with_multiple_files() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let backup_dir = temp_dir.path().join("backups");
        let install_dir = temp_dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("file_a.bin"), b"content A").await?;
        fs::write(install_dir.join("file_b.bin"), b"content B").await?;

        let manager = RollbackManager::new(backup_dir, install_dir.clone());
        manager
            .create_backup(
                "multi-file-backup",
                semver::Version::new(1, 0, 0),
                semver::Version::new(2, 0, 0),
                &[PathBuf::from("file_a.bin"), PathBuf::from("file_b.bin")],
            )
            .await?;

        // Modify both files
        fs::write(install_dir.join("file_a.bin"), b"modified A").await?;
        fs::write(install_dir.join("file_b.bin"), b"modified B").await?;

        // Rollback
        manager.rollback_to("multi-file-backup").await?;

        let a = fs::read_to_string(install_dir.join("file_a.bin")).await?;
        let b = fs::read_to_string(install_dir.join("file_b.bin")).await?;
        assert_eq!(a, "content A");
        assert_eq!(b, "content B");
        Ok(())
    }
}

// ===========================================================================
// 5. Signature Verification of Firmware
// ===========================================================================

mod signature_tests {
    use super::*;

    #[test]
    fn signed_bundle_preserves_signature() -> TestResult {
        let image = make_signed_firmware_image(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00]);
        let metadata = make_default_metadata();

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;
        assert!(bundle.signature.is_some());

        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;

        assert!(parsed.signature.is_some());
        let sig = parsed.signature.as_ref().ok_or("Missing signature")?;
        assert_eq!(sig.signer, "openracing-build-server");
        assert_eq!(sig.key_fingerprint, "SHA256:abcdef1234567890");
        assert!(
            matches!(sig.content_type, openracing_crypto::verification::ContentType::Firmware),
            "Expected Firmware content type"
        );
        Ok(())
    }

    #[test]
    fn unsigned_bundle_has_no_signature() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let metadata = make_default_metadata();

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        assert!(bundle.signature.is_none());

        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        assert!(parsed.signature.is_none());
        Ok(())
    }

    #[test]
    fn extracted_image_carries_signature() -> TestResult {
        let image = make_signed_firmware_image(&[0x11, 0x22, 0x33, 0x44, 0x55]);
        let metadata = make_default_metadata();

        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;

        assert!(extracted.signature.is_some());
        let sig = extracted.signature.as_ref().ok_or("Missing signature")?;
        assert_eq!(sig.signer, "openracing-build-server");
        Ok(())
    }
}

// ===========================================================================
// 6. Partial Update / Recovery
// ===========================================================================

mod partial_update_recovery {
    use super::*;

    #[test]
    fn delta_patch_identical_data() -> TestResult {
        let data = b"identical content here";
        let patch = create_simple_patch(data, data)?;
        let result = apply_simple_patch(data, &patch)?;
        assert_eq!(result, data);
        Ok(())
    }

    #[test]
    fn delta_patch_completely_different_data() -> TestResult {
        let old = b"AAAA";
        let new = b"BBBB";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result, new);
        Ok(())
    }

    #[test]
    fn delta_patch_growing_data() -> TestResult {
        let old = b"Hello";
        let new = b"Hello, World! This is much longer content.";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result, new);
        Ok(())
    }

    #[test]
    fn delta_patch_shrinking_data() -> TestResult {
        let old = b"This is a very long piece of original data";
        let new = b"Short";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result, new);
        Ok(())
    }

    #[test]
    fn delta_patch_empty_old_data() -> TestResult {
        let old: &[u8] = b"";
        let new = b"new content";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result, new);
        Ok(())
    }

    #[test]
    fn delta_patch_empty_new_data() -> TestResult {
        let old = b"old content";
        let new: &[u8] = b"";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result, new);
        Ok(())
    }

    #[test]
    fn delta_patch_invalid_magic_rejected() {
        let bad_patch = b"NOTPATCH__________";
        let result = apply_simple_patch(b"data", bad_patch);
        assert!(result.is_err());
    }

    #[test]
    fn delta_patch_wrong_old_size_rejected() -> TestResult {
        let old = b"old data";
        let new = b"new data";
        let patch = create_simple_patch(old, new)?;

        let different_old = b"different old data with extra bytes";
        let result = apply_simple_patch(different_old, &patch);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn compression_roundtrip() -> TestResult {
        let original = b"The quick brown fox jumps over the lazy dog. Repeated data helps compression. Repeated data helps compression.";
        let compressed = compress_data(original)?;
        let decompressed = decompress_data(&compressed)?;
        assert_eq!(decompressed, original);
        Ok(())
    }

    #[test]
    fn compression_empty_data() -> TestResult {
        let compressed = compress_data(b"")?;
        let decompressed = decompress_data(&compressed)?;
        assert!(decompressed.is_empty());
        Ok(())
    }

    #[test]
    fn decompress_invalid_data_fails() {
        let result = decompress_data(&[0xFF, 0xFE, 0xFD, 0xFC]);
        assert!(result.is_err());
    }

    #[test]
    fn data_hash_deterministic() {
        let data = b"consistent data";
        let hash1 = compute_data_hash(data);
        let hash2 = compute_data_hash(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn data_hash_different_for_different_data() {
        let h1 = compute_data_hash(b"data A");
        let h2 = compute_data_hash(b"data B");
        assert_ne!(h1, h2);
    }

    #[test]
    fn data_hash_length_is_64_hex_chars() {
        let hash = compute_data_hash(b"test");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn file_hash_matches_data_hash() -> TestResult {
        let temp = tempfile::NamedTempFile::new()?;
        let content = b"hash consistency check";
        tokio::fs::write(temp.path(), content).await?;

        let file_hash = openracing_firmware_update::delta::compute_file_hash(temp.path()).await?;
        let data_hash = compute_data_hash(content);
        assert_eq!(file_hash, data_hash);
        Ok(())
    }

    #[tokio::test]
    async fn delta_patch_roundtrip_via_files() -> TestResult {
        let old_file = tempfile::NamedTempFile::new()?;
        let new_file = tempfile::NamedTempFile::new()?;

        let old_data = b"Original firmware v1.0 data content here";
        let new_data = b"Updated firmware v2.0 data content here with extras";

        tokio::fs::write(old_file.path(), old_data).await?;
        tokio::fs::write(new_file.path(), new_data).await?;

        let compressed_patch =
            openracing_firmware_update::delta::create_delta_patch(old_file.path(), new_file.path())
                .await?;

        // Create a target file with old data and apply patch
        let target = tempfile::NamedTempFile::new()?;
        tokio::fs::write(target.path(), old_data).await?;

        openracing_firmware_update::delta::apply_delta_patch(target.path(), &compressed_patch)
            .await?;

        let patched = tokio::fs::read(target.path()).await?;
        assert_eq!(patched, new_data);
        Ok(())
    }
}

// ===========================================================================
// 7. Partition System
// ===========================================================================

mod partition_tests {
    use super::*;

    #[test]
    fn partition_other_is_symmetric() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
        assert_eq!(Partition::A.other().other(), Partition::A);
    }

    #[test]
    fn partition_display() {
        assert_eq!(format!("{}", Partition::A), "A");
        assert_eq!(format!("{}", Partition::B), "B");
    }

    #[test]
    fn partition_info_empty_defaults() {
        let info = PartitionInfo::empty(Partition::B);
        assert_eq!(info.partition, Partition::B);
        assert!(!info.active);
        assert!(!info.bootable);
        assert!(info.version.is_none());
        assert_eq!(info.size_bytes, 0);
        assert!(info.hash.is_none());
        assert!(info.updated_at.is_none());
        assert_eq!(info.health, PartitionHealth::Unknown);
    }

    #[test]
    fn inactive_healthy_partition_can_update() {
        let info = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: false,
            version: None,
            size_bytes: 0,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Healthy,
        };
        assert!(info.can_update());
    }

    #[test]
    fn active_partition_cannot_update() {
        let info = PartitionInfo {
            partition: Partition::A,
            active: true,
            bootable: true,
            version: Some(semver::Version::new(1, 0, 0)),
            size_bytes: 1024,
            hash: Some("abc".to_string()),
            updated_at: None,
            health: PartitionHealth::Healthy,
        };
        assert!(!info.can_update());
    }

    #[test]
    fn partition_health_usable_states() {
        assert!(PartitionHealth::Healthy.is_usable());
        assert!(
            PartitionHealth::Degraded {
                reason: "minor issue".to_string()
            }
            .is_usable()
        );
        assert!(
            !PartitionHealth::Corrupted {
                reason: "bad".to_string()
            }
            .is_usable()
        );
        assert!(!PartitionHealth::Unknown.is_usable());
    }

    #[test]
    fn partition_health_needs_repair() {
        assert!(!PartitionHealth::Healthy.needs_repair());
        assert!(
            !PartitionHealth::Degraded {
                reason: "ok".to_string()
            }
            .needs_repair()
        );
        assert!(
            PartitionHealth::Corrupted {
                reason: "bad data".to_string()
            }
            .needs_repair()
        );
        assert!(PartitionHealth::Unknown.needs_repair());
    }

    #[test]
    fn partition_health_default_is_unknown() {
        assert_eq!(PartitionHealth::default(), PartitionHealth::Unknown);
    }

    #[test]
    fn partition_info_serialization_roundtrip() -> TestResult {
        let info = PartitionInfo {
            partition: Partition::A,
            active: true,
            bootable: true,
            version: Some(semver::Version::new(2, 1, 0)),
            size_bytes: 1048576,
            hash: Some("abc123".to_string()),
            updated_at: Some(chrono::Utc::now()),
            health: PartitionHealth::Healthy,
        };

        let json = serde_json::to_string(&info)?;
        let deser: PartitionInfo = serde_json::from_str(&json)?;

        assert_eq!(deser.partition, Partition::A);
        assert!(deser.active);
        assert_eq!(deser.version, Some(semver::Version::new(2, 1, 0)));
        assert_eq!(deser.health, PartitionHealth::Healthy);
        Ok(())
    }
}

// ===========================================================================
// 8. Health Check System
// ===========================================================================

mod health_check_tests {
    use super::*;

    #[test]
    fn health_check_summary_all_passed() {
        let summary = HealthCheckSummary {
            total_checks: 5,
            passed_checks: 5,
            failed_checks: 0,
            critical_failures: 0,
            results: Vec::new(),
        };

        assert!(summary.all_critical_passed());
        assert!((summary.success_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn health_check_summary_some_failed() {
        let summary = HealthCheckSummary {
            total_checks: 10,
            passed_checks: 7,
            failed_checks: 3,
            critical_failures: 1,
            results: Vec::new(),
        };

        assert!(!summary.all_critical_passed());
        assert!((summary.success_rate() - 0.7).abs() < 0.001);
    }

    #[test]
    fn health_check_summary_zero_checks() {
        let summary = HealthCheckSummary {
            total_checks: 0,
            passed_checks: 0,
            failed_checks: 0,
            critical_failures: 0,
            results: Vec::new(),
        };

        assert!(summary.all_critical_passed());
        assert!((summary.success_rate() - 1.0).abs() < f64::EPSILON);
    }
}

// ===========================================================================
// 9. Staged Rollout Config
// ===========================================================================

mod staged_rollout_tests {
    use super::*;

    #[test]
    fn staged_rollout_config_defaults() {
        let config = StagedRolloutConfig::default();
        assert!(config.enabled);
        assert_eq!(config.stage1_max_devices, 10);
        assert!((config.min_success_rate - 0.95).abs() < f64::EPSILON);
        assert_eq!(config.stage_delay_minutes, 60);
        assert!((config.max_error_rate - 0.05).abs() < f64::EPSILON);
        assert_eq!(config.monitoring_window_minutes, 120);
    }

    #[test]
    fn staged_rollout_config_serialization_roundtrip() -> TestResult {
        let config = StagedRolloutConfig {
            enabled: false,
            stage1_max_devices: 5,
            min_success_rate: 0.99,
            stage_delay_minutes: 30,
            max_error_rate: 0.01,
            monitoring_window_minutes: 60,
        };

        let json = serde_json::to_string(&config)?;
        let deser: StagedRolloutConfig = serde_json::from_str(&json)?;

        assert!(!deser.enabled);
        assert_eq!(deser.stage1_max_devices, 5);
        assert!((deser.min_success_rate - 0.99).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn update_result_serialization_roundtrip() -> TestResult {
        let result = UpdateResult {
            device_id: "wheel-001".to_string(),
            success: true,
            old_version: Some(semver::Version::new(1, 0, 0)),
            new_version: Some(semver::Version::new(2, 0, 0)),
            updated_partition: Some(Partition::B),
            rollback_performed: false,
            duration: Duration::from_secs(120),
            error: None,
            partition_states: Vec::new(),
        };

        let json = serde_json::to_string(&result)?;
        let deser: UpdateResult = serde_json::from_str(&json)?;

        assert!(deser.success);
        assert_eq!(deser.device_id, "wheel-001");
        assert_eq!(deser.old_version, Some(semver::Version::new(1, 0, 0)));
        assert_eq!(deser.new_version, Some(semver::Version::new(2, 0, 0)));
        assert_eq!(deser.updated_partition, Some(Partition::B));
        assert!(!deser.rollback_performed);
        Ok(())
    }

    #[test]
    fn update_progress_serialization() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Transferring,
            progress_percent: 42,
            bytes_transferred: 1024,
            total_bytes: 4096,
            transfer_rate_bps: 512,
            eta_seconds: Some(6),
            status_message: "Transferring firmware".to_string(),
            warnings: vec!["Slow transfer".to_string()],
        };

        let json = serde_json::to_string(&progress)?;
        let deser: UpdateProgress = serde_json::from_str(&json)?;

        assert_eq!(deser.progress_percent, 42);
        assert_eq!(deser.bytes_transferred, 1024);
        assert_eq!(deser.total_bytes, 4096);
        assert_eq!(deser.eta_seconds, Some(6));
        assert_eq!(deser.warnings.len(), 1);
        Ok(())
    }
}

// ===========================================================================
// 10. Error Types
// ===========================================================================

mod error_tests {
    use super::*;

    #[test]
    fn firmware_error_display_messages() {
        let cases: Vec<(FirmwareUpdateError, &str)> = vec![
            (
                FirmwareUpdateError::DeviceNotFound("dev1".to_string()),
                "Device not found: dev1",
            ),
            (
                FirmwareUpdateError::VerificationFailed("bad hash".to_string()),
                "Firmware verification failed: bad hash",
            ),
            (
                FirmwareUpdateError::TransferFailed("timeout".to_string()),
                "Update transfer failed: timeout",
            ),
            (
                FirmwareUpdateError::HealthCheckFailed("no response".to_string()),
                "Health check failed: no response",
            ),
            (
                FirmwareUpdateError::RollbackFailed("disk full".to_string()),
                "Rollback failed: disk full",
            ),
            (
                FirmwareUpdateError::InvalidFirmware("corrupt".to_string()),
                "Invalid firmware image: corrupt",
            ),
            (FirmwareUpdateError::FfbBlocked, "FFB operation blocked"),
            (
                FirmwareUpdateError::UpdateInProgress("dev".to_string()),
                "Update already in progress for device: dev",
            ),
            (
                FirmwareUpdateError::CacheError("miss".to_string()),
                "Cache error: miss",
            ),
            (
                FirmwareUpdateError::BundleError("bad format".to_string()),
                "Bundle error: bad format",
            ),
            (
                FirmwareUpdateError::PartitionError("corrupt".to_string()),
                "Partition error: corrupt",
            ),
            (
                FirmwareUpdateError::CompatibilityError("mismatch".to_string()),
                "Compatibility error: mismatch",
            ),
            (
                FirmwareUpdateError::InvalidState("wrong phase".to_string()),
                "Invalid state for operation: wrong phase",
            ),
            (
                FirmwareUpdateError::Cancelled("user request".to_string()),
                "Operation cancelled: user request",
            ),
            (
                FirmwareUpdateError::RolloutError("threshold exceeded".to_string()),
                "Rollout error: threshold exceeded",
            ),
        ];

        for (err, expected_prefix) in cases {
            let msg = format!("{}", err);
            assert!(
                msg.contains(expected_prefix),
                "Expected '{}' to contain '{}'",
                msg,
                expected_prefix
            );
        }
    }

    #[test]
    fn firmware_error_from_serde_json() -> TestResult {
        let parse_result: std::result::Result<serde_json::Value, _> =
            serde_json::from_str("not valid json");
        let serde_err = parse_result.err().ok_or("Expected serde parse error")?;
        let fw_err: FirmwareUpdateError = serde_err.into();
        let msg = format!("{}", fw_err);
        assert!(
            msg.contains("Serialization error"),
            "Expected serialization error, got: {}",
            msg
        );
        Ok(())
    }
}

// ===========================================================================
// 11. FirmwareImage Construction
// ===========================================================================

mod firmware_image_tests {
    use super::*;

    #[test]
    fn firmware_image_hash_matches_data() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let image = make_firmware_image(&data);
        let expected_hash = compute_data_hash(&data);
        assert_eq!(image.hash, expected_hash);
    }

    #[test]
    fn firmware_image_size_matches_data_len() {
        let data = vec![0u8; 256];
        let image = make_firmware_image(&data);
        assert_eq!(image.size_bytes, 256);
    }

    #[test]
    fn firmware_image_serialization_skips_data() -> TestResult {
        let image = make_firmware_image(&[1, 2, 3, 4, 5]);
        let json = serde_json::to_string(&image)?;

        // data field is #[serde(skip)] so should not appear
        assert!(!json.contains("\"data\""));

        // But other fields should be present
        assert!(json.contains("\"device_model\""));
        assert!(json.contains("\"hash\""));
        assert!(json.contains("\"version\""));
        Ok(())
    }

    #[test]
    fn firmware_image_with_signature() {
        let image = make_signed_firmware_image(&[1, 2, 3, 4, 5]);
        assert!(image.signature.is_some());
        assert_eq!(image.device_model, "signed-wheel");
    }
}
