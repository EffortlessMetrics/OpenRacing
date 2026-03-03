//! Deep tests for firmware update subsystem.
//!
//! Covers:
//! - Firmware update state machine (all transitions)
//! - Binary validation (checksums, signatures, size limits)
//! - Update progress tracking
//! - Rollback mechanisms
//! - Concurrent update prevention
//! - Error recovery
//! - Update journal/log

use std::path::PathBuf;
use std::time::Duration;

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

fn test_image(data: &[u8]) -> FirmwareImage {
    let hash = openracing_crypto::utils::compute_sha256_hex(data);
    FirmwareImage {
        device_model: "test-wheel".to_string(),
        version: semver::Version::new(2, 0, 0),
        min_hardware_version: Some("1.0".to_string()),
        max_hardware_version: Some("5.0".to_string()),
        data: data.to_vec(),
        hash,
        size_bytes: data.len() as u64,
        build_timestamp: chrono::Utc::now(),
        release_notes: Some("test release".to_string()),
        signature: None,
    }
}

fn make_bundle(data: &[u8], compression: CompressionType) -> Result<FirmwareBundle, anyhow::Error> {
    let image = test_image(data);
    let metadata = BundleMetadata::default();
    FirmwareBundle::new(&image, metadata, compression)
}

// ===========================================================================
// 1. Firmware update state machine (all transitions)
// ===========================================================================

mod state_machine {
    use super::*;

    #[test]
    fn all_states_in_progress_classification() {
        // Terminal states: not in progress
        assert!(!UpdateState::Idle.is_in_progress());
        assert!(!UpdateState::Complete.is_in_progress());
        assert!(
            !UpdateState::Failed {
                error: "e".into(),
                recoverable: true
            }
            .is_in_progress()
        );
        assert!(
            !UpdateState::Failed {
                error: "e".into(),
                recoverable: false
            }
            .is_in_progress()
        );

        // Active states: in progress
        assert!(UpdateState::Downloading { progress: 0 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 50 }.is_in_progress());
        assert!(UpdateState::Downloading { progress: 100 }.is_in_progress());
        assert!(UpdateState::Verifying.is_in_progress());
        assert!(UpdateState::Flashing { progress: 0 }.is_in_progress());
        assert!(UpdateState::Flashing { progress: 99 }.is_in_progress());
        assert!(UpdateState::Rebooting.is_in_progress());
    }

    #[test]
    fn ffb_blocking_mirrors_in_progress() {
        let states: Vec<UpdateState> = vec![
            UpdateState::Idle,
            UpdateState::Downloading { progress: 50 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 75 },
            UpdateState::Rebooting,
            UpdateState::Complete,
            UpdateState::Failed {
                error: "e".into(),
                recoverable: true,
            },
        ];
        for state in &states {
            assert_eq!(
                state.should_block_ffb(),
                state.is_in_progress(),
                "FFB blocking must match in_progress for {:?}",
                state
            );
        }
    }

    #[test]
    fn default_state_is_idle() {
        assert_eq!(UpdateState::default(), UpdateState::Idle);
    }

    #[test]
    fn all_states_serde_round_trip() -> TestResult {
        let variants = vec![
            UpdateState::Idle,
            UpdateState::Downloading { progress: 0 },
            UpdateState::Downloading { progress: 100 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 50 },
            UpdateState::Rebooting,
            UpdateState::Complete,
            UpdateState::Failed {
                error: "err".into(),
                recoverable: true,
            },
            UpdateState::Failed {
                error: "fatal".into(),
                recoverable: false,
            },
        ];
        for state in &variants {
            let json = serde_json::to_string(state)?;
            let decoded: UpdateState = serde_json::from_str(&json)?;
            assert_eq!(&decoded, state, "serde round-trip failed for {:?}", state);
        }
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_lifecycle() -> TestResult {
        let blocker = FfbBlocker::new();

        // Initially idle and unblocked
        assert!(!blocker.is_ffb_blocked());
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        assert!(blocker.get_updating_device().await.is_none());
        blocker.try_ffb_operation()?;

        // Begin update
        blocker.begin_update("dev-1").await?;
        assert!(blocker.is_ffb_blocked());
        assert_eq!(blocker.get_state().await, UpdateState::Verifying);
        assert_eq!(
            blocker.get_updating_device().await,
            Some("dev-1".to_string())
        );
        let err = blocker.try_ffb_operation();
        assert!(err.is_err());

        // Transition through states
        blocker
            .set_state(UpdateState::Downloading { progress: 25 })
            .await;
        assert_eq!(
            blocker.get_state().await,
            UpdateState::Downloading { progress: 25 }
        );

        blocker
            .set_state(UpdateState::Flashing { progress: 80 })
            .await;
        assert_eq!(
            blocker.get_state().await,
            UpdateState::Flashing { progress: 80 }
        );

        blocker.set_state(UpdateState::Rebooting).await;
        assert_eq!(blocker.get_state().await, UpdateState::Rebooting);

        // End update
        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        assert!(blocker.get_updating_device().await.is_none());
        blocker.try_ffb_operation()?;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_rejects_second_concurrent_update() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("device-A").await?;

        let result = blocker.begin_update("device-B").await;
        assert!(result.is_err());
        if let Err(FirmwareUpdateError::UpdateInProgress(dev)) = result {
            assert_eq!(dev, "device-A");
        } else {
            return Err("Expected UpdateInProgress error".into());
        }

        blocker.end_update().await;

        // After ending, a new update is allowed
        blocker.begin_update("device-B").await?;
        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_end_update_resets_completely() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-x").await?;
        blocker
            .set_state(UpdateState::Failed {
                error: "crash".into(),
                recoverable: false,
            })
            .await;

        blocker.end_update().await;
        // All state is reset
        assert!(!blocker.is_ffb_blocked());
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        assert!(blocker.get_updating_device().await.is_none());
        Ok(())
    }
}

// ===========================================================================
// 2. Binary validation (checksums, signatures, size limits)
// ===========================================================================

mod binary_validation {
    use super::*;

    #[test]
    fn bundle_payload_hash_matches_data() -> TestResult {
        let data = vec![0x42; 256];
        let bundle = make_bundle(&data, CompressionType::None)?;
        let expected = openracing_crypto::utils::compute_sha256_hex(&data);
        assert_eq!(bundle.header.payload_hash, expected);
        Ok(())
    }

    #[test]
    fn corrupted_payload_byte_detected() -> TestResult {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let bundle = make_bundle(&data, CompressionType::None)?;
        let mut serialized = bundle.serialize()?;
        // Flip last byte
        if let Some(last) = serialized.last_mut() {
            *last ^= 0xFF;
        }
        let result = FirmwareBundle::parse(&serialized);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn invalid_magic_rejected() {
        assert!(FirmwareBundle::parse(b"NOT_OWFB").is_err());
    }

    #[test]
    fn truncated_after_magic_rejected() {
        assert!(FirmwareBundle::parse(OWFB_MAGIC).is_err());
    }

    #[test]
    fn empty_data_rejected() {
        assert!(FirmwareBundle::parse(&[]).is_err());
    }

    #[test]
    fn bundle_sizes_match_content() -> TestResult {
        let data = vec![0xFF; 512];
        let bundle = make_bundle(&data, CompressionType::None)?;
        assert_eq!(bundle.header.uncompressed_size, 512);
        assert_eq!(bundle.header.compressed_size, 512);
        let serialized = bundle.serialize()?;
        assert_eq!(bundle.bundle_size(), serialized.len());
        Ok(())
    }

    #[test]
    fn gzip_compressed_bundle_validates() -> TestResult {
        let data = vec![0xAA; 1024];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        assert!(bundle.header.compressed_size <= bundle.header.uncompressed_size);
        let serialized = bundle.serialize()?;
        let parsed = FirmwareBundle::parse(&serialized)?;
        let extracted = parsed.extract_image()?;
        assert_eq!(extracted.data, data);
        Ok(())
    }

    #[test]
    fn data_hash_is_deterministic() {
        let data = b"hello firmware";
        let h1 = compute_data_hash(data);
        let h2 = compute_data_hash(data);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA256 hex = 64 chars
    }

    #[test]
    fn different_data_produces_different_hash() {
        let h1 = compute_data_hash(b"data_a");
        let h2 = compute_data_hash(b"data_b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn compression_round_trip() -> TestResult {
        let original = b"test data for compression round trip test";
        let compressed = compress_data(original)?;
        let decompressed = decompress_data(&compressed)?;
        assert_eq!(original.as_slice(), decompressed.as_slice());
        Ok(())
    }

    #[test]
    fn empty_data_compression_round_trip() -> TestResult {
        let compressed = compress_data(b"")?;
        let decompressed = decompress_data(&compressed)?;
        assert!(decompressed.is_empty());
        Ok(())
    }

    #[test]
    fn large_data_compression_round_trip() -> TestResult {
        let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        let compressed = compress_data(&data)?;
        let decompressed = decompress_data(&compressed)?;
        assert_eq!(data, decompressed);
        Ok(())
    }

    #[test]
    fn firmware_image_hash_matches_data() {
        let data = vec![0x01, 0x02, 0x03];
        let img = test_image(&data);
        let expected = openracing_crypto::utils::compute_sha256_hex(&data);
        assert_eq!(img.hash, expected);
        assert_eq!(img.size_bytes, 3);
    }
}

// ===========================================================================
// 3. Update progress tracking
// ===========================================================================

mod progress_tracking {
    use super::*;

    #[test]
    fn update_progress_serde_round_trip() -> TestResult {
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
        for phase in phases {
            let progress = UpdateProgress {
                phase,
                progress_percent: 50,
                bytes_transferred: 1024,
                total_bytes: 2048,
                transfer_rate_bps: 512,
                eta_seconds: Some(2),
                status_message: "testing".to_string(),
                warnings: vec!["warn".to_string()],
            };
            let json = serde_json::to_string(&progress)?;
            let decoded: UpdateProgress = serde_json::from_str(&json)?;
            assert_eq!(decoded.progress_percent, 50);
            assert_eq!(decoded.bytes_transferred, 1024);
        }
        Ok(())
    }

    #[test]
    fn update_result_success_fields() -> TestResult {
        let result = UpdateResult {
            device_id: "wheel-001".to_string(),
            success: true,
            old_version: Some(semver::Version::new(1, 0, 0)),
            new_version: Some(semver::Version::new(2, 0, 0)),
            updated_partition: Some(Partition::B),
            rollback_performed: false,
            duration: Duration::from_secs(120),
            error: None,
            partition_states: vec![
                PartitionInfo::empty(Partition::A),
                PartitionInfo::empty(Partition::B),
            ],
        };
        assert!(result.success);
        assert!(!result.rollback_performed);
        assert!(result.error.is_none());
        assert_eq!(result.partition_states.len(), 2);

        let json = serde_json::to_string(&result)?;
        let decoded: UpdateResult = serde_json::from_str(&json)?;
        assert!(decoded.success);
        assert_eq!(decoded.device_id, "wheel-001");
        Ok(())
    }

    #[test]
    fn update_result_failure_fields() -> TestResult {
        let result = UpdateResult {
            device_id: "wheel-002".to_string(),
            success: false,
            old_version: None,
            new_version: None,
            updated_partition: None,
            rollback_performed: true,
            duration: Duration::from_secs(30),
            error: Some("connection lost".to_string()),
            partition_states: vec![],
        };
        assert!(!result.success);
        assert!(result.rollback_performed);
        assert_eq!(result.error.as_deref(), Some("connection lost"));

        let json = serde_json::to_string(&result)?;
        let decoded: UpdateResult = serde_json::from_str(&json)?;
        assert!(!decoded.success);
        assert!(decoded.rollback_performed);
        Ok(())
    }

    #[test]
    fn health_check_summary_success_rate() {
        let summary = HealthCheckSummary {
            total_checks: 10,
            passed_checks: 8,
            failed_checks: 2,
            critical_failures: 0,
            results: vec![],
        };
        assert!((summary.success_rate() - 0.8).abs() < f64::EPSILON);
        assert!(summary.all_critical_passed());
    }

    #[test]
    fn health_check_summary_zero_checks() {
        let summary = HealthCheckSummary {
            total_checks: 0,
            passed_checks: 0,
            failed_checks: 0,
            critical_failures: 0,
            results: vec![],
        };
        assert!((summary.success_rate() - 1.0).abs() < f64::EPSILON);
        assert!(summary.all_critical_passed());
    }

    #[test]
    fn health_check_summary_critical_failures() {
        let summary = HealthCheckSummary {
            total_checks: 5,
            passed_checks: 3,
            failed_checks: 2,
            critical_failures: 1,
            results: vec![],
        };
        assert!(!summary.all_critical_passed());
    }

    #[tokio::test]
    async fn manager_reports_no_active_updates_initially() {
        let mgr = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        assert!(!mgr.is_update_in_progress().await);
        assert!(mgr.get_active_updates().await.is_empty());
    }

    #[tokio::test]
    async fn cancel_nonexistent_update_fails() {
        let mgr = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let result = mgr.cancel_update("nonexistent").await;
        assert!(result.is_err());
    }
}

// ===========================================================================
// 4. Rollback mechanisms
// ===========================================================================

mod rollback_tests {
    use super::*;
    use tokio::fs;

    #[tokio::test]
    async fn create_backup_and_verify() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("fw.bin"), b"original firmware").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        mgr.create_backup(
            "bak-001",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        let backups = mgr.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert!(backups[0].valid);
        assert_eq!(backups[0].metadata.backup_id, "bak-001");
        assert_eq!(
            backups[0].metadata.original_version,
            semver::Version::new(1, 0, 0)
        );
        assert_eq!(
            backups[0].metadata.target_version,
            semver::Version::new(2, 0, 0)
        );
        Ok(())
    }

    #[tokio::test]
    async fn rollback_restores_original_content() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("fw.bin"), b"original").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir.clone());
        mgr.create_backup(
            "bak-restore",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Simulate failed update
        fs::write(install_dir.join("fw.bin"), b"corrupted update").await?;

        mgr.rollback_to("bak-restore").await?;

        let content = fs::read_to_string(install_dir.join("fw.bin")).await?;
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

        let mgr = RollbackManager::new(backup_dir, install_dir);
        let result = mgr.rollback_to("nonexistent").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn multiple_backups_latest_is_first() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("fw.bin"), b"content").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);

        mgr.create_backup(
            "bak-old",
            semver::Version::new(1, 0, 0),
            semver::Version::new(1, 1, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        tokio::time::sleep(Duration::from_millis(50)).await;

        mgr.create_backup(
            "bak-new",
            semver::Version::new(1, 1, 0),
            semver::Version::new(1, 2, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        let latest = mgr.get_latest_backup().await?;
        assert!(latest.is_some());
        let latest = latest.ok_or("no latest")?;
        assert_eq!(latest.backup_id, "bak-new");
        Ok(())
    }

    #[tokio::test]
    async fn rollback_with_multiple_files() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        fs::write(install_dir.join("fw.bin"), b"original_fw").await?;
        fs::write(install_dir.join("config.dat"), b"original_cfg").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir.clone());
        mgr.create_backup(
            "bak-multi",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin"), PathBuf::from("config.dat")],
        )
        .await?;

        // Corrupt both files
        fs::write(install_dir.join("fw.bin"), b"bad_fw").await?;
        fs::write(install_dir.join("config.dat"), b"bad_cfg").await?;

        mgr.rollback_to("bak-multi").await?;

        assert_eq!(
            fs::read_to_string(install_dir.join("fw.bin")).await?,
            "original_fw"
        );
        assert_eq!(
            fs::read_to_string(install_dir.join("config.dat")).await?,
            "original_cfg"
        );
        Ok(())
    }

    #[tokio::test]
    async fn empty_backup_dir_returns_no_backups() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        fs::create_dir_all(&backup_dir).await?;
        fs::create_dir_all(&install_dir).await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        let backups = mgr.get_backup_info().await?;
        assert!(backups.is_empty());

        let latest = mgr.get_latest_backup().await?;
        assert!(latest.is_none());
        Ok(())
    }

    #[test]
    fn backup_metadata_serde_round_trip() -> TestResult {
        let meta = BackupMetadata {
            backup_id: "test".to_string(),
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
// 5. Concurrent update prevention
// ===========================================================================

mod concurrent_update_prevention {
    use super::*;

    #[tokio::test]
    async fn ffb_blocker_prevents_concurrent_via_compare_exchange() -> TestResult {
        let blocker = FfbBlocker::new();

        blocker.begin_update("first").await?;
        assert!(blocker.is_ffb_blocked());

        // Second attempt must fail
        let r2 = blocker.begin_update("second").await;
        assert!(r2.is_err());

        // Third attempt must also fail
        let r3 = blocker.begin_update("third").await;
        assert!(r3.is_err());

        blocker.end_update().await;

        // Now it should succeed
        blocker.begin_update("second").await?;
        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn ffb_operation_blocked_only_during_update() -> TestResult {
        let blocker = FfbBlocker::new();

        // Before update: ok
        assert!(blocker.try_ffb_operation().is_ok());

        blocker.begin_update("dev").await?;

        // During update: blocked
        assert!(blocker.try_ffb_operation().is_err());
        match blocker.try_ffb_operation() {
            Err(FirmwareUpdateError::FfbBlocked) => {} // expected
            other => return Err(format!("Expected FfbBlocked, got {:?}", other).into()),
        }

        blocker.end_update().await;

        // After update: ok again
        assert!(blocker.try_ffb_operation().is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_begin_reports_blocking_device() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("blocking-device").await?;

        match blocker.begin_update("new-device").await {
            Err(FirmwareUpdateError::UpdateInProgress(dev)) => {
                assert_eq!(dev, "blocking-device");
            }
            other => return Err(format!("Expected UpdateInProgress, got {:?}", other).into()),
        }

        blocker.end_update().await;
        Ok(())
    }

    #[tokio::test]
    async fn manager_tracks_active_updates_list() {
        let mgr = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        assert!(mgr.get_active_updates().await.is_empty());
        assert!(!mgr.is_update_in_progress().await);
    }
}

// ===========================================================================
// 6. Error recovery
// ===========================================================================

mod error_recovery {
    use super::*;

    #[test]
    fn firmware_update_error_display() {
        let errors: Vec<FirmwareUpdateError> = vec![
            FirmwareUpdateError::DeviceNotFound("dev-1".into()),
            FirmwareUpdateError::VerificationFailed("hash mismatch".into()),
            FirmwareUpdateError::TransferFailed("timeout".into()),
            FirmwareUpdateError::HealthCheckFailed("no response".into()),
            FirmwareUpdateError::RollbackFailed("disk full".into()),
            FirmwareUpdateError::InvalidFirmware("bad format".into()),
            FirmwareUpdateError::DeviceError("usb disconnect".into()),
            FirmwareUpdateError::Timeout("30s".into()),
            FirmwareUpdateError::FfbBlocked,
            FirmwareUpdateError::UpdateInProgress("dev-2".into()),
            FirmwareUpdateError::CacheError("write failed".into()),
            FirmwareUpdateError::BundleError("corrupt".into()),
            FirmwareUpdateError::PartitionError("no space".into()),
            FirmwareUpdateError::CompatibilityError("hw too old".into()),
            FirmwareUpdateError::SerializationError("parse fail".into()),
            FirmwareUpdateError::InvalidState("wrong phase".into()),
            FirmwareUpdateError::Cancelled("user request".into()),
            FirmwareUpdateError::RolloutError("threshold exceeded".into()),
        ];
        for err in &errors {
            let msg = format!("{err}");
            assert!(!msg.is_empty(), "error display must not be empty");
        }
    }

    #[test]
    fn serde_json_error_converts_to_firmware_error() {
        let bad_json = "not valid json";
        let serde_err = serde_json::from_str::<serde_json::Value>(bad_json);
        assert!(serde_err.is_err());
        let firmware_err: FirmwareUpdateError = serde_err
            .err()
            .map(Into::into)
            .ok_or("")
            .ok()
            .unwrap_or(FirmwareUpdateError::SerializationError("fallback".into()));
        let msg = format!("{firmware_err}");
        assert!(msg.contains("erialization") || msg.contains("parse"));
    }

    #[test]
    fn failed_state_carries_error_info() {
        let state = UpdateState::Failed {
            error: "connection lost".to_string(),
            recoverable: true,
        };
        if let UpdateState::Failed { error, recoverable } = &state {
            assert_eq!(error, "connection lost");
            assert!(recoverable);
        } else {
            panic!("expected Failed state");
        }
    }

    #[test]
    fn recoverable_vs_unrecoverable_failure() {
        let recoverable = UpdateState::Failed {
            error: "timeout".into(),
            recoverable: true,
        };
        let unrecoverable = UpdateState::Failed {
            error: "hw fault".into(),
            recoverable: false,
        };
        // Both are terminal (not in progress)
        assert!(!recoverable.is_in_progress());
        assert!(!unrecoverable.is_in_progress());
        // Both don't block FFB
        assert!(!recoverable.should_block_ffb());
        assert!(!unrecoverable.should_block_ffb());
    }

    #[tokio::test]
    async fn ffb_blocker_recovers_after_failed_update() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        // Simulate failure
        blocker
            .set_state(UpdateState::Failed {
                error: "crash".into(),
                recoverable: false,
            })
            .await;

        // Recovery: end_update resets everything
        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());
        assert_eq!(blocker.get_state().await, UpdateState::Idle);

        // Can start new update
        blocker.begin_update("dev-2").await?;
        blocker.end_update().await;
        Ok(())
    }

    #[test]
    fn patch_invalid_magic_rejected() {
        let result = apply_simple_patch(b"old data", b"NOT_A_PATCH");
        assert!(result.is_err());
    }

    #[test]
    fn patch_size_mismatch_rejected() -> TestResult {
        let old = b"hello";
        let new = b"world";
        let patch = create_simple_patch(old, new)?;

        // Apply with wrong old data size
        let result = apply_simple_patch(b"hi", &patch);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn delta_patch_round_trip() -> TestResult {
        let old = b"The quick brown fox jumps over the lazy dog";
        let new = b"The quick brown cat jumps over the lazy dog";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result.as_slice(), new.as_slice());
        Ok(())
    }

    #[test]
    fn delta_patch_identical_data() -> TestResult {
        let data = b"identical content";
        let patch = create_simple_patch(data, data)?;
        let result = apply_simple_patch(data, &patch)?;
        assert_eq!(result.as_slice(), data.as_slice());
        Ok(())
    }

    #[test]
    fn delta_patch_completely_different() -> TestResult {
        let old = b"AAAA";
        let new = b"ZZZZ";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result.as_slice(), new.as_slice());
        Ok(())
    }

    #[test]
    fn delta_patch_empty_to_content() -> TestResult {
        let old = b"";
        let new = b"new content";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result.as_slice(), new.as_slice());
        Ok(())
    }

    #[test]
    fn delta_patch_content_to_empty() -> TestResult {
        let old = b"old content";
        let new = b"";
        let patch = create_simple_patch(old, new)?;
        let result = apply_simple_patch(old, &patch)?;
        assert_eq!(result.as_slice(), new.as_slice());
        Ok(())
    }
}

// ===========================================================================
// 7. Update journal/log — partition management, compatibility, config
// ===========================================================================

mod update_journal {
    use super::*;

    #[test]
    fn partition_other_is_involution() {
        assert_eq!(Partition::A.other(), Partition::B);
        assert_eq!(Partition::B.other(), Partition::A);
        assert_eq!(Partition::A.other().other(), Partition::A);
        assert_eq!(Partition::B.other().other(), Partition::B);
    }

    #[test]
    fn partition_display() {
        assert_eq!(format!("{}", Partition::A), "A");
        assert_eq!(format!("{}", Partition::B), "B");
    }

    #[test]
    fn partition_info_empty_defaults() {
        let info = PartitionInfo::empty(Partition::A);
        assert_eq!(info.partition, Partition::A);
        assert!(!info.active);
        assert!(!info.bootable);
        assert!(info.version.is_none());
        assert_eq!(info.size_bytes, 0);
        assert!(info.hash.is_none());
        assert!(info.updated_at.is_none());
        assert_eq!(info.health, PartitionHealth::Unknown);
    }

    #[test]
    fn partition_can_update_inactive_healthy() {
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
    fn partition_cannot_update_active() {
        let info = PartitionInfo {
            partition: Partition::A,
            active: true,
            bootable: true,
            version: Some(semver::Version::new(1, 0, 0)),
            size_bytes: 1024,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Healthy,
        };
        assert!(!info.can_update());
    }

    #[test]
    fn partition_health_usability() {
        assert!(PartitionHealth::Healthy.is_usable());
        assert!(!PartitionHealth::Healthy.needs_repair());

        let degraded = PartitionHealth::Degraded {
            reason: "wear".into(),
        };
        assert!(degraded.is_usable());
        assert!(!degraded.needs_repair());

        let corrupted = PartitionHealth::Corrupted {
            reason: "bad sector".into(),
        };
        assert!(!corrupted.is_usable());
        assert!(corrupted.needs_repair());

        assert!(!PartitionHealth::Unknown.is_usable());
        assert!(PartitionHealth::Unknown.needs_repair());
    }

    #[test]
    fn partition_health_default_is_unknown() {
        assert_eq!(PartitionHealth::default(), PartitionHealth::Unknown);
    }

    #[test]
    fn partition_info_serde_round_trip() -> TestResult {
        let info = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: true,
            version: Some(semver::Version::new(2, 1, 3)),
            size_bytes: 65536,
            hash: Some("deadbeef".to_string()),
            updated_at: Some(chrono::Utc::now()),
            health: PartitionHealth::Healthy,
        };
        let json = serde_json::to_string(&info)?;
        let decoded: PartitionInfo = serde_json::from_str(&json)?;
        assert_eq!(decoded.partition, Partition::B);
        assert!(!decoded.active);
        assert!(decoded.bootable);
        assert_eq!(decoded.version, Some(semver::Version::new(2, 1, 3)));
        Ok(())
    }

    #[test]
    fn hardware_version_comparison_numeric() -> Result<(), HardwareVersionError> {
        let v2 = HardwareVersion::parse("2.0")?;
        let v10 = HardwareVersion::parse("10.0")?;
        assert!(v2 < v10);
        assert!(v10 > v2);

        let v1_9 = HardwareVersion::parse("1.9")?;
        let v1_10 = HardwareVersion::parse("1.10")?;
        assert!(v1_9 < v1_10);
        Ok(())
    }

    #[test]
    fn hardware_version_trailing_zeros_equal() -> Result<(), HardwareVersionError> {
        let a = HardwareVersion::parse("1.2")?;
        let b = HardwareVersion::parse("1.2.0")?;
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        Ok(())
    }

    #[test]
    fn hardware_version_parse_errors() {
        assert!(matches!(
            HardwareVersion::parse(""),
            Err(HardwareVersionError::Empty)
        ));
        assert!(matches!(
            HardwareVersion::parse("   "),
            Err(HardwareVersionError::Empty)
        ));
        assert!(HardwareVersion::parse("abc").is_err());
        assert!(HardwareVersion::parse("1.two.3").is_err());
        assert!(HardwareVersion::parse("1.2.").is_err());
        assert!(HardwareVersion::parse(".1.2").is_err());
        assert!(HardwareVersion::parse("-1").is_err());
    }

    #[test]
    fn hardware_compatibility_matrix() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);

        // Bundle with hw range 2.0 - 5.0
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: Some("2.0".to_string()),
            max_hardware_version: Some("5.0".to_string()),
            data,
            hash,
            size_bytes: 8,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;

        // In range
        assert!(bundle.is_compatible_with_hardware("2.0"));
        assert!(bundle.is_compatible_with_hardware("3.5"));
        assert!(bundle.is_compatible_with_hardware("5.0"));

        // Out of range
        assert!(!bundle.is_compatible_with_hardware("1.9"));
        assert!(!bundle.is_compatible_with_hardware("5.1"));

        // Invalid versions fail closed
        assert!(!bundle.is_compatible_with_hardware("invalid"));
        assert!(!bundle.is_compatible_with_hardware(""));
        Ok(())
    }

    #[test]
    fn rollback_protection() -> TestResult {
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
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(0, 1, 0)));
        Ok(())
    }

    #[test]
    fn no_rollback_version_allows_any() -> TestResult {
        let bundle = make_bundle(&[0x01; 8], CompressionType::None)?;
        assert!(bundle.allows_upgrade_from(&semver::Version::new(0, 0, 1)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(99, 0, 0)));
        Ok(())
    }

    #[test]
    fn staged_rollout_config_defaults() {
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
        assert!((decoded.min_success_rate - 0.99).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn release_channel_variants() -> TestResult {
        for ch in [
            ReleaseChannel::Stable,
            ReleaseChannel::Beta,
            ReleaseChannel::Nightly,
        ] {
            let json = serde_json::to_string(&ch)?;
            let decoded: ReleaseChannel = serde_json::from_str(&json)?;
            assert_eq!(decoded, ch);
        }
        Ok(())
    }

    #[test]
    fn bundle_metadata_defaults() {
        let meta = BundleMetadata::default();
        assert_eq!(meta.channel, ReleaseChannel::Stable);
        assert!(meta.title.is_none());
        assert!(meta.changelog.is_none());
        assert!(meta.signing_key.is_none());
        assert!(meta.rollback_version.is_none());
        assert!(meta.custom.is_empty());
    }

    #[test]
    fn bundle_metadata_with_custom_fields() -> TestResult {
        let meta = BundleMetadata {
            title: Some("Release v2.0".to_string()),
            changelog: Some("Bug fixes".to_string()),
            custom: {
                let mut m = std::collections::HashMap::new();
                m.insert("build_host".to_string(), serde_json::json!("ci-server"));
                m
            },
            ..BundleMetadata::default()
        };

        let json = serde_json::to_string(&meta)?;
        let decoded: BundleMetadata = serde_json::from_str(&json)?;
        assert_eq!(decoded.title.as_deref(), Some("Release v2.0"));
        assert_eq!(decoded.changelog.as_deref(), Some("Bug fixes"));
        assert_eq!(
            decoded.custom.get("build_host"),
            Some(&serde_json::json!("ci-server"))
        );
        Ok(())
    }

    #[test]
    fn bundle_file_write_and_reload() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.owfb");
        let data = vec![0xAB; 128];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        bundle.write(&path)?;
        assert!(path.exists());

        let loaded = FirmwareBundle::load(&path)?;
        assert_eq!(loaded.header.device_model, "test-wheel");
        assert_eq!(
            loaded.header.firmware_version,
            semver::Version::new(2, 0, 0)
        );

        let extracted = loaded.extract_image()?;
        assert_eq!(extracted.data, data);
        Ok(())
    }
}

// ===========================================================================
// 8. State machine transitions via FfbBlocker
// ===========================================================================

mod state_machine_transitions {
    use super::*;

    #[tokio::test]
    async fn idle_to_verifying_on_begin_update() -> TestResult {
        let blocker = FfbBlocker::new();
        assert_eq!(blocker.get_state().await, UpdateState::Idle);

        blocker.begin_update("dev-1").await?;
        assert_eq!(blocker.get_state().await, UpdateState::Verifying);
        Ok(())
    }

    #[tokio::test]
    async fn walk_through_full_lifecycle() -> TestResult {
        let blocker = FfbBlocker::new();

        // Idle → begin → Verifying
        blocker.begin_update("dev-1").await?;
        assert_eq!(blocker.get_state().await, UpdateState::Verifying);

        // Verifying → Downloading
        blocker
            .set_state(UpdateState::Downloading { progress: 0 })
            .await;
        assert_eq!(
            blocker.get_state().await,
            UpdateState::Downloading { progress: 0 }
        );

        // Downloading progress
        blocker
            .set_state(UpdateState::Downloading { progress: 50 })
            .await;
        assert_eq!(
            blocker.get_state().await,
            UpdateState::Downloading { progress: 50 }
        );

        // Downloading → Flashing
        blocker
            .set_state(UpdateState::Flashing { progress: 0 })
            .await;
        assert_eq!(
            blocker.get_state().await,
            UpdateState::Flashing { progress: 0 }
        );

        // Flashing → Rebooting
        blocker.set_state(UpdateState::Rebooting).await;
        assert_eq!(blocker.get_state().await, UpdateState::Rebooting);

        // Rebooting → Complete
        blocker.set_state(UpdateState::Complete).await;
        assert_eq!(blocker.get_state().await, UpdateState::Complete);

        // end_update resets to Idle
        blocker.end_update().await;
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        Ok(())
    }

    #[tokio::test]
    async fn transition_to_failed_recoverable() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        blocker
            .set_state(UpdateState::Flashing { progress: 30 })
            .await;

        let failed = UpdateState::Failed {
            error: "write error".to_string(),
            recoverable: true,
        };
        blocker.set_state(failed.clone()).await;
        assert_eq!(blocker.get_state().await, failed);
        assert!(!blocker.get_state().await.is_in_progress());

        blocker.end_update().await;
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        Ok(())
    }

    #[tokio::test]
    async fn transition_to_failed_unrecoverable() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        let failed = UpdateState::Failed {
            error: "hardware fault".to_string(),
            recoverable: false,
        };
        blocker.set_state(failed.clone()).await;
        assert_eq!(blocker.get_state().await, failed);
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocked_during_active_states() -> TestResult {
        let blocker = FfbBlocker::new();
        assert!(!blocker.is_ffb_blocked());
        assert!(blocker.try_ffb_operation().is_ok());

        blocker.begin_update("dev-1").await?;
        assert!(blocker.is_ffb_blocked());
        assert!(blocker.try_ffb_operation().is_err());

        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());
        assert!(blocker.try_ffb_operation().is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_update_prevention() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        let result = blocker.begin_update("dev-2").await;
        assert!(result.is_err());
        let err = result.err().ok_or("expected error")?;
        assert!(
            matches!(err, FirmwareUpdateError::UpdateInProgress(_)),
            "expected UpdateInProgress, got {:?}",
            err
        );

        blocker.end_update().await;
        // After ending, a new update should succeed
        let result2 = blocker.begin_update("dev-2").await;
        assert!(result2.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn updating_device_tracked() -> TestResult {
        let blocker = FfbBlocker::new();
        assert!(blocker.get_updating_device().await.is_none());

        blocker.begin_update("wheel-42").await?;
        assert_eq!(
            blocker.get_updating_device().await.as_deref(),
            Some("wheel-42")
        );

        blocker.end_update().await;
        assert!(blocker.get_updating_device().await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn downloading_progress_increments() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        for pct in [0u8, 10, 25, 50, 75, 100] {
            blocker
                .set_state(UpdateState::Downloading { progress: pct })
                .await;
            let state = blocker.get_state().await;
            assert_eq!(state, UpdateState::Downloading { progress: pct });
            assert!(state.is_in_progress());
        }
        Ok(())
    }

    #[tokio::test]
    async fn flashing_progress_increments() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        for pct in [0u8, 25, 50, 99, 100] {
            blocker
                .set_state(UpdateState::Flashing { progress: pct })
                .await;
            let state = blocker.get_state().await;
            assert_eq!(state, UpdateState::Flashing { progress: pct });
            assert!(state.should_block_ffb());
        }
        Ok(())
    }
}

// ===========================================================================
// 9. Rollback manager operations
// ===========================================================================

mod rollback_operations {
    use super::*;

    #[tokio::test]
    async fn create_and_list_backup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("firmware.bin"), b"original fw data").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        mgr.create_backup(
            "bk-001",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("firmware.bin")],
        )
        .await?;

        let backups = mgr.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].metadata.backup_id, "bk-001");
        assert!(backups[0].valid);
        Ok(())
    }

    #[tokio::test]
    async fn rollback_restores_original_content() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        let original_content = b"original firmware v1.0";
        tokio::fs::write(install_dir.join("fw.bin"), original_content).await?;

        let mgr = RollbackManager::new(backup_dir, install_dir.clone());
        mgr.create_backup(
            "bk-rollback",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Simulate a failed update by overwriting the file
        tokio::fs::write(install_dir.join("fw.bin"), b"broken firmware v2.0").await?;

        // Rollback should restore original
        mgr.rollback_to("bk-rollback").await?;
        let restored = tokio::fs::read(install_dir.join("fw.bin")).await?;
        assert_eq!(restored.as_slice(), original_content);
        Ok(())
    }

    #[tokio::test]
    async fn rollback_to_nonexistent_backup_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        let result = mgr.rollback_to("nonexistent").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn latest_backup_returns_most_recent() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"data").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        mgr.create_backup(
            "bk-old",
            semver::Version::new(1, 0, 0),
            semver::Version::new(1, 1, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Small delay to ensure different timestamps
        tokio::time::sleep(Duration::from_millis(50)).await;

        mgr.create_backup(
            "bk-new",
            semver::Version::new(1, 1, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        let latest = mgr.get_latest_backup().await?;
        assert!(latest.is_some());
        let latest = latest.ok_or("no latest backup")?;
        assert_eq!(latest.backup_id, "bk-new");
        Ok(())
    }

    #[tokio::test]
    async fn empty_backup_dir_returns_no_backups() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        let backups = mgr.get_backup_info().await?;
        assert!(backups.is_empty());

        let latest = mgr.get_latest_backup().await?;
        assert!(latest.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn backup_preserves_version_metadata() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"data").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir);
        mgr.create_backup(
            "bk-meta",
            semver::Version::new(3, 1, 4),
            semver::Version::new(3, 2, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        let backups = mgr.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert_eq!(
            backups[0].metadata.original_version,
            semver::Version::new(3, 1, 4)
        );
        assert_eq!(
            backups[0].metadata.target_version,
            semver::Version::new(3, 2, 0)
        );
        assert_eq!(backups[0].metadata.files, vec![PathBuf::from("fw.bin")]);
        Ok(())
    }

    #[tokio::test]
    async fn rollback_with_multiple_files() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"firmware-v1").await?;
        tokio::fs::write(install_dir.join("config.dat"), b"config-v1").await?;

        let mgr = RollbackManager::new(backup_dir, install_dir.clone());
        mgr.create_backup(
            "bk-multi",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin"), PathBuf::from("config.dat")],
        )
        .await?;

        // Overwrite both files
        tokio::fs::write(install_dir.join("fw.bin"), b"firmware-v2").await?;
        tokio::fs::write(install_dir.join("config.dat"), b"config-v2").await?;

        mgr.rollback_to("bk-multi").await?;

        let fw = tokio::fs::read(install_dir.join("fw.bin")).await?;
        let cfg = tokio::fs::read(install_dir.join("config.dat")).await?;
        assert_eq!(fw.as_slice(), b"firmware-v1");
        assert_eq!(cfg.as_slice(), b"config-v1");
        Ok(())
    }
}

// ===========================================================================
// 10. Power failure recovery simulation
// ===========================================================================

mod power_failure_recovery {
    use super::*;

    #[tokio::test]
    async fn resume_after_interrupted_backup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"original").await?;

        let mgr = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        mgr.create_backup(
            "bk-checkpoint",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Simulate power failure during update: file is partially written
        tokio::fs::write(install_dir.join("fw.bin"), b"partial").await?;

        // On recovery, reload the rollback manager and recover from checkpoint
        let mgr2 = RollbackManager::new(backup_dir, install_dir.clone());
        let latest = mgr2.get_latest_backup().await?;
        assert!(latest.is_some(), "backup must survive simulated power loss");

        // Restore from the checkpoint
        mgr2.rollback_to("bk-checkpoint").await?;
        let restored = tokio::fs::read(install_dir.join("fw.bin")).await?;
        assert_eq!(restored.as_slice(), b"original");
        Ok(())
    }

    #[tokio::test]
    async fn state_resets_to_idle_after_recovery() -> TestResult {
        let blocker = FfbBlocker::new();
        blocker.begin_update("dev-1").await?;

        blocker
            .set_state(UpdateState::Flashing { progress: 42 })
            .await;
        assert!(blocker.is_ffb_blocked());

        // Simulate recovery: end_update resets everything
        blocker.end_update().await;
        assert_eq!(blocker.get_state().await, UpdateState::Idle);
        assert!(!blocker.is_ffb_blocked());
        assert!(blocker.get_updating_device().await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn backup_integrity_verified_on_recovery() -> TestResult {
        let dir = tempfile::tempdir()?;
        let backup_dir = dir.path().join("backups");
        let install_dir = dir.path().join("install");
        tokio::fs::create_dir_all(&backup_dir).await?;
        tokio::fs::create_dir_all(&install_dir).await?;

        tokio::fs::write(install_dir.join("fw.bin"), b"firmware data").await?;

        let mgr = RollbackManager::new(backup_dir.clone(), install_dir.clone());
        mgr.create_backup(
            "bk-verify",
            semver::Version::new(1, 0, 0),
            semver::Version::new(2, 0, 0),
            &[PathBuf::from("fw.bin")],
        )
        .await?;

        // Verify backup is intact
        let backups = mgr.get_backup_info().await?;
        assert_eq!(backups.len(), 1);
        assert!(backups[0].valid, "backup should be valid after creation");
        assert!(backups[0].size_bytes > 0, "backup should have content");
        Ok(())
    }

    #[tokio::test]
    async fn ffb_blocker_survives_rapid_begin_end_cycles() -> TestResult {
        let blocker = FfbBlocker::new();

        for i in 0..10 {
            let dev = format!("dev-{}", i);
            blocker.begin_update(&dev).await?;
            assert!(blocker.is_ffb_blocked());
            assert_eq!(
                blocker.get_updating_device().await.as_deref(),
                Some(dev.as_str())
            );

            blocker
                .set_state(UpdateState::Flashing { progress: 50 })
                .await;
            blocker.set_state(UpdateState::Complete).await;
            blocker.end_update().await;

            assert!(!blocker.is_ffb_blocked());
            assert_eq!(blocker.get_state().await, UpdateState::Idle);
        }
        Ok(())
    }
}

// ===========================================================================
// 11. A/B partition selection
// ===========================================================================

mod ab_partition_selection {
    use super::*;

    #[test]
    fn active_a_targets_b() {
        let active = Partition::A;
        let target = active.other();
        assert_eq!(target, Partition::B);
    }

    #[test]
    fn active_b_targets_a() {
        let active = Partition::B;
        let target = active.other();
        assert_eq!(target, Partition::A);
    }

    #[test]
    fn inactive_healthy_partition_accepts_update() {
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
    fn inactive_degraded_partition_accepts_update() {
        let info = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: false,
            version: None,
            size_bytes: 0,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Degraded {
                reason: "wear leveling".to_string(),
            },
        };
        assert!(info.can_update());
    }

    #[test]
    fn inactive_unknown_partition_accepts_update() {
        let info = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: false,
            version: None,
            size_bytes: 0,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Unknown,
        };
        assert!(info.can_update());
    }

    #[test]
    fn corrupted_partition_rejects_update() {
        // Note: can_update checks for Corrupted with empty reason string
        let info = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: false,
            version: None,
            size_bytes: 0,
            hash: None,
            updated_at: None,
            health: PartitionHealth::Corrupted {
                reason: String::new(),
            },
        };
        assert!(!info.can_update());
    }

    #[test]
    fn active_partition_always_rejects_update() {
        for health in [
            PartitionHealth::Healthy,
            PartitionHealth::Unknown,
            PartitionHealth::Degraded {
                reason: "test".to_string(),
            },
        ] {
            let info = PartitionInfo {
                partition: Partition::A,
                active: true,
                bootable: true,
                version: Some(semver::Version::new(1, 0, 0)),
                size_bytes: 1024,
                hash: None,
                updated_at: None,
                health,
            };
            assert!(
                !info.can_update(),
                "active partition should reject updates regardless of health"
            );
        }
    }

    #[test]
    fn partition_with_existing_version_can_update_if_inactive() {
        let info = PartitionInfo {
            partition: Partition::B,
            active: false,
            bootable: true,
            version: Some(semver::Version::new(1, 0, 0)),
            size_bytes: 65536,
            hash: Some("abc123".to_string()),
            updated_at: Some(chrono::Utc::now()),
            health: PartitionHealth::Healthy,
        };
        assert!(info.can_update());
    }
}

// ===========================================================================
// 12. Update progress reporting
// ===========================================================================

mod progress_reporting {
    use super::*;

    #[test]
    fn progress_fields_round_trip_serde() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Transferring,
            progress_percent: 42,
            bytes_transferred: 2048,
            total_bytes: 4096,
            transfer_rate_bps: 1024,
            eta_seconds: Some(2),
            status_message: "Transferring data".to_string(),
            warnings: vec!["slow link".to_string()],
        };
        let json = serde_json::to_string(&progress)?;
        let decoded: UpdateProgress = serde_json::from_str(&json)?;
        assert_eq!(decoded.progress_percent, 42);
        assert_eq!(decoded.bytes_transferred, 2048);
        assert_eq!(decoded.total_bytes, 4096);
        assert_eq!(decoded.transfer_rate_bps, 1024);
        assert_eq!(decoded.eta_seconds, Some(2));
        assert_eq!(decoded.status_message, "Transferring data");
        assert_eq!(decoded.warnings.len(), 1);
        Ok(())
    }

    #[test]
    fn all_phases_serializable() -> TestResult {
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
        for phase in phases {
            let progress = UpdateProgress {
                phase,
                progress_percent: 0,
                bytes_transferred: 0,
                total_bytes: 0,
                transfer_rate_bps: 0,
                eta_seconds: None,
                status_message: String::new(),
                warnings: Vec::new(),
            };
            let json = serde_json::to_string(&progress)?;
            let _decoded: UpdateProgress = serde_json::from_str(&json)?;
        }
        Ok(())
    }

    #[test]
    fn progress_with_zero_bytes_valid() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Initializing,
            progress_percent: 0,
            bytes_transferred: 0,
            total_bytes: 0,
            transfer_rate_bps: 0,
            eta_seconds: None,
            status_message: "Starting".to_string(),
            warnings: Vec::new(),
        };
        let json = serde_json::to_string(&progress)?;
        let decoded: UpdateProgress = serde_json::from_str(&json)?;
        assert_eq!(decoded.progress_percent, 0);
        assert_eq!(decoded.total_bytes, 0);
        Ok(())
    }

    #[test]
    fn progress_at_completion() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Completed,
            progress_percent: 100,
            bytes_transferred: 65536,
            total_bytes: 65536,
            transfer_rate_bps: 0,
            eta_seconds: Some(0),
            status_message: "Firmware update completed successfully".to_string(),
            warnings: Vec::new(),
        };
        assert_eq!(progress.progress_percent, 100);
        assert_eq!(progress.bytes_transferred, progress.total_bytes);
        let json = serde_json::to_string(&progress)?;
        let decoded: UpdateProgress = serde_json::from_str(&json)?;
        assert_eq!(decoded.eta_seconds, Some(0));
        Ok(())
    }

    #[test]
    fn progress_with_multiple_warnings() -> TestResult {
        let progress = UpdateProgress {
            phase: UpdatePhase::Transferring,
            progress_percent: 80,
            bytes_transferred: 8192,
            total_bytes: 10240,
            transfer_rate_bps: 512,
            eta_seconds: Some(4),
            status_message: "Transfer in progress".to_string(),
            warnings: vec![
                "battery low".to_string(),
                "slow transfer rate".to_string(),
                "retry count: 3".to_string(),
            ],
        };
        let json = serde_json::to_string(&progress)?;
        let decoded: UpdateProgress = serde_json::from_str(&json)?;
        assert_eq!(decoded.warnings.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn manager_subscribe_progress_channel() -> TestResult {
        let mgr = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let mut rx = mgr.subscribe_progress();

        // No updates active — channel should be empty (try_recv fails)
        assert!(rx.try_recv().is_err());
        Ok(())
    }
}

// ===========================================================================
// 13. Update cancellation
// ===========================================================================

mod update_cancellation {
    use super::*;

    #[tokio::test]
    async fn cancel_nonexistent_update_fails() -> TestResult {
        let mgr = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let result = mgr.cancel_update("no-such-device").await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn no_active_updates_initially() -> TestResult {
        let mgr = FirmwareUpdateManager::new(StagedRolloutConfig::default());
        let active = mgr.get_active_updates().await;
        assert!(active.is_empty());
        assert!(!mgr.is_update_in_progress().await);
        Ok(())
    }

    #[test]
    fn update_result_cancelled_fields() -> TestResult {
        let result = UpdateResult {
            device_id: "dev-cancel".to_string(),
            success: false,
            old_version: Some(semver::Version::new(1, 0, 0)),
            new_version: None,
            updated_partition: None,
            rollback_performed: false,
            duration: Duration::from_secs(5),
            error: Some("Update cancelled by user".to_string()),
            partition_states: Vec::new(),
        };
        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|e| e.contains("cancelled"))
        );

        let json = serde_json::to_string(&result)?;
        let decoded: UpdateResult = serde_json::from_str(&json)?;
        assert_eq!(decoded.device_id, "dev-cancel");
        assert!(!decoded.success);
        Ok(())
    }
}

// ===========================================================================
// 14. Firmware version comparison
// ===========================================================================

mod firmware_version_comparison {
    use super::*;

    #[test]
    fn semver_ordering() {
        let v1 = semver::Version::new(1, 0, 0);
        let v1_1 = semver::Version::new(1, 1, 0);
        let v2 = semver::Version::new(2, 0, 0);

        assert!(v1 < v1_1);
        assert!(v1_1 < v2);
        assert!(v1 < v2);
    }

    #[test]
    fn semver_patch_ordering() {
        let v1_0_0 = semver::Version::new(1, 0, 0);
        let v1_0_1 = semver::Version::new(1, 0, 1);
        let v1_0_10 = semver::Version::new(1, 0, 10);

        assert!(v1_0_0 < v1_0_1);
        assert!(v1_0_1 < v1_0_10);
    }

    #[test]
    fn firmware_image_version_comparison() {
        let img1 = test_image(&[1, 2, 3]);
        let img2 = FirmwareImage {
            version: semver::Version::new(3, 0, 0),
            ..test_image(&[4, 5, 6])
        };
        assert!(img1.version < img2.version);
    }

    #[test]
    fn bundle_upgrade_from_older_allowed() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(3, 0, 0),
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
            rollback_version: Some(semver::Version::new(2, 0, 0)),
            ..BundleMetadata::default()
        };
        let bundle = FirmwareBundle::new(&image, metadata, CompressionType::None)?;

        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 0, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(2, 5, 0)));
        assert!(bundle.allows_upgrade_from(&semver::Version::new(3, 0, 0)));
        assert!(!bundle.allows_upgrade_from(&semver::Version::new(1, 9, 9)));
        Ok(())
    }

    #[test]
    fn hardware_version_multi_component() -> Result<(), HardwareVersionError> {
        let v1 = HardwareVersion::parse("1.2.3")?;
        let v2 = HardwareVersion::parse("1.2.4")?;
        let v3 = HardwareVersion::parse("1.3.0")?;

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
        Ok(())
    }

    #[test]
    fn hardware_version_single_component() -> Result<(), HardwareVersionError> {
        let v1 = HardwareVersion::parse("5")?;
        let v2 = HardwareVersion::parse("10")?;
        assert!(v1 < v2);

        assert_eq!(v1.components(), &[5]);
        assert_eq!(v2.components(), &[10]);
        Ok(())
    }
}

// ===========================================================================
// 15. Firmware compatibility checking (extended)
// ===========================================================================

mod compatibility_extended {
    use super::*;

    #[test]
    fn bundle_no_hw_constraints_compatible_with_anything() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(1, 0, 0),
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

        assert!(bundle.is_compatible_with_hardware("0.1"));
        assert!(bundle.is_compatible_with_hardware("1.0"));
        assert!(bundle.is_compatible_with_hardware("99.99"));
        Ok(())
    }

    #[test]
    fn bundle_only_min_hw_version() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: Some("3.0".to_string()),
            max_hardware_version: None,
            data,
            hash,
            size_bytes: 8,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;

        assert!(!bundle.is_compatible_with_hardware("2.9"));
        assert!(bundle.is_compatible_with_hardware("3.0"));
        assert!(bundle.is_compatible_with_hardware("100.0"));
        Ok(())
    }

    #[test]
    fn bundle_only_max_hw_version() -> TestResult {
        let data = vec![0x01; 8];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "wheel".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: None,
            max_hardware_version: Some("5.0".to_string()),
            data,
            hash,
            size_bytes: 8,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;

        assert!(bundle.is_compatible_with_hardware("0.1"));
        assert!(bundle.is_compatible_with_hardware("5.0"));
        assert!(!bundle.is_compatible_with_hardware("5.1"));
        Ok(())
    }

    #[test]
    fn bundle_header_fields_from_image() -> TestResult {
        let data = vec![0xAA; 64];
        let hash = openracing_crypto::utils::compute_sha256_hex(&data);
        let image = FirmwareImage {
            device_model: "pro-wheel-gt".to_string(),
            version: semver::Version::new(4, 2, 1),
            min_hardware_version: Some("2.0".to_string()),
            max_hardware_version: Some("8.0".to_string()),
            data: data.clone(),
            hash,
            size_bytes: 64,
            build_timestamp: chrono::Utc::now(),
            release_notes: Some("Performance fixes".to_string()),
            signature: None,
        };
        let bundle = FirmwareBundle::new(&image, BundleMetadata::default(), CompressionType::None)?;

        assert_eq!(bundle.header.device_model, "pro-wheel-gt");
        assert_eq!(
            bundle.header.firmware_version,
            semver::Version::new(4, 2, 1)
        );
        assert_eq!(bundle.header.min_hw_version.as_deref(), Some("2.0"));
        assert_eq!(bundle.header.max_hw_version.as_deref(), Some("8.0"));
        assert_eq!(bundle.header.uncompressed_size, 64);
        assert!(!bundle.header.payload_hash.is_empty());
        Ok(())
    }

    #[test]
    fn extract_image_preserves_data() -> TestResult {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x42];
        let bundle = make_bundle(&data, CompressionType::Gzip)?;
        let extracted = bundle.extract_image()?;

        assert_eq!(extracted.data, data);
        assert_eq!(extracted.device_model, "test-wheel");
        assert_eq!(extracted.version, semver::Version::new(2, 0, 0));
        assert_eq!(extracted.size_bytes, data.len() as u64);
        Ok(())
    }
}
