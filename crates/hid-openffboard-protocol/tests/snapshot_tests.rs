//! Insta snapshot tests for the OpenFFBoard HID protocol encoding.
//!
//! These tests pin the exact wire-format bytes for the three canonical inputs:
//! full-negative (-1.0), zero (0.0), and full-positive (1.0) normalised torque.

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
