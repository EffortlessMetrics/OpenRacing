//! Extended snapshot tests for Simagic wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering boundary-value
//! encodings, zero-force helpers, and additional effect parameter combinations
//! that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_simagic_protocol::{
    self as simagic, CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN,
    SPRING_REPORT_LEN, SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicFrictionEncoder,
    SimagicSpringEncoder,
};

// ── Constant-force encoder boundary values ───────────────────────────────────

#[test]
fn test_snapshot_encode_zero_torque() {
    let encoder = SimagicConstantForceEncoder::new(15.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(0.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_quarter_positive_torque() {
    let encoder = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(2.5, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_quarter_negative_torque() {
    let encoder = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-2.5, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_above_max() {
    let encoder = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(50.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_below_neg_max() {
    let encoder = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(-50.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_zero_helper() {
    let encoder = SimagicConstantForceEncoder::new(15.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Spring encoder boundary values ───────────────────────────────────────────

#[test]
fn test_snapshot_spring_zero() {
    let encoder = SimagicSpringEncoder::new(15.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_spring_max_strength() {
    let encoder = SimagicSpringEncoder::new(15.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    encoder.encode(1000, 0, 0, 0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_spring_negative_steering() {
    let encoder = SimagicSpringEncoder::new(15.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    encoder.encode(500, -16000, 100, 25, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Damper encoder boundary values ───────────────────────────────────────────

#[test]
fn test_snapshot_damper_zero() {
    let encoder = SimagicDamperEncoder::new(15.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_damper_max_strength() {
    let encoder = SimagicDamperEncoder::new(15.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    encoder.encode(1000, 10000, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Friction encoder boundary values ─────────────────────────────────────────

#[test]
fn test_snapshot_friction_zero() {
    let encoder = SimagicFrictionEncoder::new(15.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_friction_max_coefficient() {
    let encoder = SimagicFrictionEncoder::new(15.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    encoder.encode(1000, 10000, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── LED report boundary values ───────────────────────────────────────────────

#[test]
fn test_snapshot_led_report_all_off() {
    let report = simagic::build_led_report(0x00);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_led_report_all_on() {
    let report = simagic::build_led_report(0xFF);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Periodic effect boundary values ──────────────────────────────────────────

#[test]
fn test_snapshot_sine_max_amplitude() {
    let report = simagic::build_sine_effect(1000, 20.0, 360);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_sine_min_frequency() {
    let report = simagic::build_sine_effect(100, 0.1, 0);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_square_zero_duty() {
    let report = simagic::build_square_effect(500, 1.0, 0);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_square_full_duty() {
    let report = simagic::build_square_effect(500, 1.0, 100);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_triangle_min_amplitude() {
    let report = simagic::build_triangle_effect(0, 0.1);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Rotation range boundary ──────────────────────────────────────────────────

#[test]
fn test_snapshot_rotation_range_270() {
    let report = simagic::build_rotation_range(270);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_rotation_range_2520() {
    let report = simagic::build_rotation_range(2520);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Device gain boundary ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_device_gain_one() {
    let report = simagic::build_device_gain(0x01);
    assert_snapshot!(format!("{report:02X?}"));
}
