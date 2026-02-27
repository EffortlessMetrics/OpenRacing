//! Snapshot tests for the Leo Bodnar HID protocol.
//!
//! These tests lock in device properties and protocol constants to catch
//! accidental regressions in device classification and wire-format constants.

use insta::assert_debug_snapshot;
use racing_wheel_hid_leo_bodnar_protocol as leo_bodnar;

fn device_summary(pid: u16) -> String {
    match leo_bodnar::LeoBodnarDevice::from_product_id(pid) {
        Some(device) => format!(
            "pid={:#06x}, name={}, channels={}, ffb={}",
            pid,
            device.name(),
            device.max_input_channels(),
            device.supports_ffb()
        ),
        None => format!("pid={:#06x}: unrecognised", pid),
    }
}

// ── DeviceInfo snapshots for every supported PID ──────────────────────────────

#[test]
fn test_snapshot_device_bbi32() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_BBI32));
}

#[test]
fn test_snapshot_device_bu0836a() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_BU0836A));
}

#[test]
fn test_snapshot_device_bu0836x() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_BU0836X));
}

#[test]
fn test_snapshot_device_bu0836_16bit() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_BU0836_16BIT));
}

#[test]
fn test_snapshot_device_ffb_joystick() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_FFB_JOYSTICK));
}

#[test]
fn test_snapshot_device_sli_m() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_SLI_M));
}

#[test]
fn test_snapshot_device_wheel_interface() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_WHEEL_INTERFACE));
}

#[test]
fn test_snapshot_device_usb_joystick() {
    assert_debug_snapshot!(device_summary(leo_bodnar::PID_USB_JOYSTICK));
}

// ── Protocol constants snapshot ───────────────────────────────────────────────

#[test]
fn test_snapshot_protocol_constants() {
    assert_debug_snapshot!(format!(
        "VENDOR_ID={:#06x}, MAX_REPORT_BYTES={}, HID_PID_USAGE_PAGE={:#06x}, \
         WHEEL_ENCODER_CPR={}, WHEEL_DEFAULT_MAX_TORQUE_NM={:.1}",
        leo_bodnar::VENDOR_ID,
        leo_bodnar::MAX_REPORT_BYTES,
        leo_bodnar::HID_PID_USAGE_PAGE,
        leo_bodnar::WHEEL_ENCODER_CPR,
        leo_bodnar::WHEEL_DEFAULT_MAX_TORQUE_NM,
    ));
}

// ── Button report: max_input_channels for every device ───────────────────────

#[test]
fn test_snapshot_button_report_channels() {
    let entries = [
        (leo_bodnar::PID_USB_JOYSTICK, "USB_JOYSTICK"),
        (leo_bodnar::PID_BU0836A, "BU0836A"),
        (leo_bodnar::PID_BBI32, "BBI32"),
        (leo_bodnar::PID_WHEEL_INTERFACE, "WHEEL_INTERFACE"),
        (leo_bodnar::PID_FFB_JOYSTICK, "FFB_JOYSTICK"),
        (leo_bodnar::PID_BU0836X, "BU0836X"),
        (leo_bodnar::PID_BU0836_16BIT, "BU0836_16BIT"),
        (leo_bodnar::PID_SLI_M, "SLI_M"),
    ];
    let summary: Vec<String> = entries
        .iter()
        .filter_map(|&(pid, name)| {
            leo_bodnar::LeoBodnarDevice::from_product_id(pid)
                .map(|d| format!("{}: max_channels={}", name, d.max_input_channels()))
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}

// ── Axis report: wheel axis constants ────────────────────────────────────────

#[test]
fn test_snapshot_axis_report_wheel_constants() {
    assert_debug_snapshot!(format!(
        "encoder_cpr={}, max_torque_nm={:.1}, fits_u16={}",
        leo_bodnar::WHEEL_ENCODER_CPR,
        leo_bodnar::WHEEL_DEFAULT_MAX_TORQUE_NM,
        leo_bodnar::WHEEL_ENCODER_CPR <= u32::from(u16::MAX),
    ));
}

// ── FFB device round-trip: VID+PID → device → name/ffb/vid check ─────────────

#[test]
fn test_snapshot_ffb_device_roundtrip() {
    let ffb_pids = [leo_bodnar::PID_WHEEL_INTERFACE, leo_bodnar::PID_FFB_JOYSTICK];
    let summary: Vec<String> = ffb_pids
        .iter()
        .filter_map(|&pid| {
            leo_bodnar::LeoBodnarDevice::from_product_id(pid).map(|d| {
                format!(
                    "pid={:#06x}, name={}, ffb={}, vid_match={}",
                    pid,
                    d.name(),
                    d.supports_ffb(),
                    leo_bodnar::is_leo_bodnar(leo_bodnar::VENDOR_ID, pid)
                )
            })
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}

// ── All known PIDs list snapshot ──────────────────────────────────────────────

#[test]
fn test_snapshot_all_known_pids() {
    let pids = [
        ("USB_JOYSTICK", leo_bodnar::PID_USB_JOYSTICK),
        ("BU0836A", leo_bodnar::PID_BU0836A),
        ("BBI32", leo_bodnar::PID_BBI32),
        ("WHEEL_INTERFACE", leo_bodnar::PID_WHEEL_INTERFACE),
        ("FFB_JOYSTICK", leo_bodnar::PID_FFB_JOYSTICK),
        ("BU0836X", leo_bodnar::PID_BU0836X),
        ("BU0836_16BIT", leo_bodnar::PID_BU0836_16BIT),
        ("SLI_M", leo_bodnar::PID_SLI_M),
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|(name, pid)| format!("{}={:#06x}", name, pid))
        .collect();
    assert_debug_snapshot!(summary.join(", "));
}
