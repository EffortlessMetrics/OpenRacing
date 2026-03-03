//! Deep tests for the openracing-crypto crate.
//!
//! Covers Ed25519 key generation, signing, verification, serialization,
//! key encoding/decoding, invalid signature detection, multiple key pairs,
//! message tampering detection, trust store / public key storage,
//! and deterministic signing behavior.

use openracing_crypto::prelude::*;
use openracing_crypto::utils;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a keypair and return it, propagating errors.
fn gen_keypair() -> Result<KeyPair, Box<dyn std::error::Error>> {
    Ok(KeyPair::generate()?)
}

// ===========================================================================
// 1. Ed25519 key generation
// ===========================================================================

#[test]
fn key_generation_produces_valid_32_byte_keys() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    assert_eq!(kp.public_key.key_bytes.len(), 32);
    assert_eq!(kp.signing_key_bytes().len(), 32);
    Ok(())
}

#[test]
fn key_generation_produces_unique_pairs() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;
    assert!(!kp1.public_key.ct_eq(&kp2.public_key), "Two generated keys must differ");
    assert_ne!(kp1.signing_key_bytes(), kp2.signing_key_bytes());
    Ok(())
}

#[test]
fn key_fingerprint_is_64_hex_chars() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();
    assert_eq!(fp.len(), 64);
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    Ok(())
}

#[test]
fn key_fingerprint_deterministic_for_same_key() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    assert_eq!(kp.fingerprint(), kp.fingerprint());
    assert_eq!(kp.fingerprint(), kp.public_key.fingerprint());
    assert_eq!(kp.fingerprint(), Ed25519Verifier::get_key_fingerprint(&kp.public_key));
    Ok(())
}

#[test]
fn different_keys_produce_different_fingerprints() -> Result<(), Box<dyn std::error::Error>> {
    let mut fps = HashSet::new();
    for _ in 0..5 {
        let kp = gen_keypair()?;
        assert!(fps.insert(kp.fingerprint()), "Fingerprint collision detected");
    }
    Ok(())
}

// ===========================================================================
// 2. Signing and verification
// ===========================================================================

#[test]
fn sign_and_verify_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"hello world";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(data, &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn sign_empty_message() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"", &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(b"", &sig, &kp.public_key)?);
    Ok(())
}

#[test]
fn sign_large_message() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = vec![0xFFu8; 1024 * 256]; // 256 KiB
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(&data, &sig, &kp.public_key)?);
    Ok(())
}

// ===========================================================================
// 3. Signature format and serialization
// ===========================================================================

#[test]
fn signature_is_64_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"msg", &kp.signing_key)?;
    assert_eq!(sig.signature_bytes.len(), 64);
    Ok(())
}

#[test]
fn signature_base64_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"data", &kp.signing_key)?;
    let b64 = sig.to_base64();
    let decoded = Signature::from_base64(&b64)?;
    assert!(sig.ct_eq(&decoded));
    Ok(())
}

#[test]
fn signature_metadata_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let meta = Ed25519Signer::sign_with_metadata(
        b"payload",
        &kp,
        "TestSigner",
        ContentType::Plugin,
        Some("deep-test".to_string()),
    )?;

    // Metadata fields populated
    assert_eq!(meta.signer, "TestSigner");
    assert_eq!(meta.key_fingerprint, kp.fingerprint());
    assert!(matches!(meta.content_type, ContentType::Plugin));
    assert_eq!(meta.comment, Some("deep-test".to_string()));

    // Signature in metadata verifies
    let sig = Signature::from_base64(&meta.signature)?;
    assert!(Ed25519Verifier::verify(b"payload", &sig, &kp.public_key)?);

    Ok(())
}

#[test]
fn signature_metadata_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let meta = Ed25519Signer::sign_with_metadata(
        b"json-rt",
        &kp,
        "JSONSigner",
        ContentType::Binary,
        None,
    )?;

    let json = serde_json::to_string(&meta)?;
    let deser: SignatureMetadata = serde_json::from_str(&json)?;

    assert_eq!(deser.signer, meta.signer);
    assert_eq!(deser.key_fingerprint, meta.key_fingerprint);
    assert_eq!(deser.signature, meta.signature);

    Ok(())
}

// ===========================================================================
// 4. Key encoding / decoding
// ===========================================================================

#[test]
fn keypair_from_bytes_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let original = gen_keypair()?;
    let bytes = original.signing_key_bytes();
    let restored = KeyPair::from_bytes(&bytes, "restored".to_string())?;

    assert!(original.public_key.ct_eq(&restored.public_key));
    Ok(())
}

#[test]
fn keypair_from_signing_key_preserves_public_key() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sk = kp.signing_key.clone();
    let kp2 = KeyPair::from_signing_key(sk, "id".to_string());
    assert!(kp.public_key.ct_eq(&kp2.public_key));
    Ok(())
}

#[test]
fn public_key_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let pk = PublicKey::from_bytes(kp.public_key.key_bytes, "test-id".to_string());
    assert!(pk.ct_eq(&kp.public_key));
    assert_eq!(pk.identifier, "test-id");
    Ok(())
}

#[test]
fn public_key_with_comment() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let pk = PublicKey::from_bytes(kp.public_key.key_bytes, "id".to_string())
        .with_comment("my comment");
    assert_eq!(pk.comment, Some("my comment".to_string()));
    Ok(())
}

#[test]
fn parse_public_key_from_base64() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let b64 = utils::encode_base64(&kp.public_key.key_bytes);
    let parsed = Ed25519Verifier::parse_public_key(&b64, "parsed".to_string())?;
    assert!(parsed.ct_eq(&kp.public_key));
    Ok(())
}

#[test]
fn parse_public_key_wrong_length_fails() {
    let b64_short = utils::encode_base64(&[0u8; 16]);
    let result = Ed25519Verifier::parse_public_key(&b64_short, "bad".to_string());
    assert!(result.is_err());

    let b64_long = utils::encode_base64(&[0u8; 48]);
    let result = Ed25519Verifier::parse_public_key(&b64_long, "bad".to_string());
    assert!(result.is_err());
}

#[test]
fn public_key_to_verifying_key() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let vk = kp.public_key.to_verifying_key()?;
    assert_eq!(vk.to_bytes(), kp.public_key.key_bytes);
    Ok(())
}

// ===========================================================================
// 5. Invalid signature detection
// ===========================================================================

#[test]
fn invalid_base64_signature_rejected() {
    let result = Signature::from_base64("not-valid-base64!!!");
    assert!(result.is_err());
}

#[test]
fn wrong_length_signature_rejected() {
    let b64_short = utils::encode_base64(&[0u8; 32]);
    let result = Signature::from_base64(&b64_short);
    assert!(result.is_err());
}

#[test]
fn forged_signature_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"real data";
    let _real_sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    // Create a forged (all-zeros) signature
    let forged = Signature::from_bytes([0u8; 64]);
    let valid = Ed25519Verifier::verify(data, &forged, &kp.public_key)?;
    assert!(!valid, "Forged all-zero signature must not verify");
    Ok(())
}

#[test]
fn wrong_key_rejects_signature() -> Result<(), Box<dyn std::error::Error>> {
    let kp_signer = gen_keypair()?;
    let kp_other = gen_keypair()?;

    let sig = Ed25519Signer::sign(b"message", &kp_signer.signing_key)?;
    let valid = Ed25519Verifier::verify(b"message", &sig, &kp_other.public_key)?;
    assert!(!valid, "Signature must not verify with wrong public key");
    Ok(())
}

#[test]
fn flipped_bit_in_signature_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"integrity test";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut bad_bytes = sig.signature_bytes;
    bad_bytes[0] ^= 0x01; // flip one bit
    let bad_sig = Signature::from_bytes(bad_bytes);
    let valid = Ed25519Verifier::verify(data, &bad_sig, &kp.public_key)?;
    assert!(!valid, "Bit-flipped signature must not verify");
    Ok(())
}

// ===========================================================================
// 6. Multiple key pairs
// ===========================================================================

#[test]
fn multiple_keypairs_sign_same_message() -> Result<(), Box<dyn std::error::Error>> {
    let data = b"shared message";
    let mut sigs = Vec::new();

    for _ in 0..3 {
        let kp = gen_keypair()?;
        let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
        assert!(Ed25519Verifier::verify(data, &sig, &kp.public_key)?);
        sigs.push(sig);
    }

    // All signatures should be different (different keys)
    for i in 0..sigs.len() {
        for j in (i + 1)..sigs.len() {
            assert!(
                !sigs[i].ct_eq(&sigs[j]),
                "Signatures from different keys must differ"
            );
        }
    }
    Ok(())
}

#[test]
fn cross_key_verification_fails() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;
    let data = b"cross-key test";

    let sig1 = Ed25519Signer::sign(data, &kp1.signing_key)?;
    let sig2 = Ed25519Signer::sign(data, &kp2.signing_key)?;

    // Each sig verifies with its own key
    assert!(Ed25519Verifier::verify(data, &sig1, &kp1.public_key)?);
    assert!(Ed25519Verifier::verify(data, &sig2, &kp2.public_key)?);

    // Cross-verification fails
    assert!(!Ed25519Verifier::verify(data, &sig1, &kp2.public_key)?);
    assert!(!Ed25519Verifier::verify(data, &sig2, &kp1.public_key)?);

    Ok(())
}

// ===========================================================================
// 7. Message tampering detection
// ===========================================================================

#[test]
fn single_byte_change_detected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let original = b"message content here".to_vec();
    let sig = Ed25519Signer::sign(&original, &kp.signing_key)?;

    for i in 0..original.len() {
        let mut tampered = original.clone();
        tampered[i] ^= 0x01;
        let valid = Ed25519Verifier::verify(&tampered, &sig, &kp.public_key)?;
        assert!(!valid, "Tampering at byte {i} must be detected");
    }
    Ok(())
}

#[test]
fn appended_data_detected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"original";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut extended = data.to_vec();
    extended.push(0x00);
    let valid = Ed25519Verifier::verify(&extended, &sig, &kp.public_key)?;
    assert!(!valid, "Appended byte must be detected");
    Ok(())
}

#[test]
fn truncated_data_detected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"original content";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let truncated = &data[..data.len() - 1];
    let valid = Ed25519Verifier::verify(truncated, &sig, &kp.public_key)?;
    assert!(!valid, "Truncated message must be detected");
    Ok(())
}

// ===========================================================================
// 8. Trust store / public key storage
// ===========================================================================

#[test]
fn trust_store_add_and_retrieve() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();

    store.add_key(
        kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("test key".to_string()),
    )?;

    assert!(store.contains_key(&fp));
    let retrieved = store.get_public_key(&fp);
    assert!(retrieved.is_some());

    let pk = retrieved.ok_or("key not found")?;
    assert!(pk.ct_eq(&kp.public_key));
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Trusted);

    Ok(())
}

#[test]
fn trust_store_unknown_key_returns_unknown() {
    let store = TrustStore::new_in_memory();
    assert_eq!(
        store.get_trust_level("nonexistent-fingerprint"),
        TrustLevel::Unknown
    );
}

#[test]
fn trust_store_update_trust_level() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();

    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Trusted);

    store.update_trust_level(&fp, TrustLevel::Distrusted, Some("compromised".to_string()))?;
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Distrusted);

    Ok(())
}

#[test]
fn trust_store_remove_user_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();

    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;
    assert!(store.contains_key(&fp));

    let removed = store.remove_key(&fp)?;
    assert!(removed);
    assert!(!store.contains_key(&fp));

    // Removing again returns false
    let removed_again = store.remove_key(&fp)?;
    assert!(!removed_again);

    Ok(())
}

#[test]
fn trust_store_system_key_protection() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();

    let system_fps: Vec<_> = store
        .list_keys()
        .iter()
        .filter(|(_, e)| !e.user_modifiable)
        .map(|(fp, _)| fp.clone())
        .collect();

    assert!(
        !system_fps.is_empty(),
        "Trust store must have at least one system key"
    );

    for fp in &system_fps {
        assert!(store.remove_key(fp).is_err(), "Must not remove system key");
        assert!(
            store
                .update_trust_level(fp, TrustLevel::Distrusted, None)
                .is_err(),
            "Must not modify system key"
        );
    }

    Ok(())
}

#[test]
fn trust_store_stats() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();

    let kp_trusted = gen_keypair()?;
    let kp_unknown = gen_keypair()?;
    let kp_distrusted = gen_keypair()?;

    store.add_key(kp_trusted.public_key, TrustLevel::Trusted, None)?;
    store.add_key(kp_unknown.public_key, TrustLevel::Unknown, None)?;
    store.add_key(kp_distrusted.public_key, TrustLevel::Distrusted, None)?;

    let stats = store.get_stats();
    assert!(stats.trusted_keys >= 2); // system key + user key
    assert!(stats.unknown_keys >= 1);
    assert!(stats.distrusted_keys >= 1);
    assert!(stats.system_keys >= 1);

    Ok(())
}

#[test]
fn trust_store_export_import() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let export_path = temp.path().join("keys.json");

    let mut source = TrustStore::new_in_memory();
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();
    source.add_key(kp.public_key, TrustLevel::Trusted, None)?;

    let exported = source.export_keys(&export_path, false)?;
    assert!(exported >= 1);

    let mut dest = TrustStore::new_in_memory();
    let result = dest.import_keys(&export_path, false)?;
    assert!(result.imported >= 1);
    assert!(dest.contains_key(&fp));

    Ok(())
}

#[test]
fn trust_store_file_backed_persistence() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::TempDir::new()?;
    let path = temp.path().join("store.json");
    let kp = gen_keypair()?;
    let fp = kp.fingerprint();

    {
        let mut store = TrustStore::new(path.clone())?;
        store.add_key(kp.public_key, TrustLevel::Trusted, None)?;
        store.save_to_file()?;
    }

    let reloaded = TrustStore::new(path)?;
    assert!(reloaded.contains_key(&fp));
    assert_eq!(reloaded.get_trust_level(&fp), TrustLevel::Trusted);

    Ok(())
}

// ===========================================================================
// 9. Deterministic signing behavior
// ===========================================================================

#[test]
fn same_key_same_message_produces_same_signature() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"deterministic signing test";

    let sig1 = Ed25519Signer::sign(data, &kp.signing_key)?;
    let sig2 = Ed25519Signer::sign(data, &kp.signing_key)?;
    assert!(sig1.ct_eq(&sig2), "Ed25519 signing must be deterministic");
    Ok(())
}

#[test]
fn restored_key_produces_same_signature() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let bytes = kp.signing_key_bytes();
    let restored = KeyPair::from_bytes(&bytes, "restored".to_string())?;
    let data = b"restored-key test";

    let sig_orig = Ed25519Signer::sign(data, &kp.signing_key)?;
    let sig_rest = Ed25519Signer::sign(data, &restored.signing_key)?;
    assert!(sig_orig.ct_eq(&sig_rest));
    Ok(())
}

#[test]
fn different_messages_produce_different_signatures() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig_a = Ed25519Signer::sign(b"message A", &kp.signing_key)?;
    let sig_b = Ed25519Signer::sign(b"message B", &kp.signing_key)?;
    assert!(!sig_a.ct_eq(&sig_b), "Different messages must produce different signatures");
    Ok(())
}

// ===========================================================================
// 10. Constant-time equality
// ===========================================================================

#[test]
fn public_key_ct_eq_reflexive() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    assert!(kp.public_key.ct_eq(&kp.public_key));
    Ok(())
}

#[test]
fn public_key_partial_eq_uses_ct() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;
    assert_eq!(kp1.public_key, kp1.public_key);
    assert_ne!(kp1.public_key, kp2.public_key);
    Ok(())
}

#[test]
fn signature_ct_eq_reflexive() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"ct-eq", &kp.signing_key)?;
    assert!(sig.ct_eq(&sig));
    Ok(())
}

// ===========================================================================
// 11. Utility functions
// ===========================================================================

#[test]
fn sha256_hex_deterministic() {
    let h1 = utils::compute_sha256_hex(b"test input");
    let h2 = utils::compute_sha256_hex(b"test input");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);
}

#[test]
fn base64_encode_decode_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let data = b"roundtrip payload";
    let encoded = utils::encode_base64(data);
    let decoded = utils::decode_base64(&encoded)?;
    assert_eq!(decoded, data);
    Ok(())
}

#[test]
fn base64_decode_invalid_fails() {
    let result = utils::decode_base64("!!!not-base64!!!");
    assert!(result.is_err());
}

#[test]
fn signature_path_extension() {
    let p = utils::get_signature_path(std::path::Path::new("plugin.wasm"));
    assert!(p.to_string_lossy().ends_with(".wasm.sig"));

    let p = utils::get_signature_path(std::path::Path::new("firmware"));
    assert!(p.to_string_lossy().ends_with(".sig"));
}

// ===========================================================================
// 12. Verification with trust store
// ===========================================================================

#[test]
fn verify_with_trust_store_trusted_key() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"trusted verification";

    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    let verifier = Ed25519Verifier::new(store);
    let valid = verifier.verify_with_trust_store(data, &sig, &kp.fingerprint())?;
    assert!(valid);

    Ok(())
}

#[test]
fn verify_with_trust_store_unknown_key_fails() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"unknown key test";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let store = TrustStore::new_in_memory();
    let verifier = Ed25519Verifier::new(store);

    let result = verifier.verify_with_trust_store(data, &sig, &kp.fingerprint());
    assert!(result.is_err(), "Unknown key must fail trust store lookup");

    Ok(())
}

#[test]
fn verify_content_with_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"metadata verify";

    let meta = Ed25519Signer::sign_with_metadata(
        data,
        &kp,
        "DeepTest",
        ContentType::Firmware,
        Some("deep test".to_string()),
    )?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;

    let verifier = Ed25519Verifier::new(store);
    let result = verifier.verify_content(data, &meta)?;

    assert!(result.signature_valid);
    assert_eq!(result.trust_level, TrustLevel::Trusted);
    assert_eq!(result.metadata.signer, "DeepTest");

    Ok(())
}

// ===========================================================================
// 13. Error types
// ===========================================================================

#[test]
fn crypto_error_display_messages() {
    let e = CryptoError::InvalidSignature;
    assert_eq!(e.to_string(), "Invalid signature");

    let e = CryptoError::UntrustedSigner("abc123".to_string());
    assert!(e.to_string().contains("abc123"));

    let e = CryptoError::InvalidKeyLength {
        expected: 32,
        actual: 16,
    };
    assert!(e.to_string().contains("32"));
    assert!(e.to_string().contains("16"));

    let e = CryptoError::InvalidSignatureLength {
        expected: 64,
        actual: 32,
    };
    assert!(e.to_string().contains("64"));
    assert!(e.to_string().contains("32"));
}

// ===========================================================================
// 14. Content types
// ===========================================================================

#[test]
fn all_content_types_signable() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"content-type coverage";

    let content_types = vec![
        ContentType::Binary,
        ContentType::Firmware,
        ContentType::Plugin,
        ContentType::Profile,
        ContentType::Update,
    ];

    for ct in content_types {
        let meta = Ed25519Signer::sign_with_metadata(
            data,
            &kp,
            "TypeTest",
            ct,
            None,
        )?;
        let sig = Signature::from_base64(&meta.signature)?;
        assert!(Ed25519Verifier::verify(data, &sig, &kp.public_key)?);
    }

    Ok(())
}
