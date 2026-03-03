//! Extended snapshot tests for VRS DirectForce Pro wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering boundary-value
//! encodings, zero-force helpers, and additional effect parameter combinations
//! that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_vrs_protocol::{
    self as vrs, CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN,
    SPRING_REPORT_LEN, VrsConstantForceEncoder, VrsDamperEncoder, VrsFrictionEncoder,
    VrsSpringEncoder,
};

// ── Constant-force encoder boundary values ───────────────────────────────────

#[test]
fn test_snapshot_encode_zero_torque() {
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(0.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_quarter_positive_torque() {
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(5.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_quarter_negative_torque() {
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-5.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_above_max() {
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(100.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_below_neg_max() {
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-100.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_zero_helper() {
    let encoder = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Spring encoder boundary values ───────────────────────────────────────────

#[test]
fn test_snapshot_spring_zero() {
    let encoder = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_spring_max_coefficient() {
    let encoder = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    encoder.encode(10000, 0, 0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_spring_negative_steering() {
    let encoder = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    encoder.encode(5000, -16000, 100, 250, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Damper encoder boundary values ───────────────────────────────────────────

#[test]
fn test_snapshot_damper_zero() {
    let encoder = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_damper_max_coefficient() {
    let encoder = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    encoder.encode(10000, 10000, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Friction encoder boundary values ─────────────────────────────────────────

#[test]
fn test_snapshot_friction_zero() {
    let encoder = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_friction_max_coefficient() {
    let encoder = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    encoder.encode(10000, 10000, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── FFB enable/disable ───────────────────────────────────────────────────────

#[test]
fn test_snapshot_ffb_enable_report() {
    let report = vrs::build_ffb_enable(true);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_ffb_disable_report() {
    let report = vrs::build_ffb_enable(false);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Rotation range boundary ──────────────────────────────────────────────────

#[test]
fn test_snapshot_rotation_range_180() {
    let report = vrs::build_rotation_range(180);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_rotation_range_2520() {
    let report = vrs::build_rotation_range(2520);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Device gain boundary ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_device_gain_one() {
    let report = vrs::build_device_gain(0x01);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_device_gain_max() {
    let report = vrs::build_device_gain(0xFF);
    assert_snapshot!(format!("{report:02X?}"));
}
