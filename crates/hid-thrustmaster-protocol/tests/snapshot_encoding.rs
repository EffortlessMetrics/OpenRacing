//! Extended snapshot tests for Thrustmaster wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering kernel command
//! builders, encoder boundary values, pedal parsing, and device identity
//! that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_thrustmaster_protocol as tm;

// ── Constant-force encoder boundary values ───────────────────────────────────

#[test]
fn test_snapshot_encode_zero_torque() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; 8];
    encoder.encode(0.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_full_positive() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; 8];
    encoder.encode(6.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_full_negative() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; 8];
    encoder.encode(-6.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_clamp_above_max() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; 8];
    encoder.encode(20.0, &mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

#[test]
fn test_snapshot_encode_zero_report() {
    let encoder = tm::ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; 8];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{out:02X?}"));
}

// ── Kernel command builders ──────────────────────────────────────────────────

#[test]
fn test_snapshot_kernel_range_540() {
    let cmd = tm::build_kernel_range_command(540);
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_range_1080() {
    let cmd = tm::build_kernel_range_command(1080);
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_range_clamp_min() {
    let cmd = tm::build_kernel_range_command(10);
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_gain_full() {
    let cmd = tm::build_kernel_gain_command(0xFFFF);
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_gain_zero() {
    let cmd = tm::build_kernel_gain_command(0);
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_open_command() {
    let cmd = tm::build_kernel_open_command();
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_close_command() {
    let cmd = tm::build_kernel_close_command();
    assert_snapshot!(format!("{cmd:02X?}"));
}

#[test]
fn test_snapshot_kernel_autocenter_mid() {
    let cmds = tm::build_kernel_autocenter_commands(0x4000);
    assert_snapshot!(format!("{cmds:02X?}"));
}

#[test]
fn test_snapshot_kernel_autocenter_off() {
    let cmds = tm::build_kernel_autocenter_commands(0);
    assert_snapshot!(format!("{cmds:02X?}"));
}

// ── Pedal report parsing ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_parse_pedal_report_all_zero() -> Result<(), String> {
    let data = [0u8; 3];
    let raw = tm::input::parse_pedal_report(&data).ok_or("parse_pedal_report returned None")?;
    let axes = raw.normalize();
    assert_snapshot!(format!(
        "throttle={:.4}, brake={:.4}, clutch={:?}",
        axes.throttle, axes.brake, axes.clutch,
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_pedal_report_full_throttle() -> Result<(), String> {
    let data = [0xFF, 0x00, 0x00];
    let raw = tm::input::parse_pedal_report(&data).ok_or("parse_pedal_report returned None")?;
    let axes = raw.normalize();
    assert_snapshot!(format!(
        "throttle={:.4}, brake={:.4}, clutch={:?}",
        axes.throttle, axes.brake, axes.clutch,
    ));
    Ok(())
}

// ── Device gain boundary ─────────────────────────────────────────────────────

#[test]
fn test_snapshot_device_gain_zero() {
    let report = tm::build_device_gain(0x00);
    assert_snapshot!(format!("{report:02X?}"));
}

#[test]
fn test_snapshot_device_gain_full() {
    let report = tm::build_device_gain(0xFF);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Actuator disable ─────────────────────────────────────────────────────────

#[test]
fn test_snapshot_actuator_disable() {
    let report = tm::build_actuator_enable(false);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── Spring/damper/friction boundary values ───────────────────────────────────

#[test]
fn test_snapshot_spring_effect_zero() {
    let effect = tm::build_spring_effect(0, 0);
    assert_snapshot!(format!("{effect:02X?}"));
}

#[test]
fn test_snapshot_spring_effect_max() {
    let effect = tm::build_spring_effect(i16::MAX, u16::MAX);
    assert_snapshot!(format!("{effect:02X?}"));
}

#[test]
fn test_snapshot_damper_effect_zero() {
    let effect = tm::build_damper_effect(0);
    assert_snapshot!(format!("{effect:02X?}"));
}

#[test]
fn test_snapshot_damper_effect_max() {
    let effect = tm::build_damper_effect(u16::MAX);
    assert_snapshot!(format!("{effect:02X?}"));
}

#[test]
fn test_snapshot_friction_effect_zero() {
    let effect = tm::build_friction_effect(0, 0);
    assert_snapshot!(format!("{effect:02X?}"));
}

#[test]
fn test_snapshot_friction_effect_max() {
    let effect = tm::build_friction_effect(u16::MAX, u16::MAX);
    assert_snapshot!(format!("{effect:02X?}"));
}

// ── Set range boundary ───────────────────────────────────────────────────────

#[test]
fn test_snapshot_set_range_270() {
    let report = tm::build_set_range_report(270);
    assert_snapshot!(format!("{report:02X?}"));
}

// ── is_pedal_product ─────────────────────────────────────────────────────────

#[test]
fn test_snapshot_is_pedal_product_unknown() {
    assert_snapshot!(format!(
        "unknown_pid={}",
        tm::is_pedal_product(0xFFFF)
    ));
}

// ── Protocol T_LCM pedal identity ────────────────────────────────────────────

#[test]
fn test_snapshot_protocol_t248() {
    let proto = tm::ThrustmasterProtocol::new(tm::product_ids::T248);
    assert_snapshot!(format!(
        "model={:?}, max_torque={}, range={}, ffb={}",
        proto.model(),
        proto.max_torque_nm(),
        proto.rotation_range(),
        proto.supports_ffb()
    ));
}
