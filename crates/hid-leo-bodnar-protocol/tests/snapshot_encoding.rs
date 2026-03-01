//! Extended snapshot tests for Leo Bodnar device classification functions.
//!
//! Complements snapshot_tests.rs by covering the `is_leo_bodnar_device()`
//! and `is_leo_bodnar_ffb_pid()` public functions, plus individual constant
//! snapshots, to detect regressions in device identification logic.

use insta::assert_debug_snapshot;
use racing_wheel_hid_leo_bodnar_protocol as leo_bodnar;

// -- Individual ID constants --------------------------------------------------

#[test]
fn snapshot_vendor_id() {
    assert_debug_snapshot!(format!("{:#06x}", leo_bodnar::VENDOR_ID));
}

#[test]
fn snapshot_max_report_bytes() {
    assert_debug_snapshot!(leo_bodnar::MAX_REPORT_BYTES);
}

#[test]
fn snapshot_hid_pid_usage_page() {
    assert_debug_snapshot!(format!("{:#06x}", leo_bodnar::HID_PID_USAGE_PAGE));
}

// -- is_leo_bodnar_device (PID-only classification) ---------------------------

#[test]
fn snapshot_is_device_all_known_pids() {
    let entries = [
        ("USB_JOYSTICK", leo_bodnar::PID_USB_JOYSTICK),
        ("BU0836A", leo_bodnar::PID_BU0836A),
        ("BBI32", leo_bodnar::PID_BBI32),
        ("WHEEL_INTERFACE", leo_bodnar::PID_WHEEL_INTERFACE),
        ("FFB_JOYSTICK", leo_bodnar::PID_FFB_JOYSTICK),
        ("BU0836X", leo_bodnar::PID_BU0836X),
        ("BU0836_16BIT", leo_bodnar::PID_BU0836_16BIT),
        ("SLI_M", leo_bodnar::PID_SLI_M),
    ];
    let summary: Vec<String> = entries
        .iter()
        .map(|(name, pid)| {
            format!(
                "{}: is_device={}",
                name,
                leo_bodnar::is_leo_bodnar_device(*pid)
            )
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}

#[test]
fn snapshot_is_device_unknown_pids() {
    let entries = [
        ("0x0000", 0x0000u16),
        ("0xFFFF", 0xFFFFu16),
        ("0x1234", 0x1234u16),
    ];
    let summary: Vec<String> = entries
        .iter()
        .map(|(name, pid)| {
            format!(
                "{}: is_device={}",
                name,
                leo_bodnar::is_leo_bodnar_device(*pid)
            )
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}

// -- is_leo_bodnar_ffb_pid (FFB-capable classification) -----------------------

#[test]
fn snapshot_ffb_pid_all_known() {
    let entries = [
        ("USB_JOYSTICK", leo_bodnar::PID_USB_JOYSTICK),
        ("BU0836A", leo_bodnar::PID_BU0836A),
        ("BBI32", leo_bodnar::PID_BBI32),
        ("WHEEL_INTERFACE", leo_bodnar::PID_WHEEL_INTERFACE),
        ("FFB_JOYSTICK", leo_bodnar::PID_FFB_JOYSTICK),
        ("BU0836X", leo_bodnar::PID_BU0836X),
        ("BU0836_16BIT", leo_bodnar::PID_BU0836_16BIT),
        ("SLI_M", leo_bodnar::PID_SLI_M),
    ];
    let summary: Vec<String> = entries
        .iter()
        .map(|(name, pid)| {
            format!(
                "{}: is_ffb={}",
                name,
                leo_bodnar::is_leo_bodnar_ffb_pid(*pid)
            )
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}

#[test]
fn snapshot_ffb_pid_unknown() {
    let entries = [("0x0000", 0x0000u16), ("0xFFFF", 0xFFFFu16)];
    let summary: Vec<String> = entries
        .iter()
        .map(|(name, pid)| {
            format!(
                "{}: is_ffb={}",
                name,
                leo_bodnar::is_leo_bodnar_ffb_pid(*pid)
            )
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}

// -- Device name lookup for all variants --------------------------------------

#[test]
fn snapshot_all_device_names() {
    let entries = [
        ("USB_JOYSTICK", leo_bodnar::PID_USB_JOYSTICK),
        ("BU0836A", leo_bodnar::PID_BU0836A),
        ("BBI32", leo_bodnar::PID_BBI32),
        ("WHEEL_INTERFACE", leo_bodnar::PID_WHEEL_INTERFACE),
        ("FFB_JOYSTICK", leo_bodnar::PID_FFB_JOYSTICK),
        ("BU0836X", leo_bodnar::PID_BU0836X),
        ("BU0836_16BIT", leo_bodnar::PID_BU0836_16BIT),
        ("SLI_M", leo_bodnar::PID_SLI_M),
    ];
    let summary: Vec<String> = entries
        .iter()
        .filter_map(|(label, pid)| {
            leo_bodnar::LeoBodnarDevice::from_product_id(*pid)
                .map(|d| format!("{}: name={}", label, d.name()))
        })
        .collect();
    assert_debug_snapshot!(summary.join("\n"));
}
