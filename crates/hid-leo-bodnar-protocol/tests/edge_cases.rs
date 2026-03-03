//! Comprehensive edge-case and boundary-value tests for the Leo Bodnar protocol.
//!
//! Covers device identification, report constants, type classification,
//! and cross-module consistency invariants.

use racing_wheel_hid_leo_bodnar_protocol::{
    HID_PID_USAGE_PAGE, LeoBodnarDevice, MAX_REPORT_BYTES, PID_BBI32, PID_BU0836A, PID_BU0836X,
    PID_BU0836_16BIT, PID_FFB_JOYSTICK, PID_SLI_M, PID_USB_JOYSTICK, PID_WHEEL_INTERFACE,
    VENDOR_ID, WHEEL_DEFAULT_MAX_TORQUE_NM, WHEEL_ENCODER_CPR, is_leo_bodnar,
    is_leo_bodnar_device, is_leo_bodnar_ffb_pid,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Constant golden values
// ---------------------------------------------------------------------------

#[test]
fn vendor_id_golden() {
    assert_eq!(VENDOR_ID, 0x1DD2);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn report_constants_golden() {
    assert_eq!(MAX_REPORT_BYTES, 64);
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
    assert_eq!(WHEEL_ENCODER_CPR, 65_535);
    assert!(WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0);
    assert!(WHEEL_DEFAULT_MAX_TORQUE_NM.is_finite());
}

#[test]
fn all_pid_constants_non_zero_and_distinct() {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
    ];
    for &pid in &pids {
        assert_ne!(pid, 0, "PID must not be zero");
    }
    // Check all PIDs are distinct
    let mut sorted = pids.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), pids.len(), "all PIDs must be distinct");
}

// ---------------------------------------------------------------------------
// Device identification – edge cases
// ---------------------------------------------------------------------------

#[test]
fn is_leo_bodnar_requires_correct_vid() {
    // Wrong VID should always fail, even with valid PIDs
    let known_pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
    ];
    for &pid in &known_pids {
        assert!(!is_leo_bodnar(0x0000, pid));
        assert!(!is_leo_bodnar(VENDOR_ID.wrapping_add(1), pid));
    }
}

#[test]
fn is_leo_bodnar_device_accepts_all_without_vid_check() {
    // is_leo_bodnar_device checks PID only (no VID)
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
    ];
    for &pid in &pids {
        assert!(is_leo_bodnar_device(pid), "PID 0x{pid:04X} must be recognised");
    }
}

#[test]
fn vid_zero_pid_zero_not_recognised() {
    assert!(!is_leo_bodnar(0, 0));
    assert!(!is_leo_bodnar_device(0));
    assert!(LeoBodnarDevice::from_product_id(0).is_none());
}

#[test]
fn vid_max_pid_max_not_recognised() {
    assert!(!is_leo_bodnar(u16::MAX, u16::MAX));
    assert!(!is_leo_bodnar_device(u16::MAX));
    assert!(LeoBodnarDevice::from_product_id(u16::MAX).is_none());
}

// ---------------------------------------------------------------------------
// Device type classification – exhaustive
// ---------------------------------------------------------------------------

#[test]
fn all_device_variants_have_non_empty_names() {
    let all_devices = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
        LeoBodnarDevice::SlimShiftLight,
        LeoBodnarDevice::Pedals,
        LeoBodnarDevice::LcPedals,
    ];
    for device in &all_devices {
        assert!(
            !device.name().is_empty(),
            "{device:?} must have non-empty name"
        );
        assert!(
            device.name().contains("Leo Bodnar"),
            "{device:?} name must contain 'Leo Bodnar'"
        );
    }
}

#[test]
fn ffb_devices_have_32_input_channels() {
    assert_eq!(LeoBodnarDevice::WheelInterface.max_input_channels(), 32);
    assert_eq!(LeoBodnarDevice::FfbJoystick.max_input_channels(), 32);
}

#[test]
fn non_input_devices_have_zero_channels() {
    assert_eq!(LeoBodnarDevice::SlimShiftLight.max_input_channels(), 0);
    assert_eq!(LeoBodnarDevice::Pedals.max_input_channels(), 0);
    assert_eq!(LeoBodnarDevice::LcPedals.max_input_channels(), 0);
}

#[test]
fn only_two_devices_support_ffb() {
    let all_devices = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
        LeoBodnarDevice::SlimShiftLight,
        LeoBodnarDevice::Pedals,
        LeoBodnarDevice::LcPedals,
    ];
    let ffb_count = all_devices.iter().filter(|d| d.supports_ffb()).count();
    assert_eq!(ffb_count, 2, "exactly WheelInterface and FfbJoystick support FFB");
}

#[test]
fn device_names_are_all_unique() {
    let all_devices = [
        LeoBodnarDevice::UsbJoystick,
        LeoBodnarDevice::Bu0836a,
        LeoBodnarDevice::Bbi32,
        LeoBodnarDevice::WheelInterface,
        LeoBodnarDevice::FfbJoystick,
        LeoBodnarDevice::Bu0836x,
        LeoBodnarDevice::Bu0836_16bit,
        LeoBodnarDevice::SlimShiftLight,
        LeoBodnarDevice::Pedals,
        LeoBodnarDevice::LcPedals,
    ];
    let mut names: Vec<&str> = all_devices.iter().map(|d| d.name()).collect();
    let len_before = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), len_before, "all device names must be unique");
}

#[test]
fn device_debug_format_non_empty() {
    let device = LeoBodnarDevice::WheelInterface;
    assert!(!format!("{device:?}").is_empty());
}

#[test]
fn device_clone_and_copy() {
    let device = LeoBodnarDevice::FfbJoystick;
    let cloned = device;
    assert_eq!(device, cloned);
}

// ---------------------------------------------------------------------------
// Cross-module consistency
// ---------------------------------------------------------------------------

#[test]
fn from_product_id_and_is_leo_bodnar_device_agree_for_all_known() {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
    ];
    for &pid in &pids {
        assert!(
            LeoBodnarDevice::from_product_id(pid).is_some(),
            "from_product_id must return Some for PID 0x{pid:04X}"
        );
        assert!(
            is_leo_bodnar_device(pid),
            "is_leo_bodnar_device must return true for PID 0x{pid:04X}"
        );
    }
}

#[test]
fn ffb_pid_function_agrees_with_device_supports_ffb() {
    let pids = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_BU0836A,
        PID_BU0836X,
        PID_BU0836_16BIT,
        PID_WHEEL_INTERFACE,
        PID_FFB_JOYSTICK,
        PID_SLI_M,
    ];
    for &pid in &pids {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            assert_eq!(
                device.supports_ffb(),
                is_leo_bodnar_ffb_pid(pid),
                "FFB support mismatch for PID 0x{pid:04X} ({device:?})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// For any u16, is_leo_bodnar with wrong VID must return false.
    #[test]
    fn prop_wrong_vid_never_recognised(pid in any::<u16>()) {
        prop_assert!(!is_leo_bodnar(0x0000, pid));
        // Skip if vid happens to be VENDOR_ID
        if 0x1234 != VENDOR_ID {
            prop_assert!(!is_leo_bodnar(0x1234, pid));
        }
    }

    /// For any recognised PID, the device's max_input_channels is <= 32.
    #[test]
    fn prop_input_channels_bounded(pid in any::<u16>()) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(device.max_input_channels() <= 32);
        }
    }

    /// For any recognised device, name starts with "Leo Bodnar".
    #[test]
    fn prop_device_name_starts_with_brand(pid in any::<u16>()) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(
                device.name().starts_with("Leo Bodnar"),
                "name for {device:?} must start with 'Leo Bodnar'"
            );
        }
    }

    /// is_leo_bodnar_ffb_pid returns false for unrecognised PIDs.
    #[test]
    fn prop_ffb_pid_false_for_unrecognised(pid in any::<u16>()) {
        if !is_leo_bodnar_device(pid) {
            prop_assert!(!is_leo_bodnar_ffb_pid(pid));
        }
    }
}
