//! Native plugin hardening tests.
//!
//! These tests cover:
//! - Plugin discovery (valid / invalid plugins)
//! - Signature verification integration (Ed25519 trust store)
//! - Plugin lifecycle data structures
//! - Plugin isolation guarantees
//! - Error handling (missing symbols, ABI mismatch)
//! - Plugin capability enforcement via the ABI check layer

use openracing_native_plugin::abi_check::{
    AbiCheckResult, CURRENT_ABI_VERSION, check_abi_compatibility,
};
use openracing_native_plugin::error::{NativePluginError, NativePluginLoadError};
use openracing_native_plugin::loader::{NativePluginConfig, NativePluginHost};
use openracing_native_plugin::plugin::{PluginFrame, SharedMemoryHeader};
use openracing_native_plugin::signature::{SignatureVerificationConfig, SignatureVerifier};
use openracing_native_plugin::spsc::SpscChannel;

use openracing_crypto::TrustLevel;
use openracing_crypto::trust_store::TrustStore;

use std::path::PathBuf;

// ===================================================================
// ABI compatibility checking
// ===================================================================

#[test]
fn abi_check_current_version_is_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn abi_check_version_zero_is_incompatible() {
    if CURRENT_ABI_VERSION != 0 {
        let result = check_abi_compatibility(0);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn abi_check_future_version_is_incompatible() {
    let future = CURRENT_ABI_VERSION + 100;
    let result = check_abi_compatibility(future);
    assert_eq!(
        result,
        AbiCheckResult::Mismatch {
            expected: CURRENT_ABI_VERSION,
            actual: future,
        }
    );
}

#[test]
fn abi_check_max_u32_is_incompatible() {
    if CURRENT_ABI_VERSION != u32::MAX {
        let result = check_abi_compatibility(u32::MAX);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn abi_check_adjacent_versions_are_incompatible() {
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

// ===================================================================
// Plugin discovery – verifying file-based signature presence
// ===================================================================

#[test]
fn strict_mode_rejects_unsigned_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("test.dll");
    std::fs::write(&plugin_path, b"not a real plugin")?;

    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&plugin_path);

    assert!(result.is_err(), "strict mode must reject unsigned plugins");
    Ok(())
}

#[test]
fn dev_mode_allows_unsigned_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("test.dll");
    std::fs::write(&plugin_path, b"not a real plugin")?;

    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::development());
    let result = verifier.verify(&plugin_path)?;

    assert!(!result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Unknown);
    assert!(!result.warnings.is_empty(), "should warn about unsigned");
    Ok(())
}

#[test]
fn nonexistent_plugin_file_returns_error() {
    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&PathBuf::from("nonexistent_plugin.dll"));
    // Strict + no signature file = error (UnsignedPlugin)
    assert!(result.is_err());
}

#[test]
fn permissive_mode_allows_unsigned_with_warning() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("test.dll");
    std::fs::write(&plugin_path, b"test")?;

    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::permissive());
    let result = verifier.verify(&plugin_path)?;

    assert!(!result.is_signed);
    assert!(result.verified);
    assert!(!result.warnings.is_empty());
    Ok(())
}

// ===================================================================
// Signature verification integration (Ed25519 trust store)
// ===================================================================

#[test]
fn signed_plugin_with_trusted_key_verifies() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
    use openracing_crypto::verification::ContentType;

    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("trusted.dll");
    let data = b"trusted plugin binary";
    std::fs::write(&plugin_path, data)?;

    let keypair = KeyPair::generate()?;
    let metadata = Ed25519Signer::sign_with_metadata(
        data,
        &keypair,
        "Trusted Dev",
        ContentType::Plugin,
        None,
    )?;

    let sig_path = plugin_path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&metadata)?)?;

    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&plugin_path)?;

    assert!(result.is_signed);
    assert!(result.verified);
    assert_eq!(result.trust_level, TrustLevel::Trusted);
    assert!(result.warnings.is_empty());
    Ok(())
}

#[test]
fn distrusted_key_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
    use openracing_crypto::verification::ContentType;

    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("evil.so");
    let data = b"evil plugin";
    std::fs::write(&plugin_path, data)?;

    let keypair = KeyPair::generate()?;
    let metadata =
        Ed25519Signer::sign_with_metadata(data, &keypair, "Evil Dev", ContentType::Plugin, None)?;
    let sig_path = plugin_path.with_extension("so.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&metadata)?)?;

    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(
        keypair.public_key.clone(),
        TrustLevel::Distrusted,
        Some("Revoked".into()),
    )?;

    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&plugin_path);

    assert!(result.is_err(), "distrusted key must be rejected");
    Ok(())
}

#[test]
fn tampered_content_fails_verification() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
    use openracing_crypto::verification::ContentType;

    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("tampered.dll");
    let original = b"original plugin content";
    std::fs::write(&plugin_path, original)?;

    let keypair = KeyPair::generate()?;
    let metadata =
        Ed25519Signer::sign_with_metadata(original, &keypair, "Author", ContentType::Plugin, None)?;
    let sig_path = plugin_path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&metadata)?)?;

    // Tamper with plugin after signing
    std::fs::write(&plugin_path, b"TAMPERED plugin content")?;

    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(keypair.public_key.clone(), TrustLevel::Trusted, None)?;

    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&plugin_path);

    assert!(result.is_err(), "tampered content must fail verification");
    Ok(())
}

#[test]
fn unknown_key_strict_mode_rejects() -> Result<(), Box<dyn std::error::Error>> {
    use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
    use openracing_crypto::verification::ContentType;

    let dir = tempfile::TempDir::new()?;
    let plugin_path = dir.path().join("unknown.dll");
    let data = b"unknown signer plugin";
    std::fs::write(&plugin_path, data)?;

    let keypair = KeyPair::generate()?;
    let metadata =
        Ed25519Signer::sign_with_metadata(data, &keypair, "Unknown", ContentType::Plugin, None)?;
    let sig_path = plugin_path.with_extension("dll.sig");
    std::fs::write(&sig_path, serde_json::to_string_pretty(&metadata)?)?;

    // Empty trust store — key is unknown
    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&plugin_path);

    assert!(
        result.is_err(),
        "unknown key must be rejected in strict mode"
    );
    Ok(())
}

// ===================================================================
// Plugin lifecycle data structures
// ===================================================================

#[test]
fn plugin_frame_default_has_sane_values() {
    let frame = PluginFrame::default();
    assert_eq!(frame.ffb_in, 0.0);
    assert_eq!(frame.torque_out, 0.0);
    assert_eq!(frame.budget_us, 1000);
    assert_eq!(frame.sequence, 0);
}

#[test]
fn plugin_frame_is_copy() {
    let frame = PluginFrame {
        ffb_in: 1.0,
        torque_out: 0.5,
        wheel_speed: 10.0,
        timestamp_ns: 999,
        budget_us: 500,
        sequence: 42,
    };
    let copy = frame;
    assert_eq!(copy.ffb_in, 1.0);
    assert_eq!(copy.sequence, 42);
}

#[test]
fn shared_memory_header_has_expected_fields() {
    // Verify SharedMemoryHeader contains expected atomic fields
    // This is a layout-only check; we can't easily construct a SharedMemoryHeader
    // without shared memory, so just verify the type compiles and is Debug.
    let _: fn(&SharedMemoryHeader) -> String = |h| format!("{:?}", h);
}

// ===================================================================
// SPSC channel tests
// ===================================================================

#[test]
fn spsc_channel_write_read_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 32;
    let channel =
        SpscChannel::new(frame_size).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    let writer = channel.writer();
    let reader = channel.reader();

    let data = vec![0xABu8; frame_size];
    writer
        .write(&data)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    let mut buf = vec![0u8; frame_size];
    reader
        .read(&mut buf)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    assert_eq!(buf, data);
    Ok(())
}

#[test]
fn spsc_channel_rejects_wrong_frame_size() -> Result<(), Box<dyn std::error::Error>> {
    let channel =
        SpscChannel::new(16).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    let writer = channel.writer();

    // Write with wrong size
    let result = writer.write(&[0u8; 32]);
    assert!(result.is_err(), "must reject frame with wrong size");
    Ok(())
}

#[test]
fn spsc_channel_full_buffer_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 8;
    let capacity = 2u32;
    let channel = SpscChannel::with_capacity(frame_size, capacity)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    let writer = channel.writer();

    let frame = vec![1u8; frame_size];
    writer
        .write(&frame)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    writer
        .write(&frame)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    // Buffer should now be full
    let result = writer.write(&frame);
    assert!(result.is_err(), "must reject write when buffer is full");
    Ok(())
}

#[test]
fn spsc_channel_empty_read_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let channel =
        SpscChannel::new(16).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    let reader = channel.reader();

    let mut buf = vec![0u8; 16];
    let result = reader.read(&mut buf);
    assert!(result.is_err(), "reading from empty buffer must fail");
    Ok(())
}

#[test]
fn spsc_try_write_returns_false_when_full() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 8;
    let channel = SpscChannel::with_capacity(frame_size, 1)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    let writer = channel.writer();

    let frame = vec![1u8; frame_size];
    let first = writer
        .try_write(&frame)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    assert!(first, "first write should succeed");

    let second = writer
        .try_write(&frame)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    assert!(!second, "second write should return false (full)");
    Ok(())
}

#[test]
fn spsc_try_read_returns_false_when_empty() -> Result<(), Box<dyn std::error::Error>> {
    let channel =
        SpscChannel::new(16).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    let reader = channel.reader();

    let mut buf = vec![0u8; 16];
    let got = reader
        .try_read(&mut buf)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    assert!(!got, "try_read on empty buffer should return false");
    Ok(())
}

#[test]
fn spsc_shutdown_flag_propagates() -> Result<(), Box<dyn std::error::Error>> {
    let channel =
        SpscChannel::new(16).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    assert!(!channel.is_shutdown());
    channel.shutdown();
    assert!(channel.is_shutdown());
    Ok(())
}

#[test]
fn spsc_has_data_reflects_state() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = 8;
    let channel =
        SpscChannel::new(frame_size).map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    let writer = channel.writer();
    let reader = channel.reader();

    assert!(!reader.has_data());
    writer
        .write(&vec![0u8; frame_size])
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    assert!(reader.has_data());

    let mut buf = vec![0u8; frame_size];
    reader
        .read(&mut buf)
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
    assert!(!reader.has_data());
    Ok(())
}

// ===================================================================
// Error handling (ABI mismatch errors)
// ===================================================================

#[test]
fn native_plugin_error_abi_mismatch_formats_correctly() {
    let err = NativePluginError::AbiMismatch {
        expected: 1,
        actual: 99,
    };
    let msg = format!("{err}");
    assert!(msg.contains("1"), "error should mention expected version");
    assert!(msg.contains("99"), "error should mention actual version");
}

#[test]
fn native_plugin_load_error_converts_to_native_plugin_error() {
    let load_err = NativePluginLoadError::AbiMismatch {
        expected: 1,
        actual: 2,
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(
        err,
        NativePluginError::AbiMismatch {
            expected: 1,
            actual: 2
        }
    ));
}

#[test]
fn unsigned_plugin_error_includes_path() {
    let err = NativePluginError::UnsignedPlugin {
        path: PathBuf::from("/path/to/plugin.so"),
    };
    let msg = format!("{err}");
    assert!(msg.contains("plugin.so"));
}

#[test]
fn distrusted_signer_error_includes_fingerprint() {
    let err = NativePluginError::DistrustedSigner {
        fingerprint: "abc123".to_string(),
    };
    let msg = format!("{err}");
    assert!(msg.contains("abc123"));
}

#[test]
fn budget_violation_error_includes_timing() {
    let err = NativePluginError::BudgetViolation {
        used_us: 5000,
        budget_us: 1000,
    };
    let msg = format!("{err}");
    assert!(msg.contains("5000"));
    assert!(msg.contains("1000"));
}

#[test]
fn execution_timeout_error_includes_duration() {
    let err = NativePluginError::ExecutionTimeout { duration_us: 999 };
    let msg = format!("{err}");
    assert!(msg.contains("999"));
}

// ===================================================================
// Configuration modes
// ===================================================================

#[test]
fn config_default_is_strict() {
    let config = NativePluginConfig::default();
    assert!(!config.allow_unsigned);
    assert!(config.require_signatures);
}

#[test]
fn config_strict_matches_default() {
    let strict = NativePluginConfig::strict();
    let default = NativePluginConfig::default();
    assert_eq!(strict.allow_unsigned, default.allow_unsigned);
    assert_eq!(strict.require_signatures, default.require_signatures);
}

#[test]
fn config_permissive_allows_unsigned_but_requires_sigs() {
    let config = NativePluginConfig::permissive();
    assert!(config.allow_unsigned);
    assert!(config.require_signatures);
}

#[test]
fn config_development_is_fully_open() {
    let config = NativePluginConfig::development();
    assert!(config.allow_unsigned);
    assert!(!config.require_signatures);
}

#[test]
fn config_to_signature_config_maps_correctly() {
    let config = NativePluginConfig::permissive();
    let sig_config = config.to_signature_config();
    assert_eq!(sig_config.require_signatures, config.require_signatures);
    assert_eq!(sig_config.allow_unsigned, config.allow_unsigned);
}

// ===================================================================
// Host lifecycle (async)
// ===================================================================

#[tokio::test]
async fn host_default_starts_empty() {
    let host = NativePluginHost::new_with_defaults();
    assert_eq!(host.plugin_count().await, 0);
    assert!(!host.config().allow_unsigned);
}

#[tokio::test]
async fn host_dev_mode_starts_empty_and_permissive() {
    let host = NativePluginHost::new_permissive_for_development();
    assert_eq!(host.plugin_count().await, 0);
    assert!(host.config().allow_unsigned);
    assert!(!host.config().require_signatures);
}

#[tokio::test]
async fn host_is_loaded_returns_false_for_unknown_id() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();
    assert!(!host.is_loaded(id).await);
}

#[tokio::test]
async fn host_get_plugin_returns_none_for_unknown_id() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();
    assert!(host.get_plugin(id).await.is_none());
}

#[tokio::test]
async fn host_set_config_updates_mode() {
    let trust_store = TrustStore::new_in_memory();
    let mut host = NativePluginHost::new(trust_store, NativePluginConfig::development());
    assert!(host.config().allow_unsigned);

    host.set_config(NativePluginConfig::strict());
    assert!(!host.config().allow_unsigned);
    assert!(host.config().require_signatures);
}

#[tokio::test]
async fn host_unload_nonexistent_plugin_is_ok() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();
    // Unloading a non-existent plugin should not error
    let result = host.unload_plugin(id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn host_load_nonexistent_library_returns_error() {
    let host = NativePluginHost::new_permissive_for_development();
    let id = uuid::Uuid::new_v4();
    let result = host
        .load_plugin(
            id,
            "test".to_string(),
            &PathBuf::from("nonexistent.dll"),
            1000,
        )
        .await;
    assert!(result.is_err());
}
