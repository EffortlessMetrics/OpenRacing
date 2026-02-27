//! Snapshot tests for the Cammus HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use insta::assert_debug_snapshot;
use racing_wheel_hid_cammus_protocol as cammus;

#[test]
fn test_snapshot_encode_torque_zero() {
    let report = cammus::encode_torque(0.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_full_positive() {
    let report = cammus::encode_torque(1.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_full_negative() {
    let report = cammus::encode_torque(-1.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_quarter() {
    let report = cammus::encode_torque(0.25);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_stop() {
    let report = cammus::encode_stop();
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_parse_center() -> Result<(), String> {
    let data = [0u8; 64];
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}, clutch={:.4}, handbrake={:.4}, buttons={}",
        report.steering,
        report.throttle,
        report.brake,
        report.clutch,
        report.handbrake,
        report.buttons
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_full_throttle() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[2] = 0xFF;
    data[3] = 0xFF;
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!(
        "steering={:.4}, throttle={:.4}, brake={:.4}",
        report.steering, report.throttle, report.brake
    ));
    Ok(())
}

#[test]
fn test_snapshot_parse_full_right_steering() -> Result<(), String> {
    let mut data = [0u8; 64];
    let bytes = i16::MAX.to_le_bytes();
    data[0] = bytes[0];
    data[1] = bytes[1];
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!("steering={:.4}", report.steering));
    Ok(())
}

#[test]
fn test_snapshot_parse_buttons() -> Result<(), String> {
    let mut data = [0u8; 64];
    data[6] = 0xAB;
    data[7] = 0xCD;
    let report = cammus::parse(&data).map_err(|e| e.to_string())?;
    assert_debug_snapshot!(format!("buttons={:#06X}", report.buttons));
    Ok(())
}

#[test]
fn test_snapshot_product_names() {
    let results = [
        ("C5", cammus::product_name(cammus::PRODUCT_C5)),
        ("C12", cammus::product_name(cammus::PRODUCT_C12)),
        ("unknown", cammus::product_name(0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_model_c5() {
    let model = cammus::CammusModel::C5;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_model_c12() {
    let model = cammus::CammusModel::C12;
    assert_debug_snapshot!(format!(
        "name={}, max_torque={:.1}",
        model.name(),
        model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_is_cammus() {
    let results = [
        (
            "c5_correct",
            cammus::is_cammus(cammus::VENDOR_ID, cammus::PRODUCT_C5),
        ),
        (
            "c12_correct",
            cammus::is_cammus(cammus::VENDOR_ID, cammus::PRODUCT_C12),
        ),
        ("wrong_vid", cammus::is_cammus(0x0000, cammus::PRODUCT_C5)),
        ("wrong_pid", cammus::is_cammus(cammus::VENDOR_ID, 0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_protocol_constants() {
    assert_debug_snapshot!(format!(
        "VID={:#06X}, C5_PID={:#06X}, C12_PID={:#06X}, \
         REPORT_LEN={}, FFB_REPORT_LEN={}, STEERING_RANGE={:.1}deg",
        cammus::VENDOR_ID,
        cammus::PRODUCT_C5,
        cammus::PRODUCT_C12,
        cammus::REPORT_LEN,
        cammus::FFB_REPORT_LEN,
        cammus::STEERING_RANGE_DEG,
    ));
}

#[test]
fn test_snapshot_parse_error_too_short() {
    let err = cammus::parse(&[0u8; 3]).expect_err("should fail for 3-byte slice");
    assert_debug_snapshot!(format!("{err}"));
}
