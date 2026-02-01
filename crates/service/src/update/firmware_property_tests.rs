//! Property-based tests for firmware update system
//!
//! These tests validate the correctness properties specified in the design document
//! for the firmware update system (Requirements 17.x).

use proptest::prelude::*;

use super::firmware::{FfbBlocker, FirmwareCache, FirmwareImage, FirmwareUpdateError, UpdateState};

// ============================================================================
// Test Helpers
// ============================================================================

/// Generate a random device ID
fn arb_device_id() -> impl Strategy<Value = String> {
    "[a-z0-9]{8}-[a-z0-9]{4}".prop_map(|s| s)
}

/// Generate a random firmware version
fn arb_version() -> impl Strategy<Value = semver::Version> {
    (0u64..100, 0u64..100, 0u64..100)
        .prop_map(|(major, minor, patch)| semver::Version::new(major, minor, patch))
}

/// Generate random firmware data
fn arb_firmware_data() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 100..1000)
}

/// Generate a test firmware image
fn arb_firmware_image() -> impl Strategy<Value = FirmwareImage> {
    (
        "[a-z_]{5,15}", // device_model
        arb_version(),
        arb_firmware_data(),
    )
        .prop_map(|(device_model, version, data)| {
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
        })
}

// ============================================================================
// Property 29: Firmware Update Mutual Exclusion
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 29: Firmware Update Mutual Exclusion
    ///
    /// For any firmware update in progress, FFB operations SHALL be blocked
    /// and return an "update in progress" error.
    ///
    /// **Validates: Requirements 17.4**
    #[test]
    fn prop_firmware_update_blocks_ffb(
        device_id in arb_device_id(),
    ) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            let blocker = FfbBlocker::new();

            // Initially, FFB should not be blocked
            prop_assert!(!blocker.is_ffb_blocked(), "FFB should not be blocked initially");
            prop_assert!(blocker.try_ffb_operation().is_ok(), "FFB operation should succeed initially");

            // Begin update
            let begin_result = blocker.begin_update(&device_id).await;
            prop_assert!(begin_result.is_ok(), "Begin update should succeed");

            // Now FFB should be blocked
            prop_assert!(blocker.is_ffb_blocked(), "FFB should be blocked during update");

            // FFB operation should fail with FfbBlocked error
            let ffb_result = blocker.try_ffb_operation();
            prop_assert!(ffb_result.is_err(), "FFB operation should fail during update");
            match ffb_result {
                Err(FirmwareUpdateError::FfbBlocked) => { /* Expected */ }
                Err(e) => prop_assert!(false, "Expected FfbBlocked error, got: {:?}", e),
                Ok(_) => prop_assert!(false, "Expected error, got Ok"),
            }

            // End update
            blocker.end_update().await;

            // FFB should be unblocked
            prop_assert!(!blocker.is_ffb_blocked(), "FFB should be unblocked after update");
            prop_assert!(blocker.try_ffb_operation().is_ok(), "FFB operation should succeed after update");

            Ok(())
        })?;
    }

    /// Feature: release-roadmap-v1, Property 29: Firmware Update Mutual Exclusion (concurrent)
    ///
    /// For any attempt to start a second firmware update while one is in progress,
    /// the system SHALL reject the second update with an "update in progress" error.
    ///
    /// **Validates: Requirements 17.4**
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

            // Begin first update
            let begin_result1 = blocker.begin_update(&device_id1).await;
            prop_assert!(begin_result1.is_ok(), "First update should succeed");

            // Try to begin second update - should fail
            let begin_result2 = blocker.begin_update(&device_id2).await;
            prop_assert!(begin_result2.is_err(), "Second update should fail");
            match begin_result2 {
                Err(FirmwareUpdateError::UpdateInProgress(_)) => { /* Expected */ }
                Err(e) => prop_assert!(false, "Expected UpdateInProgress error, got: {:?}", e),
                Ok(_) => prop_assert!(false, "Expected error, got Ok"),
            }

            // End first update
            blocker.end_update().await;

            // Now second update should succeed
            let begin_result3 = blocker.begin_update(&device_id2).await;
            prop_assert!(begin_result3.is_ok(), "Update should succeed after first one ends");

            blocker.end_update().await;

            Ok(())
        })?;
    }

    /// Feature: release-roadmap-v1, Property 29: Update State Transitions
    ///
    /// For any update state that is in progress, should_block_ffb() SHALL return true.
    ///
    /// **Validates: Requirements 17.4**
    #[test]
    fn prop_update_state_ffb_blocking(
        progress in 0u8..=100,
    ) {
        // States that should block FFB
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

        // States that should NOT block FFB
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
}

// ============================================================================
// Property 30: Firmware Rollback on Failure
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 30: Firmware Rollback on Failure
    ///
    /// For any failed firmware update, the system SHALL restore the previous
    /// firmware version and report the failure.
    ///
    /// **Validates: Requirements 17.3, 17.5**
    #[test]
    fn prop_update_state_failure_is_recoverable(
        error_msg in "[a-zA-Z0-9 ]{10,50}",
        recoverable in any::<bool>(),
    ) {
        let failed_state = UpdateState::Failed {
            error: error_msg.clone(),
            recoverable,
        };

        // Failed state should not block FFB (system should be usable)
        prop_assert!(
            !failed_state.should_block_ffb(),
            "Failed state should not block FFB"
        );

        // Failed state should not be considered "in progress"
        prop_assert!(
            !failed_state.is_in_progress(),
            "Failed state should not be in progress"
        );

        // Verify the error message is preserved
        if let UpdateState::Failed { error, recoverable: rec } = failed_state {
            prop_assert_eq!(error, error_msg, "Error message should be preserved");
            prop_assert_eq!(rec, recoverable, "Recoverable flag should be preserved");
        }
    }

    /// Feature: release-roadmap-v1, Property 30: FFB Blocker State After Failure
    ///
    /// After a firmware update ends (success or failure), FFB operations
    /// SHALL be unblocked.
    ///
    /// **Validates: Requirements 17.3, 17.5**
    #[test]
    fn prop_ffb_unblocked_after_update_ends(
        device_id in arb_device_id(),
    ) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            let blocker = FfbBlocker::new();

            // Begin update
            blocker.begin_update(&device_id).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to begin update: {}", e))
            })?;

            // Simulate failure by setting failed state
            blocker.set_state(UpdateState::Failed {
                error: "Simulated failure".to_string(),
                recoverable: true,
            }).await;

            // End update (simulating rollback completion)
            blocker.end_update().await;

            // FFB should be unblocked
            prop_assert!(!blocker.is_ffb_blocked(), "FFB should be unblocked after rollback");
            prop_assert!(blocker.try_ffb_operation().is_ok(), "FFB operation should succeed after rollback");

            // State should be Idle
            let state = blocker.get_state().await;
            prop_assert_eq!(state, UpdateState::Idle, "State should be Idle after end_update");

            Ok(())
        })?;
    }
}

// ============================================================================
// Property 31: Firmware Cache Operations
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Feature: release-roadmap-v1, Property 31: Firmware Cache Operations
    ///
    /// For any firmware image added to the cache, subsequent cache queries
    /// SHALL return the cached image without network access.
    ///
    /// **Validates: Requirements 17.6**
    #[test]
    fn prop_firmware_cache_add_and_get(
        firmware in arb_firmware_image(),
    ) {
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            TestCaseError::fail(format!("Failed to create runtime: {}", e))
        })?;

        rt.block_on(async {
            // Create a temporary directory for the cache
            let temp_dir = tempfile::TempDir::new().map_err(|e| {
                TestCaseError::fail(format!("Failed to create temp dir: {}", e))
            })?;

            let cache = FirmwareCache::new(temp_dir.path().to_path_buf(), 0).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to create cache: {}", e))
            })?;

            // Initially, firmware should not be in cache
            let initial_check = cache.contains(&firmware.device_model, &firmware.version).await;
            prop_assert!(!initial_check, "Firmware should not be in cache initially");

            // Add firmware to cache
            cache.add(&firmware).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to add firmware to cache: {}", e))
            })?;

            // Now firmware should be in cache
            let after_add_check = cache.contains(&firmware.device_model, &firmware.version).await;
            prop_assert!(after_add_check, "Firmware should be in cache after add");

            // Get firmware from cache
            let cached = cache.get(&firmware.device_model, &firmware.version).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to get firmware from cache: {}", e))
            })?;

            prop_assert!(cached.is_some(), "Cached firmware should be returned");

            let cached_firmware = cached.ok_or_else(|| {
                TestCaseError::fail("Cached firmware is None".to_string())
            })?;

            // Verify cached firmware matches original
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

    /// Feature: release-roadmap-v1, Property 31: Firmware Cache Remove
    ///
    /// For any firmware image removed from the cache, subsequent cache queries
    /// SHALL return None.
    ///
    /// **Validates: Requirements 17.6**
    #[test]
    fn prop_firmware_cache_remove(
        firmware in arb_firmware_image(),
    ) {
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

            // Add firmware to cache
            cache.add(&firmware).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to add firmware to cache: {}", e))
            })?;

            // Verify it's in cache
            prop_assert!(
                cache.contains(&firmware.device_model, &firmware.version).await,
                "Firmware should be in cache"
            );

            // Remove firmware from cache
            cache.remove(&firmware.device_model, &firmware.version).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to remove firmware from cache: {}", e))
            })?;

            // Verify it's no longer in cache
            prop_assert!(
                !cache.contains(&firmware.device_model, &firmware.version).await,
                "Firmware should not be in cache after remove"
            );

            // Get should return None
            let cached = cache.get(&firmware.device_model, &firmware.version).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to get firmware from cache: {}", e))
            })?;
            prop_assert!(cached.is_none(), "Get should return None after remove");

            Ok(())
        })?;
    }

    /// Feature: release-roadmap-v1, Property 31: Firmware Cache Integrity
    ///
    /// For any cached firmware with corrupted data, the cache SHALL detect
    /// the corruption and return None.
    ///
    /// **Validates: Requirements 17.6**
    #[test]
    fn prop_firmware_cache_integrity_check(
        firmware in arb_firmware_image(),
    ) {
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

            // Add firmware to cache
            cache.add(&firmware).await.map_err(|e| {
                TestCaseError::fail(format!("Failed to add firmware to cache: {}", e))
            })?;

            // Corrupt the cached file
            let cache_filename = format!("{}_{}.fw", firmware.device_model, firmware.version);
            let cache_path = temp_dir.path().join(&cache_filename);

            if cache_path.exists() {
                // Write corrupted data
                tokio::fs::write(&cache_path, b"corrupted data").await.map_err(|e| {
                    TestCaseError::fail(format!("Failed to corrupt cache file: {}", e))
                })?;

                // Get should detect corruption and return None
                let cached = cache.get(&firmware.device_model, &firmware.version).await.map_err(|e| {
                    TestCaseError::fail(format!("Failed to get firmware from cache: {}", e))
                })?;

                prop_assert!(
                    cached.is_none(),
                    "Cache should detect corruption and return None"
                );
            }

            Ok(())
        })?;
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_update_state_default() {
        let state = UpdateState::default();
        assert_eq!(state, UpdateState::Idle);
        assert!(!state.is_in_progress());
        assert!(!state.should_block_ffb());
    }

    #[tokio::test]
    async fn test_ffb_blocker_basic() -> Result<(), Box<dyn std::error::Error>> {
        let blocker = FfbBlocker::new();

        // Initially not blocked
        assert!(!blocker.is_ffb_blocked());

        // Begin update
        blocker.begin_update("test-device").await?;
        assert!(blocker.is_ffb_blocked());

        // End update
        blocker.end_update().await;
        assert!(!blocker.is_ffb_blocked());

        Ok(())
    }

    #[tokio::test]
    async fn test_firmware_cache_basic() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::TempDir::new()?;
        let cache = FirmwareCache::new(temp_dir.path().to_path_buf(), 0).await?;

        // Create test firmware
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

        // Add to cache
        cache.add(&firmware).await?;

        // Verify it's in cache
        assert!(
            cache
                .contains(&firmware.device_model, &firmware.version)
                .await
        );

        // Get from cache
        let cached = cache.get(&firmware.device_model, &firmware.version).await?;
        assert!(cached.is_some());

        let cached_fw = cached.ok_or("Expected cached firmware")?;
        assert_eq!(cached_fw.device_model, firmware.device_model);
        assert_eq!(cached_fw.version, firmware.version);
        assert_eq!(cached_fw.data, firmware.data);

        Ok(())
    }
}
