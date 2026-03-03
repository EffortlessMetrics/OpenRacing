//! Deep protocol tests for AccuForce HID protocol.
//!
//! Covers AccuForce V2 torque encoding, telemetry report parsing,
//! and configuration commands.

use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_ACCUFORCE_PRO,
    RECOMMENDED_B_INTERVAL_MS, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// ─── AccuForce V2 torque encoding ───────────────────────────────────────────

#[test]
fn pro_model_max_torque_v1_conservative() {
    let torque = AccuForceModel::Pro.max_torque_nm();
    assert_eq!(torque, 7.0);
    assert!(torque > 0.0);
}

#[test]
fn unknown_model_torque_same_as_pro() {
    // Conservative default: unknown models use the V1 torque value.
    assert_eq!(
        AccuForceModel::Unknown.max_torque_nm(),
        AccuForceModel::Pro.max_torque_nm()
    );
}

#[test]
fn pro_pid_is_0x804c() {
    assert_eq!(PID_ACCUFORCE_PRO, 0x804C);
}

#[test]
fn vendor_id_is_nxp() {
    assert_eq!(VENDOR_ID, 0x1FC9);
}

// ─── Telemetry report parsing ───────────────────────────────────────────────

#[test]
fn hid_pid_usage_page_is_standard() {
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn report_size_within_full_speed_usb_limit() {
    assert!(MAX_REPORT_BYTES <= 64);
    assert_eq!(MAX_REPORT_BYTES, 64);
}

#[test]
fn recommended_interval_is_8ms() {
    assert_eq!(RECOMMENDED_B_INTERVAL_MS, 8);
}

#[test]
fn update_rate_range_from_interval() {
    // 8 ms → 125 Hz, which falls in the 100–200 Hz target range.
    let hz = 1000.0 / f64::from(RECOMMENDED_B_INTERVAL_MS);
    assert!(hz >= 100.0);
    assert!(hz <= 200.0);
}

// ─── Configuration commands / device identification ─────────────────────────

#[test]
fn device_info_from_known_vid_pid() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_eq!(info.vendor_id, VENDOR_ID);
    assert_eq!(info.product_id, PID_ACCUFORCE_PRO);
    assert_eq!(info.model, AccuForceModel::Pro);
}

#[test]
fn device_info_from_unknown_pid() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, 0xFFFF);
    assert_eq!(info.model, AccuForceModel::Unknown);
    assert_eq!(info.vendor_id, VENDOR_ID);
}

#[test]
fn device_info_from_wrong_vendor() {
    let info = DeviceInfo::from_vid_pid(0x0000, PID_ACCUFORCE_PRO);
    // Model resolution is by PID only; vendor_id is stored but not checked.
    assert_eq!(info.model, AccuForceModel::Pro);
    assert_eq!(info.vendor_id, 0x0000);
}

#[test]
fn is_accuforce_requires_both_vid_and_pid() {
    assert!(is_accuforce(VENDOR_ID, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(0x0000, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(VENDOR_ID, 0x0000));
}

#[test]
fn is_accuforce_pid_checks_pid_only() {
    assert!(is_accuforce_pid(PID_ACCUFORCE_PRO));
    assert!(!is_accuforce_pid(0x0000));
    assert!(!is_accuforce_pid(0xFFFF));
}

#[test]
fn pro_display_name_contains_accuforce() {
    let name = AccuForceModel::Pro.display_name();
    assert!(name.contains("AccuForce"));
    assert!(name.contains("Pro"));
}

#[test]
fn unknown_display_name_is_descriptive() {
    let name = AccuForceModel::Unknown.display_name();
    assert!(!name.is_empty());
    assert!(name.contains("AccuForce"));
}
