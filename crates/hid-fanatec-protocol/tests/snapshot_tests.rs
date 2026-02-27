//! Insta snapshot tests for the Fanatec HID protocol encoding.
//!
//! These tests pin the exact wire-format bytes produced by the encoder
//! for the three canonical inputs: full-negative, zero, and full-positive force.

use insta::assert_snapshot;
use racing_wheel_hid_fanatec_protocol::{FanatecConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN};

/// Helper: encode `torque_nm` with `max_torque_nm = 1.0` and return formatted bytes.
fn encode_bytes(torque_nm: f32) -> String {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode(torque_nm, 0, &mut out);
    format!("{:?}", out)
}

#[test]
fn test_snapshot_encode_constant_force_neg_one() {
    assert_snapshot!(encode_bytes(-1.0));
}

#[test]
fn test_snapshot_encode_constant_force_zero() {
    assert_snapshot!(encode_bytes(0.0));
}

#[test]
fn test_snapshot_encode_constant_force_pos_one() {
    assert_snapshot!(encode_bytes(1.0));
}

#[test]
fn test_snapshot_encode_zero_report() {
    let encoder = FanatecConstantForceEncoder::new(1.0);
    let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
    encoder.encode_zero(&mut out);
    assert_snapshot!(format!("{:?}", out));
}

#[test]
fn test_snapshot_mode_switch_report() {
    use racing_wheel_hid_fanatec_protocol::build_mode_switch_report;
    let report = build_mode_switch_report();
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_stop_all_report() {
    use racing_wheel_hid_fanatec_protocol::build_stop_all_report;
    let report = build_stop_all_report();
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_set_gain_report_full() {
    use racing_wheel_hid_fanatec_protocol::build_set_gain_report;
    let report = build_set_gain_report(100);
    assert_snapshot!(format!("{:?}", report));
}

#[test]
fn test_snapshot_led_report_all_on() {
    use racing_wheel_hid_fanatec_protocol::build_led_report;
    let report = build_led_report(0xFFFF, 255);
    assert_snapshot!(format!("{:?}", report));
}
