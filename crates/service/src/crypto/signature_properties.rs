//! Property-based tests for Ed25519 signature verification
//!
//! **Property 11: Signature Verification (Consolidated)**
//! *For any* signed content (native plugin, firmware image, or registry plugin),
//! loading SHALL verify the Ed25519 signature against the trust store, and
//! invalid signatures SHALL be rejected with a security error.
//!
//! **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**

use crate::crypto::{
    ContentType, SignatureVerifier, TrustLevel,
    ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair, Signature},
    trust_store::TrustStore,
};
use proptest::prelude::*;

/// Strategy for generating arbitrary content bytes (simulating plugin/firmware/binary content)
fn arb_content() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 1..1024)
}

/// Strategy for generating content types that require signature verification
fn arb_signed_content_type() -> impl Strategy<Value = ContentType> {
    prop_oneof![
        Just(ContentType::Plugin),
        Just(ContentType::Firmware),
        Just(ContentType::Binary),
    ]
}

/// Strategy for generating all content types
fn arb_content_type() -> impl Strategy<Value = ContentType> {
    prop_oneof![
        Just(ContentType::Plugin),
        Just(ContentType::Firmware),
        Just(ContentType::Binary),
        Just(ContentType::Profile),
        Just(ContentType::Update),
    ]
}

/// Strategy for generating signer names
fn arb_signer_name() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_-]{0,31}".prop_map(|s| s)
}

/// Strategy for generating optional comments
fn arb_comment() -> impl Strategy<Value = Option<String>> {
    prop::option::of("[a-zA-Z0-9 _-]{0,64}")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Property 11.1: Valid signatures are accepted**
    ///
    /// For any content signed with a trusted key, verification SHALL succeed
    /// and return signature_valid = true.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_valid_signatures_accepted(
        content in arb_content(),
        content_type in arb_signed_content_type(),
        signer_name in arb_signer_name(),
        comment in arb_comment(),
    ) {
        // Generate a keypair
        let keypair = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair: {}", e)))?;

        // Create trust store and add the key as trusted
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test trusted key".to_string()),
        ).map_err(|e| TestCaseError::fail(format!("Failed to add key to trust store: {}", e)))?;

        // Sign the content
        let metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair,
            &signer_name,
            content_type,
            comment,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // Create verifier with trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the content
        let result = verifier.verify_content(&content, &metadata)
            .map_err(|e| TestCaseError::fail(format!("Verification failed: {}", e)))?;

        // Assert signature is valid
        prop_assert!(
            result.signature_valid,
            "Valid signature should be accepted, got signature_valid=false"
        );

        // Assert trust level is Trusted
        prop_assert_eq!(
            result.trust_level,
            TrustLevel::Trusted,
            "Trusted key should have TrustLevel::Trusted"
        );
    }

    /// **Property 11.2: Invalid signatures are rejected**
    ///
    /// For any content with a corrupted/invalid signature, verification SHALL
    /// return signature_valid = false.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_invalid_signatures_rejected(
        content in arb_content(),
        content_type in arb_signed_content_type(),
        signer_name in arb_signer_name(),
        corruption_byte in any::<u8>(),
        corruption_idx in 0usize..64,
    ) {
        // Generate a keypair
        let keypair = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair: {}", e)))?;

        // Create trust store and add the key as trusted
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test trusted key".to_string()),
        ).map_err(|e| TestCaseError::fail(format!("Failed to add key to trust store: {}", e)))?;

        // Sign the content
        let mut metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair,
            &signer_name,
            content_type,
            None,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // Corrupt the signature by modifying a byte
        let signature = Signature::from_base64(&metadata.signature)
            .map_err(|e| TestCaseError::fail(format!("Failed to parse signature: {}", e)))?;

        let mut corrupted_bytes = signature.signature_bytes;
        // Ensure we actually corrupt the signature
        let xor_val = if corruption_byte == 0 { 1 } else { corruption_byte };
        corrupted_bytes[corruption_idx] ^= xor_val;

        let corrupted_signature = Signature::from_bytes(corrupted_bytes);
        metadata.signature = corrupted_signature.to_base64();

        // Create verifier with trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the content - should return false for signature_valid
        let result = verifier.verify_content(&content, &metadata)
            .map_err(|e| TestCaseError::fail(format!("Verification call failed: {}", e)))?;

        // Assert signature is invalid
        prop_assert!(
            !result.signature_valid,
            "Corrupted signature should be rejected, got signature_valid=true"
        );
    }

    /// **Property 11.3: Tampered content fails verification**
    ///
    /// For any validly signed content that is subsequently tampered with,
    /// verification SHALL return signature_valid = false.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_tampered_content_rejected(
        content in arb_content(),
        content_type in arb_signed_content_type(),
        signer_name in arb_signer_name(),
        tamper_idx in any::<prop::sample::Index>(),
        tamper_xor in 1u8..=255u8, // Ensure non-zero to actually change content
    ) {
        // Generate a keypair
        let keypair = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair: {}", e)))?;

        // Create trust store and add the key as trusted
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test trusted key".to_string()),
        ).map_err(|e| TestCaseError::fail(format!("Failed to add key to trust store: {}", e)))?;

        // Sign the original content
        let metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair,
            &signer_name,
            content_type,
            None,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // Tamper with the content
        let mut tampered_content = content.clone();
        let idx = tamper_idx.index(tampered_content.len());
        tampered_content[idx] ^= tamper_xor;

        // Create verifier with trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the tampered content - should fail
        let result = verifier.verify_content(&tampered_content, &metadata)
            .map_err(|e| TestCaseError::fail(format!("Verification call failed: {}", e)))?;

        // Assert signature is invalid for tampered content
        prop_assert!(
            !result.signature_valid,
            "Tampered content should fail verification, got signature_valid=true"
        );
    }

    /// **Property 11.4: Untrusted keys are handled appropriately**
    ///
    /// For any content signed with a key not in the trust store, verification
    /// SHALL return an UntrustedSigner error.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_untrusted_keys_rejected(
        content in arb_content(),
        content_type in arb_signed_content_type(),
        signer_name in arb_signer_name(),
    ) {
        // Generate a keypair
        let keypair = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair: {}", e)))?;

        // Create an EMPTY trust store (key is NOT added)
        let trust_store = TrustStore::new_in_memory();

        // Sign the content
        let metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair,
            &signer_name,
            content_type,
            None,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // Create verifier with empty trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the content - should fail with untrusted signer error
        let result = verifier.verify_content(&content, &metadata);

        // Assert that verification fails due to untrusted signer
        prop_assert!(
            result.is_err(),
            "Untrusted signer should cause verification to fail"
        );

        // Check that the error is specifically about untrusted signer
        let err_string = result.unwrap_err().to_string();
        prop_assert!(
            err_string.contains("Untrusted") || err_string.contains("untrusted"),
            "Error should indicate untrusted signer, got: {}", err_string
        );
    }

    /// **Property 11.5: Signature verification works for all content types**
    ///
    /// For any content type (Plugin, Firmware, Binary), valid signatures SHALL
    /// be accepted and invalid signatures SHALL be rejected.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_all_content_types_verified(
        content in arb_content(),
        content_type in arb_content_type(),
        signer_name in arb_signer_name(),
    ) {
        // Generate a keypair
        let keypair = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair: {}", e)))?;

        // Create trust store and add the key as trusted
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test trusted key".to_string()),
        ).map_err(|e| TestCaseError::fail(format!("Failed to add key to trust store: {}", e)))?;

        // Sign the content with the specific content type
        let metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair,
            &signer_name,
            content_type.clone(),
            None,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // Create verifier with trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the content
        let result = verifier.verify_content(&content, &metadata)
            .map_err(|e| TestCaseError::fail(format!("Verification failed: {}", e)))?;

        // Assert signature is valid for all content types
        prop_assert!(
            result.signature_valid,
            "Valid signature should be accepted for content type {:?}", content_type
        );

        // Verify the content type is preserved in metadata
        prop_assert!(
            matches!(
                (&result.metadata.content_type, &content_type),
                (ContentType::Plugin, ContentType::Plugin) |
                (ContentType::Firmware, ContentType::Firmware) |
                (ContentType::Binary, ContentType::Binary) |
                (ContentType::Profile, ContentType::Profile) |
                (ContentType::Update, ContentType::Update)
            ),
            "Content type should be preserved in verification result"
        );
    }

    /// **Property 11.6: Distrusted keys are rejected**
    ///
    /// For any content signed with a key marked as Distrusted in the trust store,
    /// verification SHALL succeed cryptographically but return TrustLevel::Distrusted.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_distrusted_keys_flagged(
        content in arb_content(),
        content_type in arb_signed_content_type(),
        signer_name in arb_signer_name(),
    ) {
        // Generate a keypair
        let keypair = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair: {}", e)))?;

        // Create trust store and add the key as DISTRUSTED
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Distrusted,
            Some("Compromised key".to_string()),
        ).map_err(|e| TestCaseError::fail(format!("Failed to add key to trust store: {}", e)))?;

        // Sign the content
        let metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair,
            &signer_name,
            content_type,
            None,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // Create verifier with trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the content
        let result = verifier.verify_content(&content, &metadata)
            .map_err(|e| TestCaseError::fail(format!("Verification failed: {}", e)))?;

        // Signature should be cryptographically valid
        prop_assert!(
            result.signature_valid,
            "Signature should be cryptographically valid even for distrusted key"
        );

        // But trust level should be Distrusted
        prop_assert_eq!(
            result.trust_level,
            TrustLevel::Distrusted,
            "Distrusted key should have TrustLevel::Distrusted"
        );
    }

    /// **Property 11.7: Signature with wrong key fails**
    ///
    /// For any content signed with key A, verification with key B (different key)
    /// SHALL return signature_valid = false.
    ///
    /// **Validates: Requirements 9.2, 9.3, 16.2, 16.5, 17.2**
    #[test]
    fn prop_wrong_key_rejected(
        content in arb_content(),
        content_type in arb_signed_content_type(),
        signer_name in arb_signer_name(),
    ) {
        // Generate two different keypairs
        let keypair_a = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair A: {}", e)))?;
        let keypair_b = KeyPair::generate()
            .map_err(|e| TestCaseError::fail(format!("Failed to generate keypair B: {}", e)))?;

        // Sign with keypair A
        let mut metadata = Ed25519Signer::sign_with_metadata(
            &content,
            &keypair_a,
            &signer_name,
            content_type,
            None,
        ).map_err(|e| TestCaseError::fail(format!("Failed to sign content: {}", e)))?;

        // But claim it was signed by keypair B (change fingerprint)
        metadata.key_fingerprint = keypair_b.fingerprint();

        // Create trust store with keypair B (not A)
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair_b.public_key.clone(),
            TrustLevel::Trusted,
            Some("Test key B".to_string()),
        ).map_err(|e| TestCaseError::fail(format!("Failed to add key to trust store: {}", e)))?;

        // Create verifier with trust store
        let verifier = Ed25519Verifier::new(trust_store);

        // Verify the content - should fail because signature was made with key A
        // but we're trying to verify with key B
        let result = verifier.verify_content(&content, &metadata)
            .map_err(|e| TestCaseError::fail(format!("Verification call failed: {}", e)))?;

        // Assert signature is invalid (wrong key)
        prop_assert!(
            !result.signature_valid,
            "Signature verified with wrong key should be rejected"
        );
    }
}

/// Additional unit-style property tests for edge cases
#[cfg(test)]
mod edge_case_tests {
    use super::*;

    /// Test empty content can be signed and verified
    #[test]
    fn test_empty_content_verification() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let empty_content: Vec<u8> = vec![];
        let metadata = Ed25519Signer::sign_with_metadata(
            &empty_content,
            &keypair,
            "test",
            ContentType::Plugin,
            None,
        )?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(&empty_content, &metadata)?;

        assert!(result.signature_valid, "Empty content should be verifiable");
        Ok(())
    }

    /// Test large content can be signed and verified
    #[test]
    fn test_large_content_verification() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        // 1MB of content
        let large_content: Vec<u8> = vec![0xAB; 1024 * 1024];
        let metadata = Ed25519Signer::sign_with_metadata(
            &large_content,
            &keypair,
            "test",
            ContentType::Firmware,
            None,
        )?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(&large_content, &metadata)?;

        assert!(result.signature_valid, "Large content should be verifiable");
        Ok(())
    }

    /// Test that signature metadata is preserved through verification
    #[test]
    fn test_metadata_preservation() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

        let content = b"test content";
        let signer_name = "Test Signer Corp";
        let comment = Some("Release v1.0.0".to_string());

        let metadata = Ed25519Signer::sign_with_metadata(
            content,
            &keypair,
            signer_name,
            ContentType::Binary,
            comment.clone(),
        )?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(content, &metadata)?;

        assert!(result.signature_valid);
        assert_eq!(result.metadata.signer, signer_name);
        assert_eq!(result.metadata.comment, comment);
        assert_eq!(result.metadata.key_fingerprint, keypair.fingerprint());

        Ok(())
    }

    /// Test verification with Unknown trust level (key in store but not explicitly trusted)
    #[test]
    fn test_unknown_trust_level() -> Result<(), Box<dyn std::error::Error>> {
        let keypair = KeyPair::generate()?;
        let mut trust_store = TrustStore::new_in_memory();
        trust_store.add_key(
            keypair.public_key.clone(),
            TrustLevel::Unknown,
            Some("Unknown trust key".to_string()),
        )?;

        let content = b"test content";
        let metadata = Ed25519Signer::sign_with_metadata(
            content,
            &keypair,
            "test",
            ContentType::Plugin,
            None,
        )?;

        let verifier = Ed25519Verifier::new(trust_store);
        let result = verifier.verify_content(content, &metadata)?;

        assert!(
            result.signature_valid,
            "Signature should be cryptographically valid"
        );
        assert_eq!(
            result.trust_level,
            TrustLevel::Unknown,
            "Trust level should be Unknown"
        );

        Ok(())
    }
}
