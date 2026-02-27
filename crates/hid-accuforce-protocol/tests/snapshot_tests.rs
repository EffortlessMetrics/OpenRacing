//! Insta snapshot tests for AccuForce protocol constants and types.
//!
//! Snapshots are stored in tests/snapshots/. Regenerate with:
//! INSTA_UPDATE=always cargo test -p racing-wheel-hid-accuforce-protocol

use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_ACCUFORCE_PRO,
    RECOMMENDED_B_INTERVAL_MS, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// -- IDs ----------------------------------------------------------------------

#[test]
fn snapshot_vendor_id() {
    insta::assert_debug_snapshot!(VENDOR_ID);
}

#[test]
fn snapshot_pid_accuforce_pro() {
    insta::assert_debug_snapshot!(PID_ACCUFORCE_PRO);
}

#[test]
fn snapshot_is_accuforce_pro_vid_pid() {
    insta::assert_debug_snapshot!(is_accuforce(VENDOR_ID, PID_ACCUFORCE_PRO));
}

#[test]
fn snapshot_is_accuforce_pid_pro() {
    insta::assert_debug_snapshot!(is_accuforce_pid(PID_ACCUFORCE_PRO));
}

#[test]
fn snapshot_is_accuforce_wrong_vid() {
    insta::assert_debug_snapshot!(is_accuforce(0x16D0, PID_ACCUFORCE_PRO));
}

// -- AccuForceModel -----------------------------------------------------------

#[test]
fn snapshot_model_from_pro_pid() {
    insta::assert_debug_snapshot!(AccuForceModel::from_product_id(PID_ACCUFORCE_PRO));
}

#[test]
fn snapshot_model_from_unknown_pid() {
    insta::assert_debug_snapshot!(AccuForceModel::from_product_id(0xFFFF));
}

#[test]
fn snapshot_pro_display_name() {
    insta::assert_debug_snapshot!(AccuForceModel::Pro.display_name());
}

#[test]
fn snapshot_unknown_display_name() {
    insta::assert_debug_snapshot!(AccuForceModel::Unknown.display_name());
}

#[test]
fn snapshot_pro_max_torque_nm() {
    insta::assert_debug_snapshot!(AccuForceModel::Pro.max_torque_nm());
}

// -- DeviceInfo ---------------------------------------------------------------

#[test]
fn snapshot_device_info_pro() {
    insta::assert_debug_snapshot!(DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO));
}

#[test]
fn snapshot_device_info_unknown_pid() {
    insta::assert_debug_snapshot!(DeviceInfo::from_vid_pid(VENDOR_ID, 0xFFFF));
}

// -- Report constants ---------------------------------------------------------

#[test]
fn snapshot_max_report_bytes() {
    insta::assert_debug_snapshot!(MAX_REPORT_BYTES);
}

#[test]
fn snapshot_hid_pid_usage_page() {
    insta::assert_debug_snapshot!(HID_PID_USAGE_PAGE);
}

#[test]
fn snapshot_recommended_b_interval_ms() {
    insta::assert_debug_snapshot!(RECOMMENDED_B_INTERVAL_MS);
}
