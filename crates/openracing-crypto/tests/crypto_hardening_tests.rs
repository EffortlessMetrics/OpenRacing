//! Crypto hardening tests.
//!
//! Covers key generation, sign/verify round-trips, tampered data detection,
//! key format parsing, invalid key/signature lengths, timing-safe comparison,
//! trust store operations, fail-closed mode, base64 encoding, and known-answer tests.

use openracing_crypto::{
    CryptoError, Ed25519Signer, Ed25519Verifier, KeyPair, PublicKey, Signature, TrustLevel,
    TrustStore,
};
use openracing_crypto::utils;
use openracing_crypto::verification::ContentType;

// ===========================================================================
// 1. Key generation
// ===========================================================================

#[test]
fn keygen_produces_valid_keypair() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    assert_eq!(kp.public_key.key_bytes.len(), 32);
    assert!(!kp.public_key.identifier.is_empty());
    Ok(())
}

#[test]
fn keygen_unique_keys() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = KeyPair::generate()?;
    let kp2 = KeyPair::generate()?;
    assert_ne!(kp1.public_key.key_bytes, kp2.public_key.key_bytes);
    Ok(())
}

#[test]
fn keygen_from_bytes_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let bytes = kp.signing_key_bytes();
    let kp2 = KeyPair::from_bytes(&bytes, "restored".to_string())?;
    assert!(kp.public_key.ct_eq(&kp2.public_key));
    Ok(())
}

#[test]
fn keygen_fingerprint_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let fp1 = kp.fingerprint();
    let fp2 = kp.fingerprint();
    assert_eq!(fp1, fp2);
    assert!(!fp1.is_empty());
    Ok(())
}

#[test]
fn keygen_fingerprint_differs_for_different_keys() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = KeyPair::generate()?;
    let kp2 = KeyPair::generate()?;
    assert_ne!(kp1.fingerprint(), kp2.fingerprint());
    Ok(())
}

// ===========================================================================
// 2. Sign / verify round-trip
// ===========================================================================

#[test]
fn sign_verify_basic() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let data = b"hello, world!";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
    let valid = Ed25519Verifier::verify(data, &sig, &kp.public_key)?;
    assert!(valid);
    Ok(())
}

#[test]
fn sign_verify_empty_data() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(b"", &kp.signing_key)?;
    let valid = Ed25519Verifier::verify(b"", &sig, &kp.public_key)?;
    assert!(valid);
    Ok(())
}

#[test]
fn sign_verify_large_data() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let data = vec![0xABu8; 1024 * 1024]; // 1 MiB
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;
    let valid = Ed25519Verifier::verify(&data, &sig, &kp.public_key)?;
    assert!(valid);
    Ok(())
}

#[test]
fn sign_with_metadata_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let data = b"plugin.wasm binary";
    let metadata = Ed25519Signer::sign_with_metadata(
        data,
        &kp,
        "test-signer",
        ContentType::Plugin,
        Some("test run".to_string()),
    )?;

    assert_eq!(metadata.signer, "test-signer");
    assert_eq!(metadata.key_fingerprint, kp.fingerprint());
    assert!(metadata.comment.is_some());

    // verify via the raw signature
    let sig = Signature::from_base64(&metadata.signature)?;
    let valid = Ed25519Verifier::verify(data, &sig, &kp.public_key)?;
    assert!(valid);
    Ok(())
}

// ===========================================================================
// 3. Tampered data detection
// ===========================================================================

#[test]
fn tampered_data_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let data = b"original content";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
    let valid = Ed25519Verifier::verify(b"tampered content", &sig, &kp.public_key)?;
    assert!(!valid);
    Ok(())
}

#[test]
fn tampered_single_bit_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let data = b"critical firmware image";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut corrupted = data.to_vec();
    corrupted[0] ^= 0x01;

    let valid = Ed25519Verifier::verify(&corrupted, &sig, &kp.public_key)?;
    assert!(!valid, "single-bit flip must be detected");
    Ok(())
}

#[test]
fn wrong_key_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let kp_sign = KeyPair::generate()?;
    let kp_other = KeyPair::generate()?;
    let data = b"test data";
    let sig = Ed25519Signer::sign(data, &kp_sign.signing_key)?;
    let valid = Ed25519Verifier::verify(data, &sig, &kp_other.public_key)?;
    assert!(!valid, "wrong public key must reject");
    Ok(())
}

#[test]
fn corrupted_signature_bytes_reject() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let data = b"data";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    let mut corrupted_bytes = sig.signature_bytes;
    corrupted_bytes[0] ^= 0xFF;
    let corrupted_sig = Signature::from_bytes(corrupted_bytes);

    let valid = Ed25519Verifier::verify(data, &corrupted_sig, &kp.public_key)?;
    assert!(!valid);
    Ok(())
}

// ===========================================================================
// 4. Key format parsing
// ===========================================================================

#[test]
fn public_key_from_bytes_preserves_data() {
    let bytes = [42u8; 32];
    let pk = PublicKey::from_bytes(bytes, "test-key".to_string());
    assert_eq!(pk.key_bytes, bytes);
    assert_eq!(pk.identifier, "test-key");
    assert!(pk.comment.is_none());
}

#[test]
fn public_key_with_comment() {
    let pk = PublicKey::from_bytes([0u8; 32], "key".to_string())
        .with_comment("Official signing key");
    assert_eq!(
        pk.comment.as_deref(),
        Some("Official signing key")
    );
}

#[test]
fn parse_public_key_from_base64_valid() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let b64 = utils::encode_base64(&kp.public_key.key_bytes);
    let pk = Ed25519Verifier::parse_public_key(&b64, "parsed".to_string())?;
    assert!(kp.public_key.ct_eq(&pk));
    Ok(())
}

#[test]
fn parse_public_key_wrong_length() {
    let b64 = utils::encode_base64(&[0u8; 16]); // only 16 bytes
    let result = Ed25519Verifier::parse_public_key(&b64, "bad".to_string());
    assert!(result.is_err());
}

#[test]
fn parse_public_key_invalid_base64() {
    let result = Ed25519Verifier::parse_public_key("!!!not_base64!!!", "bad".to_string());
    assert!(result.is_err());
}

// ===========================================================================
// 5. Invalid key / signature lengths
// ===========================================================================

#[test]
fn signature_from_base64_wrong_length() {
    let too_short = utils::encode_base64(&[0u8; 32]);
    let result = Signature::from_base64(&too_short);
    assert!(result.is_err());
    if let Err(CryptoError::InvalidSignatureLength { expected, actual }) = result {
        assert_eq!(expected, 64);
        assert_eq!(actual, 32);
    } else {
        panic!("expected InvalidSignatureLength");
    }
}

#[test]
fn signature_from_base64_valid_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(b"data", &kp.signing_key)?;
    let b64 = sig.to_base64();
    let parsed = Signature::from_base64(&b64)?;
    assert!(sig.ct_eq(&parsed));
    Ok(())
}

#[test]
fn signature_from_base64_invalid_encoding() {
    let result = Signature::from_base64("!!!definitely_not_base64!!!");
    assert!(result.is_err());
}

// ===========================================================================
// 6. Timing-safe comparison
// ===========================================================================

#[test]
fn ct_eq_public_keys_identical() {
    let bytes = [7u8; 32];
    let a = PublicKey::from_bytes(bytes, "a".to_string());
    let b = PublicKey::from_bytes(bytes, "b".to_string());
    assert!(a.ct_eq(&b));
    assert_eq!(a, b);
}

#[test]
fn ct_eq_public_keys_differ() {
    let a = PublicKey::from_bytes([1u8; 32], "a".to_string());
    let b = PublicKey::from_bytes([2u8; 32], "b".to_string());
    assert!(!a.ct_eq(&b));
    assert_ne!(a, b);
}

#[test]
fn ct_eq_signatures_identical() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let sig = Ed25519Signer::sign(b"data", &kp.signing_key)?;
    let sig2 = Signature::from_bytes(sig.signature_bytes);
    assert!(sig.ct_eq(&sig2));
    Ok(())
}

#[test]
fn ct_eq_signatures_differ() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let sig1 = Ed25519Signer::sign(b"data1", &kp.signing_key)?;
    let sig2 = Ed25519Signer::sign(b"data2", &kp.signing_key)?;
    assert!(!sig1.ct_eq(&sig2));
    Ok(())
}

// ===========================================================================
// 7. Trust store operations
// ===========================================================================

#[test]
fn trust_store_in_memory_starts_with_default_key() {
    let store = TrustStore::new_in_memory();
    let stats = store.get_stats();
    assert!(stats.system_keys >= 1, "default key must exist");
    assert!(stats.trusted_keys >= 1);
}

#[test]
fn trust_store_add_and_retrieve_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = KeyPair::generate()?;
    let fingerprint = kp.fingerprint();

    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, Some("test".to_string()))?;

    let level = store.get_trust_level(&fingerprint);
    assert_eq!(level, TrustLevel::Trusted);
    assert!(store.is_key_trusted(&fingerprint));
    Ok(())
}

#[test]
fn trust_store_unknown_key_returns_unknown() {
    let store = TrustStore::new_in_memory();
    let level = store.get_trust_level("nonexistent-fingerprint");
    assert_eq!(level, TrustLevel::Unknown);
}

#[test]
fn trust_store_update_trust_level() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = KeyPair::generate()?;
    let fp = kp.fingerprint();

    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Trusted);

    store.update_trust_level(&fp, TrustLevel::Distrusted, Some("revoked".to_string()))?;
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Distrusted);
    assert!(!store.is_key_trusted(&fp));
    Ok(())
}

#[test]
fn trust_store_remove_user_key() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = KeyPair::generate()?;
    let fp = kp.fingerprint();

    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;
    assert!(store.is_key_trusted(&fp));

    let removed = store.remove_key(&fp)?;
    assert!(removed);
    assert_eq!(store.get_trust_level(&fp), TrustLevel::Unknown);
    Ok(())
}

#[test]
fn trust_store_list_keys_includes_added() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let initial_count = store.list_keys().len();

    let kp = KeyPair::generate()?;
    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;

    assert_eq!(store.list_keys().len(), initial_count + 1);
    Ok(())
}

#[test]
fn trust_store_stats_accurate() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp1 = KeyPair::generate()?;
    let kp2 = KeyPair::generate()?;

    store.add_key(kp1.public_key, TrustLevel::Trusted, None)?;
    store.add_key(kp2.public_key, TrustLevel::Distrusted, None)?;

    let stats = store.get_stats();
    // At least 2 trusted (default + kp1) and 1 distrusted
    assert!(stats.trusted_keys >= 2);
    assert!(stats.distrusted_keys >= 1);
    Ok(())
}

// ===========================================================================
// 8. Fail-closed mode
// ===========================================================================

#[test]
fn fail_closed_all_lookups_distrusted() {
    let store = TrustStore::new_fail_closed("test failure");
    assert!(store.is_failed());
    assert_eq!(
        store.get_trust_level("any-fingerprint"),
        TrustLevel::Distrusted
    );
    assert!(!store.is_key_trusted("any-fingerprint"));
}

#[test]
fn fail_closed_get_public_key_returns_none() {
    let store = TrustStore::new_fail_closed("test");
    assert!(store.get_public_key("some-fingerprint").is_none());
}

#[test]
fn fail_closed_verifier_rejects_all() {
    let store = TrustStore::new_fail_closed("compromised");
    let verifier = Ed25519Verifier::new(store);

    let result = verifier.verify_with_trust_store(b"data", &Signature::from_bytes([0u8; 64]), "fp");
    assert!(result.is_err());
}

// ===========================================================================
// 9. Base64 encoding helpers
// ===========================================================================

#[test]
fn base64_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let original = b"arbitrary binary data \x00\xFF\x80";
    let encoded = utils::encode_base64(original);
    let decoded = utils::decode_base64(&encoded)?;
    assert_eq!(original.as_slice(), decoded.as_slice());
    Ok(())
}

#[test]
fn base64_empty_data() -> Result<(), Box<dyn std::error::Error>> {
    let encoded = utils::encode_base64(b"");
    let decoded = utils::decode_base64(&encoded)?;
    assert!(decoded.is_empty());
    Ok(())
}

#[test]
fn base64_invalid_input_fails() {
    let result = utils::decode_base64("not*valid*base64!!");
    assert!(result.is_err());
}

// ===========================================================================
// 10. SHA256 utilities
// ===========================================================================

#[test]
fn sha256_deterministic() {
    let hash1 = utils::compute_sha256_hex(b"hello");
    let hash2 = utils::compute_sha256_hex(b"hello");
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // 32 bytes => 64 hex chars
}

#[test]
fn sha256_different_inputs_differ() {
    let h1 = utils::compute_sha256_hex(b"foo");
    let h2 = utils::compute_sha256_hex(b"bar");
    assert_ne!(h1, h2);
}

#[test]
fn sha256_known_answer() {
    // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    let hash = utils::compute_sha256_hex(b"");
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

// ===========================================================================
// 11. Key fingerprint
// ===========================================================================

#[test]
fn key_fingerprint_matches_sha256_of_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let kp = KeyPair::generate()?;
    let manual_fp = utils::compute_sha256_hex(&kp.public_key.key_bytes);
    assert_eq!(kp.fingerprint(), manual_fp);
    assert_eq!(Ed25519Verifier::get_key_fingerprint(&kp.public_key), manual_fp);
    Ok(())
}

// ===========================================================================
// 12. Signature path helpers
// ===========================================================================

#[test]
fn signature_path_appends_sig_extension() {
    use std::path::Path;
    let path = utils::get_signature_path(Path::new("firmware.bin"));
    assert_eq!(path.to_string_lossy(), "firmware.bin.sig");
}

#[test]
fn signature_path_no_extension() {
    use std::path::Path;
    let path = utils::get_signature_path(Path::new("README"));
    assert_eq!(path.to_string_lossy(), "README.sig");
}

// ===========================================================================
// 13. Known-answer test with fixed key material
// ===========================================================================

#[test]
fn kat_fixed_key_sign_verify() -> Result<(), Box<dyn std::error::Error>> {
    // Deterministic seed: 32 zero bytes
    let kp = KeyPair::from_bytes(&[0u8; 32], "kat-key".to_string())?;
    let data = b"known answer test";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;

    // Signature must verify
    let valid = Ed25519Verifier::verify(data, &sig, &kp.public_key)?;
    assert!(valid);

    // Re-signing produces same signature (Ed25519 is deterministic)
    let sig2 = Ed25519Signer::sign(data, &kp.signing_key)?;
    assert!(sig.ct_eq(&sig2), "Ed25519 signing must be deterministic");
    Ok(())
}

#[test]
fn kat_fixed_key_public_key_stable() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = KeyPair::from_bytes(&[0u8; 32], "a".to_string())?;
    let kp2 = KeyPair::from_bytes(&[0u8; 32], "b".to_string())?;
    assert!(kp1.public_key.ct_eq(&kp2.public_key));
    assert_eq!(kp1.fingerprint(), kp2.fingerprint());
    Ok(())
}

// ===========================================================================
// 14. Detached signature file round-trip
// ===========================================================================

#[test]
fn detached_signature_file_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let content_path = dir.path().join("plugin.wasm");
    std::fs::write(&content_path, b"fake wasm binary")?;

    let kp = KeyPair::generate()?;
    let metadata = Ed25519Signer::sign_file(
        &content_path,
        &kp,
        "ci-signer",
        ContentType::Plugin,
        None,
    )?;

    assert!(utils::signature_exists(&content_path));

    let extracted = utils::extract_signature_metadata(&content_path)?;
    assert!(extracted.is_some());
    let extracted = extracted.unwrap_or_else(|| unreachable!());
    assert_eq!(extracted.signer, metadata.signer);

    let deleted = utils::delete_detached_signature(&content_path)?;
    assert!(deleted);
    assert!(!utils::signature_exists(&content_path));
    Ok(())
}

// ===========================================================================
// 15. add_key_from_hex
// ===========================================================================

#[test]
fn add_key_from_hex_valid() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = TrustStore::new_in_memory();
    let kp = KeyPair::generate()?;
    let hex_key = hex::encode(kp.public_key.key_bytes);

    store.add_key_from_hex(&hex_key, "hex-key".to_string(), TrustLevel::Trusted, None)?;

    let fp = utils::compute_key_fingerprint(&kp.public_key.key_bytes);
    assert!(store.is_key_trusted(&fp));
    Ok(())
}

#[test]
fn add_key_from_hex_wrong_length() {
    let mut store = TrustStore::new_in_memory();
    let result = store.add_key_from_hex("aabb", "short".to_string(), TrustLevel::Trusted, None);
    assert!(result.is_err());
}

#[test]
fn add_key_from_hex_invalid_hex() {
    let mut store = TrustStore::new_in_memory();
    let result =
        store.add_key_from_hex("not_hex!", "bad".to_string(), TrustLevel::Trusted, None);
    assert!(result.is_err());
}

// ===========================================================================
// 16. System key protection
// ===========================================================================

#[test]
fn system_key_cannot_be_removed() {
    let store = TrustStore::new_in_memory();
    let keys = store.list_keys();

    // Find the system key (not user_modifiable)
    let system_fp = keys
        .iter()
        .find(|(_, entry)| !entry.user_modifiable)
        .map(|(fp, _)| fp.clone());

    if let Some(fp) = system_fp {
        let mut store = TrustStore::new_in_memory();
        let result = store.remove_key(&fp);
        assert!(result.is_err(), "system keys must be protected from removal");
    }
}

#[test]
fn system_key_cannot_be_modified() {
    let store = TrustStore::new_in_memory();
    let keys = store.list_keys();

    let system_fp = keys
        .iter()
        .find(|(_, entry)| !entry.user_modifiable)
        .map(|(fp, _)| fp.clone());

    if let Some(fp) = system_fp {
        let mut store = TrustStore::new_in_memory();
        let result = store.update_trust_level(&fp, TrustLevel::Distrusted, None);
        assert!(
            result.is_err(),
            "system keys must be protected from modification"
        );
    }
}
