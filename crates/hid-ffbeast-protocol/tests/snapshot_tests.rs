//! Snapshot tests for the FFBeast HID protocol.
//!
//! These tests lock in the wire format to catch accidental protocol regressions.

use insta::assert_debug_snapshot;
use racing_wheel_hid_ffbeast_protocol as ffbeast;

#[test]
fn test_snapshot_encode_torque_zero() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    let report = enc.encode(0.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_full_positive() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    let report = enc.encode(1.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_full_negative() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    let report = enc.encode(-1.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_quarter() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    let report = enc.encode(0.25);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_clamped_over() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    let report = enc.encode(2.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_encode_torque_clamped_under() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    let report = enc.encode(-2.0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_enable_ffb_on() {
    let report = ffbeast::build_enable_ffb(true);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_enable_ffb_off() {
    let report = ffbeast::build_enable_ffb(false);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_set_gain_full() {
    let report = ffbeast::build_set_gain(255);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_set_gain_zero() {
    let report = ffbeast::build_set_gain(0);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_set_gain_half() {
    let report = ffbeast::build_set_gain(128);
    assert_debug_snapshot!(report);
}

#[test]
fn test_snapshot_is_ffbeast_product() {
    let results = [
        ("wheel", ffbeast::is_ffbeast_product(ffbeast::FFBEAST_PRODUCT_ID_WHEEL)),
        (
            "joystick",
            ffbeast::is_ffbeast_product(ffbeast::FFBEAST_PRODUCT_ID_JOYSTICK),
        ),
        ("wrong_pid_zero", ffbeast::is_ffbeast_product(0x0000)),
        ("wrong_pid_ffff", ffbeast::is_ffbeast_product(0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_is_ffbeast_product_all_devices() {
    let results = [
        ("joystick", ffbeast::is_ffbeast_product(ffbeast::FFBEAST_PRODUCT_ID_JOYSTICK)),
        ("rudder", ffbeast::is_ffbeast_product(ffbeast::FFBEAST_PRODUCT_ID_RUDDER)),
        ("wheel", ffbeast::is_ffbeast_product(ffbeast::FFBEAST_PRODUCT_ID_WHEEL)),
        ("wrong_pid_zero", ffbeast::is_ffbeast_product(0x0000)),
        ("wrong_pid_ffff", ffbeast::is_ffbeast_product(0xFFFF)),
    ];
    assert_debug_snapshot!(format!("{:?}", results));
}

#[test]
fn test_snapshot_protocol_constants() {
    assert_debug_snapshot!(format!(
        "VID={:#06X}, PID_JOYSTICK={:#06X}, PID_RUDDER={:#06X}, PID_WHEEL={:#06X}, REPORT_ID={:#04X}, REPORT_LEN={}, GAIN_ID={:#04X}",
        ffbeast::FFBEAST_VENDOR_ID,
        ffbeast::FFBEAST_PRODUCT_ID_JOYSTICK,
        ffbeast::FFBEAST_PRODUCT_ID_RUDDER,
        ffbeast::FFBEAST_PRODUCT_ID_WHEEL,
        ffbeast::CONSTANT_FORCE_REPORT_ID,
        ffbeast::CONSTANT_FORCE_REPORT_LEN,
        ffbeast::GAIN_REPORT_ID,
    ));
}