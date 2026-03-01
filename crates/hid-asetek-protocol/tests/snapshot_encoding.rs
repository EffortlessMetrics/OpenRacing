//! Extended snapshot tests for Asetek wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering half-torque values,
//! sequence numbers, wheel capabilities, input parsing with non-zero fields,
//! and WheelModel identifications that would detect wire-format regressions.

use hid_asetek_protocol as asetek;
use insta::assert_debug_snapshot;

// ── Output report boundary values ────────────────────────────────────────────

#[test]
fn test_snapshot_output_half_positive_torque() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(asetek::MAX_TORQUE_NM * 0.5);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_half_negative_torque() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(-asetek::MAX_TORQUE_NM * 0.5);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_clamp_above_max() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(100.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_clamp_below_neg_max() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(0).with_torque(-100.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

// ── Sequence number in output report ─────────────────────────────────────────

#[test]
fn test_snapshot_output_with_sequence_number() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(42).with_torque(5.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

#[test]
fn test_snapshot_output_max_sequence_number() -> Result<(), String> {
    let report = asetek::AsetekOutputReport::new(u16::MAX).with_torque(0.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_debug_snapshot!(data);
    Ok(())
}

// ── Input report parsing with non-zero fields ────────────────────────────────

#[test]
fn test_snapshot_input_report_non_zero() -> Result<(), String> {
    let mut data = [0u8; 32];
    // sequence = 100 (u16 LE)
    data[0] = 100;
    data[1] = 0;
    // wheel_angle = 45000 (i32 LE, means 45.0 degrees)
    let angle_bytes = 45000_i32.to_le_bytes();
    data[2] = angle_bytes[0];
    data[3] = angle_bytes[1];
    data[4] = angle_bytes[2];
    data[5] = angle_bytes[3];
    // wheel_speed = 500 (i16 LE)
    let speed_bytes = 500_i16.to_le_bytes();
    data[6] = speed_bytes[0];
    data[7] = speed_bytes[1];
    // torque = 1000 (i16 LE, = 10.0 Nm)
    let torque_bytes = 1000_i16.to_le_bytes();
    data[8] = torque_bytes[0];
    data[9] = torque_bytes[1];
    // temperature = 42
    data[10] = 42;
    // status = 0x03 (connected + enabled)
    data[11] = 0x03;

    let report = asetek::AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "angle={:.3}deg, speed={:.4}rad_s, torque={:.3}Nm, temp={}, connected={}, enabled={}",
        report.wheel_angle_degrees(),
        report.wheel_speed_rad_s(),
        report.applied_torque_nm(),
        report.temperature,
        report.is_connected(),
        report.is_enabled(),
    ));
    Ok(())
}

#[test]
fn test_snapshot_input_report_negative_angle() -> Result<(), String> {
    let mut data = [0u8; 32];
    let angle_bytes = (-90000_i32).to_le_bytes();
    data[2] = angle_bytes[0];
    data[3] = angle_bytes[1];
    data[4] = angle_bytes[2];
    data[5] = angle_bytes[3];
    let report = asetek::AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "angle={:.3}deg",
        report.wheel_angle_degrees(),
    ));
    Ok(())
}

// ── Wheel capabilities ───────────────────────────────────────────────────────

#[test]
fn test_snapshot_wheel_capabilities_forte() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::Forte);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, max_speed={}, quick_release={}",
        caps.max_torque_nm, caps.max_speed_rpm, caps.supports_quick_release,
    ));
}

#[test]
fn test_snapshot_wheel_capabilities_invicta() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::Invicta);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, max_speed={}, quick_release={}",
        caps.max_torque_nm, caps.max_speed_rpm, caps.supports_quick_release,
    ));
}

#[test]
fn test_snapshot_wheel_capabilities_laprima() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::LaPrima);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, max_speed={}, quick_release={}",
        caps.max_torque_nm, caps.max_speed_rpm, caps.supports_quick_release,
    ));
}

#[test]
fn test_snapshot_wheel_capabilities_unknown() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::Unknown);
    assert_debug_snapshot!(format!(
        "max_torque={:.1}, max_speed={}, quick_release={}",
        caps.max_torque_nm, caps.max_speed_rpm, caps.supports_quick_release,
    ));
}
