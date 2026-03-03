//! Deep tests for the firmware update subsystem.
//!
//! Covers version parsing/comparison, bundle format validation, hardware
//! compatibility matrix, update state machine transitions, rollback on failure,
//! progress reporting, and property-based version ordering.

use std::cmp::Ordering;
use std::time::Duration;

use openracing_firmware_update::bundle::{
    BUNDLE_FORMAT_VERSION, BundleMetadata, CompressionType, FirmwareBundle, OWFB_MAGIC,
    ReleaseChannel,
};
use openracing_firmware_update::hardware_version::{HardwareVersion, HardwareVersionError};
use openracing_firmware_update::manager::{
    FfbBlocker, FirmwareImage, StagedRolloutConfig, UpdatePhase, UpdateProgress, UpdateResult,
    UpdateState,
};
use openracing_firmware_update::partition::{Partition, PartitionHealth, PartitionInfo};
use openracing_firmware_update::rollback::BackupMetadata;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_firmware_image(data: &[u8]) -> FirmwareImage {
    let hash = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(data);
        hex::encode(h.finalize())
    };
    FirmwareImage {
        device_model: "test-wheel-v2".to_string(),
        version: semver::Version::new(2, 1, 0),
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: Some("3.0".to_string()),
        data: data.to_vec(),
        hash,
        size_bytes: data.len() as u64,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some("Deep-test release".to_string()),
        signature: None,
    }
}

fn make_bundle(data: &[u8], compression: CompressionType) -> Result<FirmwareBundle, anyhow::Error> {
    let image = test_firmware_image(data);
    let metadata = BundleMetadata::default();
    FirmwareBundle::new(&image, metadata, compression)
}

// ===========================================================================
// Firmware version parsing and comparison
// ===========================================================================

mod version_parsing {
    use super::*;

    #[test]
    fn parse_single_component() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("5")?;
        assert_eq!(v.components(), &[5]);
        assert_eq!(v.as_str(), "5");
        Ok(())
    }

    #[test]
    fn parse_two_components() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("3.14")?;
        assert_eq!(v.components(), &[3, 14]);
        Ok(())
    }

    #[test]
    fn parse_four_components() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2.3.4")?;
        assert_eq!(v.components(), &[1, 2, 3, 4]);
        Ok(())
    }

    #[test]
    fn parse_with_leading_whitespace() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("  2.0  ")?;
        assert_eq!(v.components(), &[2, 0]);
        Ok(())
    }

    #[test]
    fn parse_empty_string_fails() {
        let result = HardwareVersion::parse("");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn parse_whitespace_only_fails() {
        let result = HardwareVersion::parse("   ");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn parse_non_numeric_fails() {
        let result = HardwareVersion::parse("abc");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn parse_mixed_valid_invalid_fails() {
        let result = HardwareVersion::parse("1.two.3");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn parse_trailing_dot_fails() {
        let result = HardwareVersion::parse("1.2.");
        assert!(result.is_err());
    }

    #[test]
    fn parse_leading_dot_fails() {
        let result = HardwareVersion::parse(".1.2");
        assert!(result.is_err());
    }

    #[test]
    fn parse_negative_number_fails() {
        let result = HardwareVersion::parse("-1.0");
        assert!(result.is_err());
    }

    #[test]
    fn numeric_comparison_avoids_lexicographic_bug() -> Result<(), HardwareVersionError> {
        let v2 = HardwareVersion::parse("2.0")?;
        let v10 = HardwareVersion::parse("10.0")?;
        assert!(v2 < v10);
        assert!(v10 > v2);
        Ok(())
    }

    #[test]
    fn equal_versions_with_trailing_zeros() -> Result<(), HardwareVersionError> {
        let a = HardwareVersion::parse("1.2")?;
        let b = HardwareVersion::parse("1.2.0")?;
        assert_eq!(a.cmp(&b), Ordering::Equal);
        Ok(())
    }

    #[test]
    fn ordering_respects_minor_component() -> Result<(), HardwareVersionError> {
        let a = HardwareVersion::parse("1.9")?;
        let b = HardwareVersion::parse("1.10")?;
        assert!(a < b);
        Ok(())
    }

    #[test]
    fn try_compare_valid_returns_some() {
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
    fn try_compare_invalid_returns_none() {
        assert_eq!(HardwareVersion::try_compare("bad", "1.0"), None);
        assert_eq!(HardwareVersion::try_compare("1.0", "bad"), None);
        assert_eq!(HardwareVersion::try_compare("", ""), None);
    }

    #[test]
    fn display_preserves_original_string() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2.3")?;
        assert_eq!(format!("{v}"), "1.2.3");
        Ok(())
    }

    #[test]
    fn from_str_works() -> Result<(), HardwareVersionError> {
        let v: HardwareVersion = "4.5.6".parse()?;
        assert_eq!(v.components(), &[4, 5, 6]);
        Ok(())
    }

    #[test]
    fn semver_version_comparison() {
        let a = semver::Version::new(1, 0, 0);
        let b = semver::Version::new(1, 1, 0);
        let c = semver::Version::new(2, 0, 0);
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }
}

// ===========================================================================
// Firmware file format validation (header, checksum)
// ===========================================================================

mod bundle_format {
    use super::*;

    #[test]
    fn bundle_round_trip_no_compression() -> TestResult {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x42];
        let bundle = make_bundle(&data, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, data);
        assert_eq!(parsed.header.format_version, BUNDLE_FORMAT_VERSION);
        assert_eq!(parsed.header.compression, CompressionType::None);
        Ok(())
    }

    #[test]
    fn bundle_round_trip_gzip_compression() -> TestResult {
        let data = vec![0x01; 1024];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, data);
        assert_eq!(parsed.header.compression, CompressionType::Gzip);
        // Gzip should compress repeated bytes
        assert!(bundle.header.compressed_size <= bundle.header.uncompressed_size);
        Ok(())
    }

    #[test]
    fn bundle_file_write_and_load() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("firmware.owfb");
        let data = vec![0xAB; 256];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        bundle.write(&path)?;
        assert!(path.exists());
        let loaded = FirmwareBundle::load(&path)?;
        assert_eq!(loaded.header.device_model, "test-wheel-v2");
        assert_eq!(
            loaded.header.firmware_version,
            semver::Version::new(2, 1, 0)
        );
        Ok(())
    }

    #[test]
    fn invalid_magic_bytes_rejected() {
        let result = FirmwareBundle::parse(b"NOT_OWFB_DATA");
        assert!(result.is_err());
    }

    #[test]
    fn truncated_data_rejected() {
        let result = FirmwareBundle::parse(OWFB_MAGIC);
        assert!(result.is_err());
    }

    #[test]
    fn corrupted_payload_detected_via_hash() -> TestResult {
        let data = vec![0x01, 0x02, 0x03];
        let bundle = make_bundle(&data, CompressionType::None)?;
        let mut serialized = bundle.serialize()?;
        // Corrupt the last payload byte
        if let Some(last) = serialized.last_mut() {
            *last ^= 0xFF;
        }
        let result = FirmwareBundle::parse(&serialized);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn bundle_header_contains_correct_sizes() -> TestResult {
        let data = vec![0xFF; 512];
        let bundle = make_bundle(&data, CompressionType::None)?;
        assert_eq!(bundle.header.uncompressed_size, 512);
        assert_eq!(bundle.header.compressed_size, 512); // No compression
        Ok(())
    }

    #[test]
    fn bundle_header_contains_correct_hash() -> TestResult {
        let data = vec![0x42; 10];
        let bundle = make_bundle(&data, CompressionType::None)?;
        let expected_hash = openracing_crypto::utils::compute_sha256_hex(&data);
        assert_eq!(bundle.header.payload_hash, expected_hash);
        Ok(())
    }

    #[test]
    fn bundle_metadata_defaults_to_stable_channel() -> TestResult {
        let metadata = BundleMetadata::default();
        assert_eq!(metadata.channel, ReleaseChannel::Stable);
        assert!(metadata.title.is_none());
        assert!(metadata.changelog.is_none());
        assert!(metadata.rollback_version.is_none());
        Ok(())
    }

    #[test]
    fn bundle_size_matches_serialized_length() -> TestResult {
        let data = vec![0x01; 100];
        let bundle = make_bundle(&data, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        assert_eq!(bundle.bundle_size(), serialized.len());
        Ok(())
    }

    #[test]
    fn release_channel_serde_round_trip() -> TestResult {
        for channel in [
            ReleaseChannel::Stable,
            ReleaseChannel::Beta,
            ReleaseChannel::Nightly,
        ] {
            let json = serde_json::to_string(&channel)?;
            let decoded: ReleaseChannel = serde_json::from_str(&json)?;
            assert_eq!(decoded, channel);
        }
        Ok(())
    }

    #[test]
    fn compression_type_serde_round_trip() -> TestResult {
        for ct in [CompressionType::None, CompressionType::Gzip] {
            let json = serde_json::to_string(&ct)?;
            let decoded: CompressionType = serde_json::from_str(&json)?;
            assert_eq!(decoded, ct);
        }
        Ok(())
    }
}

// ===========================================================================
// Firmware compatibility checking (device → firmware version matrix)
// ===========================================================================

mod compatibility {
    use super::*;

    fn bundle_with_hw_range(
        min: Option<&str>,
        max: Option<&str>,
    ) -> Result<FirmwareBundle, anyhow::Error> {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(1, 0, 0),
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
    fn compatible_within_range() -> TestResult {
        let bundle = bundle_with_hw_range(Some("1.0"), Some("3.0"))?;
        assert!(bundle.is_compatible_with_hardware("1.0"));
        assert!(bundle.is_compatible_with_hardware("2.0"));
        assert!(bundle.is_compatible_with_hardware("3.0"));
        Ok(())
    }

    #[test]
    fn incompatible_below_minimum() -> TestResult {
        let bundle = bundle_with_hw_range(Some("2.0"), Some("5.0"))?;
        assert!(!bundle.is_compatible_with_hardware("1.9"));
        assert!(!bundle.is_compatible_with_hardware("0.1"));
        Ok(())
    }

    #[test]
    fn incompatible_above_maximum() -> TestResult {
        let bundle = bundle_with_hw_range(Some("1.0"), Some("3.0"))?;
        assert!(!bundle.is_compatible_with_hardware("3.1"));
        assert!(!bundle.is_compatible_with_hardware("10.0"));
        Ok(())
    }

    #[test]
    fn no_hw_constraints_always_compatible() -> TestResult {
        let bundle = bundle_with_hw_range(None, None)?;
        assert!(bundle.is_compatible_with_hardware("0.1"));
        assert!(bundle.is_compatible_with_hardware("999.0"));
        Ok(())
    }

    #[test]
    fn only_min_constraint() -> TestResult {
        let bundle = bundle_with_hw_range(Some("2.0"), None)?;
        assert!(!bundle.is_compatible_with_hardware("1.0"));
        assert!(bundle.is_compatible_with_hardware("2.0"));
        assert!(bundle.is_compatible_with_hardware("100.0"));
        Ok(())
    }

    #[test]
    fn only_max_constraint() -> TestResult {
        let bundle = bundle_with_hw_range(None, Some("5.0"))?;
        assert!(bundle.is_compatible_with_hardware("0.1"));
        assert!(bundle.is_compatible_with_hardware("5.0"));
        assert!(!bundle.is_compatible_with_hardware("5.1"));
        Ok(())
    }

    #[test]
    fn invalid_hw_version_fails_closed() -> TestResult {
        let bundle = bundle_with_hw_range(Some("1.0"), Some("3.0"))?;
        assert!(!bundle.is_compatible_with_hardware("invalid"));
        assert!(!bundle.is_compatible_with_hardware(""));
        Ok(())
    }

    #[test]
    fn numeric_comparison_10_vs_2() -> TestResult {
        let bundle = bundle_with_hw_range(Some("2.0"), Some("10.0"))?;
        assert!(bundle.is_compatible_with_hardware("5.0"));
        assert!(bundle.is_compatible_with_hardware("10.0"));
        assert!(!bundle.is_compatible_with_hardware("1.0"));
        assert!(!bundle.is_compatible_with_hardware("11.0"));
        Ok(())
    }

    #[test]
    fn rollback_protection_allows_upgrade_from_same_version() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(2, 0, 0),
            min_hardware_version: None,
            max_hardware_version: None,
            data,
            hash,
            size_bytes: 8,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };
        let metadata = BundleMetadata {
            rollback_version: Some(semver::Version::new(1, 5, 0)),
            ..BundleMetadata::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        assert!(bundle.allows_upgrade_from(&semver::Version::new(1, 5, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 0, 0)));
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(1, 4, 9)));
        Ok(())
    }

    #[test]
    fn no_rollback_version_allows_any_upgrade() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(2, 0, 0),
            min_hardware_version: None,
            max_hardware_version: None,
            data,
            hash,
            size_bytes: 8,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;
        assert!(bundle.allows_upgrade_from(&semver::Version::new(0, 0, 1)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(99, 0, 0)));
        Ok(())
    }
}

// ===========================================================================
// Update state machine transitions
// ===========================================================================

mod state_machine {
    use super::*;

    #[test]
    fn idle_is_not_in_progress() {
        assert!(!UpdateState::Idle.is_in_progress());
    }

    #[test]
    fn complete_is_not_in_progress() {
        assert!(!UpdateState::Complete.is_in_progress());
    }

    #[test]
    fn failed_is_not_in_progress() {
        let state = UpdateState::Failed {
            error: "oops".to_string(),
            recoverable: true,
        };
        assert!(!state.is_in_progress());
    }

    #[test]
    fn downloading_is_in_progress() {
        assert!(UpdateState::Downloading { progress: 0 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 50 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 100 }.is_in_progress());
    }

    #[test]
    fn verifying_is_in_progress() {
        assert!(UpdateState::Verifying.is_in_progress());
    }

    #[test]
    fn flashing_is_in_progress() {
        assert!(UpdateState::Flashing { progress: 0 }.is_in_progress());
        assert!(UpdateState::Flashing { progress: 99 }.is_in_progress());
    }

    #[test]
    fn rebooting_is_in_progress() {
        assert!(UpdateState::Rebooting.is_in_progress());
    }

    #[test]
    fn idle_does_not_block_ffb() {
        assert!(!UpdateState::Idle.should_block_ffb());
    }

    #[test]
    fn complete_does_not_block_ffb() {
        assert!(!UpdateState::Complete.should_block_ffb());
    }

    #[test]
    fn failed_does_not_block_ffb() {
        let state = UpdateState::Failed {
            error: "err".to_string(),
            recoverable: false,
        };
        assert!(!state.should_block_ffb());
    }

    #[test]
    fn in_progress_states_block_ffb() {
        assert!(UpdateState::Downloading { progress: 50 }.should_block_ffb());
        assert!(UpdateState::Verifying.should_block_ffb());
        assert!(UpdateState::Flashing { progress: 75 }.should_block_ffb());
        assert!(UpdateState::Rebooting.should_block_ffb());
    }

    #[test]
    fn default_state_is_idle() {
        assert_eq!(UpdateState::default(), UpdateState::Idle);
    }

    #[test]
    fn update_state_serde_round_trip_all_variants() -> TestResult {
        let variants: Vec<UpdateState> = vec![
            UpdateState::Idle,
            UpdateState::Downloading { progress: 42 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 80 },
            UpdateState::Rebooting,
            UpdateState::Complete,
            UpdateState::Failed {
                error: "test error".to_string(),
                recoverable: true,
            },
            UpdateState::Failed {
                error: "fatal".to_string(),
                recoverable: false,
            },
        ];
        for state in &variants {
            let json = serde_json::to_string(state)?;
            let decoded: UpdateState = serde_json::from_str(&json)?;
            assert_eq!(&decoded, state);
        }
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_blocks_during_update() -> TestResult {
        let blocker = FfbBlocker::new();
        assert!(!blocker.is_ffb_blocked());
        blocker.try_ffb_operation()?; // Should succeed

        blocker.begin_update("device-001").await?;
        assert!(blocker.is_ffb_blocked());
        let err = blocker.try_ffb_operation();
        assert!(err.is_err());

        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());
        blocker.try_ffb_operation()?; // Should succeed again
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_rejects_concurrent_updates() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("device-A").await?;
        let result = blocker.begin_update("device-B").await;
        assert!(result.is_err());
        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_state_transitions() -> TestResult {
        let blocker = FfbBlocker::new();
        let state = blocker.get_state().await;
        assert_eq!(state, UpdateState::Idle);

        blocker.begin_update("dev-1").await?;
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

        blocker.begin_update("my-wheel").await?;
        assert_eq!(
            blocker.get_updating_device().await,
            Some("my-wheel".to_string())
        );

        blocker.end_update().await;
        assert!(blocker.get_updating_device().await.is_none());
        Ok(())
    }
}

// ===========================================================================
// Rollback on failure
// ===========================================================================

mod rollback {
    use super::*;
    use openracing_firmware_update::rollback::RollbackManager;
    use std::path::PathBuf;
    use tokio::fs;

    #[tokio::test]
    async fn create_and_verify_backup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        // Create a file to back up
        fs::write(install_dir.join("firmware.bin"), b"original content").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        manager
            .create_backup(
                "bak-001",
                semver::Version::new(1, 0, 0),
                semver::Version::new(1, 1, 0),
                &[PathBuf::from("firmware.bin")],
            )
            .await?;

        let backups = manager.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert!(backups[0].valid);
        assert_eq!(backups[0].metadata.backup_id, "bak-001");
        assert_eq!(
            backups[0].metadata.original_version,
            semver::Version::new(1, 0, 0)
        );
        Ok(())
    }

    #[tokio::test]
    async fn rollback_restores_original_file() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("firmware.bin"), b"original").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        manager
            .create_backup(
                "bak-002",
                semver::Version::new(1, 0, 0),
                semver::Version::new(2, 0, 0),
                &[PathBuf::from("firmware.bin")],
            )
            .await?;

        // Simulate update overwriting file
        fs::write(install_dir.join("firmware.bin"), b"updated-bad").await?;

        // Perform rollback
        manager.rollback_to("bak-002").await?;

        let content = fs::read_to_string(install_dir.join("firmware.bin")).await?;
        assert_eq!(content, "original");
        Ok(())
    }

    #[tokio::test]
    async fn rollback_to_nonexistent_backup_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        let manager = RollbackManager::new(backup_dir, install_dir);
        let result = manager.rollback_to("nonexistent").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn get_latest_backup_returns_most_recent() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("fw.bin"), b"content").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        manager
            .create_backup(
                "bak-a",
                semver::Version::new(1, 0, 0),
                semver::Version::new(1, 1, 0),
                &[PathBuf::from("fw.bin")],
            )
            .await?;

        // Small delay so timestamps differ
        tokio::time::sleep(Duration::from_millis(50)).await;

        manager
            .create_backup(
                "bak-b",
                semver::Version::new(1, 1, 0),
                semver::Version::new(1, 2, 0),
                &[PathBuf::from("fw.bin")],
            )
            .await?;

        let latest = manager.get_latest_backup().await?;
        assert!(latest.is_some());
        let latest = latest.ok_or("no latest backup")?;
        assert_eq!(latest.backup_id, "bak-b");
        Ok(())
    }

    #[test]
    fn backup_metadata_serde_round_trip() -> TestResult {
        let meta = BackupMetadata {
            backup_id: "test-bak".to_string(),
            created_at: chrono::Utc::now(),
            original_version: semver::Version::new(1, 0, 0),
            target_version: semver::Version::new(2, 0, 0),
            files: vec![PathBuf::from("a.bin"), PathBuf::from("b.bin")],
        };
        let json = serde_json::to_string(&meta)?;
        let decoded: BackupMetadata = serde_json::from_str(&json)?;
        assert_eq!(decoded.backup_id, meta.backup_id);
        assert_eq!(decoded.original_version, meta.original_version);
        assert_eq!(decoded.target_version, meta.target_version);
        assert_eq!(decoded.files, meta.files);
        Ok(())
    }
}

// ===========================================================================
// Progress reporting
// ===========================================================================

mod progress_reporting {
    use super::*;

    #[test]
    fn update_progress_serde_round_trip() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Transferring,
            progress_percent: 55,
            bytes_transferred: 1024,
            total_bytes: 2048,
            transfer_rate_bps: 512,
            eta_seconds: Some(2),
            status_message: "Transferring...".to_string(),
            warnings: vec!["slow connection".to_string()],
        };
        let json = serde_json::to_string(&progress)?;
        let decoded: UpdateProgress = serde_json::from_str(&json)?;
        assert_eq!(decoded.progress_percent, 55);
        assert_eq!(decoded.bytes_transferred, 1024);
        assert_eq!(decoded.total_bytes, 2048);
        assert_eq!(decoded.warnings.len(), 1);
        Ok(())
    }

    #[test]
    fn update_result_success_serde_round_trip() -> TestResult {
        let result = UpdateResult {
            device_id: "wheel-001".to_string(),
            success: true,
            old_version: Some(semver::Version::new(1, 0, 0)),
            new_version: Some(semver::Version::new(2, 0, 0)),
            updated_partition: Some(Partition::B),
            rollback_performed: false,
            duration: Duration::from_secs(120),
            error: None,
            partition_states: vec![PartitionInfo::empty(Partition::A)],
        };
        let json = serde_json::to_string(&result)?;
        let decoded: UpdateResult = serde_json::from_str(&json)?;
        assert!(decoded.success);
        assert_eq!(decoded.device_id, "wheel-001");
        assert_eq!(decoded.old_version, Some(semver::Version::new(1, 0, 0)));
        assert_eq!(decoded.new_version, Some(semver::Version::new(2, 0, 0)));
        assert!(!decoded.rollback_performed);
        Ok(())
    }

    #[test]
    fn update_result_failure_serde_round_trip() -> TestResult {
        let result = UpdateResult {
            device_id: "wheel-002".to_string(),
            success: false,
            old_version: None,
            new_version: None,
            updated_partition: None,
            rollback_performed: true,
            duration: Duration::from_secs(30),
            error: Some("Device communication lost".to_string()),
            partition_states: vec![],
        };
        let json = serde_json::to_string(&result)?;
        let decoded: UpdateResult = serde_json::from_str(&json)?;
        assert!(!decoded.success);
        assert!(decoded.rollback_performed);
        assert!(decoded.error.is_some());
        Ok(())
    }

    #[test]
    fn partition_info_serde_round_trip() -> TestResult {
        let info = PartitionInfo {
            partition: Partition::A,
            active: true,
            bootable: true,
            version: Some(semver::Version::new(1, 5, 0)),
            size_bytes: 1024 * 1024,
            hash: Some("abc123".to_string()),
            updated_at: Some(chrono::Utc::now()),
            health: PartitionHealth::Healthy,
        };
        let json = serde_json::to_string(&info)?;
        let decoded: PartitionInfo = serde_json::from_str(&json)?;
        assert_eq!(decoded.partition, Partition::A);
        assert!(decoded.active);
        assert!(decoded.bootable);
        assert_eq!(decoded.version, Some(semver::Version::new(1, 5, 0)));
        Ok(())
    }

    #[test]
    fn staged_rollout_config_default_values() {
        let cfg = StagedRolloutConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.stage1_max_devices, 10);
        assert!((cfg.min_success_rate - 0.95).abs() < f64::EPSILON);
        assert_eq!(cfg.stage_delay_minutes, 60);
        assert!((cfg.max_error_rate - 0.05).abs() < f64::EPSILON);
        assert_eq!(cfg.monitoring_window_minutes, 120);
    }

    #[test]
    fn staged_rollout_config_serde_round_trip() -> TestResult {
        let cfg = StagedRolloutConfig {
            enabled: false,
            stage1_max_devices: 5,
            min_success_rate: 0.99,
            stage_delay_minutes: 30,
            max_error_rate: 0.01,
            monitoring_window_minutes: 60,
        };
        let json = serde_json::to_string(&cfg)?;
        let decoded: StagedRolloutConfig = serde_json::from_str(&json)?;
        assert!(!decoded.enabled);
        assert_eq!(decoded.stage1_max_devices, 5);
        Ok(())
    }

    #[test]
    fn partition_health_variants() {
        assert!(PartitionHealth::Healthy.is_usable());
        assert!(!PartitionHealth::Healthy.needs_repair());

        let degraded = PartitionHealth::Degraded {
            reason: "minor issue".to_string(),
        };
        assert!(degraded.is_usable());
        assert!(!degraded.needs_repair());

        let corrupted = PartitionHealth::Corrupted {
            reason: "bad sector".to_string(),
        };
        assert!(!corrupted.is_usable());
        assert!(corrupted.needs_repair());

        assert!(!PartitionHealth::Unknown.is_usable());
        assert!(PartitionHealth::Unknown.needs_repair());
    }

    #[test]
    fn partition_other_is_involution() {
        assert_eq!(Partition::A.other().other(), Partition::A);
        assert_eq!(Partition::B.other().other(), Partition::B);
    }

    #[test]
    fn partition_display() {
        assert_eq!(format!("{}", Partition::A), "A");
        assert_eq!(format!("{}", Partition::B), "B");
    }
}

// ===========================================================================
// Property test: version ordering is total and transitive
// ===========================================================================

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_hw_version() -> impl Strategy<Value = HardwareVersion> {
        prop::collection::vec(0u32..1000, 1..=4).prop_map(|components| {
            let s = components
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(".");
            // parse is infallible for valid numeric components
            HardwareVersion::parse(&s).ok().unwrap_or_else(|| {
                HardwareVersion::parse("0")
                    .ok()
                    .unwrap_or_else(|| unreachable!())
            })
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn version_ordering_is_total(a in arb_hw_version(), b in arb_hw_version()) {
            // Total: exactly one of <, =, > holds
            let ord = a.cmp(&b);
            let rev = b.cmp(&a);
            match ord {
                Ordering::Less => prop_assert_eq!(rev, Ordering::Greater),
                Ordering::Equal => prop_assert_eq!(rev, Ordering::Equal),
                Ordering::Greater => prop_assert_eq!(rev, Ordering::Less),
            }
        }

        #[test]
        fn version_ordering_is_reflexive(a in arb_hw_version()) {
            prop_assert_eq!(a.cmp(&a), Ordering::Equal);
        }

        #[test]
        fn version_ordering_is_transitive(
            a in arb_hw_version(),
            b in arb_hw_version(),
            c in arb_hw_version(),
        ) {
            if a <= b && b <= c {
                prop_assert!(a <= c, "transitivity violated: {:?} <= {:?} <= {:?} but {:?} > {:?}", a, b, c, a, c);
            }
            if a >= b && b >= c {
                prop_assert!(a >= c);
            }
        }

        #[test]
        fn version_partial_ord_consistent_with_ord(a in arb_hw_version(), b in arb_hw_version()) {
            prop_assert_eq!(a.partial_cmp(&b), Some(a.cmp(&b)));
        }

        #[test]
        fn semver_ordering_is_total(
            major_a in 0u64..50,
            minor_a in 0u64..50,
            patch_a in 0u64..50,
            major_b in 0u64..50,
            minor_b in 0u64..50,
            patch_b in 0u64..50,
        ) {
            let a = semver::Version::new(major_a, minor_a, patch_a);
            let b = semver::Version::new(major_b, minor_b, patch_b);
            let ord = a.cmp(&b);
            let rev = b.cmp(&a);
            match ord {
                Ordering::Less => prop_assert_eq!(rev, Ordering::Greater),
                Ordering::Equal => prop_assert_eq!(rev, Ordering::Equal),
                Ordering::Greater => prop_assert_eq!(rev, Ordering::Less),
            }
        }
    }
}

// ===========================================================================
// Device model compatibility and filtering
// ===========================================================================

mod device_model_compat {
    use super::*;

    #[test]
    fn bundle_preserves_custom_device_model() -> TestResult {
        let data = vec![0xCD; 16];
        let mut image = test_firmware_image(&data);
        image.device_model = "fanatec-csl-dd-v2".to_string();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        assert_eq!(&*bundle.header.device_model, "fanatec-csl-dd-v2");
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(&*extracted.device_model, "fanatec-csl-dd-v2");
        Ok(())
    }

    #[test]
    fn different_device_models_produce_different_bundles() -> TestResult {
        let data = vec![0x01; 8];
        let mut img_a = test_firmware_image(&data);
        img_a.device_model = "model-alpha".to_string();
        let mut img_b = test_firmware_image(&data);
        img_b.device_model = "model-beta".to_string();

        let bundle_a = FirmwareBundle::new(&img_a, BundleMetadata::default(), CompressionType::None)?;
        let bundle_b = FirmwareBundle::new(&img_b, BundleMetadata::default(), CompressionType::None)?;

        assert_ne!(&*bundle_a.header.device_model, &*bundle_b.header.device_model);
        Ok(())
    }

    #[test]
    fn device_model_with_special_chars_round_trips() -> TestResult {
        let data = vec![0xAA; 8];
        let mut image = test_firmware_image(&data);
        image.device_model = "vendor/wheel_v3.1-pro".to_string();
        let metadata = BundleMetadata::default();
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        assert_eq!(&*parsed.header.device_model, "vendor/wheel_v3.1-pro");
        Ok(())
    }
}

// ===========================================================================
// Firmware changelog and release notes
// ===========================================================================

mod changelog_tests {
    use super::*;

    #[test]
    fn bundle_changelog_preserved_in_metadata() -> TestResult {
        let data = vec![0x42; 16];
        let image = test_firmware_image(&data);
        let metadata = BundleMetadata {
            changelog: Some("## v2.1.0\n- Fixed jitter\n- Improved FFB".to_string()),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let changelog = parsed.metadata.changelog.as_deref();
        assert_eq!(changelog, Some("## v2.1.0\n- Fixed jitter\n- Improved FFB"));
        Ok(())
    }

    #[test]
    fn bundle_title_preserved() -> TestResult {
        let data = vec![0x11; 8];
        let image = test_firmware_image(&data);
        let metadata = BundleMetadata {
            title: Some("Critical Safety Fix".to_string()),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        assert_eq!(parsed.metadata.title.as_deref(), Some("Critical Safety Fix"));
        Ok(())
    }

    #[test]
    fn release_notes_flow_to_extracted_image() -> TestResult {
        let data = vec![0x22; 12];
        let image = test_firmware_image(&data);
        let metadata = BundleMetadata {
            changelog: Some("Firmware release notes here".to_string()),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::Gzip)?;
        let extracted = bundle.extract_image()?;
        assert_eq!(
            extracted.release_notes.as_deref(),
            Some("Firmware release notes here")
        );
        Ok(())
    }

    #[test]
    fn empty_changelog_is_none() -> TestResult {
        let data = vec![0x33; 8];
        let image = test_firmware_image(&data);
        let metadata = BundleMetadata {
            changelog: None,
            title: None,
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        assert!(parsed.metadata.changelog.is_none());
        assert!(parsed.metadata.title.is_none());
        Ok(())
    }

    #[test]
    fn bundle_custom_metadata_fields_preserved() -> TestResult {
        let data = vec![0x44; 8];
        let image = test_firmware_image(&data);
        let mut custom = std::collections::HashMap::new();
        custom.insert(
            "build_id".to_string(),
            serde_json::Value::String("abc-123".to_string()),
        );
        custom.insert(
            "tested".to_string(),
            serde_json::Value::Bool(true),
        );
        let metadata = BundleMetadata {
            custom,
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        assert_eq!(parsed.metadata.custom.len(), 2);
        let build_id = parsed.metadata.custom.get("build_id");
        assert!(build_id.is_some());
        Ok(())
    }
}

// ===========================================================================
// Binary format parsing edge cases
// ===========================================================================

mod binary_format_tests {
    use super::*;

    #[test]
    fn magic_bytes_are_correct() {
        assert_eq!(OWFB_MAGIC, b"OWFB\0\0\0\x01");
        assert_eq!(OWFB_MAGIC.len(), 8);
    }

    #[test]
    fn bundle_format_version_is_one() {
        assert_eq!(openracing_firmware_update::bundle::BUNDLE_FORMAT_VERSION, 1);
    }

    #[test]
    fn large_payload_round_trips() -> TestResult {
        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data.len(), 10_000);
        assert_eq!(extracted.data, data);
        Ok(())
    }

    #[test]
    fn compressed_smaller_than_uncompressed_for_repetitive_data() -> TestResult {
        let data = vec![0xAA; 4096];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        assert!(
            bundle.header.compressed_size < bundle.header.uncompressed_size,
            "compressed {} should be < uncompressed {}",
            bundle.header.compressed_size,
            bundle.header.uncompressed_size,
        );
        Ok(())
    }

    #[test]
    fn uncompressed_bundle_sizes_match() -> TestResult {
        let data = vec![0xBB; 128];
        let bundle = make_bundle(&data, CompressionType::None)?;
        assert_eq!(bundle.header.uncompressed_size, 128);
        assert_eq!(bundle.header.compressed_size, 128);
        Ok(())
    }

    #[test]
    fn bundle_header_hash_matches_payload() -> TestResult {
        let data = b"payload for hash verification test";
        let bundle = make_bundle(data, CompressionType::None)?;
        let expected_hash = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(data);
            hex::encode(h.finalize())
        };
        assert_eq!(&*bundle.header.payload_hash, &*expected_hash);
        Ok(())
    }

    #[test]
    fn bundle_with_single_byte_payload() -> TestResult {
        let data = vec![0xFF];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, vec![0xFF]);
        Ok(())
    }
}

// ===========================================================================
// Version comparison: upgrade, downgrade, same version
// ===========================================================================

mod version_comparison_tests {
    use super::*;

    #[test]
    fn upgrade_path_major_version() {
        let old = semver::Version::new(1, 0, 0);
        let new = semver::Version::new(2, 0, 0);
        assert!(new > old);
    }

    #[test]
    fn upgrade_path_minor_version() {
        let old = semver::Version::new(1, 0, 0);
        let new = semver::Version::new(1, 1, 0);
        assert!(new > old);
    }

    #[test]
    fn upgrade_path_patch_version() {
        let old = semver::Version::new(1, 0, 0);
        let new = semver::Version::new(1, 0, 1);
        assert!(new > old);
    }

    #[test]
    fn downgrade_detected() {
        let current = semver::Version::new(3, 0, 0);
        let target = semver::Version::new(2, 5, 0);
        assert!(target < current, "downgrade: target should be less than current");
    }

    #[test]
    fn same_version_detected() {
        let a = semver::Version::new(1, 2, 3);
        let b = semver::Version::new(1, 2, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn rollback_protection_blocks_downgrade() -> TestResult {
        let data = vec![0x01; 8];
        let image = test_firmware_image(&data);
        let metadata = BundleMetadata {
            rollback_version: Some(semver::Version::new(2, 0, 0)),
            ..Default::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;
        // Device running 1.0.0 should be blocked (below rollback minimum)
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(1, 0, 0)));
        // Device running 2.0.0 should be allowed
        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 0, 0)));
        // Device running 3.0.0 should be allowed
        assert!(bundle.allows_upgrade_from(&semver::Version::new(3, 0, 0)));
        Ok(())
    }
}

// ===========================================================================
// Interrupted update recovery and rollback
// ===========================================================================

mod interrupted_update_tests {
    use super::*;
    use openracing_firmware_update::rollback::RollbackManager;
    use std::path::PathBuf;
    use tokio::fs;

    #[tokio::test]
    async fn rollback_with_multiple_files() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        // Create multiple files to back up
        fs::write(install_dir.join("firmware.bin"), b"fw-original").await?;
        fs::write(install_dir.join("config.dat"), b"cfg-original").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        let files = vec![
            PathBuf::from("firmware.bin"),
            PathBuf::from("config.dat"),
        ];
        manager
            .create_backup(
                "multi-file-bak",
                semver::Version::new(1, 0, 0),
                semver::Version::new(2, 0, 0),
                &files,
            )
            .await?;

        // Simulate partial update (only firmware overwritten)
        fs::write(install_dir.join("firmware.bin"), b"fw-corrupted").await?;

        // Rollback should restore both files
        manager.rollback_to("multi-file-bak").await?;

        let fw = fs::read_to_string(install_dir.join("firmware.bin")).await?;
        let cfg = fs::read_to_string(install_dir.join("config.dat")).await?;
        assert_eq!(&*fw, "fw-original");
        assert_eq!(&*cfg, "cfg-original");
        Ok(())
    }

    #[tokio::test]
    async fn rollback_with_nested_directories() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(install_dir.join("sub")).await?;

        fs::write(install_dir.join("sub").join("deep.bin"), b"deep-orig").await?;

        let manager = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        manager
            .create_backup(
                "nested-bak",
                semver::Version::new(1, 0, 0),
                semver::Version::new(1, 1, 0),
                &[PathBuf::from("sub").join("deep.bin")],
            )
            .await?;

        fs::write(install_dir.join("sub").join("deep.bin"), b"deep-bad").await?;
        manager.rollback_to("nested-bak").await?;

        let content = fs::read_to_string(install_dir.join("sub").join("deep.bin")).await?;
        assert_eq!(&*content, "deep-orig");
        Ok(())
    }

    #[tokio::test]
    async fn cleanup_with_no_old_backups_succeeds() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        let manager = RollbackManager::new(backup_dir, install_dir);
        // Cleanup when no backups exist should be a no-op (not an error)
        manager.cleanup_old(30).await?;
        Ok(())
    }
}

// ===========================================================================
// Progress reporting accuracy
// ===========================================================================

mod progress_reporting_tests {
    use super::*;

    #[test]
    fn progress_percentage_at_boundaries() -> TestResult {
        let progress_start = UpdateProgress {
            phase: UpdatePhase::Transferring,
            progress_percent: 0,
            bytes_transferred: 0,
            total_bytes: 10000,
            transfer_rate_bps: 0,
            eta_seconds: Some(100),
            status_message: "Starting transfer".to_string(),
            warnings: Vec::new(),
        };
        assert_eq!(progress_start.progress_percent, 0);

        let progress_end = UpdateProgress {
            phase: UpdatePhase::Completed,
            progress_percent: 100,
            bytes_transferred: 10000,
            total_bytes: 10000,
            transfer_rate_bps: 5000,
            eta_seconds: Some(0),
            status_message: "Transfer complete".to_string(),
            warnings: Vec::new(),
        };
        assert_eq!(progress_end.progress_percent, 100);
        assert_eq!(progress_end.bytes_transferred, progress_end.total_bytes);
        Ok(())
    }

    #[test]
    fn progress_with_warnings_serializes() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Validating,
            progress_percent: 75,
            bytes_transferred: 7500,
            total_bytes: 10000,
            transfer_rate_bps: 2500,
            eta_seconds: Some(1),
            status_message: "Validating checksum".to_string(),
            warnings: vec![
                "Slow transfer rate detected".to_string(),
                "Battery low".to_string(),
            ],
        };
        let json = serde_json::to_string(&progress)?;
        let restored: UpdateProgress = serde_json::from_str(&json)?;
        assert_eq!(restored.warnings.len(), 2);
        assert_eq!(&*restored.warnings[0], "Slow transfer rate detected");
        Ok(())
    }

    #[test]
    fn all_update_phases_are_distinct() -> TestResult {
        let phases = vec![
            UpdatePhase::Initializing,
            UpdatePhase::Verifying,
            UpdatePhase::Preparing,
            UpdatePhase::Transferring,
            UpdatePhase::Validating,
            UpdatePhase::Activating,
            UpdatePhase::HealthCheck,
            UpdatePhase::Completed,
            UpdatePhase::RollingBack,
            UpdatePhase::Failed,
        ];
        // Verify all phases serialize to distinct JSON strings
        let mut json_values = std::collections::HashSet::new();
        for phase in &phases {
            let json = serde_json::to_string(phase)?;
            json_values.insert(json);
        }
        assert_eq!(json_values.len(), phases.len(), "all phases should be distinct");
        Ok(())
    }
}

// ===========================================================================
// Concurrent update rejection
// ===========================================================================

mod concurrent_update_tests {
    use super::*;

    #[tokio::test]
    async fn ffb_blocker_rejects_second_device_update() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("wheel-A").await?;

        let result = blocker.begin_update("wheel-B").await;
        assert!(result.is_err(), "second concurrent update should be rejected");

        blocker.end_update().await;
        // Now a new update should be possible
        blocker.begin_update("wheel-B").await?;
        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocked_during_update_unblocked_after() -> TestResult {
        let blocker = FfbBlocker::new();
        assert!(!blocker.is_ffb_blocked());

        blocker.begin_update("dev-1").await?;
        assert!(blocker.is_ffb_blocked());
        let err = blocker.try_ffb_operation();
        assert!(err.is_err());

        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());
        blocker.try_ffb_operation()?;
        Ok(())
    }

    #[tokio::test]
    async fn rapid_begin_end_cycles_stable() -> TestResult {
        let blocker = FfbBlocker::new();
        for i in 0..10 {
            let device_id = format!("device-{}", i);
            blocker.begin_update(&device_id).await?;
            assert!(blocker.is_ffb_blocked());
            blocker.end_update().await;
            assert!(!blocker.is_ffb_blocked());
        }
        Ok(())
    }
}
