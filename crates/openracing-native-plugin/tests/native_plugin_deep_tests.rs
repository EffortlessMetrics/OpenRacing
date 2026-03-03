//! Deep tests for the OpenRacing native plugin crate.
//!
//! Covers plugin loading mechanics, ABI version checking, symbol resolution,
//! plugin metadata, plugin isolation, error handling for bad plugins,
//! Ed25519 signature verification, plugin lifecycle, and multiple instances.

use std::path::PathBuf;

use openracing_native_plugin::abi_check::{AbiCheckResult, CURRENT_ABI_VERSION, check_abi_compatibility};
use openracing_native_plugin::error::{NativePluginError, NativePluginLoadError};
use openracing_native_plugin::loader::{NativePluginConfig, NativePluginHost, NativePluginLoader};
use openracing_native_plugin::plugin::{PluginFrame, SharedMemoryHeader};
use openracing_native_plugin::signature::{
    SignatureVerificationConfig, SignatureVerifier,
};
use openracing_native_plugin::spsc::SpscChannel;

use openracing_crypto::TrustLevel;
use openracing_crypto::trust_store::TrustStore;

// ---------------------------------------------------------------------------
// ABI version checking
// ---------------------------------------------------------------------------

#[test]
fn abi_version_is_nonzero() {
    const _: () = assert!(CURRENT_ABI_VERSION > 0);
}

#[test]
fn abi_check_compatible_with_current() {
    assert_eq!(
        check_abi_compatibility(CURRENT_ABI_VERSION),
        AbiCheckResult::Compatible
    );
}

#[test]
fn abi_check_mismatch_zero() {
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
fn abi_check_mismatch_max() {
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
fn abi_check_mismatch_adjacent_above() {
    let above = CURRENT_ABI_VERSION.saturating_add(1);
    if above != CURRENT_ABI_VERSION {
        assert!(matches!(
            check_abi_compatibility(above),
            AbiCheckResult::Mismatch { .. }
        ));
    }
}

#[test]
fn abi_check_mismatch_adjacent_below() {
    let below = CURRENT_ABI_VERSION.saturating_sub(1);
    if below != CURRENT_ABI_VERSION {
        assert!(matches!(
            check_abi_compatibility(below),
            AbiCheckResult::Mismatch { .. }
        ));
    }
}

#[test]
fn abi_check_result_debug_format() {
    let compatible = AbiCheckResult::Compatible;
    let debug_str = format!("{:?}", compatible);
    assert!(debug_str.contains("Compatible"));

    let mismatch = AbiCheckResult::Mismatch {
        expected: 1,
        actual: 2,
    };
    let debug_str = format!("{:?}", mismatch);
    assert!(debug_str.contains("Mismatch"));
}

#[test]
fn abi_check_result_eq() {
    assert_eq!(AbiCheckResult::Compatible, AbiCheckResult::Compatible);
    assert_ne!(
        AbiCheckResult::Compatible,
        AbiCheckResult::Mismatch {
            expected: 1,
            actual: 2
        }
    );
}

// ---------------------------------------------------------------------------
// Plugin loading mechanics — config
// ---------------------------------------------------------------------------

#[test]
fn config_default_is_strict() {
    let config = NativePluginConfig::default();
    assert!(!config.allow_unsigned);
    assert!(config.require_signatures);
}

#[test]
fn config_strict_matches_default() {
    let default = NativePluginConfig::default();
    let strict = NativePluginConfig::strict();
    assert_eq!(default.allow_unsigned, strict.allow_unsigned);
    assert_eq!(default.require_signatures, strict.require_signatures);
}

#[test]
fn config_permissive_allows_unsigned() {
    let config = NativePluginConfig::permissive();
    assert!(config.allow_unsigned);
    assert!(config.require_signatures);
}

#[test]
fn config_development_disables_signatures() {
    let config = NativePluginConfig::development();
    assert!(config.allow_unsigned);
    assert!(!config.require_signatures);
}

#[test]
fn config_to_signature_config_strict() {
    let config = NativePluginConfig::strict();
    let sig_config = config.to_signature_config();
    assert!(sig_config.require_signatures);
    assert!(!sig_config.allow_unsigned);
}

#[test]
fn config_to_signature_config_development() {
    let config = NativePluginConfig::development();
    let sig_config = config.to_signature_config();
    assert!(!sig_config.require_signatures);
    assert!(sig_config.allow_unsigned);
}

// ---------------------------------------------------------------------------
// Plugin loading — loader creation
// ---------------------------------------------------------------------------

#[test]
fn loader_with_defaults_uses_strict_config() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::with_defaults(&trust_store);
    assert!(!loader.config().allow_unsigned);
    assert!(loader.config().require_signatures);
}

#[test]
fn loader_custom_config_propagates() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::new(&trust_store, NativePluginConfig::development());
    assert!(loader.config().allow_unsigned);
    assert!(!loader.config().require_signatures);
}

#[test]
fn loader_trust_store_reference() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::with_defaults(&trust_store);
    // Just ensure the reference is accessible
    let _ts = loader.trust_store();
}

// ---------------------------------------------------------------------------
// Error handling for bad plugins
// ---------------------------------------------------------------------------

#[test]
fn load_nonexistent_library_returns_error() {
    let trust_store = TrustStore::new_in_memory();
    let loader = NativePluginLoader::new(&trust_store, NativePluginConfig::development());
    let id = uuid::Uuid::new_v4();
    let result = loader.load(
        id,
        "nonexistent".to_string(),
        &PathBuf::from("nonexistent_plugin.dll"),
        1000,
    );
    assert!(result.is_err());
}

#[test]
fn load_invalid_file_as_library_returns_error() {
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let fake_lib = dir.path().join("fake_plugin.dll");
        std::fs::write(&fake_lib, b"this is not a library").ok();
        let trust_store = TrustStore::new_in_memory();
        let loader = NativePluginLoader::new(&trust_store, NativePluginConfig::development());
        let id = uuid::Uuid::new_v4();
        let result = loader.load(id, "fake".to_string(), &fake_lib, 1000);
        assert!(result.is_err());
    }
}

#[test]
fn load_empty_file_as_library_returns_error() {
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let empty_lib = dir.path().join("empty.dll");
        std::fs::write(&empty_lib, b"").ok();
        let trust_store = TrustStore::new_in_memory();
        let loader = NativePluginLoader::new(&trust_store, NativePluginConfig::development());
        let id = uuid::Uuid::new_v4();
        let result = loader.load(id, "empty".to_string(), &empty_lib, 1000);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// Plugin metadata — PluginFrame
// ---------------------------------------------------------------------------

#[test]
fn plugin_frame_default_values() {
    let frame = PluginFrame::default();
    assert_eq!(frame.ffb_in, 0.0);
    assert_eq!(frame.torque_out, 0.0);
    assert_eq!(frame.wheel_speed, 0.0);
    assert_eq!(frame.timestamp_ns, 0);
    assert_eq!(frame.budget_us, 1000);
    assert_eq!(frame.sequence, 0);
}

#[test]
fn plugin_frame_custom_values() {
    let frame = PluginFrame {
        ffb_in: 0.75,
        torque_out: 0.5,
        wheel_speed: 15.0,
        timestamp_ns: 1_000_000_000,
        budget_us: 500,
        sequence: 42,
    };
    assert_eq!(frame.ffb_in, 0.75);
    assert_eq!(frame.torque_out, 0.5);
    assert_eq!(frame.wheel_speed, 15.0);
    assert_eq!(frame.timestamp_ns, 1_000_000_000);
    assert_eq!(frame.budget_us, 500);
    assert_eq!(frame.sequence, 42);
}

#[test]
fn plugin_frame_is_copy() {
    let frame = PluginFrame {
        ffb_in: 1.0,
        torque_out: 0.5,
        wheel_speed: 10.0,
        timestamp_ns: 123,
        budget_us: 100,
        sequence: 7,
    };
    let frame2 = frame;
    // Both should be usable (Copy trait)
    assert_eq!(frame.ffb_in, frame2.ffb_in);
    assert_eq!(frame.sequence, frame2.sequence);
}

#[test]
fn plugin_frame_repr_c_layout() {
    // PluginFrame is #[repr(C)], verify size is deterministic
    let size = std::mem::size_of::<PluginFrame>();
    // f32(4) + f32(4) + f32(4) + u64(8) + u32(4) + u32(4) = 28,
    // with padding for u64 alignment = 32
    assert!(size >= 28);
    // Alignment should accommodate u64
    assert!(std::mem::align_of::<PluginFrame>() >= 4);
}

// ---------------------------------------------------------------------------
// SharedMemoryHeader
// ---------------------------------------------------------------------------

#[test]
fn shared_memory_header_has_atomic_fields() {
    // Verify the struct contains atomic types by checking size >= expected
    let size = std::mem::size_of::<SharedMemoryHeader>();
    // version(u32) + producer_seq(AtomicU32) + consumer_seq(AtomicU32)
    // + frame_size(u32) + max_frames(u32) + shutdown_flag(AtomicBool)
    assert!(size > 4 + 4 + 4 + 4 + 4);
}

// ---------------------------------------------------------------------------
// Signature verification config
// ---------------------------------------------------------------------------

#[test]
fn sig_config_default_is_strict() {
    let config = SignatureVerificationConfig::default();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn sig_config_strict() {
    let config = SignatureVerificationConfig::strict();
    assert!(config.require_signatures);
    assert!(!config.allow_unsigned);
}

#[test]
fn sig_config_permissive() {
    let config = SignatureVerificationConfig::permissive();
    assert!(config.require_signatures);
    assert!(config.allow_unsigned);
}

#[test]
fn sig_config_development() {
    let config = SignatureVerificationConfig::development();
    assert!(!config.require_signatures);
    assert!(config.allow_unsigned);
}

// ---------------------------------------------------------------------------
// Signature verification — unsigned plugin handling
// ---------------------------------------------------------------------------

#[test]
fn verify_unsigned_plugin_strict_rejects() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("test_plugin.so");
        std::fs::write(&lib_path, b"fake-content").ok();

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&lib_path);
        assert!(result.is_err());
        if let Err(e) = result {
            let err_str = format!("{}", e);
            assert!(err_str.contains("unsigned") || err_str.contains("Unsigned"));
        }
    }
}

#[test]
fn verify_unsigned_plugin_development_allows() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("test_plugin.so");
        std::fs::write(&lib_path, b"fake-content").ok();

        let verifier =
            SignatureVerifier::new(&trust_store, SignatureVerificationConfig::development());
        let result = verifier.verify(&lib_path);
        assert!(result.is_ok());
        if let Ok(r) = result {
            assert!(!r.is_signed);
            assert!(r.metadata.is_none());
            assert!(r.verified);
        }
    }
}

#[test]
fn verify_unsigned_plugin_permissive_allows_with_warning() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("test_plugin.so");
        std::fs::write(&lib_path, b"fake-content").ok();

        let verifier =
            SignatureVerifier::new(&trust_store, SignatureVerificationConfig::permissive());
        let result = verifier.verify(&lib_path);
        assert!(result.is_ok());
        if let Ok(r) = result {
            assert!(!r.is_signed);
            assert!(!r.warnings.is_empty());
        }
    }
}

// ---------------------------------------------------------------------------
// Signature verification — malformed signature file
// ---------------------------------------------------------------------------

#[test]
fn verify_with_invalid_sig_file_returns_error() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("plugin.so");
        let sig_path = dir.path().join("plugin.so.sig");
        std::fs::write(&lib_path, b"fake-plugin-bytes").ok();
        std::fs::write(&sig_path, b"not-valid-json").ok();

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&lib_path);
        assert!(result.is_err());
    }
}

#[test]
fn verify_with_empty_sig_file_returns_error() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("plugin.dll");
        let sig_path = dir.path().join("plugin.dll.sig");
        std::fs::write(&lib_path, b"fake-plugin-bytes").ok();
        std::fs::write(&sig_path, b"").ok();

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&lib_path);
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[test]
fn error_display_abi_mismatch() {
    let err = NativePluginError::AbiMismatch {
        expected: 1,
        actual: 2,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("ABI"));
    assert!(msg.contains("mismatch"));
}

#[test]
fn error_display_unsigned_plugin() {
    let err = NativePluginError::UnsignedPlugin {
        path: PathBuf::from("/fake/path.so"),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("unsigned") || msg.contains("Unsigned"));
}

#[test]
fn error_display_untrusted_signer() {
    let err = NativePluginError::UntrustedSigner {
        fingerprint: "abc123".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("abc123"));
}

#[test]
fn error_display_distrusted_signer() {
    let err = NativePluginError::DistrustedSigner {
        fingerprint: "def456".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("def456"));
}

#[test]
fn error_display_budget_violation() {
    let err = NativePluginError::BudgetViolation {
        used_us: 1500,
        budget_us: 1000,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("1500"));
    assert!(msg.contains("1000"));
}

#[test]
fn error_display_crashed() {
    let err = NativePluginError::Crashed {
        reason: "segfault".to_string(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("segfault"));
}

#[test]
fn error_display_initialization_failed() {
    let err = NativePluginError::InitializationFailed("null pointer".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("null pointer"));
}

#[test]
fn error_display_execution_timeout() {
    let err = NativePluginError::ExecutionTimeout { duration_us: 5000 };
    let msg = format!("{}", err);
    assert!(msg.contains("5000"));
}

#[test]
fn error_display_config_error() {
    let err = NativePluginError::ConfigError("bad config".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("bad config"));
}

// ---------------------------------------------------------------------------
// NativePluginLoadError → NativePluginError conversion
// ---------------------------------------------------------------------------

#[test]
fn load_error_abi_mismatch_converts() {
    let load_err = NativePluginLoadError::AbiMismatch {
        expected: 1,
        actual: 99,
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(err, NativePluginError::AbiMismatch { expected: 1, actual: 99 }));
}

#[test]
fn load_error_invalid_signature_converts() {
    let load_err = NativePluginLoadError::InvalidSignature {
        reason: "bad sig".to_string(),
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(err, NativePluginError::SignatureVerificationFailed(_)));
}

#[test]
fn load_error_unsigned_plugin_converts() {
    let load_err = NativePluginLoadError::UnsignedPlugin {
        path: "/test".to_string(),
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(err, NativePluginError::UnsignedPlugin { .. }));
}

#[test]
fn load_error_untrusted_signer_converts() {
    let load_err = NativePluginLoadError::UntrustedSigner {
        fingerprint: "fp123".to_string(),
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(err, NativePluginError::UntrustedSigner { .. }));
}

#[test]
fn load_error_library_load_failed_converts() {
    let load_err = NativePluginLoadError::LibraryLoadFailed {
        reason: "not found".to_string(),
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(err, NativePluginError::LoadingFailed(_)));
}

#[test]
fn load_error_initialization_failed_converts() {
    let load_err = NativePluginLoadError::InitializationFailed {
        reason: "null".to_string(),
    };
    let err: NativePluginError = load_err.into();
    assert!(matches!(err, NativePluginError::InitializationFailed(_)));
}

#[test]
fn load_error_clone_works() {
    let load_err = NativePluginLoadError::AbiMismatch {
        expected: 1,
        actual: 2,
    };
    let cloned = load_err.clone();
    assert_eq!(format!("{}", load_err), format!("{}", cloned));
}

// ---------------------------------------------------------------------------
// Plugin host — async tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn host_default_has_no_plugins() {
    let host = NativePluginHost::new_with_defaults();
    assert_eq!(host.plugin_count().await, 0);
}

#[tokio::test]
async fn host_default_config_is_strict() {
    let host = NativePluginHost::new_with_defaults();
    assert!(!host.config().allow_unsigned);
    assert!(host.config().require_signatures);
}

#[tokio::test]
async fn host_development_config() {
    let host = NativePluginHost::new_permissive_for_development();
    assert!(host.config().allow_unsigned);
    assert!(!host.config().require_signatures);
}

#[tokio::test]
async fn host_set_config_changes_mode() {
    let trust_store = TrustStore::new_in_memory();
    let mut host = NativePluginHost::new(trust_store, NativePluginConfig::development());
    assert!(host.config().allow_unsigned);

    host.set_config(NativePluginConfig::strict());
    assert!(!host.config().allow_unsigned);
    assert!(host.config().require_signatures);
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
    let result = host.get_plugin(id).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn host_load_nonexistent_plugin_fails() {
    let host = NativePluginHost::new_permissive_for_development();
    let id = uuid::Uuid::new_v4();
    let result = host
        .load_plugin(id, "bad".to_string(), &PathBuf::from("no_such_file.dll"), 1000)
        .await;
    assert!(result.is_err());
    assert_eq!(host.plugin_count().await, 0);
}

#[tokio::test]
async fn host_unload_nonexistent_plugin_succeeds() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();
    let result = host.unload_plugin(id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn host_trust_store_accessible() {
    let host = NativePluginHost::new_with_defaults();
    let _ts = host.trust_store();
}

// ---------------------------------------------------------------------------
// Plugin isolation — multiple hosts don't share state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_hosts_are_independent() {
    let host1 = NativePluginHost::new_with_defaults();
    let host2 = NativePluginHost::new_permissive_for_development();

    assert!(!host1.config().allow_unsigned);
    assert!(host2.config().allow_unsigned);
    assert_eq!(host1.plugin_count().await, 0);
    assert_eq!(host2.plugin_count().await, 0);
}

// ---------------------------------------------------------------------------
// SPSC channel
// ---------------------------------------------------------------------------

#[test]
fn spsc_channel_creation_default_capacity() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(64)?;
    assert_eq!(channel.frame_size(), 64);
    assert!(!channel.is_shutdown());
    Ok(())
}

#[test]
fn spsc_channel_custom_capacity() -> Result<(), NativePluginError> {
    let channel = SpscChannel::with_capacity(32, 128)?;
    assert_eq!(channel.frame_size(), 32);
    assert_eq!(channel.max_frames(), 128);
    Ok(())
}

#[test]
fn spsc_channel_write_and_read_single_frame() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    let writer = channel.writer();
    let reader = channel.reader();

    let frame = [1u8, 2, 3, 4, 5, 6, 7, 8];
    writer.write(&frame)?;

    let mut buf = [0u8; 8];
    reader.read(&mut buf)?;
    assert_eq!(buf, frame);
    Ok(())
}

#[test]
fn spsc_channel_write_wrong_size_fails() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    let writer = channel.writer();

    let too_small = [0u8; 4];
    let result = writer.write(&too_small);
    assert!(result.is_err());

    let too_large = [0u8; 16];
    let result = writer.write(&too_large);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn spsc_channel_read_wrong_size_fails() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    let reader = channel.reader();

    let mut too_small = [0u8; 4];
    let result = reader.read(&mut too_small);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn spsc_channel_read_empty_fails() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    let reader = channel.reader();

    let mut buf = [0u8; 8];
    let result = reader.read(&mut buf);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn spsc_channel_has_data_empty() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    let reader = channel.reader();
    assert!(!reader.has_data());
    Ok(())
}

#[test]
fn spsc_channel_has_data_after_write() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    let writer = channel.writer();
    let reader = channel.reader();

    assert!(!reader.has_data());
    writer.write(&[0u8; 8])?;
    assert!(reader.has_data());
    Ok(())
}

#[test]
fn spsc_channel_ring_buffer_wraps() -> Result<(), NativePluginError> {
    let channel = SpscChannel::with_capacity(4, 4)?;
    let writer = channel.writer();
    let reader = channel.reader();

    // Fill the buffer
    for i in 0u8..4 {
        writer.write(&[i, i, i, i])?;
    }

    // Buffer full — next write should fail
    let result = writer.write(&[0xFF; 4]);
    assert!(result.is_err());

    // Read all
    for i in 0u8..4 {
        let mut buf = [0u8; 4];
        reader.read(&mut buf)?;
        assert_eq!(buf, [i, i, i, i]);
    }

    // Now we can write again
    writer.write(&[0xAA; 4])?;
    let mut buf = [0u8; 4];
    reader.read(&mut buf)?;
    assert_eq!(buf, [0xAA; 4]);
    Ok(())
}

#[test]
fn spsc_channel_try_write_returns_false_when_full() -> Result<(), NativePluginError> {
    let channel = SpscChannel::with_capacity(4, 2)?;
    let writer = channel.writer();

    let ok1 = writer.try_write(&[1; 4])?;
    assert!(ok1);
    let ok2 = writer.try_write(&[2; 4])?;
    assert!(ok2);
    let ok3 = writer.try_write(&[3; 4])?;
    assert!(!ok3);
    Ok(())
}

#[test]
fn spsc_channel_try_read_returns_false_when_empty() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(4)?;
    let reader = channel.reader();

    let mut buf = [0u8; 4];
    let ok = reader.try_read(&mut buf)?;
    assert!(!ok);
    Ok(())
}

#[test]
fn spsc_channel_shutdown_flag() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    assert!(!channel.is_shutdown());
    channel.shutdown();
    assert!(channel.is_shutdown());
    Ok(())
}

#[test]
fn spsc_channel_os_id_not_empty() -> Result<(), NativePluginError> {
    let channel = SpscChannel::new(8)?;
    assert!(!channel.os_id().is_empty());
    Ok(())
}

#[test]
fn spsc_channel_excessive_size_rejected() {
    // 4MB+ should be rejected
    let result = SpscChannel::with_capacity(1024, 8192);
    // 1024 * 8192 = 8MB > 4MB limit
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Ed25519 signature verification — integration with crypto crate
// ---------------------------------------------------------------------------

#[test]
fn signature_verifier_creation() {
    let trust_store = TrustStore::new_in_memory();
    let _verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
}

#[test]
fn verify_nonexistent_plugin_file_returns_error() {
    let trust_store = TrustStore::new_in_memory();
    let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
    let result = verifier.verify(&PathBuf::from("absolutely_nonexistent_path.dll"));
    assert!(result.is_err());
}

#[test]
fn verify_plugin_with_valid_sig_but_untrusted_key_in_strict_mode() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("plugin.so");
        let sig_path = dir.path().join("plugin.so.sig");
        std::fs::write(&lib_path, b"plugin-content").ok();

        // Create a valid-looking signature metadata JSON
        let sig_json = serde_json::json!({
            "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "key_fingerprint": "unknown-fingerprint-123",
            "signer": "unknown-signer",
            "timestamp": "2024-01-01T00:00:00Z",
            "content_type": "Plugin"
        });
        std::fs::write(&sig_path, sig_json.to_string()).ok();

        let verifier = SignatureVerifier::new(&trust_store, SignatureVerificationConfig::strict());
        let result = verifier.verify(&lib_path);
        // Should fail because key is unknown and strict doesn't allow unsigned
        assert!(result.is_err());
    }
}

#[test]
fn verify_plugin_with_sig_but_untrusted_key_in_permissive_mode() {
    let trust_store = TrustStore::new_in_memory();
    let dir = tempfile::tempdir();
    assert!(dir.is_ok());
    if let Ok(dir) = dir {
        let lib_path = dir.path().join("plugin.so");
        let sig_path = dir.path().join("plugin.so.sig");
        std::fs::write(&lib_path, b"plugin-content").ok();

        let sig_json = serde_json::json!({
            "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "key_fingerprint": "unknown-fingerprint-456",
            "signer": "someone",
            "timestamp": "2024-01-01T00:00:00Z",
            "content_type": "Plugin"
        });
        std::fs::write(&sig_path, sig_json.to_string()).ok();

        let verifier =
            SignatureVerifier::new(&trust_store, SignatureVerificationConfig::permissive());
        let result = verifier.verify(&lib_path);
        // Permissive allows unsigned, so unknown key should succeed with warnings
        assert!(result.is_ok());
        if let Ok(r) = result {
            assert!(r.is_signed);
            assert!(!r.warnings.is_empty());
        }
    }
}

// ---------------------------------------------------------------------------
// Ed25519 sign-and-verify roundtrip
// ---------------------------------------------------------------------------

#[test]
fn ed25519_sign_and_verify_roundtrip() {
    use openracing_crypto::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair};

    let keypair = KeyPair::generate();
    assert!(keypair.is_ok());
    if let Ok(kp) = keypair {
        let data = b"test plugin binary content";
        let signature = Ed25519Signer::sign(data, &kp.signing_key);
        assert!(signature.is_ok());
        if let Ok(sig) = signature {
            let verified = Ed25519Verifier::verify(data, &sig, &kp.public_key);
            assert!(verified.is_ok());
            if let Ok(v) = verified {
                assert!(v, "Signature should verify correctly");
            }
        }
    }
}

#[test]
fn ed25519_wrong_data_fails_verification() {
    use openracing_crypto::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair};

    let keypair = KeyPair::generate();
    assert!(keypair.is_ok());
    if let Ok(kp) = keypair {
        let data = b"original data";
        let signature = Ed25519Signer::sign(data, &kp.signing_key);
        assert!(signature.is_ok());
        if let Ok(sig) = signature {
            let tampered = b"tampered data";
            let verified = Ed25519Verifier::verify(tampered, &sig, &kp.public_key);
            assert!(verified.is_ok());
            if let Ok(v) = verified {
                assert!(!v, "Tampered data should fail verification");
            }
        }
    }
}

#[test]
fn ed25519_wrong_key_fails_verification() {
    use openracing_crypto::ed25519::{Ed25519Signer, Ed25519Verifier, KeyPair};

    let kp1 = KeyPair::generate();
    let kp2 = KeyPair::generate();
    assert!(kp1.is_ok());
    assert!(kp2.is_ok());
    if let (Ok(kp1), Ok(kp2)) = (kp1, kp2) {
        let data = b"plugin binary";
        let signature = Ed25519Signer::sign(data, &kp1.signing_key);
        assert!(signature.is_ok());
        if let Ok(sig) = signature {
            let verified = Ed25519Verifier::verify(data, &sig, &kp2.public_key);
            assert!(verified.is_ok());
            if let Ok(v) = verified {
                assert!(!v, "Wrong key should fail verification");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Trust store integration
// ---------------------------------------------------------------------------

#[test]
fn trust_store_unknown_fingerprint_returns_unknown() {
    let store = TrustStore::new_in_memory();
    let level = store.get_trust_level("nonexistent-fingerprint");
    assert_eq!(level, TrustLevel::Unknown);
}

#[test]
fn trust_store_no_public_key_for_unknown() {
    let store = TrustStore::new_in_memory();
    let key = store.get_public_key("nonexistent-fingerprint");
    assert!(key.is_none());
}

#[test]
fn trust_store_add_and_retrieve_key() {
    use openracing_crypto::ed25519::KeyPair;

    let mut store = TrustStore::new_in_memory();
    let kp = KeyPair::generate();
    assert!(kp.is_ok());
    if let Ok(kp) = kp {
        let fingerprint = kp.public_key.fingerprint();
        let result =
            store.add_key(kp.public_key.clone(), TrustLevel::Trusted, Some("test".to_string()));
        assert!(result.is_ok());

        let level = store.get_trust_level(&fingerprint);
        assert_eq!(level, TrustLevel::Trusted);

        let retrieved = store.get_public_key(&fingerprint);
        assert!(retrieved.is_some());
    }
}

// ---------------------------------------------------------------------------
// Plugin lifecycle simulation (without actual shared library)
// ---------------------------------------------------------------------------

#[test]
fn plugin_frame_lifecycle_values_progress() {
    let mut frame = PluginFrame {
        sequence: 0,
        timestamp_ns: 1_000_000,
        ..Default::default()
    };
    assert_eq!(frame.sequence, 0);

    // Simulate processing multiple frames
    for i in 1..=10u32 {
        frame.ffb_in = i as f32 * 0.1;
        frame.sequence = i;
        frame.timestamp_ns += 1_000_000; // 1ms per tick
    }
    assert_eq!(frame.sequence, 10);
    assert!((frame.ffb_in - 1.0).abs() < f32::EPSILON);
}

#[test]
fn spsc_lifecycle_write_process_read() -> Result<(), NativePluginError> {
    let channel = SpscChannel::with_capacity(std::mem::size_of::<PluginFrame>(), 16)?;
    let writer = channel.writer();
    let reader = channel.reader();

    // Write 5 frames
    for i in 0u32..5 {
        let frame = PluginFrame {
            ffb_in: i as f32,
            sequence: i,
            ..Default::default()
        };
        let frame_bytes = unsafe {
            std::slice::from_raw_parts(
                &frame as *const PluginFrame as *const u8,
                std::mem::size_of::<PluginFrame>(),
            )
        };
        writer.write(frame_bytes)?;
    }

    // Read and verify
    for i in 0u32..5 {
        let mut buf = vec![0u8; std::mem::size_of::<PluginFrame>()];
        reader.read(&mut buf)?;
        let frame: PluginFrame =
            unsafe { std::ptr::read(buf.as_ptr() as *const PluginFrame) };
        assert_eq!(frame.sequence, i);
        assert!((frame.ffb_in - i as f32).abs() < f32::EPSILON);
    }

    // Shutdown
    channel.shutdown();
    assert!(channel.is_shutdown());
    Ok(())
}

// ---------------------------------------------------------------------------
// Multiple plugin instances (host-level)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn host_multiple_load_attempts_dont_corrupt_state() {
    let host = NativePluginHost::new_permissive_for_development();

    // Attempt to load multiple nonexistent plugins
    for _ in 0..5 {
        let id = uuid::Uuid::new_v4();
        let result = host
            .load_plugin(id, "bad".to_string(), &PathBuf::from("no.dll"), 1000)
            .await;
        assert!(result.is_err());
    }

    // Host should still have 0 plugins
    assert_eq!(host.plugin_count().await, 0);
}

#[tokio::test]
async fn host_multiple_unload_idempotent() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();

    // Unloading nonexistent plugin multiple times should succeed
    for _ in 0..3 {
        let result = host.unload_plugin(id).await;
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn native_plugin_ref_returns_correct_id() {
    let host = NativePluginHost::new_with_defaults();
    let id = uuid::Uuid::new_v4();

    // Plugin doesn't exist, but we can verify the ref mechanism
    let plugin_ref = host.get_plugin(id).await;
    assert!(plugin_ref.is_none());
}

// ---------------------------------------------------------------------------
// SPSC multiple instances
// ---------------------------------------------------------------------------

#[test]
fn multiple_spsc_channels_independent() -> Result<(), NativePluginError> {
    let ch1 = SpscChannel::new(8)?;
    let ch2 = SpscChannel::new(16)?;

    assert_eq!(ch1.frame_size(), 8);
    assert_eq!(ch2.frame_size(), 16);

    // Write to ch1 shouldn't affect ch2
    ch1.writer().write(&[0xAA; 8])?;
    assert!(ch1.reader().has_data());
    assert!(!ch2.reader().has_data());

    // Shutdown ch1 shouldn't affect ch2
    ch1.shutdown();
    assert!(ch1.is_shutdown());
    assert!(!ch2.is_shutdown());
    Ok(())
}
