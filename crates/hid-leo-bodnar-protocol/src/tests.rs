//! Integration tests for the Leo Bodnar protocol crate.

use crate::ids::{
    PID_BBI32, PID_BU0836_16BIT, PID_BU0836A, PID_BU0836X, PID_FFB_JOYSTICK, PID_SLI_M,
    PID_USB_JOYSTICK, PID_WHEEL_INTERFACE, VENDOR_ID, is_leo_bodnar_device, is_leo_bodnar_ffb_pid,
};
use crate::types::LeoBodnarDevice;

// ── All known PIDs ────────────────────────────────────────────────────────────

const ALL_KNOWN_PIDS: &[u16] = &[
    PID_USB_JOYSTICK,
    PID_BU0836A,
    PID_BBI32,
    PID_WHEEL_INTERFACE,
    PID_FFB_JOYSTICK,
    PID_BU0836X,
    PID_BU0836_16BIT,
    PID_SLI_M,
];

// ── Unit tests ────────────────────────────────────────────────────────────────

#[test]
fn all_known_pids_are_recognised() {
    for &pid in ALL_KNOWN_PIDS {
        assert!(
            is_leo_bodnar_device(pid),
            "PID 0x{pid:04X} must be recognised as a Leo Bodnar device"
        );
    }
}

#[test]
fn all_known_pids_map_to_device_variant() {
    for &pid in ALL_KNOWN_PIDS {
        assert!(
            LeoBodnarDevice::from_product_id(pid).is_some(),
            "PID 0x{pid:04X} must resolve to a LeoBodnarDevice variant"
        );
    }
}

#[test]
fn unknown_pid_is_not_recognised() {
    let unknown = [0x0000u16, 0x0002, 0x0100, 0xDEAD, 0xFFFF];
    for &pid in &unknown {
        assert!(
            !is_leo_bodnar_device(pid),
            "PID 0x{pid:04X} must not be recognised"
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(pid),
            None,
            "PID 0x{pid:04X} must return None"
        );
    }
}

#[test]
fn ffb_pids_are_subset_of_known_pids() {
    let ffb_pids = [PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK];
    for &pid in &ffb_pids {
        assert!(
            is_leo_bodnar_device(pid),
            "FFB PID 0x{pid:04X} must also be a known device"
        );
        assert!(
            is_leo_bodnar_ffb_pid(pid),
            "PID 0x{pid:04X} must be recognised as FFB-capable"
        );
    }
}

#[test]
fn non_ffb_pids_do_not_claim_ffb() {
    let non_ffb = [
        PID_USB_JOYSTICK,
        PID_BBI32,
        PID_SLI_M,
        PID_BU0836A,
        PID_BU0836X,
    ];
    for &pid in &non_ffb {
        assert!(
            !is_leo_bodnar_ffb_pid(pid),
            "PID 0x{pid:04X} must not claim FFB support"
        );
    }
}

#[test]
fn device_from_ffb_pids_reports_ffb_support() {
    let ffb_pids = [PID_WHEEL_INTERFACE, PID_FFB_JOYSTICK];
    for &pid in &ffb_pids {
        let device = LeoBodnarDevice::from_product_id(pid)
            .unwrap_or_else(|| panic!("PID 0x{pid:04X} should resolve to a device"));
        assert!(
            device.supports_ffb(),
            "{:?} must report supports_ffb() == true",
            device
        );
    }
}

#[test]
fn input_channel_count_is_consistent_with_device_type() {
    // Button boxes: always 32 channels
    for &pid in &[PID_BBI32, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT] {
        let device = LeoBodnarDevice::from_product_id(pid)
            .unwrap_or_else(|| panic!("PID 0x{pid:04X} should resolve"));
        assert_eq!(
            device.max_input_channels(),
            32,
            "{:?} must report 32 input channels",
            device
        );
    }
    // Output-only device
    assert_eq!(
        LeoBodnarDevice::SlimShiftLight.max_input_channels(),
        0,
        "SLI-M must report 0 input channels"
    );
}

#[test]
fn vendor_id_is_leo_bodnar() {
    assert_eq!(VENDOR_ID, 0x1DD2);
}

// ── Proptest property tests ───────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    /// For any u16 PID, `from_product_id` returns `Some` iff `is_leo_bodnar_device`
    /// returns `true`.
    #[test]
    fn prop_device_from_pid_consistent_with_is_leo_bodnar_device(pid in any::<u16>()) {
        let device = LeoBodnarDevice::from_product_id(pid);
        let recognised = is_leo_bodnar_device(pid);
        prop_assert_eq!(
            device.is_some(),
            recognised,
            "from_product_id and is_leo_bodnar_device must agree for PID 0x{:04X}",
            pid
        );
    }

    /// For any recognised device, `max_input_channels` must be ≤ 32 (USB HID
    /// joystick button limit for standard descriptors).
    #[test]
    fn prop_max_input_channels_within_hid_limit(pid in any::<u16>()) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(
                device.max_input_channels() <= 32,
                "{:?} reports {} channels, must be ≤ 32",
                device,
                device.max_input_channels()
            );
        }
    }

    /// FFB support on a device is consistent with `is_leo_bodnar_ffb_pid`.
    #[test]
    fn prop_ffb_support_consistent(pid in any::<u16>()) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert_eq!(
                device.supports_ffb(),
                is_leo_bodnar_ffb_pid(pid),
                "supports_ffb and is_leo_bodnar_ffb_pid must agree for {:?} (PID 0x{:04X})",
                device,
                pid
            );
        }
    }

    /// Device names must always be non-empty for any recognised PID.
    #[test]
    fn prop_device_name_non_empty(pid in any::<u16>()) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(
                !device.name().is_empty(),
                "device name must not be empty for {:?}",
                device
            );
        }
    }
}
