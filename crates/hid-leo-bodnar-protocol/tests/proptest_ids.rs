//! Property-based tests for Leo Bodnar device identification and classification.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants and the three identification predicates
//! - LeoBodnarDevice type classification and metadata
//! - Consistency between predicate functions and the enum's own methods

use proptest::prelude::*;
use racing_wheel_hid_leo_bodnar_protocol::{
    LeoBodnarDevice, PID_BBI32, PID_BU0836_16BIT, PID_BU0836A, PID_BU0836X, PID_FFB_JOYSTICK,
    PID_SLI_M, PID_USB_JOYSTICK, PID_WHEEL_INTERFACE, VENDOR_ID, WHEEL_DEFAULT_MAX_TORQUE_NM,
    WHEEL_ENCODER_CPR, is_leo_bodnar, is_leo_bodnar_device, is_leo_bodnar_ffb_pid,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VENDOR_ID must always equal 0x1DD2 (Leo Bodnar Electronics Ltd).
    #[test]
    fn prop_vendor_id_constant_is_leo_bodnar(_unused: u8) {
        prop_assert_eq!(VENDOR_ID, 0x1DD2u16,
            "VENDOR_ID must always be 0x1DD2 (Leo Bodnar Electronics Ltd)");
    }

    /// is_leo_bodnar with the correct VID must agree with is_leo_bodnar_device for any PID.
    #[test]
    fn prop_is_leo_bodnar_with_vendor_id_agrees_with_device_check(pid: u16) {
        prop_assert_eq!(
            is_leo_bodnar(VENDOR_ID, pid),
            is_leo_bodnar_device(pid),
            "is_leo_bodnar(VENDOR_ID, {:#06x}) must equal is_leo_bodnar_device({:#06x})", pid, pid
        );
    }

    /// is_leo_bodnar with any VID other than VENDOR_ID must always return false.
    #[test]
    fn prop_wrong_vid_never_recognised(
        vid in any::<u16>().prop_filter("not Leo Bodnar VID", |v| *v != VENDOR_ID),
        pid: u16,
    ) {
        prop_assert!(!is_leo_bodnar(vid, pid),
            "VID {:#06x} must not be recognised as Leo Bodnar for any PID", vid);
    }

    /// All confirmed PIDs must be recognised by is_leo_bodnar_device.
    #[test]
    fn prop_all_known_pids_recognised(
        idx in 0usize..8usize,
    ) {
        let pids = [
            PID_USB_JOYSTICK, PID_BU0836A, PID_BBI32, PID_WHEEL_INTERFACE,
            PID_FFB_JOYSTICK, PID_BU0836X, PID_BU0836_16BIT, PID_SLI_M,
        ];
        prop_assert!(is_leo_bodnar_device(pids[idx]),
            "PID {:#06x} must be recognised as a Leo Bodnar device", pids[idx]);
    }

    /// LeoBodnarDevice::from_product_id returns Some iff is_leo_bodnar_device returns true.
    #[test]
    fn prop_from_pid_consistent_with_is_device(pid: u16) {
        prop_assert_eq!(
            LeoBodnarDevice::from_product_id(pid).is_some(),
            is_leo_bodnar_device(pid),
            "from_product_id and is_leo_bodnar_device must agree for PID {:#06x}", pid
        );
    }

    /// is_leo_bodnar_ffb_pid must be true only for WHEEL_INTERFACE and FFB_JOYSTICK.
    #[test]
    fn prop_ffb_pid_only_for_wheel_and_ffb_joystick(pid: u16) {
        let expected = pid == PID_WHEEL_INTERFACE || pid == PID_FFB_JOYSTICK;
        prop_assert_eq!(
            is_leo_bodnar_ffb_pid(pid), expected,
            "is_leo_bodnar_ffb_pid must be true only for WHEEL_INTERFACE and \
             FFB_JOYSTICK (pid={:#06x})", pid
        );
    }

    /// For a recognised device, supports_ffb() must agree with is_leo_bodnar_ffb_pid.
    #[test]
    fn prop_supports_ffb_consistent_with_ffb_pid(pid: u16) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert_eq!(
                device.supports_ffb(),
                is_leo_bodnar_ffb_pid(pid),
                "{:?} supports_ffb must agree with is_leo_bodnar_ffb_pid({:#06x})", device, pid
            );
        }
    }

    /// max_input_channels must never exceed 32 (USB HID standard button limit).
    #[test]
    fn prop_max_input_channels_within_hid_limit(pid: u16) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(
                device.max_input_channels() <= 32,
                "{device:?} reports {} channels, must be ≤ 32",
                device.max_input_channels()
            );
        }
    }

    /// LeoBodnarDevice::name must never be empty for any recognised PID.
    #[test]
    fn prop_device_name_non_empty(pid: u16) {
        if let Some(device) = LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(!device.name().is_empty(),
                "{device:?} must have a non-empty name");
        }
    }

    /// WHEEL_ENCODER_CPR must fit in a u16 axis range (≤ 65535).
    #[test]
    fn prop_encoder_cpr_within_u16_range(_unused: u8) {
        prop_assert!(
            WHEEL_ENCODER_CPR <= u32::from(u16::MAX),
            "WHEEL_ENCODER_CPR={WHEEL_ENCODER_CPR} must fit in a u16 axis range"
        );
    }

    /// WHEEL_DEFAULT_MAX_TORQUE_NM must always be positive and finite.
    #[test]
    fn prop_default_torque_positive_and_finite(_unused: u8) {
        prop_assert!(WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0,
            "WHEEL_DEFAULT_MAX_TORQUE_NM must be > 0.0, got {WHEEL_DEFAULT_MAX_TORQUE_NM}");
        prop_assert!(WHEEL_DEFAULT_MAX_TORQUE_NM.is_finite(),
            "WHEEL_DEFAULT_MAX_TORQUE_NM must be finite, got {WHEEL_DEFAULT_MAX_TORQUE_NM}");
    }
}
