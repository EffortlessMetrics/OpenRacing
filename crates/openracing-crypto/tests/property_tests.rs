#![allow(clippy::redundant_closure)]
//! Property-based tests for openracing-crypto.
//!
//! Tests cover:
//! - Key generation invariants
//! - Ed25519 sign→verify round-trips
//! - Signature base64 encoding roundtrips
//! - Edge cases: empty payloads, maximum size, corrupted signatures
//! - Trust store property invariants

use openracing_crypto::prelude::*;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

fn arbitrary_payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..4096)
}

fn large_payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 4096..=65536)
}

// ---------------------------------------------------------------------------
// Key generation invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn prop_generated_keys_have_correct_sizes(_seed in any::<u64>()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert_eq!(keypair.public_key.key_bytes.len(), 32);
        prop_assert_eq!(keypair.signing_key_bytes().len(), 32);
    }

    #[test]
    fn prop_generated_fingerprints_are_64_hex_chars(_seed in any::<u64>()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let fp = keypair.fingerprint();
        prop_assert_eq!(fp.len(), 64);
        prop_assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn prop_two_generated_keys_differ(_seed in any::<u64>()) {
        let kp1 = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let kp2 = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(!kp1.public_key.ct_eq(&kp2.public_key));
    }

    #[test]
    fn prop_keypair_from_bytes_roundtrip(_seed in any::<u64>()) {
        let original = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let bytes = original.signing_key_bytes();
        let restored = KeyPair::from_bytes(&bytes, "restored".to_string())
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(original.public_key.ct_eq(&restored.public_key));
    }
}

// ---------------------------------------------------------------------------
// Sign→verify round-trips
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn prop_sign_verify_roundtrip(payload in arbitrary_payload()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig = Ed25519Signer::sign(&payload, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let valid = Ed25519Verifier::verify(&payload, &sig, &keypair.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(valid, "Signature must verify for same payload");
    }

    #[test]
    fn prop_sign_verify_large_payload(payload in large_payload()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig = Ed25519Signer::sign(&payload, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let valid = Ed25519Verifier::verify(&payload, &sig, &keypair.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(valid, "Signature must verify for large payload");
    }

    #[test]
    fn prop_wrong_key_fails_verify(payload in arbitrary_payload()) {
        let kp1 = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let kp2 = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig = Ed25519Signer::sign(&payload, &kp1.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let valid = Ed25519Verifier::verify(&payload, &sig, &kp2.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(!valid, "Signature must NOT verify with wrong key");
    }

    #[test]
    fn prop_signature_base64_roundtrip(payload in arbitrary_payload()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig = Ed25519Signer::sign(&payload, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;

        let b64 = sig.to_base64();
        let parsed = Signature::from_base64(&b64)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(sig.ct_eq(&parsed));
    }

    #[test]
    fn prop_deterministic_signatures(payload in arbitrary_payload()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig1 = Ed25519Signer::sign(&payload, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig2 = Ed25519Signer::sign(&payload, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(sig1.ct_eq(&sig2), "Ed25519 signatures must be deterministic");
    }
}

// ---------------------------------------------------------------------------
// Edge cases (non-proptest)
// ---------------------------------------------------------------------------

#[test]
fn test_sign_verify_empty_payload() -> TestResult {
    let keypair = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(&[], &keypair.signing_key)?;
    let valid = Ed25519Verifier::verify(&[], &sig, &keypair.public_key)?;
    assert!(valid, "Signature must verify for empty payload");
    Ok(())
}

#[test]
fn test_sign_verify_single_byte() -> TestResult {
    let keypair = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(&[0x42], &keypair.signing_key)?;
    let valid = Ed25519Verifier::verify(&[0x42], &sig, &keypair.public_key)?;
    assert!(valid);
    Ok(())
}

#[test]
fn test_corrupted_signature_single_bit_flip() -> TestResult {
    let keypair = KeyPair::generate()?;
    let data = b"test data for corruption";
    let sig = Ed25519Signer::sign(data, &keypair.signing_key)?;

    let mut corrupted_bytes = sig.signature_bytes;
    corrupted_bytes[0] ^= 0x01; // flip one bit
    let corrupted = Signature::from_bytes(corrupted_bytes);

    let valid = Ed25519Verifier::verify(data, &corrupted, &keypair.public_key)?;
    assert!(!valid, "Corrupted signature must fail verification");
    Ok(())
}

#[test]
fn test_corrupted_signature_all_zeros() -> TestResult {
    let keypair = KeyPair::generate()?;
    let data = b"test data";
    let zero_sig = Signature::from_bytes([0u8; 64]);

    let valid = Ed25519Verifier::verify(data, &zero_sig, &keypair.public_key)?;
    assert!(!valid, "All-zeros signature must fail verification");
    Ok(())
}

#[test]
fn test_corrupted_signature_all_ones() -> TestResult {
    let keypair = KeyPair::generate()?;
    let data = b"test data";
    let ones_sig = Signature::from_bytes([0xFF; 64]);

    let valid = Ed25519Verifier::verify(data, &ones_sig, &keypair.public_key)?;
    assert!(!valid, "All-ones signature must fail verification");
    Ok(())
}

#[test]
fn test_wrong_payload_fails_verify() -> TestResult {
    let keypair = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(b"original", &keypair.signing_key)?;
    let valid = Ed25519Verifier::verify(b"tampered", &sig, &keypair.public_key)?;
    assert!(!valid, "Signature must fail for different payload");
    Ok(())
}

#[test]
fn test_invalid_base64_signature_rejected() {
    let result = Signature::from_base64("not-valid-base64!!!");
    assert!(result.is_err());
}

#[test]
fn test_wrong_length_base64_signature_rejected() {
    let too_short = openracing_crypto::utils::encode_base64(&[0u8; 32]);
    let result = Signature::from_base64(&too_short);
    assert!(result.is_err());
}

#[test]
fn test_invalid_public_key_length_rejected() {
    let short_key = openracing_crypto::utils::encode_base64(&[0u8; 16]);
    let result = Ed25519Verifier::parse_public_key(&short_key, "test".to_string());
    assert!(result.is_err());
}

#[test]
fn test_public_key_constant_time_equality() -> TestResult {
    let kp1 = KeyPair::generate()?;
    let kp2 = KeyPair::generate()?;

    assert!(kp1.public_key.ct_eq(&kp1.public_key));
    assert!(!kp1.public_key.ct_eq(&kp2.public_key));

    // PartialEq should use ct_eq
    assert_eq!(kp1.public_key, kp1.public_key);
    assert_ne!(kp1.public_key, kp2.public_key);
    Ok(())
}

#[test]
fn test_signature_constant_time_equality() -> TestResult {
    let keypair = KeyPair::generate()?;
    let sig1 = Ed25519Signer::sign(b"data", &keypair.signing_key)?;
    let sig2 = Ed25519Signer::sign(b"data", &keypair.signing_key)?;
    let sig3 = Ed25519Signer::sign(b"other", &keypair.signing_key)?;

    assert!(sig1.ct_eq(&sig2));
    assert!(!sig1.ct_eq(&sig3));
    Ok(())
}

// ---------------------------------------------------------------------------
// Trust store property invariants
// ---------------------------------------------------------------------------

#[test]
fn test_trust_store_unknown_key_returns_unknown() -> TestResult {
    let store = TrustStore::new_in_memory();
    let level = store.get_trust_level("nonexistent-fingerprint-abc");
    assert_eq!(level, TrustLevel::Unknown);
    Ok(())
}

#[test]
fn test_trust_store_add_retrieve_roundtrip() -> TestResult {
    let mut store = TrustStore::new_in_memory();
    let keypair = KeyPair::generate()?;
    let fp = keypair.fingerprint();

    store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

    let retrieved = store.get_public_key(&fp);
    assert!(retrieved.is_some());
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Trusted);
    Ok(())
}

#[test]
fn test_sign_with_metadata_roundtrip() -> TestResult {
    let keypair = KeyPair::generate()?;
    let data = b"metadata roundtrip test";
    let metadata = Ed25519Signer::sign_with_metadata(
        data,
        &keypair,
        "Test Author",
        ContentType::Plugin,
        Some("comment".to_string()),
    )?;

    assert_eq!(metadata.signer, "Test Author");
    assert_eq!(metadata.key_fingerprint, keypair.fingerprint());
    assert_eq!(metadata.comment, Some("comment".to_string()));

    let sig = Signature::from_base64(&metadata.signature)?;
    let valid = Ed25519Verifier::verify(data, &sig, &keypair.public_key)?;
    assert!(valid);
    Ok(())
}
