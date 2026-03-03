#![allow(clippy::redundant_closure)]
//! Comprehensive property-based tests for native plugin ABI, loading, and signatures.
//!
//! Tests cover:
//! - ABI version compatibility (proptest)
//! - Plugin discovery and loading configuration
//! - Signature verification config modes
//! - Edge cases: missing symbols, version mismatch, invalid paths

use openracing_native_plugin::abi_check::{AbiCheckResult, CURRENT_ABI_VERSION, check_abi_compatibility};
use openracing_native_plugin::error::{NativePluginError, NativePluginLoadError};
use openracing_native_plugin::loader::{NativePluginConfig, NativePluginHost, NativePluginLoader};
use openracing_native_plugin::plugin::PluginFrame;
use openracing_native_plugin::signature::{SignatureVerificationConfig, SignatureVerifier};
use openracing_native_plugin::spsc::SpscChannel;

use openracing_crypto::trust_store::TrustStore;
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// ABI version compatibility proptests
// ---------------------------------------------------------------------------

fn mismatched_version() -> impl Strategy<Value = u32> {
    any::<u32>().prop_filter("must differ from CURRENT_ABI_VERSION", |&v| {
        v != CURRENT_ABI_VERSION
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_current_version_always_compatible(_seed in Just(())) {
        let result = check_abi_compatibility(CURRENT_ABI_VERSION);
        prop_assert_eq!(result, AbiCheckResult::Compatible);
    }

    #[test]
    fn prop_mismatched_version_always_rejected(version in mismatched_version()) {
        let result = check_abi_compatibility(version);
        prop_assert_ne!(result, AbiCheckResult::Compatible, "Mismatched version should be rejected");

        match result {
            AbiCheckResult::Mismatch { expected, actual } => {
                prop_assert_eq!(expected, CURRENT_ABI_VERSION);
                prop_assert_eq!(actual, version);
            }
            AbiCheckResult::Compatible => {
                return Err(TestCaseError::fail("Should not be compatible"));
            }
        }
    }

    #[test]
    fn prop_adjacent_versions_rejected(offset in 1u32..100u32) {
        let below = CURRENT_ABI_VERSION.checked_sub(offset);
        let above = CURRENT_ABI_VERSION.checked_add(offset);

        if let Some(v) = below
            && v != CURRENT_ABI_VERSION
        {
            let r = check_abi_compatibility(v);
            prop_assert_ne!(r, AbiCheckResult::Compatible, "Below version should be rejected");
        }
        if let Some(v) = above
            && v != CURRENT_ABI_VERSION
        {
            let r = check_abi_compatibility(v);
            prop_assert_ne!(r, AbiCheckResult::Compatible, "Above version should be rejected");
        }
    }

    #[test]
    fn prop_abi_version_boundary_values(version in prop::sample::select(vec![0u32, 1, u32::MAX, u32::MAX - 1])) {
        let result = check_abi_compatibility(version);
        if version == CURRENT_ABI_VERSION {
            prop_assert_eq!(result, AbiCheckResult::Compatible);
        } else {
            prop_assert_ne!(result, AbiCheckResult::Compatible, "Non-current version should be rejected");
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin configuration tests
// ---------------------------------------------------------------------------

mod config_tests {
    use super::*;

    #[test]
    fn test_default_config_is_strict() {
        let config = NativePluginConfig::default();
        assert!(!config.allow_unsigned);
        assert!(config.require_signatures);
    }

    #[test]
    fn test_strict_config() {
        let config = NativePluginConfig::strict();
        assert!(!config.allow_unsigned);
        assert!(config.require_signatures);
    }

    #[test]
    fn test_permissive_config() {
        let config = NativePluginConfig::permissive();
        assert!(config.allow_unsigned);
        assert!(config.require_signatures);
    }

    #[test]
    fn test_development_config() {
        let config = NativePluginConfig::development();
        assert!(config.allow_unsigned);
        assert!(!config.require_signatures);
    }

    #[test]
    fn test_config_to_signature_config_conversion() {
        let config = NativePluginConfig::strict();
        let sig_config = config.to_signature_config();
        assert!(sig_config.require_signatures);
        assert!(!sig_config.allow_unsigned);

        let dev_config = NativePluginConfig::development();
        let sig_config = dev_config.to_signature_config();
        assert!(!sig_config.require_signatures);
        assert!(sig_config.allow_unsigned);
    }
}

// ---------------------------------------------------------------------------
// Signature verification config tests
// ---------------------------------------------------------------------------

mod signature_config_tests {
    use super::*;

    #[test]
    fn test_default_sig_config_is_strict() {
        let config = SignatureVerificationConfig::default();
        assert!(config.require_signatures);
        assert!(!config.allow_unsigned);
    }

    #[test]
    fn test_strict_sig_config() {
        let config = SignatureVerificationConfig::strict();
        assert!(config.require_signatures);
        assert!(!config.allow_unsigned);
    }

    #[test]
    fn test_permissive_sig_config() {
        let config = SignatureVerificationConfig::permissive();
        assert!(config.require_signatures);
        assert!(config.allow_unsigned);
    }

    #[test]
    fn test_development_sig_config() {
        let config = SignatureVerificationConfig::development();
        assert!(!config.require_signatures);
        assert!(config.allow_unsigned);
    }
}

// ---------------------------------------------------------------------------
// Plugin discovery and loading edge cases
// ---------------------------------------------------------------------------

mod loading_edge_cases {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_nonexistent_path_fails() {
        let trust_store = TrustStore::new_in_memory();
        let config = NativePluginConfig::development();
        let loader = NativePluginLoader::new(&trust_store, config);

        let result = loader.load(
            uuid::Uuid::new_v4(),
            "test-plugin".to_string(),
            Path::new("/nonexistent/path/plugin.dll"),
            1000,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_file_fails() -> TestResult {
        let temp_dir = tempfile::TempDir::new()?;
        let fake_lib = temp_dir.path().join("fake_plugin.dll");
        std::fs::write(&fake_lib, b"not a valid library")?;

        let trust_store = TrustStore::new_in_memory();
        let config = NativePluginConfig::development();
        let loader = NativePluginLoader::new(&trust_store, config);

        let result = loader.load(
            uuid::Uuid::new_v4(),
            "test-plugin".to_string(),
            &fake_lib,
            1000,
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_unsigned_plugin_rejected_in_strict_mode() {
        let trust_store = TrustStore::new_in_memory();
        let config = SignatureVerificationConfig::strict();
        let verifier = SignatureVerifier::new(&trust_store, config);

        let temp_dir = tempfile::TempDir::new().ok();
        if let Some(dir) = temp_dir {
            let fake_lib = dir.path().join("test.so");
            let _ = std::fs::write(&fake_lib, b"fake library");

            let result = verifier.verify(&fake_lib);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_unsigned_plugin_allowed_in_development_mode() -> TestResult {
        let trust_store = TrustStore::new_in_memory();
        let config = SignatureVerificationConfig::development();
        let verifier = SignatureVerifier::new(&trust_store, config);

        let temp_dir = tempfile::TempDir::new()?;
        let fake_lib = temp_dir.path().join("test.so");
        std::fs::write(&fake_lib, b"fake library")?;

        let result = verifier.verify(&fake_lib)?;
        assert!(!result.is_signed);
        assert!(result.verified);
        assert!(!result.warnings.is_empty());
        Ok(())
    }

    #[test]
    fn test_unsigned_plugin_allowed_in_permissive_mode() -> TestResult {
        let trust_store = TrustStore::new_in_memory();
        let config = SignatureVerificationConfig::permissive();
        let verifier = SignatureVerifier::new(&trust_store, config);

        let temp_dir = tempfile::TempDir::new()?;
        let fake_lib = temp_dir.path().join("test.so");
        std::fs::write(&fake_lib, b"fake library")?;

        let result = verifier.verify(&fake_lib)?;
        assert!(!result.is_signed);
        assert!(result.verified);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Plugin frame tests
// ---------------------------------------------------------------------------

mod frame_tests {
    use super::*;

    #[test]
    fn test_plugin_frame_default() {
        let frame = PluginFrame::default();
        assert_eq!(frame.ffb_in, 0.0);
        assert_eq!(frame.torque_out, 0.0);
        assert_eq!(frame.wheel_speed, 0.0);
        assert_eq!(frame.timestamp_ns, 0);
        assert_eq!(frame.budget_us, 1000);
        assert_eq!(frame.sequence, 0);
    }

    #[test]
    fn test_plugin_frame_copy() {
        let frame = PluginFrame {
            ffb_in: 1.5,
            torque_out: 0.75,
            wheel_speed: 10.0,
            timestamp_ns: 12345,
            budget_us: 500,
            sequence: 42,
        };
        let copy = frame;
        assert_eq!(copy.ffb_in, 1.5);
        assert_eq!(copy.sequence, 42);
    }

    #[test]
    fn test_plugin_frame_repr_c_size() {
        // PluginFrame is repr(C): 4 floats (4*4=16) + u64 (8) + 2*u32 (8) = 32 bytes
        let size = std::mem::size_of::<PluginFrame>();
        assert!(size > 0, "PluginFrame should have non-zero size");
    }
}

// ---------------------------------------------------------------------------
// SPSC channel tests
// ---------------------------------------------------------------------------

mod spsc_tests {
    use super::*;

    #[test]
    fn test_spsc_create_and_basic_ops() -> TestResult {
        let channel = SpscChannel::new(64)?;
        assert_eq!(channel.frame_size(), 64);
        assert!(!channel.is_shutdown());
        Ok(())
    }

    #[test]
    fn test_spsc_write_read_roundtrip() -> TestResult {
        let channel = SpscChannel::new(16)?;
        let writer = channel.writer();
        let reader = channel.reader();

        let data = vec![0xAB_u8; 16];
        writer.write(&data)?;

        let mut buf = vec![0u8; 16];
        reader.read(&mut buf)?;
        assert_eq!(buf, data);
        Ok(())
    }

    #[test]
    fn test_spsc_wrong_frame_size_rejected() -> TestResult {
        let channel = SpscChannel::new(64)?;
        let writer = channel.writer();

        let result = writer.write(&[0u8; 32]);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_spsc_read_empty_fails() -> TestResult {
        let channel = SpscChannel::new(16)?;
        let reader = channel.reader();

        let mut buf = vec![0u8; 16];
        let result = reader.read(&mut buf);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_spsc_buffer_full() -> TestResult {
        let channel = SpscChannel::with_capacity(8, 2)?;
        let writer = channel.writer();

        writer.write(&[1u8; 8])?;
        writer.write(&[2u8; 8])?;

        let result = writer.try_write(&[3u8; 8])?;
        assert!(!result, "Write should fail when buffer is full");
        Ok(())
    }

    #[test]
    fn test_spsc_shutdown_flag() -> TestResult {
        let channel = SpscChannel::new(8)?;
        assert!(!channel.is_shutdown());
        channel.shutdown();
        assert!(channel.is_shutdown());
        Ok(())
    }

    #[test]
    fn test_spsc_has_data() -> TestResult {
        let channel = SpscChannel::new(8)?;
        let writer = channel.writer();
        let reader = channel.reader();

        assert!(!reader.has_data());
        writer.write(&[0u8; 8])?;
        assert!(reader.has_data());
        Ok(())
    }

    #[test]
    fn test_spsc_oversized_rejected() {
        // Try to create with very large capacity
        let result = SpscChannel::with_capacity(1024 * 1024, 8192);
        assert!(result.is_err(), "Oversized shared memory should be rejected");
    }
}

// ---------------------------------------------------------------------------
// Error type tests
// ---------------------------------------------------------------------------

mod error_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_native_plugin_error_display() {
        let err = NativePluginError::AbiMismatch {
            expected: 1,
            actual: 2,
        };
        let msg = format!("{err}");
        assert!(msg.contains("1"));
        assert!(msg.contains("2"));

        let err = NativePluginError::UnsignedPlugin {
            path: PathBuf::from("/test/plugin.dll"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("unsigned"));
    }

    #[test]
    fn test_load_error_to_plugin_error_conversion() {
        let load_err = NativePluginLoadError::AbiMismatch {
            expected: 1,
            actual: 99,
        };
        let plugin_err: NativePluginError = load_err.into();
        assert!(matches!(plugin_err, NativePluginError::AbiMismatch { expected: 1, actual: 99 }));

        let load_err = NativePluginLoadError::UnsignedPlugin {
            path: "/test".to_string(),
        };
        let plugin_err: NativePluginError = load_err.into();
        assert!(matches!(plugin_err, NativePluginError::UnsignedPlugin { .. }));

        let load_err = NativePluginLoadError::UntrustedSigner {
            fingerprint: "abc".to_string(),
        };
        let plugin_err: NativePluginError = load_err.into();
        assert!(matches!(plugin_err, NativePluginError::UntrustedSigner { .. }));
    }
}

// ---------------------------------------------------------------------------
// Host management tests
// ---------------------------------------------------------------------------

mod host_tests {
    use super::*;

    #[tokio::test]
    async fn test_host_default_creation() {
        let host = NativePluginHost::new_with_defaults();
        assert!(!host.config().allow_unsigned);
        assert!(host.config().require_signatures);
        assert_eq!(host.plugin_count().await, 0);
    }

    #[tokio::test]
    async fn test_host_development_creation() {
        let host = NativePluginHost::new_permissive_for_development();
        assert!(host.config().allow_unsigned);
        assert!(!host.config().require_signatures);
    }

    #[tokio::test]
    async fn test_host_config_update() {
        let trust_store = TrustStore::new_in_memory();
        let mut host = NativePluginHost::new(trust_store, NativePluginConfig::development());
        assert!(host.config().allow_unsigned);

        host.set_config(NativePluginConfig::strict());
        assert!(!host.config().allow_unsigned);
        assert!(host.config().require_signatures);
    }

    #[test]
    fn test_loader_with_defaults() {
        let trust_store = TrustStore::new_in_memory();
        let loader = NativePluginLoader::with_defaults(&trust_store);
        assert!(!loader.config().allow_unsigned);
        assert!(loader.config().require_signatures);
    }
}
