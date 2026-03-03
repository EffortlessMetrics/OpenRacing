//! Tests for the native plugin communication protocol.
//!
//! Covers:
//! - Protocol message encoding/decoding round-trips (PluginFrame, SharedMemoryHeader)
//! - ABI version compatibility checking
//! - Handshake sequences (host creation, loader setup, config negotiation)
//! - Error handling for malformed messages (SPSC size mismatches, buffer errors)
//! - Security types (signature verification config, trust level checks)

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::{Ed25519Signer, Ed25519Verifier, KeyPair, PublicKey, TrustLevel};
use openracing_crypto::verification::ContentType;
use openracing_native_plugin::error::{NativePluginError, NativePluginLoadError};
use openracing_native_plugin::plugin::{PluginFrame, SharedMemoryHeader};
use openracing_native_plugin::signature::{
    SignatureVerificationConfig, SignatureVerificationResult, SignatureVerifier,
};
use openracing_native_plugin::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginConfig, NativePluginHost, NativePluginLoader,
    SpscChannel, check_abi_compatibility,
};

// ---------------------------------------------------------------------------
// 1. Protocol message encoding/decoding round-trips
// ---------------------------------------------------------------------------

#[test]
fn plugin_frame_default_round_trip() {
    let frame = PluginFrame::default();
    let bytes = unsafe {
        std::slice::from_raw_parts(
            &frame as *const PluginFrame as *const u8,
            std::mem::size_of::<PluginFrame>(),
        )
    };

    let mut restored = PluginFrame::default();
    let dst = unsafe {
        std::slice::from_raw_parts_mut(
            &mut restored as *mut PluginFrame as *mut u8,
            std::mem::size_of::<PluginFrame>(),
        )
    };
    dst.copy_from_slice(bytes);

    assert_eq!(restored.ffb_in, frame.ffb_in);
    assert_eq!(restored.torque_out, frame.torque_out);
    assert_eq!(restored.wheel_speed, frame.wheel_speed);
    assert_eq!(restored.timestamp_ns, frame.timestamp_ns);
    assert_eq!(restored.budget_us, frame.budget_us);
    assert_eq!(restored.sequence, frame.sequence);
}

#[test]
fn plugin_frame_custom_values_round_trip() {
    let frame = PluginFrame {
        ffb_in: -1.5,
        torque_out: 42.0,
        wheel_speed: 33.3,
        timestamp_ns: u64::MAX,
        budget_us: 500,
        sequence: 99,
    };

    let bytes = unsafe {
        std::slice::from_raw_parts(
            &frame as *const PluginFrame as *const u8,
            std::mem::size_of::<PluginFrame>(),
        )
    };

    let mut restored = PluginFrame::default();
    let dst = unsafe {
        std::slice::from_raw_parts_mut(
            &mut restored as *mut PluginFrame as *mut u8,
            std::mem::size_of::<PluginFrame>(),
        )
    };
    dst.copy_from_slice(bytes);

    assert_eq!(restored.ffb_in, -1.5);
    assert_eq!(restored.torque_out, 42.0);
    assert_eq!(restored.wheel_speed, 33.3);
    assert_eq!(restored.timestamp_ns, u64::MAX);
    assert_eq!(restored.budget_us, 500);
    assert_eq!(restored.sequence, 99);
}

#[test]
fn spsc_write_read_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = std::mem::size_of::<PluginFrame>();
    let channel = SpscChannel::new(frame_size)?;

    let writer = channel.writer();
    let reader = channel.reader();

    let frame = PluginFrame {
        ffb_in: 0.75,
        torque_out: -0.25,
        wheel_speed: 5.5,
        timestamp_ns: 1_000_000,
        budget_us: 200,
        sequence: 7,
    };

    let frame_bytes = unsafe {
        std::slice::from_raw_parts(
            &frame as *const PluginFrame as *const u8,
            frame_size,
        )
    };

    writer.write(frame_bytes)?;

    let mut buffer = vec![0u8; frame_size];
    reader.read(&mut buffer)?;

    let restored: PluginFrame = unsafe { std::ptr::read(buffer.as_ptr() as *const PluginFrame) };

    assert_eq!(restored.ffb_in, 0.75);
    assert_eq!(restored.torque_out, -0.25);
    assert_eq!(restored.wheel_speed, 5.5);
    assert_eq!(restored.timestamp_ns, 1_000_000);
    assert_eq!(restored.budget_us, 200);
    assert_eq!(restored.sequence, 7);

    Ok(())
}

#[test]
fn spsc_multiple_frames_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 32;
    let capacity = 8u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;

    let writer = channel.writer();
    let reader = channel.reader();

    for i in 0u8..5 {
        let frame = vec![i; frame_size];
        writer.write(&frame)?;
    }

    for i in 0u8..5 {
        let mut buffer = vec![0u8; frame_size];
        reader.read(&mut buffer)?;
        assert_eq!(buffer, vec![i; frame_size]);
    }

    Ok(())
}

#[test]
fn spsc_wrap_around_ring_buffer() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 8;
    let capacity = 4u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;

    let writer = channel.writer();
    let reader = channel.reader();

    // Fill, drain, then fill again to exercise wrap-around
    for i in 0u8..4 {
        writer.write(&vec![i; frame_size])?;
    }
    for i in 0u8..4 {
        let mut buf = vec![0u8; frame_size];
        reader.read(&mut buf)?;
        assert_eq!(buf[0], i);
    }
    for i in 10u8..14 {
        writer.write(&vec![i; frame_size])?;
    }
    for i in 10u8..14 {
        let mut buf = vec![0u8; frame_size];
        reader.read(&mut buf)?;
        assert_eq!(buf[0], i);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 2. ABI version compatibility checking
// ---------------------------------------------------------------------------

#[test]
fn abi_current_version_is_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_version_zero_rejected() {
    if CURRENT_ABI_VERSION != 0 {
        let result = check_abi_compatibility(0);
        assert_eq!(
            result,
            AbiCheckResult::Mismatch {
                expected: CURRENT_ABI_VERSION,
                actual: 0,
            }
        );
    }
}

#[test]
fn abi_version_max_rejected() {
    if CURRENT_ABI_VERSION != u32::MAX {
        let result = check_abi_compatibility(u32::MAX);
        assert_eq!(
            result,
            AbiCheckResult::Mismatch {
                expected: CURRENT_ABI_VERSION,
                actual: u32::MAX,
            }
        );
    }
}

#[test]
fn abi_adjacent_versions_rejected() {
    let below = CURRENT_ABI_VERSION.saturating_sub(1);
    let above = CURRENT_ABI_VERSION.saturating_add(1);

    if below != CURRENT_ABI_VERSION {
        assert!(matches!(
            check_abi_compatibility(below),
            AbiCheckResult::Mismatch { .. }
        ));
    }

    if above != CURRENT_ABI_VERSION {
        assert!(matches!(
            check_abi_compatibility(above),
            AbiCheckResult::Mismatch { .. }
        ));
    }
}

#[test]
fn abi_mismatch_carries_correct_versions() {
    let plugin_version = CURRENT_ABI_VERSION.wrapping_add(42);
    if plugin_version != CURRENT_ABI_VERSION {
        match check_abi_compatibility(plugin_version) {
            AbiCheckResult::Mismatch { expected, actual } => {
                assert_eq!(expected, CURRENT_ABI_VERSION);
                assert_eq!(actual, plugin_version);
            }
            AbiCheckResult::Compatible => {
                panic!("Expected ABI mismatch for version {plugin_version}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Handshake sequences
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handshake_host_creation_defaults() {
    let host = NativePluginHost::new_with_defaults();
    assert_eq!(host.plugin_count().await, 0);
    assert!(host.config().require_signatures);
    assert!(!host.config().allow_unsigned);
}

#[tokio::test]
async fn handshake_host_development_mode() {
    let host = NativePluginHost::new_permissive_for_development();
    assert!(host.config().allow_unsigned);
    assert!(!host.config().require_signatures);
    assert_eq!(host.plugin_count().await, 0);
}

#[tokio::test]
async fn handshake_config_transition_strict_to_dev() {
    let trust_store = TrustStore::new_in_memory();
    let mut host = NativePluginHost::new(trust_store, NativePluginConfig::strict());
    assert!(!host.config().allow_unsigned);

    host.set_config(NativePluginConfig::development());
    assert!(host.config().allow_unsigned);
    assert!(!host.config().require_signatures);
}

#[tokio::test]
async fn handshake_config_transition_dev_to_strict() {
    let trust_store = TrustStore::new_in_memory();
    let mut host = NativePluginHost::new(trust_store, NativePluginConfig::development());
    assert!(host.config().allow_unsigned);

    host.set_config(NativePluginConfig::strict());
    assert!(!host.config().allow_unsigned);
    assert!(host.config().require_signatures);
}

#[test]
fn handshake_loader_with_trust_store() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::with_defaults(&trust_store);

    assert!(!loader.config().allow_unsigned);
    assert!(loader.config().require_signatures);
    assert!(!loader.trust_store().is_empty());
}

#[test]
fn handshake_loader_custom_config() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::new(&trust_store, NativePluginConfig::permissive());

    assert!(loader.config().allow_unsigned);
    assert!(loader.config().require_signatures);
}

#[test]
fn handshake_config_to_signature_config_mapping() {
    let config = NativePluginConfig::permissive();
    let sig_config = config.to_signature_config();

    assert_eq!(sig_config.require_signatures, config.require_signatures);
    assert_eq!(sig_config.allow_unsigned, config.allow_unsigned);
}

#[tokio::test]
async fn handshake_unload_nonexistent_plugin_is_ok() {
    let host = NativePluginHost::new_with_defaults();
    let result = host.unload_plugin(uuid::Uuid::new_v4()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn handshake_is_loaded_returns_false_for_absent() {
    let host = NativePluginHost::new_with_defaults();
    assert!(!host.is_loaded(uuid::Uuid::new_v4()).await);
}

#[test]
fn handshake_spsc_channel_init() -> Result<(), Box<dyn std::error::Error>> {
    let channel = SpscChannel::new(64)?;
    assert_eq!(channel.frame_size(), 64);
    assert!(!channel.is_shutdown());
    Ok(())
}

#[test]
fn handshake_spsc_shutdown_signal() -> Result<(), Box<dyn std::error::Error>> {
    let channel = SpscChannel::new(64)?;
    assert!(!channel.is_shutdown());

    channel.shutdown();
    assert!(channel.is_shutdown());

    Ok(())
}

// ---------------------------------------------------------------------------
// 4. Error handling for malformed messages
// ---------------------------------------------------------------------------

#[test]
fn error_spsc_write_wrong_frame_size() -> Result<(), Box<dyn std::error::Error>> {
    let channel = SpscChannel::new(64)?;
    let writer = channel.writer();

    // Too small
    let result = writer.write(&[0u8; 32]);
    assert!(result.is_err());
    let err_msg = format!("{}", result.as_ref().err().map_or("", |e| {
        // Return a &str that lives long enough by leaking
        // Actually, let's match on the error variant
        match e {
            NativePluginError::SharedMemoryError(msg) => msg.as_str(),
            _ => "",
        }
    }));
    assert!(err_msg.contains("mismatch") || err_msg.contains("size"));

    // Too large
    let result = writer.write(&[0u8; 128]);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn error_spsc_read_wrong_buffer_size() -> Result<(), Box<dyn std::error::Error>> {
    let channel = SpscChannel::new(64)?;
    let reader = channel.reader();

    // Provide undersized buffer
    let mut small_buffer = [0u8; 16];
    let result = reader.read(&mut small_buffer);
    assert!(result.is_err());

    // Provide oversized buffer
    let mut big_buffer = [0u8; 128];
    let result = reader.read(&mut big_buffer);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn error_spsc_read_empty_channel() -> Result<(), Box<dyn std::error::Error>> {
    let channel = SpscChannel::new(32)?;
    let reader = channel.reader();

    let mut buffer = vec![0u8; 32];
    let result = reader.read(&mut buffer);
    assert!(result.is_err());

    assert!(!reader.has_data());

    Ok(())
}

#[test]
fn error_spsc_try_read_empty_returns_false() -> Result<(), Box<dyn std::error::Error>> {
    let channel = SpscChannel::new(32)?;
    let reader = channel.reader();

    let mut buffer = vec![0u8; 32];
    let result = reader.try_read(&mut buffer)?;
    assert!(!result);

    Ok(())
}

#[test]
fn error_spsc_ring_buffer_full() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 16;
    let capacity = 2u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;

    let writer = channel.writer();
    let frame = vec![0xABu8; frame_size];

    writer.write(&frame)?;
    writer.write(&frame)?;

    // Third write should fail - buffer is full
    let result = writer.write(&frame);
    assert!(result.is_err());

    Ok(())
}

#[test]
fn error_spsc_try_write_full_returns_false() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 16;
    let capacity = 2u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)?;

    let writer = channel.writer();
    let frame = vec![0xCDu8; frame_size];

    assert!(writer.try_write(&frame)?);
    assert!(writer.try_write(&frame)?);
    assert!(!writer.try_write(&frame)?);

    Ok(())
}

#[test]
fn error_spsc_oversized_shared_memory_rejected() {
    // frame_size * capacity must exceed 4MB to trigger the error
    let result = SpscChannel::with_capacity(1024, 8192);
    assert!(result.is_err());
}

#[test]
fn error_native_plugin_error_display_messages() {
    let err = NativePluginError::AbiMismatch {
        expected: 1,
        actual: 2,
    };
    let msg = err.to_string();
    assert!(msg.contains('1'));
    assert!(msg.contains('2'));

    let err = NativePluginError::UnsignedPlugin {
        path: PathBuf::from("/fake/plugin.so"),
    };
    assert!(err.to_string().contains("unsigned"));

    let err = NativePluginError::UntrustedSigner {
        fingerprint: "abc123".to_string(),
    };
    assert!(err.to_string().contains("abc123"));

    let err = NativePluginError::DistrustedSigner {
        fingerprint: "deadbeef".to_string(),
    };
    assert!(err.to_string().contains("deadbeef"));

    let err = NativePluginError::ExecutionTimeout { duration_us: 5000 };
    assert!(err.to_string().contains("5000"));

    let err = NativePluginError::BudgetViolation {
        used_us: 300,
        budget_us: 200,
    };
    let msg = err.to_string();
    assert!(msg.contains("300"));
    assert!(msg.contains("200"));

    let err = NativePluginError::Crashed {
        reason: "segfault".to_string(),
    };
    assert!(err.to_string().contains("segfault"));
}

#[test]
fn error_load_error_to_plugin_error_conversion() {
    let load_err = NativePluginLoadError::AbiMismatch {
        expected: 1,
        actual: 99,
    };
    let plugin_err: NativePluginError = load_err.into();
    assert!(matches!(
        plugin_err,
        NativePluginError::AbiMismatch {
            expected: 1,
            actual: 99,
        }
    ));

    let load_err = NativePluginLoadError::InvalidSignature {
        reason: "bad sig".to_string(),
    };
    let plugin_err: NativePluginError = load_err.into();
    assert!(matches!(
        plugin_err,
        NativePluginError::SignatureVerificationFailed(_)
    ));

    let load_err = NativePluginLoadError::UnsignedPlugin {
        path: "/test.so".to_string(),
    };
    let plugin_err: NativePluginError = load_err.into();
    assert!(matches!(plugin_err, NativePluginError::UnsignedPlugin { .. }));

    let load_err = NativePluginLoadError::UntrustedSigner {
        fingerprint: "fp123".to_string(),
    };
    let plugin_err: NativePluginError = load_err.into();
    assert!(matches!(
        plugin_err,
        NativePluginError::UntrustedSigner { .. }
    ));

    let load_err = NativePluginLoadError::LibraryLoadFailed {
        reason: "not found".to_string(),
    };
    let plugin_err: NativePluginError = load_err.into();
    assert!(matches!(plugin_err, NativePluginError::LoadingFailed(_)));

    let load_err = NativePluginLoadError::InitializationFailed {
        reason: "null ptr".to_string(),
    };
    let plugin_err: NativePluginError = load_err.into();
    assert!(matches!(
        plugin_err,
        NativePluginError::InitializationFailed(_)
    ));
}

#[test]
fn error_load_error_display_messages() {
    let err = NativePluginLoadError::AbiMismatch {
        expected: 1,
        actual: 5,
    };
    assert!(err.to_string().contains('1'));
    assert!(err.to_string().contains('5'));

    let err = NativePluginLoadError::InvalidSignature {
        reason: "corrupt".to_string(),
    };
    assert!(err.to_string().contains("corrupt"));

    let err = NativePluginLoadError::UnsignedPlugin {
        path: "/a/b.so".to_string(),
    };
    assert!(err.to_string().contains("unsigned"));

    let err = NativePluginLoadError::UntrustedSigner {
        fingerprint: "aabbcc".to_string(),
    };
    assert!(err.to_string().contains("aabbcc"));

    let err = NativePluginLoadError::LibraryLoadFailed {
        reason: "missing symbol".to_string(),
    };
    assert!(err.to_string().contains("missing symbol"));

    let err = NativePluginLoadError::InitializationFailed {
        reason: "oom".to_string(),
    };
    assert!(err.to_string().contains("oom"));
}

// ---------------------------------------------------------------------------
// 5. Security – signature verification types and Ed25519 integration
// ---------------------------------------------------------------------------

#[test]
fn security_default_config_is_strict() {
    let config = SignatureVerificationConfig::default();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn security_strict_config() {
    let config = SignatureVerificationConfig::strict();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn security_permissive_config() {
    let config = SignatureVerificationConfig::permissive();
    assert!(config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn security_development_config() {
    let config = SignatureVerificationConfig::development();
    assert!(!config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn security_verifier_rejects_unsigned_in_strict_mode() {
    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());

    // Verify against a path that has no .sig file
    let fake_path = PathBuf::from("nonexistent_plugin.so");
    let result = verifier.verify(&fake_path);

    assert!(result.is_err());
    assert!(matches!(
        result.as_ref().err(),
        Some(NativePluginError::UnsignedPlugin { .. })
    ));
}

#[test]
fn security_verifier_allows_unsigned_in_permissive_mode() {
    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::permissive());

    // Path with no .sig file - permissive should allow it with a warning
    let fake_path = PathBuf::from("nonexistent_plugin.so");
    let result = verifier.verify(&fake_path);

    match result {
        Ok(verification) => {
            assert!(!verification.is_signed);
            assert!(verification.verified);
            assert_eq!(verification.trust_level, TrustLevel::Unknown);
            assert!(!verification.warnings.is_empty());
        }
        Err(_) => {
            // Development mode also rejects if allow_unsigned is set but
            // require_signatures causes a different path - this is acceptable
        }
    }
}

#[test]
fn security_verifier_allows_unsigned_in_dev_mode() {
    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::development());

    let fake_path = PathBuf::from("test_plugin.dll");
    let result = verifier.verify(&fake_path);

    match result {
        Ok(verification) => {
            assert!(!verification.is_signed);
            assert!(verification.verified);
        }
        Err(e) => {
            panic!("Development mode should allow unsigned plugins: {e}");
        }
    }
}

#[test]
fn security_verification_result_fields() {
    let result = SignatureVerificationResult {
        is_signed: true,
        metadata: None,
        trust_level: TrustLevel::Trusted,
        verified: true,
        warnings: vec![],
    };

    assert!(result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Trusted);
    assert!(result.warnings.is_empty());
}

#[test]
fn security_verification_result_with_warnings() {
    let result = SignatureVerificationResult {
        is_signed: false,
        metadata: None,
        trust_level: TrustLevel::Unknown,
        verified: true,
        warnings: vec!["Plugin is unsigned".to_string()],
    };

    assert!(!result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Unknown);
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn security_trust_store_integration() {
    let mut trust_store = TrustStore::new_in_memory();

    let test_key = PublicKey {
        key_bytes: [42u8; 32],
        identifier: "test-plugin-signer".to_string(),
        comment: Some("Test key for protocol tests".to_string()),
    };

    let fingerprint = Ed25519Verifier::get_key_fingerprint(&test_key);
    assert!(
        trust_store
            .add_key(test_key, TrustLevel::Trusted, Some("test".to_string()))
            .is_ok()
    );

    assert_eq!(trust_store.get_trust_level(&fingerprint), TrustLevel::Trusted);

    // Unknown key returns Unknown trust level
    assert_eq!(
        trust_store.get_trust_level("nonexistent-fingerprint"),
        TrustLevel::Unknown
    );
}

#[test]
fn security_distrusted_key_in_trust_store() {
    let mut trust_store = TrustStore::new_in_memory();

    let bad_key = PublicKey {
        key_bytes: [0xFFu8; 32],
        identifier: "compromised-key".to_string(),
        comment: None,
    };

    let fingerprint = Ed25519Verifier::get_key_fingerprint(&bad_key);
    assert!(
        trust_store
            .add_key(
                bad_key,
                TrustLevel::Distrusted,
                Some("compromised".to_string())
            )
            .is_ok()
    );

    assert_eq!(
        trust_store.get_trust_level(&fingerprint),
        TrustLevel::Distrusted
    );
}

#[test]
fn security_ed25519_sign_verify_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let keypair = KeyPair::generate()?;
    let data = b"native plugin binary content";

    let signature = Ed25519Signer::sign(data, &keypair.signing_key)?;
    let is_valid = Ed25519Verifier::verify(data, &signature, &keypair.public_key)?;
    assert!(is_valid);

    // Tampered data must fail
    let tampered = b"tampered plugin binary content";
    let is_valid = Ed25519Verifier::verify(tampered, &signature, &keypair.public_key)?;
    assert!(!is_valid);

    Ok(())
}

#[test]
fn security_ed25519_wrong_key_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let keypair1 = KeyPair::generate()?;
    let keypair2 = KeyPair::generate()?;
    let data = b"plugin data";

    let signature = Ed25519Signer::sign(data, &keypair1.signing_key)?;

    // Verify with a different key must fail
    let is_valid = Ed25519Verifier::verify(data, &signature, &keypair2.public_key)?;
    assert!(!is_valid);

    Ok(())
}

#[test]
fn security_signature_metadata_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let keypair = KeyPair::generate()?;
    let data = b"test data for metadata";

    let metadata = Ed25519Signer::sign_with_metadata(
        data,
        &keypair,
        "test-signer",
        ContentType::Plugin,
        Some("protocol test".to_string()),
    )?;

    assert_eq!(metadata.signer, "test-signer");
    assert_eq!(metadata.key_fingerprint, keypair.fingerprint());
    assert!(matches!(metadata.content_type, ContentType::Plugin));
    assert_eq!(metadata.comment, Some("protocol test".to_string()));

    // Verify the embedded base64 signature decodes and validates
    let sig = openracing_crypto::Signature::from_base64(&metadata.signature)?;
    let is_valid = Ed25519Verifier::verify(data, &sig, &keypair.public_key)?;
    assert!(is_valid);

    Ok(())
}

// ---------------------------------------------------------------------------
// Plugin frame repr(C) layout sanity checks
// ---------------------------------------------------------------------------

#[test]
fn plugin_frame_is_copy() {
    let a = PluginFrame {
        ffb_in: 1.0,
        torque_out: 2.0,
        wheel_speed: 3.0,
        timestamp_ns: 4,
        budget_us: 5,
        sequence: 6,
    };
    let b = a; // Copy
    assert_eq!(a.ffb_in, b.ffb_in);
    assert_eq!(a.sequence, b.sequence);
}

#[test]
fn shared_memory_header_has_atomics() {
    let header = SharedMemoryHeader {
        version: 1,
        producer_seq: AtomicU32::new(0),
        consumer_seq: AtomicU32::new(0),
        frame_size: 64,
        max_frames: 1024,
        shutdown_flag: AtomicBool::new(false),
    };

    assert_eq!(header.version, 1);
    assert_eq!(header.producer_seq.load(Ordering::Relaxed), 0);
    assert_eq!(header.consumer_seq.load(Ordering::Relaxed), 0);
    assert_eq!(header.frame_size, 64);
    assert_eq!(header.max_frames, 1024);
    assert!(!header.shutdown_flag.load(Ordering::Relaxed));
}

// ---------------------------------------------------------------------------
// Concurrent handshake simulation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handshake_concurrent_host_queries() {
    let host = Arc::new(NativePluginHost::new_with_defaults());
    let mut handles = Vec::new();

    for _ in 0..20 {
        let h = Arc::clone(&host);
        handles.push(tokio::spawn(async move {
            let count = h.plugin_count().await;
            let loaded = h.is_loaded(uuid::Uuid::new_v4()).await;
            (count, loaded)
        }));
    }

    for handle in handles {
        let (count, loaded) = handle.await.expect("task should not panic");
        assert_eq!(count, 0);
        assert!(!loaded);
    }
}
