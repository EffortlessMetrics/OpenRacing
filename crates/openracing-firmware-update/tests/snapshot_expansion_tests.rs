//! Snapshot tests for FirmwareUpdateError, BundleError, and HardwareVersionError —
//! ensure error messages are stable.

use openracing_firmware_update::bundle::BundleError;
use openracing_firmware_update::error::FirmwareUpdateError;
use openracing_firmware_update::hardware_version::HardwareVersionError;

// --- FirmwareUpdateError Display (constructible variants, skip IoError) ---

#[test]
fn snapshot_firmware_error_device_not_found() {
    let err = FirmwareUpdateError::DeviceNotFound("moza-r9-001".to_string());
    insta::assert_snapshot!("firmware_error_device_not_found", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_verification_failed() {
    let err = FirmwareUpdateError::VerificationFailed("checksum mismatch".to_string());
    insta::assert_snapshot!("firmware_error_verification_failed", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_transfer_failed() {
    let err = FirmwareUpdateError::TransferFailed("USB write timeout".to_string());
    insta::assert_snapshot!("firmware_error_transfer_failed", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_health_check_failed() {
    let err = FirmwareUpdateError::HealthCheckFailed("device not responding".to_string());
    insta::assert_snapshot!("firmware_error_health_check", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_rollback_failed() {
    let err = FirmwareUpdateError::RollbackFailed("backup partition corrupt".to_string());
    insta::assert_snapshot!("firmware_error_rollback_failed", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_invalid_firmware() {
    let err = FirmwareUpdateError::InvalidFirmware("missing header".to_string());
    insta::assert_snapshot!("firmware_error_invalid_firmware", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_ffb_blocked() {
    insta::assert_snapshot!(
        "firmware_error_ffb_blocked",
        format!("{}", FirmwareUpdateError::FfbBlocked)
    );
}

#[test]
fn snapshot_firmware_error_update_in_progress() {
    let err = FirmwareUpdateError::UpdateInProgress("moza-r9".to_string());
    insta::assert_snapshot!("firmware_error_update_in_progress", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_compatibility() {
    let err =
        FirmwareUpdateError::CompatibilityError("firmware requires hardware v2.0+".to_string());
    insta::assert_snapshot!("firmware_error_compatibility", format!("{}", err));
}

#[test]
fn snapshot_firmware_error_cancelled() {
    let err = FirmwareUpdateError::Cancelled("user requested abort".to_string());
    insta::assert_snapshot!("firmware_error_cancelled", format!("{}", err));
}

// --- BundleError Display ---

#[test]
fn snapshot_bundle_error_signature_required() {
    insta::assert_snapshot!(
        "bundle_error_signature_required",
        format!("{}", BundleError::SignatureRequired)
    );
}

#[test]
fn snapshot_bundle_error_verification_failed() {
    let err = BundleError::SignatureVerificationFailed("Ed25519 verify failed".to_string());
    insta::assert_snapshot!("bundle_error_sig_verification", format!("{}", err));
}

#[test]
fn snapshot_bundle_error_untrusted_signer() {
    let err = BundleError::UntrustedSigner("SHA256:unknown-key".to_string());
    insta::assert_snapshot!("bundle_error_untrusted_signer", format!("{}", err));
}

#[test]
fn snapshot_bundle_error_hash_mismatch() {
    let err = BundleError::PayloadHashMismatch {
        expected: "abc123".to_string(),
        actual: "def456".to_string(),
    };
    insta::assert_snapshot!("bundle_error_hash_mismatch", format!("{}", err));
}

#[test]
fn snapshot_bundle_error_invalid_format() {
    let err = BundleError::InvalidFormat("missing OWFB magic bytes".to_string());
    insta::assert_snapshot!("bundle_error_invalid_format", format!("{}", err));
}

// --- HardwareVersionError Display ---

#[test]
fn snapshot_hw_version_error_empty() {
    insta::assert_snapshot!(
        "hw_version_error_empty",
        format!("{}", HardwareVersionError::Empty)
    );
}

#[test]
fn snapshot_hw_version_error_invalid_component() {
    let err = HardwareVersionError::InvalidComponent("abc".to_string(), "not a number".to_string());
    insta::assert_snapshot!("hw_version_error_invalid_component", format!("{}", err));
}

#[test]
fn snapshot_hw_version_error_invalid_character() {
    insta::assert_snapshot!(
        "hw_version_error_invalid_char",
        format!("{}", HardwareVersionError::InvalidCharacter)
    );
}
