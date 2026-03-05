//! Example native plugin tests exercising the real native plugin infrastructure.
//!
//! Covers:
//!  1. ABI version checking
//!  2. Ed25519 signature verification flow
//!  3. Plugin frame allocation and lifecycle

use tempfile::TempDir;
use uuid::Uuid;

use openracing_crypto::TrustLevel;
use openracing_crypto::ed25519::{Ed25519Signer, KeyPair};
use openracing_crypto::trust_store::TrustStore;
use openracing_crypto::verification::ContentType;

use openracing_native_plugin::{
    AbiCheckResult, CURRENT_ABI_VERSION, NativePluginConfig, NativePluginHost, PluginFrame,
    SignatureVerificationConfig, SignatureVerifier, SpscChannel, check_abi_compatibility,
};

// ===================================================================
// 1. ABI version checking
// ===================================================================

#[test]
fn native_example_current_abi_compatible() {
    let result = check_abi_compatibility(CURRENT_ABI_VERSION);
    assert_eq!(result, AbiCheckResult::Compatible);
}

#[test]
fn native_example_wrong_abi_mismatch() {
    let wrong = CURRENT_ABI_VERSION.wrapping_add(1);
    match check_abi_compatibility(wrong) {
        AbiCheckResult::Mismatch { expected, actual } => {
            assert_eq!(expected, CURRENT_ABI_VERSION);
            assert_eq!(actual, wrong);
        }
        _ => {
            panic!("expected Mismatch for wrong ABI version");
        }
    }
}

#[test]
fn native_example_abi_version_zero_rejected() {
    if CURRENT_ABI_VERSION != 0 {
        let result = check_abi_compatibility(0);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn native_example_abi_max_version_rejected() {
    if CURRENT_ABI_VERSION != u32::MAX {
        let result = check_abi_compatibility(u32::MAX);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
    }
}

#[test]
fn native_example_adjacent_versions_rejected() {
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
// 2. Ed25519 signature verification flow
// ===================================================================

#[test]
fn native_example_strict_rejects_unsigned() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("unsigned_plugin.dll");
    std::fs::write(&plugin_path, b"fake DLL content")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path);
    assert!(result.is_err(), "strict mode must reject unsigned plugin");
    Ok(())
}

#[test]
fn native_example_permissive_allows_unsigned() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("dev_plugin.dll");
    std::fs::write(&plugin_path, b"dev plugin content")?;

    let trust_store = TrustStore::new_in_memory();
    let config = SignatureVerificationConfig::permissive();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path)?;
    assert!(!result.is_signed);
    assert!(result.verified, "permissive must allow unsigned plugins");
    assert!(
        !result.warnings.is_empty(),
        "should include unsigned warning"
    );
    Ok(())
}

#[test]
fn native_example_tampered_content_detected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_path = temp.path().join("tampered.dll");
    let original_content = b"original DLL content";
    std::fs::write(&plugin_path, original_content)?;

    // Sign the original content
    let keypair = KeyPair::generate()?;
    let metadata = Ed25519Signer::sign_with_metadata(
        original_content,
        &keypair,
        "Test Signer",
        ContentType::Plugin,
        None,
    )?;

    let sig_path = plugin_path.with_extension("dll.sig");
    let sig_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&sig_path, &sig_json)?;

    // Tamper with plugin content
    std::fs::write(&plugin_path, b"TAMPERED content")?;

    let mut trust_store = TrustStore::new_in_memory();
    trust_store.add_key(
        keypair.public_key.clone(),
        TrustLevel::Trusted,
        Some("test key".to_string()),
    )?;

    let config = SignatureVerificationConfig::strict();
    let verifier = SignatureVerifier::new(&trust_store, config);

    let result = verifier.verify(&plugin_path);
    assert!(result.is_err(), "tampered content must fail verification");
    Ok(())
}

#[test]
fn native_example_config_modes() {
    let strict = NativePluginConfig::strict();
    assert!(!strict.allow_unsigned);
    assert!(strict.require_signatures);

    let dev = NativePluginConfig::development();
    assert!(dev.allow_unsigned);
    assert!(!dev.require_signatures);

    let permissive = NativePluginConfig::permissive();
    assert!(permissive.allow_unsigned);
    assert!(permissive.require_signatures);

    let default_cfg = NativePluginConfig::default();
    assert!(!default_cfg.allow_unsigned);
    assert!(default_cfg.require_signatures);
}

#[test]
fn native_example_sig_config_to_verification_config() {
    let config = NativePluginConfig::strict();
    let sig_config = config.to_signature_config();
    assert!(sig_config.require_signatures);
    assert!(!sig_config.allow_unsigned);
}

// ===================================================================
// 3. Plugin frame allocation and lifecycle
// ===================================================================

#[test]
fn native_example_plugin_frame_defaults() {
    let frame = PluginFrame::default();

    assert!((frame.ffb_in - 0.0).abs() < f32::EPSILON);
    assert!((frame.torque_out - 0.0).abs() < f32::EPSILON);
    assert!((frame.wheel_speed - 0.0).abs() < f32::EPSILON);
    assert_eq!(frame.timestamp_ns, 0);
    assert_eq!(frame.budget_us, 1000);
    assert_eq!(frame.sequence, 0);
}

#[test]
fn native_example_plugin_frame_mutation() {
    let frame = PluginFrame {
        ffb_in: 0.75,
        torque_out: -0.5,
        wheel_speed: 12.3,
        timestamp_ns: 1_000_000,
        budget_us: 500,
        sequence: 42,
    };

    assert!((frame.ffb_in - 0.75).abs() < f32::EPSILON);
    assert!((frame.torque_out - (-0.5)).abs() < f32::EPSILON);
    assert!((frame.wheel_speed - 12.3).abs() < f32::EPSILON);
    assert_eq!(frame.timestamp_ns, 1_000_000);
    assert_eq!(frame.budget_us, 500);
    assert_eq!(frame.sequence, 42);
}

#[test]
fn native_example_plugin_frame_clone() {
    let original = PluginFrame {
        ffb_in: 1.0,
        sequence: 7,
        ..PluginFrame::default()
    };

    let cloned = original;
    assert!((cloned.ffb_in - 1.0).abs() < f32::EPSILON);
    assert_eq!(cloned.sequence, 7);
}

#[test]
fn native_example_spsc_channel_creation() -> Result<(), Box<dyn std::error::Error>> {
    let frame_size = std::mem::size_of::<PluginFrame>();
    let channel = SpscChannel::new(frame_size)?;

    // Channel should be created successfully with valid frame size
    assert!(frame_size > 0);
    drop(channel);
    Ok(())
}

#[test]
fn native_example_spsc_oversized_rejected() {
    // Requesting an impossibly large shared memory should fail
    let result = SpscChannel::with_capacity(1024 * 1024, 1024 * 1024);
    assert!(result.is_err(), "oversized SPSC channel must be rejected");
}

#[tokio::test]
async fn native_example_strict_host_rejects_unsigned() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let plugin_dir = temp.path().join("unsigned-native");
    tokio::fs::create_dir_all(&plugin_dir).await?;

    let lib_name = if cfg!(windows) {
        "plugin.dll"
    } else if cfg!(target_os = "macos") {
        "plugin.dylib"
    } else {
        "plugin.so"
    };

    tokio::fs::write(plugin_dir.join(lib_name), b"fake binary").await?;

    let host = NativePluginHost::new_with_defaults();
    let result = host
        .load_plugin(
            Uuid::new_v4(),
            "unsigned-test".to_string(),
            &plugin_dir.join(lib_name),
            1000,
        )
        .await;

    assert!(
        result.is_err(),
        "strict host must reject unsigned native plugin"
    );
    Ok(())
}
