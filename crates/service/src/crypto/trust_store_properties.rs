//! Property-based tests for Trust Store operations
//!
//! **Property 14: Trust Store Operations**
//! *For any* sequence of add, remove, and query operations on the trust store,
//! the trust store state SHALL be consistent (added keys are trusted, removed keys are not).
//!
//! **Validates: Requirements 9.4**

use crate::crypto::{
    TrustLevel,
    ed25519::{Ed25519Verifier, PublicKey},
    trust_store::TrustStore,
};
use proptest::prelude::*;

/// Strategy for generating valid public key bytes (32 bytes)
fn arb_key_bytes() -> impl Strategy<Value = [u8; 32]> {
    prop::array::uniform32(any::<u8>())
}

/// Strategy for generating key identifiers
fn arb_identifier() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_-]{0,31}".prop_map(|s| s)
}

/// Strategy for generating trust levels
fn arb_trust_level() -> impl Strategy<Value = TrustLevel> {
    prop_oneof![
        Just(TrustLevel::Trusted),
        Just(TrustLevel::Unknown),
        Just(TrustLevel::Distrusted),
    ]
}

/// Strategy for generating optional reason strings
fn arb_reason() -> impl Strategy<Value = Option<String>> {
    prop::option::of("[a-zA-Z0-9 _-]{0,64}")
}

/// Strategy for generating a public key
fn arb_public_key() -> impl Strategy<Value = PublicKey> {
    (arb_key_bytes(), arb_identifier()).prop_map(|(key_bytes, identifier)| PublicKey {
        key_bytes,
        identifier,
        comment: None,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Property 14.1: Added keys can be retrieved**
    ///
    /// For any key added to the trust store, querying for that key SHALL return
    /// the key with the correct trust level.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_added_keys_retrievable(
        key in arb_public_key(),
        trust_level in arb_trust_level(),
        reason in arb_reason(),
    ) {
        let mut store = TrustStore::new_in_memory();

        // Add the key
        store.add_key(key.clone(), trust_level, reason)
            .map_err(|e| TestCaseError::fail(format!("Failed to add key: {}", e)))?;

        // Get the fingerprint
        let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);

        // Query the key - should be present
        let retrieved = store.get_public_key(&fingerprint);
        prop_assert!(
            retrieved.is_some(),
            "Added key should be retrievable"
        );

        // Verify the key bytes match
        let retrieved_key = retrieved.ok_or_else(|| TestCaseError::fail("Key not found"))?;
        prop_assert_eq!(
            retrieved_key.key_bytes,
            key.key_bytes,
            "Retrieved key bytes should match original"
        );

        // Verify trust level matches
        let retrieved_trust = store.get_trust_level(&fingerprint);
        prop_assert_eq!(
            retrieved_trust,
            trust_level,
            "Retrieved trust level should match what was set"
        );
    }

    /// **Property 14.2: Removed keys cannot be retrieved**
    ///
    /// For any user-modifiable key that is removed from the trust store,
    /// querying for that key SHALL return None and trust level Unknown.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_removed_keys_not_retrievable(
        key in arb_public_key(),
        trust_level in arb_trust_level(),
        reason in arb_reason(),
    ) {
        let mut store = TrustStore::new_in_memory();

        // Add the key
        store.add_key(key.clone(), trust_level, reason)
            .map_err(|e| TestCaseError::fail(format!("Failed to add key: {}", e)))?;

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);

        // Verify key is present
        prop_assert!(
            store.get_public_key(&fingerprint).is_some(),
            "Key should be present after adding"
        );

        // Remove the key
        let removed = store.remove_key(&fingerprint)
            .map_err(|e| TestCaseError::fail(format!("Failed to remove key: {}", e)))?;

        prop_assert!(removed, "Remove should return true for existing key");

        // Query the key - should NOT be present
        let retrieved = store.get_public_key(&fingerprint);
        prop_assert!(
            retrieved.is_none(),
            "Removed key should not be retrievable"
        );

        // Trust level should be Unknown for removed key
        let trust = store.get_trust_level(&fingerprint);
        prop_assert_eq!(
            trust,
            TrustLevel::Unknown,
            "Trust level for removed key should be Unknown"
        );
    }

    /// **Property 14.3: Trust levels are correctly stored and retrieved**
    ///
    /// For any key with any trust level, the stored trust level SHALL match
    /// the trust level that was set.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_trust_levels_stored_correctly(
        key in arb_public_key(),
        initial_trust in arb_trust_level(),
        updated_trust in arb_trust_level(),
    ) {
        let mut store = TrustStore::new_in_memory();

        // Add key with initial trust level
        store.add_key(key.clone(), initial_trust, None)
            .map_err(|e| TestCaseError::fail(format!("Failed to add key: {}", e)))?;

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);

        // Verify initial trust level
        prop_assert_eq!(
            store.get_trust_level(&fingerprint),
            initial_trust,
            "Initial trust level should match"
        );

        // Update trust level
        store.update_trust_level(&fingerprint, updated_trust, Some("Updated".to_string()))
            .map_err(|e| TestCaseError::fail(format!("Failed to update trust level: {}", e)))?;

        // Verify updated trust level
        prop_assert_eq!(
            store.get_trust_level(&fingerprint),
            updated_trust,
            "Updated trust level should match"
        );
    }

    /// **Property 14.4: System keys cannot be removed**
    ///
    /// For any system key (user_modifiable = false), attempting to remove it
    /// SHALL fail with an error.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_system_keys_protected(
        _dummy in any::<u8>(), // Just to make proptest happy
    ) {
        let mut store = TrustStore::new_in_memory();

        // Find system keys (those with user_modifiable = false)
        let system_keys: Vec<_> = store.list_keys()
            .into_iter()
            .filter(|(_, entry)| !entry.user_modifiable)
            .map(|(fingerprint, _)| fingerprint)
            .collect();

        // There should be at least one system key (the official key)
        prop_assert!(
            !system_keys.is_empty(),
            "Trust store should have at least one system key"
        );

        // Try to remove each system key - should fail
        for fingerprint in system_keys {
            let result = store.remove_key(&fingerprint);
            prop_assert!(
                result.is_err(),
                "Removing system key should fail"
            );

            // Key should still be present
            prop_assert!(
                store.get_public_key(&fingerprint).is_some(),
                "System key should still be present after failed removal"
            );
        }
    }

    /// **Property 14.5: System keys cannot be modified**
    ///
    /// For any system key (user_modifiable = false), attempting to update its
    /// trust level SHALL fail with an error.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_system_keys_not_modifiable(
        new_trust in arb_trust_level(),
    ) {
        let mut store = TrustStore::new_in_memory();

        // Find system keys
        let system_keys: Vec<_> = store.list_keys()
            .into_iter()
            .filter(|(_, entry)| !entry.user_modifiable)
            .map(|(fingerprint, entry)| (fingerprint, entry.trust_level))
            .collect();

        prop_assert!(
            !system_keys.is_empty(),
            "Trust store should have at least one system key"
        );

        // Try to modify each system key - should fail
        for (fingerprint, original_trust) in system_keys {
            let result = store.update_trust_level(&fingerprint, new_trust, None);
            prop_assert!(
                result.is_err(),
                "Modifying system key trust level should fail"
            );

            // Trust level should remain unchanged
            let current_trust = store.get_trust_level(&fingerprint);
            prop_assert_eq!(
                current_trust,
                original_trust,
                "System key trust level should remain unchanged after failed modification"
            );
        }
    }

    /// **Property 14.6: Sequences of operations maintain consistency**
    ///
    /// For any sequence of add, remove, and query operations, the trust store
    /// state SHALL be consistent: added keys are present, removed keys are absent.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_operation_sequence_consistency(
        keys in prop::collection::vec(arb_public_key(), 1..10),
        trust_levels in prop::collection::vec(arb_trust_level(), 1..10),
        remove_indices in prop::collection::vec(any::<prop::sample::Index>(), 0..5),
    ) {
        let mut store = TrustStore::new_in_memory();
        let mut expected_present: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Add all keys
        for (key, trust_level) in keys.iter().zip(trust_levels.iter().cycle()) {
            store.add_key(key.clone(), *trust_level, None)
                .map_err(|e| TestCaseError::fail(format!("Failed to add key: {}", e)))?;

            let fingerprint = Ed25519Verifier::get_key_fingerprint(key);
            expected_present.insert(fingerprint);
        }

        // Remove some keys
        let fingerprints: Vec<_> = expected_present.iter().cloned().collect();
        for idx in remove_indices {
            if !fingerprints.is_empty() {
                let fingerprint = &fingerprints[idx.index(fingerprints.len())];
                let _ = store.remove_key(fingerprint);
                expected_present.remove(fingerprint);
            }
        }

        // Verify consistency
        for fingerprint in &expected_present {
            prop_assert!(
                store.get_public_key(fingerprint).is_some(),
                "Key {} should be present", fingerprint
            );
        }

        // Verify removed keys are not present
        for fingerprint in &fingerprints {
            if !expected_present.contains(fingerprint) {
                prop_assert!(
                    store.get_public_key(fingerprint).is_none(),
                    "Removed key {} should not be present", fingerprint
                );
            }
        }
    }

    /// **Property 14.7: Unknown keys return Unknown trust level**
    ///
    /// For any fingerprint not in the trust store, querying trust level
    /// SHALL return TrustLevel::Unknown.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_unknown_keys_return_unknown(
        random_fingerprint in "[a-f0-9]{64}",
    ) {
        let store = TrustStore::new_in_memory();

        // Query a random fingerprint that's not in the store
        // (very unlikely to collide with the system key)
        let trust = store.get_trust_level(&random_fingerprint);

        // Should return Unknown for keys not in store
        // Note: There's a tiny chance this could be the system key fingerprint,
        // but with 64 hex chars, collision is astronomically unlikely
        if store.get_public_key(&random_fingerprint).is_none() {
            prop_assert_eq!(
                trust,
                TrustLevel::Unknown,
                "Unknown key should have TrustLevel::Unknown"
            );
        }
    }

    /// **Property 14.8: Removing non-existent key returns false**
    ///
    /// For any fingerprint not in the trust store, remove_key SHALL return
    /// Ok(false) without error.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_remove_nonexistent_returns_false(
        random_fingerprint in "[a-f0-9]{64}",
    ) {
        let mut store = TrustStore::new_in_memory();

        // Try to remove a key that doesn't exist
        if store.get_public_key(&random_fingerprint).is_none() {
            let result = store.remove_key(&random_fingerprint);

            prop_assert!(
                result.is_ok(),
                "Removing non-existent key should not error"
            );

            let removed = result
                .map_err(|e| TestCaseError::fail(format!("Unexpected error: {}", e)))?;

            prop_assert!(
                !removed,
                "Removing non-existent key should return false"
            );
        }
    }

    /// **Property 14.9: Adding same key twice updates the entry**
    ///
    /// For any key added twice with different trust levels, the second add
    /// SHALL update the trust level to the new value.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_add_twice_updates(
        key in arb_public_key(),
        first_trust in arb_trust_level(),
        second_trust in arb_trust_level(),
    ) {
        let mut store = TrustStore::new_in_memory();
        let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);

        // Add key first time
        store.add_key(key.clone(), first_trust, Some("First add".to_string()))
            .map_err(|e| TestCaseError::fail(format!("Failed first add: {}", e)))?;

        prop_assert_eq!(
            store.get_trust_level(&fingerprint),
            first_trust,
            "First trust level should be set"
        );

        // Add same key again with different trust level
        store.add_key(key, second_trust, Some("Second add".to_string()))
            .map_err(|e| TestCaseError::fail(format!("Failed second add: {}", e)))?;

        prop_assert_eq!(
            store.get_trust_level(&fingerprint),
            second_trust,
            "Second add should update trust level"
        );
    }

    /// **Property 14.10: Stats reflect actual store state**
    ///
    /// For any sequence of add operations, the stats SHALL accurately reflect
    /// the number of keys at each trust level.
    ///
    /// **Validates: Requirements 9.4**
    #[test]
    fn prop_stats_accurate(
        keys_with_trust in prop::collection::vec(
            (arb_public_key(), arb_trust_level()),
            0..10
        ),
    ) {
        let mut store = TrustStore::new_in_memory();

        // Track expected counts (start with system key which is Trusted)
        let initial_stats = store.get_stats();
        let mut expected_trusted = initial_stats.trusted_keys;
        let mut expected_unknown = initial_stats.unknown_keys;
        let mut expected_distrusted = initial_stats.distrusted_keys;

        // Add keys and track expected counts
        // Use a set to handle duplicate fingerprints
        let mut seen_fingerprints: std::collections::HashMap<String, TrustLevel> =
            std::collections::HashMap::new();

        for (key, trust_level) in &keys_with_trust {
            let fingerprint = Ed25519Verifier::get_key_fingerprint(key);

            // If we've seen this fingerprint before, adjust counts
            if let Some(old_trust) = seen_fingerprints.get(&fingerprint) {
                match old_trust {
                    TrustLevel::Trusted => expected_trusted -= 1,
                    TrustLevel::Unknown => expected_unknown -= 1,
                    TrustLevel::Distrusted => expected_distrusted -= 1,
                }
            }

            // Add new counts
            match trust_level {
                TrustLevel::Trusted => expected_trusted += 1,
                TrustLevel::Unknown => expected_unknown += 1,
                TrustLevel::Distrusted => expected_distrusted += 1,
            }

            seen_fingerprints.insert(fingerprint, *trust_level);

            store.add_key(key.clone(), *trust_level, None)
                .map_err(|e| TestCaseError::fail(format!("Failed to add key: {}", e)))?;
        }

        // Verify stats
        let stats = store.get_stats();

        prop_assert_eq!(
            stats.trusted_keys,
            expected_trusted,
            "Trusted key count should match"
        );
        prop_assert_eq!(
            stats.unknown_keys,
            expected_unknown,
            "Unknown key count should match"
        );
        prop_assert_eq!(
            stats.distrusted_keys,
            expected_distrusted,
            "Distrusted key count should match"
        );
    }
}

/// Additional edge case tests
#[cfg(test)]
mod edge_case_tests {
    use super::*;

    /// Test that list_keys returns all added keys
    #[test]
    fn test_list_keys_completeness() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = TrustStore::new_in_memory();

        // Add several keys
        let mut added_fingerprints = std::collections::HashSet::new();

        for i in 0..5 {
            let key = PublicKey {
                key_bytes: [i as u8; 32],
                identifier: format!("test-key-{}", i),
                comment: None,
            };
            let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);
            added_fingerprints.insert(fingerprint);
            store.add_key(key, TrustLevel::Trusted, None)?;
        }

        // List all keys
        let listed_keys = store.list_keys();
        let listed_fingerprints: std::collections::HashSet<_> =
            listed_keys.iter().map(|(fp, _)| fp.clone()).collect();

        // All added keys should be in the list
        for fp in &added_fingerprints {
            assert!(
                listed_fingerprints.contains(fp),
                "Added key {} should be in list",
                fp
            );
        }

        Ok(())
    }

    /// Test update_trust_level on non-existent key fails
    #[test]
    fn test_update_nonexistent_fails() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = TrustStore::new_in_memory();

        let result = store.update_trust_level("nonexistent-fingerprint", TrustLevel::Trusted, None);

        assert!(result.is_err(), "Updating non-existent key should fail");

        Ok(())
    }

    /// Test that key identifier is preserved
    #[test]
    fn test_key_identifier_preserved() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = TrustStore::new_in_memory();

        let key = PublicKey {
            key_bytes: [42u8; 32],
            identifier: "my-special-key".to_string(),
            comment: Some("A special key".to_string()),
        };

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);
        store.add_key(key.clone(), TrustLevel::Trusted, None)?;

        let retrieved = store.get_public_key(&fingerprint).ok_or("Key not found")?;

        assert_eq!(retrieved.identifier, key.identifier);
        assert_eq!(retrieved.comment, key.comment);

        Ok(())
    }

    /// Test multiple trust level transitions
    #[test]
    fn test_trust_level_transitions() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = TrustStore::new_in_memory();

        let key = PublicKey {
            key_bytes: [99u8; 32],
            identifier: "transition-test".to_string(),
            comment: None,
        };

        let fingerprint = Ed25519Verifier::get_key_fingerprint(&key);

        // Add as Trusted
        store.add_key(key, TrustLevel::Trusted, None)?;
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Trusted);

        // Update to Unknown
        store.update_trust_level(&fingerprint, TrustLevel::Unknown, None)?;
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Unknown);

        // Update to Distrusted
        store.update_trust_level(&fingerprint, TrustLevel::Distrusted, None)?;
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Distrusted);

        // Update back to Trusted
        store.update_trust_level(&fingerprint, TrustLevel::Trusted, None)?;
        assert_eq!(store.get_trust_level(&fingerprint), TrustLevel::Trusted);

        Ok(())
    }
}
