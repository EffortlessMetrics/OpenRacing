//! Insta snapshot tests for the OpenFFBoard HID protocol encoding.
//!
//! These tests pin the exact wire-format bytes for all key protocol inputs
//! and lock in the protocol constants to prevent accidental regressions.

use insta::assert_snapshot;
use racing_wheel_hid_openffboard_protocol::OpenFFBoardTorqueEncoder;

#[test]
fn test_snapshot_encode_neg_one() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-1.0);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_zero() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.0);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_pos_one() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(1.0);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_half_positive() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.5);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_half_negative() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-0.5);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_torque_quarter() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(0.25);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_torque_clamped_over() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(2.0);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_encode_torque_clamped_under() {
    let enc = OpenFFBoardTorqueEncoder;
    let report = enc.encode(-2.0);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_enable_ffb_on() {
    use racing_wheel_hid_openffboard_protocol::build_enable_ffb;
    let report = build_enable_ffb(true);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_enable_ffb_off() {
    use racing_wheel_hid_openffboard_protocol::build_enable_ffb;
    let report = build_enable_ffb(false);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_set_gain_full() {
    use racing_wheel_hid_openffboard_protocol::build_set_gain;
    let report = build_set_gain(255);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_protocol_constants() {
    use racing_wheel_hid_openffboard_protocol::{
        CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
        OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
    };
    assert_snapshot!(format!(
        "VID={:#06X}, PID={:#06X}, PID_ALT={:#06X}, REPORT_ID={:#04X}, REPORT_LEN={}, GAIN_ID={:#04X}",
        OPENFFBOARD_VENDOR_ID,
        OPENFFBOARD_PRODUCT_ID,
        OPENFFBOARD_PRODUCT_ID_ALT,
        CONSTANT_FORCE_REPORT_ID,
        CONSTANT_FORCE_REPORT_LEN,
        GAIN_REPORT_ID,
    ));
}

#[test]
fn test_snapshot_is_openffboard_product() {
    use racing_wheel_hid_openffboard_protocol::{
        is_openffboard_product, OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT,
    };
    let results = [
        ("main", is_openffboard_product(OPENFFBOARD_PRODUCT_ID)),
        ("alt", is_openffboard_product(OPENFFBOARD_PRODUCT_ID_ALT)),
        ("wrong_pid_zero", is_openffboard_product(0x0000)),
        ("wrong_pid_ffff", is_openffboard_product(0xFFFF)),
    ];
    assert_snapshot!(format!("{:?}", results));
}
