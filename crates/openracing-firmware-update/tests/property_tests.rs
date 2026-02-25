//! Property-based tests for firmware update system

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
            .expect("Patch creation failed");
        let result = apply_simple_patch(&old_data, &patch)
            .expect("Patch application failed");

        prop_assert_eq!(result, new_data, "Patched data should match new data");
    }

    #[test]
    fn prop_compression_roundtrip(data in prop::collection::vec(any::<u8>(), 100..10000)) {
        use openracing_firmware_update::delta::{compress_data, decompress_data};

        let compressed = compress_data(&data).expect("Compression failed");
        let decompressed = decompress_data(&compressed).expect("Decompression failed");

        prop_assert_eq!(decompressed, data, "Decompressed data should match original");
    }
}
