//! Extended snapshot tests for AccuForce wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering hex-formatted
//! constant values, boundary device classification, and formatted DeviceInfo
//! output that would detect wire-format regressions.

use insta::assert_snapshot;
use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_ACCUFORCE_PRO,
    RECOMMENDED_B_INTERVAL_MS, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// ── Hex-formatted ID constants ───────────────────────────────────────────────

#[test]
fn test_snapshot_vendor_id_hex() {
    assert_snapshot!(format!("0x{VENDOR_ID:04X}"));
}

#[test]
fn test_snapshot_pid_pro_hex() {
    assert_snapshot!(format!("0x{PID_ACCUFORCE_PRO:04X}"));
}

#[test]
fn test_snapshot_hid_pid_usage_page_hex() {
    assert_snapshot!(format!("0x{HID_PID_USAGE_PAGE:04X}"));
}

// ── Device identification boundary values ────────────────────────────────────

#[test]
fn test_snapshot_is_accuforce_all_known() {
    let results: Vec<String> = [(VENDOR_ID, PID_ACCUFORCE_PRO)]
        .iter()
        .map(|&(vid, pid)| format!("VID=0x{vid:04X},PID=0x{pid:04X}={}", is_accuforce(vid, pid)))
        .collect();
    assert_snapshot!(results.join("\n"));
}

#[test]
fn test_snapshot_is_accuforce_wrong_vid_variants() {
    let wrong_vids: Vec<String> = [0x0000u16, 0x16D0, 0x1DD2, 0x0483, 0xFFFF]
        .iter()
        .map(|&vid| format!("VID=0x{vid:04X}={}", is_accuforce(vid, PID_ACCUFORCE_PRO)))
        .collect();
    assert_snapshot!(wrong_vids.join(", "));
}

#[test]
fn test_snapshot_is_accuforce_pid_boundary() {
    let pids: Vec<String> = [0x0000u16, PID_ACCUFORCE_PRO, 0x804B, 0x804D, 0xFFFF]
        .iter()
        .map(|&pid| format!("0x{pid:04X}={}", is_accuforce_pid(pid)))
        .collect();
    assert_snapshot!(pids.join(", "));
}

// ── DeviceInfo formatting ────────────────────────────────────────────────────

#[test]
fn test_snapshot_device_info_pro_formatted() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_snapshot!(format!(
        "vid=0x{:04X}, pid=0x{:04X}, model={:?}, name={}, torque={:.1}Nm",
        info.vendor_id,
        info.product_id,
        info.model,
        info.model.display_name(),
        info.model.max_torque_nm()
    ));
}

#[test]
fn test_snapshot_device_info_unknown_formatted() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, 0xFFFF);
    assert_snapshot!(format!(
        "vid=0x{:04X}, pid=0x{:04X}, model={:?}, name={}, torque={:.1}Nm",
        info.vendor_id,
        info.product_id,
        info.model,
        info.model.display_name(),
        info.model.max_torque_nm()
    ));
}

// ── Model exhaustive ─────────────────────────────────────────────────────────

#[test]
fn test_snapshot_all_model_variants() {
    let models = [AccuForceModel::Pro, AccuForceModel::Unknown];
    let summary: Vec<String> = models
        .iter()
        .map(|m| {
            format!(
                "{:?}: name={}, torque={:.1}Nm",
                m,
                m.display_name(),
                m.max_torque_nm()
            )
        })
        .collect();
    assert_snapshot!(summary.join("\n"));
}

// ── Report constants formatted ───────────────────────────────────────────────

#[test]
fn test_snapshot_report_constants_summary() {
    assert_snapshot!(format!(
        "max_report_bytes={}, usage_page=0x{:04X}, interval_ms={}",
        MAX_REPORT_BYTES, HID_PID_USAGE_PAGE, RECOMMENDED_B_INTERVAL_MS
    ));
}
