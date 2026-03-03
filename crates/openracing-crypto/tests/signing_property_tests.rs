//! Proptest-driven signing property tests for openracing-crypto.
//!
//! Covers:
//! - Sign-then-verify always succeeds for valid keys
//! - Bit flip in signature causes verification failure
//! - Bit flip in data causes verification failure
//! - Key generation determinism with fixed seeds
//! - Multiple signatures from same key
//! - Signature format stability (snapshot)

use openracing_crypto::prelude::*;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

fn arbitrary_payload() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..4096)
}

/// Strategy for a valid byte index into a 64-byte signature.
fn sig_byte_index() -> impl Strategy<Value = usize> {
    0..64usize
}

/// Strategy for a valid bit position within a byte.
fn bit_position() -> impl Strategy<Value = u8> {
    0..8u8
}

/// Strategy for a valid byte index into a data payload of known length.
fn data_byte_index(len: usize) -> impl Strategy<Value = usize> {
    0..len
}

// ---------------------------------------------------------------------------
// 1. Sign then verify always succeeds for valid keys
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_sign_then_verify_succeeds(payload in arbitrary_payload()) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let sig = Ed25519Signer::sign(&payload, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let valid = Ed25519Verifier::verify(&payload, &sig, &keypair.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(valid, "sign-then-verify must always succeed for valid keys");
    }
}

// ---------------------------------------------------------------------------
// 2. Any bit flip in signature causes verification failure
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_bit_flip_in_signature_fails(
        byte_idx in sig_byte_index(),
        bit_pos in bit_position()
    ) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let data = b"deterministic data for signature bit-flip test";
        let sig = Ed25519Signer::sign(data.as_slice(), &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;

        let mut flipped_bytes = sig.signature_bytes;
        flipped_bytes[byte_idx] ^= 1 << bit_pos;
        let flipped_sig = Signature::from_bytes(flipped_bytes);

        let valid = Ed25519Verifier::verify(data.as_slice(), &flipped_sig, &keypair.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(
            !valid,
            "flipping bit {} in byte {} of signature must cause verification failure",
            bit_pos,
            byte_idx
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Any bit flip in data causes verification failure
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_bit_flip_in_data_fails(
        bit_pos in bit_position()
    ) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let data: Vec<u8> = (0..64).collect();
        let sig = Ed25519Signer::sign(&data, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;

        // Flip a bit in the middle of the data
        let byte_idx = 32usize;
        let mut flipped_data = data.clone();
        flipped_data[byte_idx] ^= 1 << bit_pos;

        let valid = Ed25519Verifier::verify(&flipped_data, &sig, &keypair.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(
            !valid,
            "flipping bit {} in data byte {} must cause verification failure",
            bit_pos,
            byte_idx
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    #[test]
    fn prop_bit_flip_at_any_data_position(
        byte_idx in data_byte_index(128),
        bit_pos in bit_position()
    ) {
        let keypair = KeyPair::generate().map_err(|e| TestCaseError::fail(format!("{e}")))?;
        let data: Vec<u8> = (0..128).map(|i| (i & 0xFF) as u8).collect();
        let sig = Ed25519Signer::sign(&data, &keypair.signing_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;

        let mut flipped = data.clone();
        flipped[byte_idx] ^= 1 << bit_pos;

        let valid = Ed25519Verifier::verify(&flipped, &sig, &keypair.public_key)
            .map_err(|e| TestCaseError::fail(format!("{e}")))?;
        prop_assert!(
            !valid,
            "flipping any bit in data must invalidate signature"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Key generation determinism with fixed seeds
// ---------------------------------------------------------------------------

#[test]
fn key_generation_determinism_with_fixed_seed() -> TestResult {
    let seed: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
        0x1d, 0x1e, 0x1f, 0x20,
    ];

    let kp1 = KeyPair::from_bytes(&seed, "seed-key-1".to_string())?;
    let kp2 = KeyPair::from_bytes(&seed, "seed-key-2".to_string())?;

    // Same seed produces same public key
    assert!(
        kp1.public_key.ct_eq(&kp2.public_key),
        "same seed must produce same key pair"
    );

    // Same seed produces same fingerprint
    assert_eq!(kp1.fingerprint(), kp2.fingerprint());

    // Different seed produces different key
    let other_seed: [u8; 32] = [0xFF; 32];
    let kp3 = KeyPair::from_bytes(&other_seed, "other-key".to_string())?;
    assert!(
        !kp1.public_key.ct_eq(&kp3.public_key),
        "different seeds must produce different keys"
    );

    Ok(())
}

#[test]
fn key_generation_determinism_signatures_match() -> TestResult {
    let seed: [u8; 32] = [42u8; 32];
    let kp1 = KeyPair::from_bytes(&seed, "a".to_string())?;
    let kp2 = KeyPair::from_bytes(&seed, "b".to_string())?;

    let data = b"deterministic signing test";
    let sig1 = Ed25519Signer::sign(data, &kp1.signing_key)?;
    let sig2 = Ed25519Signer::sign(data, &kp2.signing_key)?;

    assert!(
        sig1.ct_eq(&sig2),
        "same key seed must produce identical signatures for same data"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Multiple signatures from same key
// ---------------------------------------------------------------------------

#[test]
fn multiple_signatures_same_key() -> TestResult {
    let keypair = KeyPair::generate()?;

    let payloads: Vec<&[u8]> = vec![b"alpha", b"beta", b"gamma", b"delta", b""];

    let mut signatures = Vec::new();
    for payload in &payloads {
        let sig = Ed25519Signer::sign(payload, &keypair.signing_key)?;
        signatures.push(sig);
    }

    // Each signature verifies for its own payload
    for (payload, sig) in payloads.iter().zip(signatures.iter()) {
        let valid = Ed25519Verifier::verify(payload, sig, &keypair.public_key)?;
        assert!(valid, "signature must verify for its own payload");
    }

    // Cross-verify: each signature fails for a different payload
    for (i, sig) in signatures.iter().enumerate() {
        for (j, payload) in payloads.iter().enumerate() {
            if i != j {
                let valid = Ed25519Verifier::verify(payload, sig, &keypair.public_key)?;
                assert!(
                    !valid,
                    "signature[{i}] must not verify for payload[{j}]"
                );
            }
        }
    }

    Ok(())
}

#[test]
fn multiple_signatures_same_key_deterministic() -> TestResult {
    let keypair = KeyPair::generate()?;
    let data = b"repeated signing";

    let sig1 = Ed25519Signer::sign(data, &keypair.signing_key)?;
    let sig2 = Ed25519Signer::sign(data, &keypair.signing_key)?;
    let sig3 = Ed25519Signer::sign(data, &keypair.signing_key)?;

    assert!(sig1.ct_eq(&sig2), "Ed25519 signing must be deterministic");
    assert!(sig2.ct_eq(&sig3), "Ed25519 signing must be deterministic");

    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Signature format stability (snapshot)
// ---------------------------------------------------------------------------

#[test]
fn signature_format_stability_snapshot() -> TestResult {
    // Use a fixed seed so the key pair is always the same
    let seed: [u8; 32] = [
        0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
        0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14,
        0x15, 0x16, 0x17, 0x18,
    ];
    let keypair = KeyPair::from_bytes(&seed, "snapshot-key".to_string())?;

    let data = b"snapshot test data";
    let sig = Ed25519Signer::sign(data, &keypair.signing_key)?;

    // Signature should be exactly 64 bytes
    assert_eq!(sig.signature_bytes.len(), 64);

    // Base64-encoded signature should be stable
    let b64 = sig.to_base64();
    assert!(!b64.is_empty());

    // Re-signing with same key and data must produce identical output
    let sig2 = Ed25519Signer::sign(data, &keypair.signing_key)?;
    assert_eq!(sig.to_base64(), sig2.to_base64(), "signature must be stable across calls");

    // Signature must roundtrip through base64
    let parsed = Signature::from_base64(&b64)?;
    assert!(sig.ct_eq(&parsed), "base64 roundtrip must preserve signature");

    // Public key fingerprint must be stable
    let fp1 = keypair.fingerprint();
    let fp2 = keypair.fingerprint();
    assert_eq!(fp1, fp2, "fingerprint must be stable");
    assert_eq!(fp1.len(), 64, "fingerprint must be 64 hex chars");

    Ok(())
}

#[test]
fn signature_format_bytes_are_64() -> TestResult {
    let keypair = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(b"test", &keypair.signing_key)?;

    assert_eq!(
        sig.signature_bytes.len(),
        64,
        "Ed25519 signature must always be 64 bytes"
    );

    // Verify base64 roundtrip preserves exact bytes
    let b64 = sig.to_base64();
    let parsed = Signature::from_base64(&b64)?;
    assert_eq!(sig.signature_bytes, parsed.signature_bytes);

    Ok(())
}
