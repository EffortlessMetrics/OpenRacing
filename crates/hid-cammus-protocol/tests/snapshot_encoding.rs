//! Extended snapshot tests for Cammus wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering clamping behaviour,
//! model lookup via PID, and boundary-value parse edge cases that would
//! detect wire-format regressions.

use insta::assert_debug_snapshot;
use racing_wheel_hid_cammus_protocol as cammus;

// ── Encoder clamping / out-of-range ──────────────────────────────────────────

#[test]
fn test_snapshot_encode_torque_above_one_clamps() {
    let report = cammus::encode_torque(2.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_below_neg_one_clamps() {
    let report = cammus::encode_torque(-2.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_half_positive() {
    let report = cammus::encode_torque(0.5);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_half_negative() {
    let report = cammus::encode_torque(-0.5);
    assert_debug_snapshot!(report);
}

// ── Model lookup from PID ────────────────────────────────────────────────────

#[test]
fn test_snapshot_model_from_pid_c5() {
    let model = cammus::CammusModel::from_pid(cammus::PRODUCT_C5);
    assert_debug_snapshot!(format!("{model:?}"));
}

#[test]
fn test_snapshot_model_from_pid_c12() {
    let model = cammus::CammusModel::from_pid(cammus::PRODUCT_C12);
    assert_debug_snapshot!(format!("{model:?}"));
}

#[test]
fn test_snapshot_model_from_pid_cp5_pedals() {
    let model = cammus::CammusModel::from_pid(cammus::PRODUCT_CP5_PEDALS);
    assert_debug_snapshot!(format!("{model:?}"));
}

#[test]
fn test_snapshot_model_from_pid_unknown() {
    let model = cammus::CammusModel::from_pid(0xFFFF);
    assert_debug_snapshot!(format!("{model:?}"));
}

// ── Parse edge cases ─────────────────────────────────────────────────────────

#[test]
fn test_snapshot_parse_all_ff() -> Result<(), String> {
    let data = [0xFFu8; 64];
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, handbrake={:.4}, buttons=0x{:04X}",
        report.steering, report.throttle, report.brake,
        report.clutch, report.handbrake, report.buttons,
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_min_length_boundary() {
    // Just under minimum length should fail
    let err = cammus::parse(&[0u8; 7]).expect_err("should fail for 7-byte slice");
    assert_debug_snapshot!(format!("{err}"));
}
