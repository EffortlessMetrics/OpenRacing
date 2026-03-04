//! Deep security and code signing tests for the plugin system.
//!
//! Covers: Ed25519 key generation, signature creation/verification roundtrips,
//! tampered binary detection, key rotation, expired signature handling,
//! malformed signature rejection, wrong key rejection, empty/large payload signing,
//! key serialization, signature format stability, trust store integration,
//! WASM module integrity verification, native plugin signature checks,
//! unsigned plugin rejection, and capability permission enforcement.

use std::path::Path;

use tempfile::TempDir;
use uuid::Uuid;

use openracing_crypto::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair, PublicKey, Signature};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::{ContentType, VerificationConfig, VerificationService};
use openracing_crypto::{SignatureMetadata, SignatureVerifier as _, TrustLevel};

use racing_wheel_plugins::PluginClass;
use racing_wheel_plugins::capability::CapabilityChecker;
use racing_wheel_plugins::manifest::{Capability, ManifestValidator, PluginManifest};
use racing_wheel_plugins::native::{SignatureVerificationConfig, SignatureVerifier};
use racing_wheel_plugins::wasm::{ResourceLimits, WasmRuntime};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn gen_keypair() -> Result<KeyPair, Box<dyn std::error::Error>> {
    Ok(KeyPair::generate()?)
}

fn write_fake_plugin(
    dir: &Path,
    name: &str,
    content: &[u8],
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let path = dir.join(name);
    std::fs::write(&path, content)?;
    Ok(path)
}

fn sign_plugin_file(
    plugin_path: &Path,
    content: &[u8],
    keypair: &KeyPair,
    signer: &str,
) -> Result<SignatureMetadata, Box<dyn std::error::Error>> {
    let meta =
        Ed25519Signer::sign_with_metadata(content, keypair, signer, ContentType::Plugin, None)?;
    openracing_crypto::utils::create_detached_signature(plugin_path, &meta)?;
    Ok(meta)
}

fn write_custom_signature(
    plugin_path: &Path,
    metadata: &SignatureMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    openracing_crypto::utils::create_detached_signature(plugin_path, metadata)?;
    Ok(())
}

fn make_test_manifest(class: PluginClass) -> PluginManifest {
    use racing_wheel_plugins::manifest::{EntryPoints, PluginConstraints, PluginOperation};

    PluginManifest {
        id: Uuid::new_v4(),
        name: "Security Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "Plugin for security tests".to_string(),
        author: "Test Author".to_string(),
        license: "MIT".to_string(),
        homepage: None,
        class,
        capabilities: vec![Capability::ReadTelemetry],
        operations: vec![PluginOperation::TelemetryProcessor],
        constraints: PluginConstraints {
            max_execution_time_us: 100,
            max_memory_bytes: 1024 * 1024,
            update_rate_hz: 60,
            cpu_affinity: None,
        },
        entry_points: EntryPoints {
            wasm_module: Some("plugin.wasm".to_string()),
            native_library: None,
            main_function: "process".to_string(),
            init_function: Some("init".to_string()),
            cleanup_function: Some("cleanup".to_string()),
        },
        config_schema: None,
        signature: None,
    }
}

fn compile_wat(wat: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let wasm = wat::parse_str(wat)?;
    Ok(wasm)
}

const PASSTHROUGH_WAT: &str = r#"
(module
    (memory (export "memory") 1)
    (func (export "process") (param f32 f32) (result f32)
        local.get 0
    )
)
"#;

// ===================================================================
// 1. Ed25519 key generation produces unique keys
// ===================================================================

#[test]
fn keygen_multiple_keys_are_unique() -> Result<(), Box<dyn std::error::Error>> {
    let mut seen_fingerprints = std::collections::HashSet::new();
    for _ in 0..10 {
        let kp = gen_keypair()?;
        assert!(
            seen_fingerprints.insert(kp.fingerprint()),
            "Generated keys must have unique fingerprints"
        );
    }
    Ok(())
}

// ===================================================================
// 2. Signature creation and verification roundtrip
// ===================================================================

#[test]
fn sign_verify_roundtrip_various_payloads() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let payloads: &[&[u8]] = &[
        b"simple text",
        b"\x00\x01\x02\xff\xfe",
        &[0u8; 1],
        &[0xAB; 512],
    ];

    for payload in payloads {
        let sig = Ed25519Signer::sign(payload, &kp.signing_key)?;
        assert!(
            Ed25519Verifier::verify(payload, &sig, &kp.public_key)?,
            "Roundtrip must succeed for payload of length {}",
            payload.len()
        );
    }
    Ok(())
}

// ===================================================================
// 3. Tampered binary detection — single bit flip
// ===================================================================

#[test]
fn tampered_binary_single_bit_flip_detected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let mut data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;

    // Flip one bit in the first byte
    data[0] ^= 0x01;
    let valid = Ed25519Verifier::verify(&data, &sig, &kp.public_key)?;
    assert!(!valid, "Single bit flip must invalidate signature");

    Ok(())
}

#[test]
fn tampered_binary_appended_byte_detected() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"original content".to_vec();
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;

    let mut tampered = data.clone();
    tampered.push(0x00);
    let valid = Ed25519Verifier::verify(&tampered, &sig, &kp.public_key)?;
    assert!(!valid, "Appended byte must invalidate signature");

    Ok(())
}

// ===================================================================
// 4. Key rotation — old key → new key
// ===================================================================

#[test]
fn key_rotation_old_signature_invalid_with_new_key() -> Result<(), Box<dyn std::error::Error>> {
    let old_kp = gen_keypair()?;
    let new_kp = gen_keypair()?;
    let data = b"plugin binary content";

    // Sign with old key
    let sig = Ed25519Signer::sign(data, &old_kp.signing_key)?;

    // Verify with old key succeeds
    assert!(Ed25519Verifier::verify(data, &sig, &old_kp.public_key)?);
    // Verify with new key fails
    assert!(
        !Ed25519Verifier::verify(data, &sig, &new_kp.public_key)?,
        "Old signature must not verify with new key"
    );

    // Re-sign with new key
    let new_sig = Ed25519Signer::sign(data, &new_kp.signing_key)?;
    assert!(Ed25519Verifier::verify(data, &new_sig, &new_kp.public_key)?);
    assert!(
        !Ed25519Verifier::verify(data, &new_sig, &old_kp.public_key)?,
        "New signature must not verify with old key"
    );

    Ok(())
}

#[test]
fn key_rotation_trust_store_update() -> Result<(), Box<dyn std::error::Error>> {
    let old_kp = gen_keypair()?;
    let new_kp = gen_keypair()?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(
        old_kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("old production key".to_string()),
    )?;

    // Old key is trusted
    assert_eq!(
        store.get_trust_level(&old_kp.fingerprint()),
        TrustLevel::Trusted
    );

    // Rotate: distrust old, trust new
    store.update_trust_level(
        &old_kp.fingerprint(),
        TrustLevel::Distrusted,
        Some("rotated out".to_string()),
    )?;
    store.add_key(
        new_kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("new production key".to_string()),
    )?;

    assert_eq!(
        store.get_trust_level(&old_kp.fingerprint()),
        TrustLevel::Distrusted
    );
    assert_eq!(
        store.get_trust_level(&new_kp.fingerprint()),
        TrustLevel::Trusted
    );

    Ok(())
}

// ===================================================================
// 5. Expired signature handling
// ===================================================================

#[test]
fn expired_signature_generates_warning() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let kp = gen_keypair()?;
    let content = b"plugin content for expiry test";
    let plugin_path = write_fake_plugin(tmp.path(), "expired_plugin.wasm", content)?;

    // Create signature with timestamp > 365 days ago
    let sig = Ed25519Signer::sign(content, &kp.signing_key)?;
    let old_timestamp = chrono::Utc::now() - chrono::Duration::days(400);
    let metadata = SignatureMetadata {
        signature: sig.to_base64(),
        key_fingerprint: kp.fingerprint(),
        signer: "OldSigner".to_string(),
        timestamp: old_timestamp,
        content_type: ContentType::Plugin,
        comment: None,
    };
    write_custom_signature(&plugin_path, &metadata)?;

    // Set up verification service with this key
    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    let verifier = Ed25519Verifier::new(trust_store);
    let result = verifier.verify_content(content, &metadata)?;

    assert!(result.signature_valid);
    assert!(
        result.warnings.iter().any(|w| w.contains("days old")),
        "Expected age warning, got: {:?}",
        result.warnings
    );

    Ok(())
}

// ===================================================================
// 6. Malformed signature rejection
// ===================================================================

#[test]
fn malformed_signature_base64_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let result = Signature::from_base64("not-valid-base64!!!");
    assert!(result.is_err(), "Invalid base64 must be rejected");
    Ok(())
}

#[test]
fn malformed_signature_wrong_length_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // 32 bytes encoded as base64 (wrong length, should be 64)
    let short = openracing_crypto::utils::encode_base64(&[0u8; 32]);
    let result = Signature::from_base64(&short);
    assert!(result.is_err(), "32-byte signature must be rejected");

    // 128 bytes (too long)
    let long = openracing_crypto::utils::encode_base64(&[0u8; 128]);
    let result = Signature::from_base64(&long);
    assert!(result.is_err(), "128-byte signature must be rejected");

    Ok(())
}

#[test]
fn malformed_signature_empty_string_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let result = Signature::from_base64("");
    assert!(result.is_err(), "Empty signature string must be rejected");
    Ok(())
}

// ===================================================================
// 7. Wrong key rejection
// ===================================================================

#[test]
fn wrong_key_rejects_valid_signature() -> Result<(), Box<dyn std::error::Error>> {
    let signer_kp = gen_keypair()?;
    let wrong_kp = gen_keypair()?;
    let data = b"signed by signer, verified with wrong key";

    let sig = Ed25519Signer::sign(data, &signer_kp.signing_key)?;
    let valid = Ed25519Verifier::verify(data, &sig, &wrong_kp.public_key)?;
    assert!(!valid, "Signature must not verify with wrong public key");

    Ok(())
}

// ===================================================================
// 8. Empty payload signing
// ===================================================================

#[test]
fn empty_payload_sign_and_verify() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig = Ed25519Signer::sign(b"", &kp.signing_key)?;

    assert!(
        Ed25519Verifier::verify(b"", &sig, &kp.public_key)?,
        "Empty payload signature must verify"
    );
    // Empty payload signature must not verify non-empty data
    assert!(
        !Ed25519Verifier::verify(b"x", &sig, &kp.public_key)?,
        "Empty payload sig must not match non-empty data"
    );

    Ok(())
}

// ===================================================================
// 9. Large payload signing (stress test)
// ===================================================================

#[test]
fn large_payload_sign_and_verify() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    // 1 MiB payload
    let data = vec![0xAA; 1024 * 1024];
    let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;

    assert!(
        Ed25519Verifier::verify(&data, &sig, &kp.public_key)?,
        "Large payload signature must verify"
    );
    Ok(())
}

// ===================================================================
// 10. Key serialization / deserialization
// ===================================================================

#[test]
fn key_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let bytes = kp.signing_key_bytes();

    let restored = KeyPair::from_bytes(&bytes, "roundtrip-test".to_string())?;
    assert!(
        kp.public_key.ct_eq(&restored.public_key),
        "Restored key must match original"
    );

    // Sign with original, verify with restored
    let data = b"serialization test";
    let sig = Ed25519Signer::sign(data, &kp.signing_key)?;
    assert!(Ed25519Verifier::verify(data, &sig, &restored.public_key)?);

    Ok(())
}

#[test]
fn public_key_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let json = serde_json::to_string(&kp.public_key)?;
    let deserialized: PublicKey = serde_json::from_str(&json)?;

    assert!(kp.public_key.ct_eq(&deserialized));
    assert_eq!(kp.public_key.fingerprint(), deserialized.fingerprint());

    Ok(())
}

// ===================================================================
// 11. Signature format stability — known test vectors
// ===================================================================

#[test]
fn signature_deterministic_for_same_key_and_data() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let data = b"deterministic signing test";

    let sig1 = Ed25519Signer::sign(data, &kp.signing_key)?;
    let sig2 = Ed25519Signer::sign(data, &kp.signing_key)?;

    assert!(
        sig1.ct_eq(&sig2),
        "Ed25519 must produce deterministic signatures for the same key and data"
    );
    Ok(())
}

#[test]
fn signature_differs_for_different_data() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let sig1 = Ed25519Signer::sign(b"message A", &kp.signing_key)?;
    let sig2 = Ed25519Signer::sign(b"message B", &kp.signing_key)?;

    assert!(
        !sig1.ct_eq(&sig2),
        "Different messages must produce different signatures"
    );
    Ok(())
}

#[test]
fn signature_byte_length_always_64() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    for size in [0, 1, 64, 256, 1024, 65536] {
        let data = vec![0xBB; size];
        let sig = Ed25519Signer::sign(&data, &kp.signing_key)?;
        assert_eq!(
            sig.signature_bytes.len(),
            64,
            "Signature must always be 64 bytes for data size {}",
            size
        );
    }
    Ok(())
}

// ===================================================================
// 12. Constant-time comparison integrity
// ===================================================================

#[test]
fn constant_time_eq_same_keys() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let pk_clone = PublicKey::from_bytes(kp.public_key.key_bytes, "clone".to_string());

    assert!(kp.public_key.ct_eq(&pk_clone));
    // Standard eq uses ct_eq
    assert_eq!(kp.public_key, pk_clone);

    Ok(())
}

#[test]
fn constant_time_eq_different_keys() -> Result<(), Box<dyn std::error::Error>> {
    let kp1 = gen_keypair()?;
    let kp2 = gen_keypair()?;

    assert!(!kp1.public_key.ct_eq(&kp2.public_key));
    assert_ne!(kp1.public_key, kp2.public_key);

    Ok(())
}

// ===================================================================
// 13. Trust store — distrusted key rejection
// ===================================================================

#[test]
fn distrusted_key_rejected_in_verification_service() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let kp = gen_keypair()?;
    let content = b"plugin signed by distrusted key";
    let plugin_path = write_fake_plugin(tmp.path(), "distrusted.wasm", content)?;
    sign_plugin_file(&plugin_path, content, &kp, "EvilSigner")?;

    let ts_path = tmp.path().join("trust_store.json");
    let mut store = TrustStore::new(ts_path.clone())?;
    store.add_key(
        kp.public_key.clone(),
        TrustLevel::Distrusted,
        Some("compromised key".to_string()),
    )?;
    store.save_to_file()?;

    let service = VerificationService::new(VerificationConfig {
        require_plugin_signatures: true,
        allow_unknown_signers: false,
        trust_store_path: ts_path,
        ..Default::default()
    })?;

    let result = service.verify_plugin(&plugin_path);
    assert!(
        result.is_err(),
        "Plugin signed by distrusted key must be rejected"
    );

    Ok(())
}

// ===================================================================
// 14. Trust store — system key immutability
// ===================================================================

#[test]
fn system_key_cannot_be_removed_or_modified() -> Result<(), Box<dyn std::error::Error>> {
    let store = TrustStore::new_in_memory();
    let system_keys: Vec<_> = store
        .list_keys()
        .into_iter()
        .filter(|(_, entry)| !entry.user_modifiable)
        .collect();

    assert!(!system_keys.is_empty(), "Must have at least one system key");

    let (fingerprint, _) = &system_keys[0];

    let mut store2 = TrustStore::new_in_memory();
    assert!(
        store2.remove_key(fingerprint).is_err(),
        "System key removal must fail"
    );
    assert!(
        store2
            .update_trust_level(fingerprint, TrustLevel::Distrusted, None)
            .is_err(),
        "System key modification must fail"
    );

    Ok(())
}

// ===================================================================
// 15. Secure key storage — bytes zeroization pattern
// ===================================================================

#[test]
fn keypair_bytes_can_be_extracted_and_restored() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;
    let secret_bytes = kp.signing_key_bytes();

    // The bytes must not be all zeros (key was generated)
    assert_ne!(secret_bytes, [0u8; 32], "Secret key must not be all zeros");

    // Restore and verify same public key
    let restored = KeyPair::from_bytes(&secret_bytes, "restored".to_string())?;
    assert!(kp.public_key.ct_eq(&restored.public_key));

    Ok(())
}

// ===================================================================
// 16. WASM module integrity — unsigned WASM loads when allowed
// ===================================================================

#[test]
fn wasm_unsigned_module_loads_in_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let wasm_bytes = compile_wat(PASSTHROUGH_WAT)?;
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    // The WASM runtime does not enforce signing at this layer;
    // it loads any valid WASM module. Signing is enforced by the
    // verification service before handoff to the runtime.
    runtime.load_plugin_from_bytes(id, &wasm_bytes, vec![Capability::ReadTelemetry])?;
    let output = runtime.process(&id, 42.0, 0.001)?;
    assert!(
        (output - 42.0).abs() < f32::EPSILON,
        "Unsigned WASM module must execute when loaded directly"
    );

    Ok(())
}

// ===================================================================
// 17. WASM module integrity — invalid WASM rejected
// ===================================================================

#[test]
fn wasm_invalid_module_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result =
        runtime.load_plugin_from_bytes(id, b"not valid wasm", vec![Capability::ReadTelemetry]);
    assert!(result.is_err(), "Invalid WASM bytes must be rejected");

    Ok(())
}

// ===================================================================
// 18. WASM module integrity — corrupted WASM rejected
// ===================================================================

#[test]
fn wasm_corrupted_module_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut wasm_bytes = compile_wat(PASSTHROUGH_WAT)?;
    // Corrupt the WASM magic header
    if wasm_bytes.len() > 4 {
        wasm_bytes[0] = 0xFF;
        wasm_bytes[1] = 0xFF;
    }

    let mut runtime = WasmRuntime::new()?;
    let id = Uuid::new_v4();

    let result = runtime.load_plugin_from_bytes(id, &wasm_bytes, vec![Capability::ReadTelemetry]);
    assert!(result.is_err(), "Corrupted WASM must be rejected");

    Ok(())
}

// ===================================================================
// 19. Native plugin signature check — signed plugin verified
// ===================================================================

#[test]
fn native_signed_plugin_verification_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let content = b"native plugin binary for signing test";
    let plugin_path = write_fake_plugin(tmp.path(), "signed_plugin.dll", content)?;
    let kp = gen_keypair()?;

    sign_plugin_file(&plugin_path, content, &kp, "TrustedAuthor")?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&store, config);
    let result = verifier.verify(&plugin_path)?;

    assert!(result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Trusted);

    Ok(())
}

// ===================================================================
// 20. Unsigned plugin rejection in strict mode
// ===================================================================

#[test]
fn native_unsigned_plugin_rejected_strict_mode() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let plugin_path = write_fake_plugin(tmp.path(), "no_sig.dll", b"unsigned content")?;

    let store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&store, config);

    let result = verifier.verify(&plugin_path);
    assert!(
        result.is_err(),
        "Unsigned plugin must be rejected in strict mode"
    );

    Ok(())
}

// ===================================================================
// 21. Unsigned plugin allowed in permissive mode
// ===================================================================

#[test]
fn native_unsigned_plugin_allowed_permissive_mode() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let plugin_path = write_fake_plugin(tmp.path(), "dev_plugin.dll", b"dev content")?;

    let store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::permissive();
    let verifier = SignatureVerifier::new(&store, config);

    let result = verifier.verify(&plugin_path)?;
    assert!(!result.is_signed);
    assert!(result.verified, "Permissive mode must allow unsigned");

    Ok(())
}

// ===================================================================
// 22. Native plugin tampered binary detection
// ===================================================================

#[test]
fn native_tampered_plugin_detected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let original = b"original native plugin content";
    let plugin_path = write_fake_plugin(tmp.path(), "tampered.dll", original)?;
    let kp = gen_keypair()?;

    sign_plugin_file(&plugin_path, original, &kp, "Author")?;

    // Tamper with the binary after signing
    std::fs::write(&plugin_path, b"TAMPERED native plugin content")?;

    let mut store = TrustStore::new_in_memory();
    store.add_key(kp.public_key.clone(), TrustLevel::Trusted, None)?;

    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&store, config);
    let result = verifier.verify(&plugin_path);

    assert!(
        result.is_err(),
        "Tampered native plugin must fail verification"
    );

    Ok(())
}

// ===================================================================
// 23. Plugin capability permission — read telemetry only
// ===================================================================

#[test]
fn capability_read_only_cannot_modify() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::new(vec![Capability::ReadTelemetry]);

    assert!(checker.check_telemetry_read().is_ok());
    assert!(
        checker.check_telemetry_modify().is_err(),
        "Read-only plugin must not modify telemetry"
    );
    assert!(
        checker.check_led_control().is_err(),
        "Read-only plugin must not control LEDs"
    );
    assert!(
        checker.check_dsp_processing().is_err(),
        "Read-only plugin must not process DSP"
    );
    assert!(
        checker.check_inter_plugin_comm().is_err(),
        "Read-only plugin must not do IPC"
    );

    Ok(())
}

// ===================================================================
// 24. Plugin capability — network scoping
// ===================================================================

#[test]
fn capability_network_scoped_to_allowed_hosts() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::new(vec![Capability::Network {
        hosts: vec![
            "api.openracing.io".to_string(),
            "telemetry.openracing.io".to_string(),
        ],
    }]);

    assert!(checker.check_network_access("api.openracing.io").is_ok());
    assert!(
        checker
            .check_network_access("telemetry.openracing.io")
            .is_ok()
    );
    assert!(
        checker.check_network_access("evil.com").is_err(),
        "Unlisted host must be denied"
    );
    assert!(
        checker.check_network_access("localhost").is_err(),
        "localhost must be denied unless explicitly listed"
    );

    Ok(())
}

// ===================================================================
// 25. Plugin capability — filesystem path scoping
// ===================================================================

#[test]
fn capability_filesystem_scoped_to_granted_paths() -> Result<(), Box<dyn std::error::Error>> {
    let checker = CapabilityChecker::new(vec![Capability::FileSystem {
        paths: vec!["/plugins/data".to_string(), "/tmp/plugin_cache".to_string()],
    }]);

    assert!(
        checker
            .check_file_access(Path::new("/plugins/data/config.json"))
            .is_ok()
    );
    assert!(
        checker
            .check_file_access(Path::new("/tmp/plugin_cache/state.bin"))
            .is_ok()
    );
    assert!(
        checker.check_file_access(Path::new("/etc/passwd")).is_err(),
        "System paths must be denied"
    );
    assert!(
        checker
            .check_file_access(Path::new("/home/user/.ssh/id_rsa"))
            .is_err(),
        "User home must be denied"
    );

    Ok(())
}

// ===================================================================
// 26. Manifest validator — Safe plugin cannot request dangerous capabilities
// ===================================================================

#[test]
fn manifest_safe_plugin_rejects_dangerous_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let validator = ManifestValidator::default();

    // ProcessDsp not allowed for Safe
    let mut m = make_test_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::ProcessDsp];
    assert!(
        validator.validate(&m).is_err(),
        "Safe plugin must not request ProcessDsp"
    );

    // Network not allowed for Safe
    let mut m = make_test_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::Network {
        hosts: vec!["example.com".to_string()],
    }];
    assert!(
        validator.validate(&m).is_err(),
        "Safe plugin must not request Network"
    );

    // FileSystem not allowed for Safe
    let mut m = make_test_manifest(PluginClass::Safe);
    m.capabilities = vec![Capability::FileSystem {
        paths: vec!["/".to_string()],
    }];
    assert!(
        validator.validate(&m).is_err(),
        "Safe plugin must not request FileSystem"
    );

    Ok(())
}

// ===================================================================
// 27. Signature metadata — content type preserved
// ===================================================================

#[test]
fn signature_metadata_content_type_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let kp = gen_keypair()?;

    for content_type in [
        ContentType::Binary,
        ContentType::Firmware,
        ContentType::Plugin,
        ContentType::Profile,
        ContentType::Update,
    ] {
        let meta =
            Ed25519Signer::sign_with_metadata(b"test", &kp, "Signer", content_type.clone(), None)?;

        // Serialize and deserialize
        let json = serde_json::to_string(&meta)?;
        let deserialized: SignatureMetadata = serde_json::from_str(&json)?;
        assert_eq!(
            std::mem::discriminant(&meta.content_type),
            std::mem::discriminant(&deserialized.content_type)
        );
    }

    Ok(())
}

// ===================================================================
// 28. WASM resource limits — fuel exhaustion terminates infinite loop
// ===================================================================

#[test]
fn wasm_fuel_exhaustion_terminates_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let infinite_loop_wat = r#"
    (module
        (memory (export "memory") 1)
        (func (export "process") (param f32 f32) (result f32)
            (loop $lp
                br $lp
            )
            local.get 0
        )
    )
    "#;
    let wasm_bytes = compile_wat(infinite_loop_wat)?;
    let limits = ResourceLimits::default().with_fuel(1_000);
    let mut runtime = WasmRuntime::with_limits(limits)?;
    let id = Uuid::new_v4();

    runtime.load_plugin_from_bytes(id, &wasm_bytes, vec![Capability::ReadTelemetry])?;
    let result = runtime.process(&id, 1.0, 0.001);
    assert!(result.is_err(), "Infinite loop must be terminated by fuel");

    Ok(())
}

// ===================================================================
// 29. Trust store import/export with key integrity
// ===================================================================

#[test]
fn trust_store_export_import_preserves_keys() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let export_path = tmp.path().join("exported.json");

    let kp = gen_keypair()?;
    let mut source = TrustStore::new_in_memory();
    source.add_key(
        kp.public_key.clone(),
        TrustLevel::Trusted,
        Some("export test".to_string()),
    )?;

    let exported = source.export_keys(&export_path, false)?;
    assert!(exported >= 1);

    let mut dest = TrustStore::new_in_memory();
    let import_result = dest.import_keys(&export_path, false)?;
    assert!(import_result.imported >= 1);

    let retrieved = dest.get_public_key(&kp.fingerprint());
    assert!(
        retrieved.is_some(),
        "Imported key must be retrievable by fingerprint"
    );

    let retrieved_key = retrieved.ok_or("key not found")?;
    assert!(
        kp.public_key.ct_eq(&retrieved_key),
        "Imported key bytes must match"
    );

    Ok(())
}

// ===================================================================
// 30. Verification service — unsigned plugin when required
// ===================================================================

#[test]
fn verification_service_rejects_unsigned_plugin_when_required()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let plugin_path = write_fake_plugin(tmp.path(), "no_sig.wasm", b"(module)")?;

    let ts_path = tmp.path().join("trust_store.json");
    let _store = TrustStore::new(ts_path.clone())?;

    let service = VerificationService::new(VerificationConfig {
        require_plugin_signatures: true,
        allow_unknown_signers: false,
        trust_store_path: ts_path,
        ..Default::default()
    })?;

    let result = service.verify_plugin(&plugin_path);
    assert!(
        result.is_err(),
        "Unsigned plugin must be rejected when signatures are required"
    );

    Ok(())
}
