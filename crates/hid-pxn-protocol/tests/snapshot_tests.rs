//! Snapshot tests for the PXN HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use insta::assert_yaml_snapshot;
use racing_wheel_hid_pxn_protocol as pxn;

#[test]
fn test_snapshot_encode_torque_zero() {
    let report = pxn::encode_torque(0.0);
    assert_yaml_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_full_positive() {
    let report = pxn::encode_torque(1.0);
    assert_yaml_snapshot!(report);
}

#[test]
fn test_snapshot_encode_stop() {
    let report = pxn::encode_stop();
    assert_yaml_snapshot!(report);
}

#[test]
fn test_snapshot_parse_center() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_full_lock_left() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    let val: i16 = -i16::MAX;
    let bytes = val.to_le_bytes();
    data[1] = bytes[0];
    data[2] = bytes[1];
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_full_lock_right() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    let bytes = i16::MAX.to_le_bytes();
    data[1] = bytes[0];
    data[2] = bytes[1];
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_half_throttle() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    let val: u16 = u16::MAX / 2;
    let bytes = val.to_le_bytes();
    data[3] = bytes[0];
    data[4] = bytes[1];
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_combined_input() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    // Steering ~half right
    let steer: i16 = i16::MAX / 2;
    let steer_bytes = steer.to_le_bytes();
    data[1] = steer_bytes[0];
    data[2] = steer_bytes[1];
    // Full throttle
    data[3] = 0xFF;
    data[4] = 0xFF;
    // Half brake
    let brake: u16 = u16::MAX / 2;
    let brake_bytes = brake.to_le_bytes();
    data[5] = brake_bytes[0];
    data[6] = brake_bytes[1];
    // Full clutch
    data[9] = 0xFF;
    data[10] = 0xFF;
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_multiple_buttons() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    // Buttons 0, 2, 4 pressed (bits 0, 2, 4 set → 0x0015)
    data[7] = 0x15;
    data[8] = 0x00;
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_all_buttons() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[0] = pxn::REPORT_ID;
    // All 16 buttons pressed
    data[7] = 0xFF;
    data[8] = 0xFF;
    let report = pxn::parse(&data).map_err(|e| e.to_string())?;
    assert_yaml_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, buttons={}",
        report.steering, report.throttle, report.brake, report.clutch, report.buttons
    ));
    Ok(())
}
