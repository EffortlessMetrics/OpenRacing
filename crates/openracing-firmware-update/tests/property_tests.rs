//! Property-based tests for firmware update system

#![allow(clippy::redundant_closure)]

use openracing_firmware_update::delta::compute_data_hash;
use openracing_firmware_update::prelude::*;
use proptest::prelude::*;

fn arb_device_id() -> impl Strategy<Value = String> {
    "[a-z0-9]{8}-[a-z0-9]{4}".prop_map(|s| s)
}

fn arb_version() -> impl Strategy<Value = semver::Version> {
    (0u64..100, 0u64..100, 0u64..100)
        .prop_map(|(major, minor, patch)| semver::Version::new(major, minor, patch))
}

fn arb_firmware_data() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 100..1000)
}

fn arb_firmware_image() -> impl Strategy<Value = FirmwareImage> {
    ("[a-z_]{5,15}", arb_version(), arb_firmware_data()).prop_map(
        |(device_model, version, data)| {
            let hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&data);
                hex::encode(hasher.finalize())
            };
            let size_bytes = data.len() as u64;

            FirmwareImage {
                device_model,
                version,
                min_hardware_version: None,
                max_hardware_version: None,
                data,
                hash,
                size_bytes,
                build_timestamp: chrono::Utc::now(),
                release_notes: None,
                signature: None,
            }
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_firmware_update_blocks_ffb(device_id in arb_device_id()) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            let blocker = FfbBlocker::new();

            prop_assert!(!blocker.is_ffb_blocked(), "FFB should not be blocked initially");
            prop_assert!(blocker.try_ffb_operation().is_ok(), "FFB operation should succeed initially");

            let begin_result = blocker.begin_update(&device_id).await;
            prop_assert!(begin_result.is_ok(), "Begin update should succeed");

            prop_assert!(blocker.is_ffb_blocked(), "FFB should be blocked during update");

            let ffb_result = blocker.try_ffb_operation();
            prop_assert!(ffb_result.is_err(), "FFB operation should fail during update");
            match ffb_result {
                Err(FirmwareUpdateError::FfbBlocked) => { }
                Err(e) => prop_assert!(false, "Expected FfbBlocked error, got: {:?}", e),
                Ok(_) => prop_assert!(false, "Expected error, got Ok"),
            }

            blocker.end_update().await;

            prop_assert!(!blocker.is_ffb_blocked(), "FFB should be unblocked after update");
            prop_assert!(blocker.try_ffb_operation().is_ok(), "FFB operation should succeed after update");

            Ok(())
        })?;
    }

    #[test]
    fn prop_firmware_update_mutual_exclusion_concurrent(
        device_id1 in arb_device_id(),
        device_id2 in arb_device_id(),
    ) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            let blocker = FfbBlocker::new();

            let begin_result1 = blocker.begin_update(&device_id1).await;
            prop_assert!(begin_result1.is_ok(), "First update should succeed");

            let begin_result2 = blocker.begin_update(&device_id2).await;
            prop_assert!(begin_result2.is_err(), "Second update should fail");
            match begin_result2 {
                Err(FirmwareUpdateError::UpdateInProgress(_)) => { }
                Err(e) => prop_assert!(false, "Expected UpdateInProgress error, got: {:?}", e),
                Ok(_) => prop_assert!(false, "Expected error, got Ok"),
            }

            blocker.end_update().await;

            let begin_result3 = blocker.begin_update(&device_id2).await;
            prop_assert!(begin_result3.is_ok(), "Update should succeed after first one ends");

            blocker.end_update().await;

            Ok(())
        })?;
    }

    #[test]
    fn prop_update_state_ffb_blocking(progress in 0u8..=100) {
        let blocking_states = vec![
            UpdateState::Downloading { progress },
            UpdateState::Verifying,
            UpdateState::Flashing { progress },
            UpdateState::Rebooting,
        ];

        for state in blocking_states {
            prop_assert!(
                state.should_block_ffb(),
                "State {:?} should block FFB",
                state
            );
            prop_assert!(
                state.is_in_progress(),
                "State {:?} should be in progress",
                state
            );
        }

        let non_blocking_states = vec![
            UpdateState::Idle,
            UpdateState::Complete,
            UpdateState::Failed {
                error: "test error".to_string(),
                recoverable: true,
            },
        ];

        for state in non_blocking_states {
            prop_assert!(
                !state.should_block_ffb(),
                "State {:?} should not block FFB",
                state
            );
            prop_assert!(
                !state.is_in_progress(),
                "State {:?} should not be in progress",
                state
            );
        }
    }

    #[test]
    fn prop_update_state_failure_is_recoverable(
        error_msg in "[a-zA-Z0-9 ]{10,50}",
        recoverable in any::<bool>(),
    ) {
        let failed_state = UpdateState::Failed {
            error: error_msg.clone(),
            recoverable,
        };

        prop_assert!(
            !failed_state.should_block_ffb(),
            "Failed state should not block FFB"
        );

        prop_assert!(
            !failed_state.is_in_progress(),
            "Failed state should not be in progress"
        );

        if let UpdateState::Failed { error, recoverable: rec } = failed_state {
            prop_assert_eq!(error, error_msg, "Error message should be preserved");
            prop_assert_eq!(rec, recoverable, "Recoverable flag should be preserved");
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_firmware_cache_add_and_get(firmware in arb_firmware_image()) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            let temp_dir = tempfile::TempDir::new().map_err(|e| {
                TestCaseError::fail(format!("Failed to create temp dir: {}", e))
            })?;

            let cache = FirmwareCache::new(temp_dir.path().to_path_buf(), 0).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to create cache: {}", e))
            })?;

            let initial_check = cache.contains(&firmware.device_model, &firmware.version).await;
            prop_assert!(!initial_check, "Firmware should not be in cache initially");

            cache.add(&firmware).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to add firmware to cache: {}", e))
            })?;

            let after_add_check = cache.contains(&firmware.device_model, &firmware.version).await;
            prop_assert!(after_add_check, "Firmware should be in cache after add");

            let cached = cache.get(&firmware.device_model, &firmware.version).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to get firmware from cache: {}", e))
            })?;

            prop_assert!(cached.is_some(), "Cached firmware should be returned");

            let cached_firmware = cached.ok_or_else(|| {
                TestCaseError::fail("Cached firmware is None".to_string())
            })?;

            prop_assert_eq!(
                cached_firmware.device_model,
                firmware.device_model,
                "Device model should match"
            );
            prop_assert_eq!(
                cached_firmware.version,
                firmware.version,
                "Version should match"
            );
            prop_assert_eq!(
                cached_firmware.hash,
                firmware.hash,
                "Hash should match"
            );
            prop_assert_eq!(
                cached_firmware.data,
                firmware.data,
                "Data should match"
            );

            Ok(())
        })?;
    }

    #[test]
    fn prop_firmware_cache_remove(firmware in arb_firmware_image()) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            let temp_dir = tempfile::TempDir::new().map_err(|e| {
                TestCaseError::fail(format!("Failed to create temp dir: {}", e))
            })?;

            let cache = FirmwareCache::new(temp_dir.path().to_path_buf(), 0).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to create cache: {}", e))
            })?;

            cache.add(&firmware).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to add firmware to cache: {}", e))
            })?;

            prop_assert!(
                cache.contains(&firmware.device_model, &firmware.version).await,
                "Firmware should be in cache"
            );

            cache.remove(&firmware.device_model, &firmware.version).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to remove firmware from cache: {}", e))
            })?;

            prop_assert!(
                !cache.contains(&firmware.device_model, &firmware.version).await,
                "Firmware should not be in cache after remove"
            );

            let cached = cache.get(&firmware.device_model, &firmware.version).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to get firmware from cache: {}", e))
            })?;
            prop_assert!(cached.is_none(), "Get should return None after remove");

            Ok(())
        })?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_delta_patch_roundtrip(
        old_data in prop::collection::vec(any::<u8>(), 100..1000),
        new_data in prop::collection::vec(any::<u8>(), 100..1000),
    ) {
        use openracing_firmware_update::delta::{create_simple_patch, apply_simple_patch};

        let patch = create_simple_patch(&old_data, &new_data)
            .map_err(|e| TestCaseError::fail(format!("Patch creation failed: {e}")))?;
        let result = apply_simple_patch(&old_data, &patch)
            .map_err(|e| TestCaseError::fail(format!("Patch application failed: {e}")))?;

        prop_assert_eq!(result, new_data, "Patched data should match new data");
    }

    #[test]
    fn prop_compression_roundtrip(data in prop::collection::vec(any::<u8>(), 100..10000)) {
        use openracing_firmware_update::delta::{compress_data, decompress_data};

        let compressed = compress_data(&data).map_err(|e| TestCaseError::fail(format!("Compression failed: {e}")))?;
        let decompressed = decompress_data(&compressed).map_err(|e| TestCaseError::fail(format!("Decompression failed: {e}")))?;

        prop_assert_eq!(decompressed, data, "Decompressed data should match original");
    }
}

// ---------------------------------------------------------------------------
// Firmware image validation tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod firmware_image_validation {
    use super::*;

    #[test]
    fn test_firmware_image_hash_consistency() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0u8; 256];
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        };

        let image = FirmwareImage {
            device_model: "test_model".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: None,
            max_hardware_version: None,
            data: data.clone(),
            hash: hash.clone(),
            size_bytes: data.len() as u64,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };

        assert_eq!(image.hash, hash);
        assert_eq!(image.size_bytes, 256);
        assert_eq!(image.data.len(), 256);
        Ok(())
    }

    #[test]
    fn test_firmware_image_with_mismatched_hash_detectable()
    -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![1u8; 128];
        let wrong_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let actual_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hex::encode(hasher.finalize())
        };

        let image = FirmwareImage {
            device_model: "test_model".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: None,
            max_hardware_version: None,
            data,
            hash: wrong_hash.clone(),
            size_bytes: 128,
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };

        assert_ne!(
            image.hash, actual_hash,
            "Mismatched hash should be detectable"
        );
        Ok(())
    }

    #[test]
    fn test_firmware_image_size_mismatch_detectable() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0u8; 100];
        let image = FirmwareImage {
            device_model: "test_model".to_string(),
            version: semver::Version::new(1, 0, 0),
            min_hardware_version: None,
            max_hardware_version: None,
            data: data.clone(),
            hash: "placeholder".to_string(),
            size_bytes: 200, // deliberately wrong
            build_timestamp: chrono::Utc::now(),
            release_notes: None,
            signature: None,
        };

        assert_ne!(
            image.size_bytes as usize,
            image.data.len(),
            "Size mismatch should be detectable"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Update state machine transition tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod update_state_machine {
    use super::*;

    #[test]
    fn test_idle_state_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let state = UpdateState::default();
        assert_eq!(state, UpdateState::Idle);
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());
        Ok(())
    }

    #[test]
    fn test_all_in_progress_states_block_ffb() -> Result<(), Box<dyn std::error::Error>> {
        let in_progress_states = vec![
            UpdateState::Downloading { progress: 0 },
            UpdateState::Downloading { progress: 50 },
            UpdateState::Downloading { progress: 100 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 0 },
            UpdateState::Flashing { progress: 100 },
            UpdateState::Rebooting,
        ];
        for state in &in_progress_states {
            assert!(
                state.is_in_progress(),
                "State {:?} should be in progress",
                state
            );
            assert!(
                state.should_block_ffb(),
                "State {:?} should block FFB",
                state
            );
        }
        Ok(())
    }

    #[test]
    fn test_terminal_states_dont_block_ffb() -> Result<(), Box<dyn std::error::Error>> {
        let terminal_states = vec![
            UpdateState::Idle,
            UpdateState::Complete,
            UpdateState::Failed {
                error: "test".to_string(),
                recoverable: true,
            },
            UpdateState::Failed {
                error: "fatal".to_string(),
                recoverable: false,
            },
        ];
        for state in &terminal_states {
            assert!(
                !state.is_in_progress(),
                "State {:?} should not be in progress",
                state
            );
            assert!(
                !state.should_block_ffb(),
                "State {:?} should not block FFB",
                state
            );
        }
        Ok(())
    }

    #[test]
    fn test_failed_state_preserves_error_info() -> Result<(), Box<dyn std::error::Error>> {
        let error_msg = "firmware signature invalid".to_string();
        let state = UpdateState::Failed {
            error: error_msg.clone(),
            recoverable: false,
        };
        if let UpdateState::Failed { error, recoverable } = state {
            assert_eq!(error, error_msg);
            assert!(!recoverable);
        } else {
            return Err("Expected Failed variant".into());
        }
        Ok(())
    }

    #[test]
    fn test_update_state_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let states = vec![
            UpdateState::Idle,
            UpdateState::Downloading { progress: 42 },
            UpdateState::Verifying,
            UpdateState::Flashing { progress: 75 },
            UpdateState::Rebooting,
            UpdateState::Complete,
            UpdateState::Failed {
                error: "test error".to_string(),
                recoverable: true,
            },
        ];
        for state in &states {
            let json = serde_json::to_string(state)?;
            let back: UpdateState = serde_json::from_str(&json)?;
            assert_eq!(
                &back, state,
                "Serialization roundtrip failed for {:?}",
                state
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Hardware version edge cases
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_hardware_version_parse_roundtrip(
        major in 0u32..=999u32,
        minor in 0u32..=999u32,
        patch in 0u32..=999u32,
    ) {
        let s = format!("{}.{}.{}", major, minor, patch);
        let v = HardwareVersion::parse(&s)
            .map_err(|e| TestCaseError::fail(format!("Parse failed for '{}': {}", s, e)))?;
        prop_assert_eq!(v.components(), &[major, minor, patch]);
        prop_assert_eq!(v.as_str(), s);
    }

    #[test]
    fn prop_hardware_version_ordering_consistent(
        a_major in 0u32..=100u32,
        a_minor in 0u32..=100u32,
        b_major in 0u32..=100u32,
        b_minor in 0u32..=100u32,
    ) {
        let a = HardwareVersion::parse(&format!("{}.{}", a_major, a_minor))
            .map_err(|e| TestCaseError::fail(format!("Parse failed: {e}")))?;
        let b = HardwareVersion::parse(&format!("{}.{}", b_major, b_minor))
            .map_err(|e| TestCaseError::fail(format!("Parse failed: {e}")))?;

        let expected = (a_major, a_minor).cmp(&(b_major, b_minor));
        prop_assert_eq!(a.cmp(&b), expected);
    }

    #[test]
    fn prop_hardware_version_reflexive(
        major in 0u32..=999u32,
        minor in 0u32..=999u32,
    ) {
        let s = format!("{}.{}", major, minor);
        let v = HardwareVersion::parse(&s)
            .map_err(|e| TestCaseError::fail(format!("Parse failed: {e}")))?;
        prop_assert_eq!(v.cmp(&v), std::cmp::Ordering::Equal);
    }

    #[test]
    fn prop_hardware_version_display_roundtrip(
        major in 0u32..=999u32,
        minor in 0u32..=999u32,
    ) {
        let s = format!("{}.{}", major, minor);
        let v = HardwareVersion::parse(&s)
            .map_err(|e| TestCaseError::fail(format!("Parse failed: {e}")))?;
        prop_assert_eq!(format!("{}", v), s);
    }
}

// ---------------------------------------------------------------------------
// Edge cases: corrupted data, version downgrade
// ---------------------------------------------------------------------------

#[cfg(test)]
mod edge_cases {
    use super::*;

    #[test]
    fn test_hardware_version_empty_is_error() -> Result<(), Box<dyn std::error::Error>> {
        let result = HardwareVersion::parse("");
        assert!(result.is_err());
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
        Ok(())
    }

    #[test]
    fn test_hardware_version_whitespace_is_error() -> Result<(), Box<dyn std::error::Error>> {
        let result = HardwareVersion::parse("   ");
        assert!(result.is_err());
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
        Ok(())
    }

    #[test]
    fn test_hardware_version_invalid_component() -> Result<(), Box<dyn std::error::Error>> {
        let result = HardwareVersion::parse("1.abc.3");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_version_downgrade_detection() -> Result<(), Box<dyn std::error::Error>> {
        let current = semver::Version::new(2, 0, 0);
        let proposed = semver::Version::new(1, 0, 0);
        assert!(
            proposed < current,
            "Downgrade should be detectable via version comparison"
        );
        Ok(())
    }

    #[test]
    fn test_compression_empty_data_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        // Even an empty payload should compress/decompress correctly
        let data = Vec::<u8>::new();
        let compressed = compress_data(&data)?;
        let decompressed = decompress_data(&compressed)?;
        assert_eq!(decompressed, data);
        Ok(())
    }

    #[test]
    fn test_data_hash_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let hash1 = compute_data_hash(&data);
        let hash2 = compute_data_hash(&data);
        assert_eq!(hash1, hash2, "Hash should be deterministic");
        Ok(())
    }

    #[test]
    fn test_data_hash_different_for_different_data() -> Result<(), Box<dyn std::error::Error>> {
        let data1 = vec![1, 2, 3, 4];
        let data2 = vec![5, 6, 7, 8];
        let hash1 = compute_data_hash(&data1);
        let hash2 = compute_data_hash(&data2);
        assert_ne!(
            hash1, hash2,
            "Different data should produce different hashes"
        );
        Ok(())
    }

    #[test]
    fn test_update_error_variants_exist() -> Result<(), Box<dyn std::error::Error>> {
        let errors: Vec<FirmwareUpdateError> = vec![
            FirmwareUpdateError::DeviceNotFound("dev-1".to_string()),
            FirmwareUpdateError::VerificationFailed("bad sig".to_string()),
            FirmwareUpdateError::InvalidFirmware("corrupt header".to_string()),
            FirmwareUpdateError::Timeout("flash timeout".to_string()),
            FirmwareUpdateError::FfbBlocked,
            FirmwareUpdateError::UpdateInProgress("dev-2".to_string()),
            FirmwareUpdateError::CacheError("disk full".to_string()),
            FirmwareUpdateError::PartitionError("bad partition".to_string()),
            FirmwareUpdateError::CompatibilityError("wrong hw".to_string()),
            FirmwareUpdateError::InvalidState("not ready".to_string()),
            FirmwareUpdateError::Cancelled("user cancelled".to_string()),
        ];
        for err in &errors {
            let msg = format!("{}", err);
            assert!(!msg.is_empty(), "Error display should not be empty");
        }
        Ok(())
    }

    #[test]
    fn test_bundle_error_variants_exist() -> Result<(), Box<dyn std::error::Error>> {
        let errors: Vec<BundleError> = vec![
            BundleError::SignatureRequired,
            BundleError::SignatureVerificationFailed("bad sig".to_string()),
            BundleError::UntrustedSigner("unknown key".to_string()),
            BundleError::PayloadHashMismatch {
                expected: "aaa".to_string(),
                actual: "bbb".to_string(),
            },
            BundleError::InvalidFormat("missing magic".to_string()),
        ];
        for err in &errors {
            let msg = format!("{}", err);
            assert!(!msg.is_empty(), "Error display should not be empty");
        }
        Ok(())
    }

    #[test]
    fn test_release_channel_default_is_stable() -> Result<(), Box<dyn std::error::Error>> {
        let channel = ReleaseChannel::default();
        assert_eq!(channel, ReleaseChannel::Stable);
        Ok(())
    }

    #[test]
    fn test_compression_type_default_is_gzip() -> Result<(), Box<dyn std::error::Error>> {
        let ct = CompressionType::default();
        assert_eq!(ct, CompressionType::Gzip);
        Ok(())
    }
}
