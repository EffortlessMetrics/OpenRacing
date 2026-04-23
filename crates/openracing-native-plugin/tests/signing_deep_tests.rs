//! Deep tests for native plugin signature verification.
//!
//! Covers the full plugin signing workflow, signature format (detached),
//! trust chain validation, expired signatures, revoked key handling,
//! tampered binary detection, configuration modes, and key rotation.

use std::path::Path;

use openracing_crypto::ed25519::{Ed25519Signer, KeyPair, Signature};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::ContentType;
use openracing_crypto::{SignatureMetadata, SignatureVerifier as _, TrustLevel, utils};

use openracing_native_plugin::error::NativePluginError;
use openracing_native_plugin::signature::{SignatureVerificationConfig, SignatureVerifier};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generate a key pair, propagating errors.
fn gen_keypair() -> Result<KeyPair, Box<dyn std::error::Error>> {
    Ok(KeyPair::generate()?)
}

/// Write a fake plugin binary and return its path inside `dir`.
fn write_fake_plugin(
    dir: &Path,
    name: &str,
    content: &[u8],
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = dir.join(name);
    std::fs::write(&path, content)?;
    Ok(path)
}

/// Sign a file and write a detached `.sig` alongside it.
fn sign_plugin_file(
    plugin_path: &Path,
    content: &[u8],
    keypair: &KeyPair,
    signer: &str,
) -> Result<SignatureMetadata, Box<dyn std::error::Error>> {
    let meta =
        Ed25519Signer::sign_with_metadata(content, keypair, signer, ContentType::Plugin, None)?;
    utils::create_detached_signature(plugin_path, &meta)?;
    Ok(meta)
}

/// Create a `.sig` file with custom metadata (for testing expired/tampered signatures).
fn write_custom_signature(
    plugin_path: &Path,
    metadata: &SignatureMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    utils::create_detached_signature(plugin_path, metadata)?;
    Ok(())
}

// ===========================================================================
// 1. Full plugin signing workflow
// ===========================================================================

#[test]
fn plugin_signing_workflow_generate_sign_verify() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"fake native plugin binary content";
    let plugin_path = write_fake_plugin(tmp.path(), "plugin.dll", content)?;
    let kp = gen_keypair()?;

    // Sign
    let meta = sign_plugin_file(&plugin_path, content, &kp, "PluginAuthor")?;

    // Build trust store with this key
    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    // Verify via SignatureVerifier
    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&store, config);
    let result = verifier.verify(&plugin_path)?;

    assert!(result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Trusted);
    assert!(result.metadata.is_some());

    let result_meta = result.metadata.as_ref().ok_or("missing metadata")?;
    assert_eq!(result_meta.signer, "PluginAuthor");
    assert_eq!(result_meta.key_fingerprint, meta.key_fingerprint);

    Ok(())
}

// ===========================================================================
// 2. Detached signature format
// ===========================================================================

#[test]
fn detached_signature_file_is_valid_json() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"binary blob";
    let plugin_path = write_fake_plugin(tmp.path(), "myplugin.so", content)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&plugin_path, content, &kp, "Tester")?;

    let sig_path = plugin_path.with_extension("so.sig");
    assert!(sig_path.exists(), "Detached .sig file must exist");

    let sig_content = std::fs::read_to_string(&sig_path)?;
    let parsed: SignatureMetadata = serde_json::from_str(&sig_content)?;
    assert_eq!(parsed.signer, "Tester");

    Ok(())
}

#[test]
fn signature_path_follows_convention() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;

    for (name, expected_sig) in [
        ("plugin.dll", "plugin.dll.sig"),
        ("plugin.so", "plugin.so.sig"),
        ("plugin.dylib", "plugin.dylib.sig"),
    ] {
        let content = b"data";
        let path = write_fake_plugin(tmp.path(), name, content)?;
        let kp = gen_keypair()?;
        sign_plugin_file(&path, content, &kp, "Test")?;

        let expected_path = tmp.path().join(expected_sig);
        assert!(
            expected_path.exists(),
            "Expected signature at {} for {}",
            expected_sig,
            name
        );
    }

    Ok(())
}

// ===========================================================================
// 3. Signed plugin with trusted key
// ===========================================================================

#[test]
fn signed_plugin_trusted_key_passes() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"trusted plugin binary";
    let path = write_fake_plugin(tmp.path(), "trusted.dll", content)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, content, &kp, "TrustedDev")?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path)?;
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Trusted);
    assert!(result.warnings.is_empty());

    Ok(())
}

// ===========================================================================
// 4. Distrusted key rejection
// ===========================================================================

#[test]
fn signed_plugin_distrusted_key_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"distrusted plugin";
    let path = write_fake_plugin(tmp.path(), "bad.dll", content)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, content, &kp, "BadActor")?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(
        kp.public_key,
        TrustLevel::Distrusted,
        Some("Compromised key".to_string()),
    )?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(result.is_err(), "Distrusted signer must be rejected");

    let err_str = result.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(
        err_str.to_lowercase().contains("distrust"),
        "Error should mention distrust: {}",
        err_str
    );

    Ok(())
}

// ===========================================================================
// 5. Unsigned plugin handling
// ===========================================================================

#[test]
fn unsigned_plugin_strict_mode_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let path = write_fake_plugin(tmp.path(), "unsigned.dll", b"no sig")?;

    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(
        result.is_err(),
        "Unsigned plugin must be rejected in strict mode"
    );

    Ok(())
}

#[test]
fn unsigned_plugin_permissive_mode_allowed() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let path = write_fake_plugin(tmp.path(), "unsigned.dll", b"no sig")?;

    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::permissive());
    let result = verifier.verify(&path)?;

    assert!(!result.is_signed);
    assert!(result.verified, "Permissive mode should allow unsigned");
    assert_eq!(result.trust_level, TrustLevel::Unknown);
    assert!(!result.warnings.is_empty());

    Ok(())
}

#[test]
fn unsigned_plugin_development_mode_allowed() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let path = write_fake_plugin(tmp.path(), "dev.so", b"dev binary")?;

    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::development());
    let result = verifier.verify(&path)?;

    assert!(!result.is_signed);
    assert!(result.verified);

    Ok(())
}

// ===========================================================================
// 6. Tampered binary detection
// ===========================================================================

#[test]
fn tampered_plugin_binary_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let original = b"original plugin binary content here";
    let path = write_fake_plugin(tmp.path(), "tampered.dll", original)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, original, &kp, "Author")?;

    // Tamper with the binary after signing
    let mut tampered = original.to_vec();
    tampered[0] ^= 0xFF;
    std::fs::write(&path, &tampered)?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(
        result.is_err(),
        "Tampered binary must fail signature verification"
    );

    Ok(())
}

#[test]
fn appended_bytes_to_plugin_detected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let original = b"plugin binary v2";
    let path = write_fake_plugin(tmp.path(), "appended.dll", original)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, original, &kp, "Author")?;

    // Append data after signing
    let mut extended = original.to_vec();
    extended.extend_from_slice(b"INJECTED");
    std::fs::write(&path, &extended)?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key, TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(result.is_err(), "Appended data must invalidate signature");

    Ok(())
}

// ===========================================================================
// 7. Expired signature handling
// ===========================================================================

#[test]
fn expired_signature_metadata_timestamp() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"plugin with old signature";
    let path = write_fake_plugin(tmp.path(), "old.dll", content)?;
    let kp = gen_keypair()?;

    // Create a signature with a timestamp from 2 years ago
    let sig = Ed25519Signer::sign(content, &kp.signing_key)?;
    let old_timestamp = chrono::Utc::now() - chrono::Duration::days(730);
    let meta = SignatureMetadata {
        signature: sig.to_base64(),
        key_fingerprint: kp.fingerprint(),
        signer: "OldSigner".to_string(),
        timestamp: old_timestamp,
        content_type: ContentType::Plugin,
        comment: Some("Expired test".to_string()),
    };
    write_custom_signature(&path, &meta)?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    // The verifier at the native-plugin layer does crypto check; policy is above.
    // Verify the signature is still cryptographically valid despite old timestamp.
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path)?;
    assert!(result.verified);

    // The crypto-level VerificationService would flag this with a warning
    let crypto_verifier = openracing_crypto::ed25519::Ed25519Verifier::new(store);
    let vresult = crypto_verifier.verify_content(content, &meta)?;
    assert!(vresult.signature_valid);

    // The warning about old signature comes from the verification service layer
    let has_age_warning = vresult
        .warnings
        .iter()
        .any(|w| w.to_lowercase().contains("days old"));
    assert!(
        has_age_warning,
        "Old signature should produce age warning, got: {:?}",
        vresult.warnings
    );

    Ok(())
}

// ===========================================================================
// 8. Revoked key handling
// ===========================================================================

#[test]
fn revoked_key_prevents_plugin_load() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"plugin from revoked signer";
    let path = write_fake_plugin(tmp.path(), "revoked.dll", content)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, content, &kp, "RevokedDev")?;

    let mut store = TrustStore::new_in_memory();
    // Initially trusted, then revoked
    store.add_key(
        kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("Initially trusted".to_string()),
    )?;
    store.update_trust_level(
        &kp.fingerprint(),
        TrustLevel::Distrusted,
        Some("Key compromised, revoked".to_string()),
    )?;

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(result.is_err(), "Revoked key must prevent plugin loading");

    Ok(())
}

#[test]
fn revoked_key_entry_is_distrusted() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let mut store = TrustStore::new_in_memory();

    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;
    assert_eq!(
        store.get_trust_level(&kp.fingerprint()),
        TrustLevel::Trusted
    );

    store.update_trust_level(
        &kp.fingerprint(),
        TrustLevel::Distrusted,
        Some("Revoked".to_string()),
    )?;
    assert_eq!(
        store.get_trust_level(&kp.fingerprint()),
        TrustLevel::Distrusted
    );

    Ok(())
}

// ===========================================================================
// 9. Trust chain / unknown key validation
// ===========================================================================

#[test]
fn unknown_key_strict_mode_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"plugin from unknown signer";
    let path = write_fake_plugin(tmp.path(), "unknown.dll", content)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, content, &kp, "UnknownDev")?;

    // Trust store does NOT have this key
    let store = TrustStore::new_in_memory();

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);
    assert!(
        result.is_err(),
        "Unknown signer must be rejected in strict mode"
    );

    Ok(())
}

#[test]
fn unknown_key_permissive_mode_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"plugin from unknown signer";
    let path = write_fake_plugin(tmp.path(), "unknown.dll", content)?;
    let kp = gen_keypair()?;
    sign_plugin_file(&path, content, &kp, "UnknownDev")?;

    let store = TrustStore::new_in_memory();

    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::permissive());
    let result = verifier.verify(&path);
    assert!(
        result.is_err(),
        "Unknown signer must be rejected in permissive mode because signature cannot be verified"
    );

    Ok(())
}

// ===========================================================================
// 10. Configuration modes
// ===========================================================================

#[test]
fn config_strict_defaults_are_secure() {
    let config = SignatureVerificationConfig::strict();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn config_development_is_permissive() {
    let config = SignatureVerificationConfig::development();
    assert!(!config.require_signatures);
    assert!(config.allow_unsigned);
}

// ===========================================================================
// 11. Key rotation for plugin signing
// ===========================================================================

#[test]
fn key_rotation_resign_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"plugin that needs re-signing";
    let path = write_fake_plugin(tmp.path(), "rotate.dll", content)?;

    let old_kp = gen_keypair()?;
    let new_kp = gen_keypair()?;

    // Sign with old key
    sign_plugin_file(&path, content, &old_kp, "OldDev")?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(old_kp.public_key.clone(), TrustLevel::Trusted, None)?;
    store.add_key(new_kp.public_key.clone(), TrustLevel::Trusted, None)?;

    // Verify with old key works
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path)?;
    assert!(result.verified);

    // Revoke old key and re-sign with new key
    store.update_trust_level(
        &old_kp.fingerprint(),
        TrustLevel::Distrusted,
        Some("Rotated".to_string()),
    )?;
    sign_plugin_file(&path, content, &new_kp, "NewDev")?;

    // Old key rejected
    // (but we already re-signed, so this verifies with the new signature)
    let verifier2 = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result2 = verifier2.verify(&path)?;
    assert!(result2.verified);

    let result_meta = result2.metadata.as_ref().ok_or("missing metadata")?;
    assert_eq!(result_meta.signer, "NewDev");

    Ok(())
}

// ===========================================================================
// 12. Multiple plugins with the same key
// ===========================================================================

#[test]
fn multiple_plugins_same_signing_key() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let kp = gen_keypair()?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    for i in 0..5 {
        let name = format!("plugin_{}.dll", i);
        let content = format!("binary content for plugin {}", i);
        let path = write_fake_plugin(tmp.path(), &name, content.as_bytes())?;
        sign_plugin_file(&path, content.as_bytes(), &kp, "SharedSigner")?;

        let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&path)?;
        assert!(result.verified, "Plugin {} should verify", i);
        assert_eq!(result.trust_level, TrustLevel::Trusted);
    }

    Ok(())
}

// ===========================================================================
// 13. Signature metadata field validation
// ===========================================================================

#[test]
fn signature_metadata_fields_populated() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let content = b"metadata fields test";
    let path = write_fake_plugin(tmp.path(), "meta.dll", content)?;
    let kp = gen_keypair()?;
    let meta = sign_plugin_file(&path, content, &kp, "MetaTester")?;

    assert_eq!(meta.signer, "MetaTester");
    assert_eq!(meta.key_fingerprint, kp.fingerprint());
    assert!(!meta.signature.is_empty());
    assert!(matches!(meta.content_type, ContentType::Plugin));

    // Verify the base64 signature is decodable and 64 bytes
    let sig = Signature::from_base64(&meta.signature)?;
    assert_eq!(sig.signature_bytes.len(), 64);

    Ok(())
}

// ===========================================================================
// 14. Missing signature file
// ===========================================================================

#[test]
fn missing_sig_file_with_strict_config() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let path = write_fake_plugin(tmp.path(), "nosig.dll", b"no signature file")?;

    // Ensure no .sig file exists
    let sig_path = path.with_extension("dll.sig");
    assert!(!sig_path.exists());

    let store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&path);

    assert!(result.is_err(), "Missing sig file in strict mode must fail");

    Ok(())
}

// ===========================================================================
// 15. Signature content type variations
// ===========================================================================

#[test]
fn sign_plugin_with_various_content_types() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::TempDir::new()?;
    let kp = gen_keypair()?;
    let content = b"plugin binary for content type test";

    let content_types = [
        ContentType::Plugin,
        ContentType::Binary,
        ContentType::Firmware,
        ContentType::Update,
        ContentType::Profile,
    ];

    for (i, ct) in content_types.iter().enumerate() {
        let name = format!("ct_{}.dll", i);
        let path = write_fake_plugin(tmp.path(), &name, content)?;

        let meta =
            Ed25519Signer::sign_with_metadata(content, &kp, "ContentTypeTester", ct.clone(), None)?;
        utils::create_detached_signature(&path, &meta)?;

        // Crypto signature is valid regardless of content type
        let sig = Signature::from_base64(&meta.signature)?;
        assert!(
            openracing_crypto::ed25519::Ed25519Verifier::verify(content, &sig, &kp.public_key)?,
            "Signature must be valid for content type variant {}",
            i
        );
    }

    Ok(())
}

// ===========================================================================
// 16. Error type coverage
// ===========================================================================

#[test]
fn error_unsigned_plugin_display() {
    let err = NativePluginError::UnsignedPlugin {
        path: std::path::PathBuf::from("/path/to/plugin.dll"),
    };
    let msg = err.to_string();
    assert!(msg.contains("unsigned") || msg.contains("Unsigned") || msg.contains("plugin"));
}

#[test]
fn error_distrusted_signer_display() {
    let err = NativePluginError::DistrustedSigner {
        fingerprint: "abc123def456".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("abc123def456"),
        "Error must include fingerprint"
    );
}

#[test]
fn error_signature_verification_failed_display() {
    let err = NativePluginError::SignatureVerificationFailed("bad sig".to_string());
    let msg = err.to_string();
    assert!(msg.contains("bad sig"));
}
