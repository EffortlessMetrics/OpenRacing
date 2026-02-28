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
