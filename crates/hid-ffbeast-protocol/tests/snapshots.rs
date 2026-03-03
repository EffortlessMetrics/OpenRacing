//! Additional insta snapshot tests for the FFBeast HID protocol.
//!
//! Complements `snapshot_tests.rs` with half-negative torque, precise small
//! values, report constant snapshots, and a device summary table.

use insta::assert_debug_snapshot;
use racing_wheel_hid_ffbeast_protocol as ffbeast;

#[test]
fn snapshot_encode_torque_half_negative() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    assert_debug_snapshot!(enc.encode(-0.5));
}

#[test]
fn snapshot_encode_torque_small_positive() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    assert_debug_snapshot!(enc.encode(0.1));
}

#[test]
fn snapshot_encode_torque_small_negative() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    assert_debug_snapshot!(enc.encode(-0.1));
}

#[test]
fn snapshot_encode_torque_three_quarter() {
    let enc = ffbeast::FFBeastTorqueEncoder;
    assert_debug_snapshot!(enc.encode(0.75));
}

#[test]
fn snapshot_report_id_constants() {
    assert_debug_snapshot!(format!(
        "CONSTANT_FORCE_ID={:#04X}, CONSTANT_FORCE_LEN={}, ENABLE_FFB_ID={:#04X}, GAIN_ID={:#04X}",
        ffbeast::CONSTANT_FORCE_REPORT_ID,
        ffbeast::CONSTANT_FORCE_REPORT_LEN,
        ffbeast::output::ENABLE_FFB_REPORT_ID,
        ffbeast::GAIN_REPORT_ID,
    ));
}

#[test]
fn snapshot_device_summary() {
    let devices = [
        ("Joystick", ffbeast::FFBEAST_PRODUCT_ID_JOYSTICK),
        ("Rudder", ffbeast::FFBEAST_PRODUCT_ID_RUDDER),
        ("Wheel", ffbeast::FFBEAST_PRODUCT_ID_WHEEL),
    ];
    let summary: Vec<String> = devices
        .iter()
        .map(|(name, pid)| {
            format!(
                "{name}: PID={pid:#06X}, is_ffbeast={}",
                ffbeast::is_ffbeast_product(*pid)
            )
        })
        .collect();
    assert_debug_snapshot!(summary);
}
