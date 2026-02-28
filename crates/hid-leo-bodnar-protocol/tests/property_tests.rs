//! Property-based tests for the Leo Bodnar HID protocol.
//!
//! Uses proptest with 500 cases to verify invariants on device identification,
//! FFB classification, button channel counts, and axis constants.

use proptest::prelude::*;
use racing_wheel_hid_leo_bodnar_protocol as leo_bodnar;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// VID constant is always 0x1DD2, regardless of the PID argument.
    #[test]
    fn prop_vendor_id_constant_is_leo_bodnar(_pid in any::<u16>()) {
        prop_assert_eq!(
            leo_bodnar::VENDOR_ID,
            0x1DD2u16,
            "VENDOR_ID must always be 0x1DD2"
        );
    }

    /// is_leo_bodnar_ffb_pid returns true only for WHEEL_INTERFACE and FFB_JOYSTICK;
    /// no other PID must ever claim FFB capability.
    #[test]
    fn prop_ffb_pid_covers_only_known_ffb_devices(pid in any::<u16>()) {
        let is_ffb = leo_bodnar::is_leo_bodnar_ffb_pid(pid);
        let expected = pid == leo_bodnar::PID_WHEEL_INTERFACE
            || pid == leo_bodnar::PID_FFB_JOYSTICK;
        prop_assert_eq!(
            is_ffb,
            expected,
            "is_leo_bodnar_ffb_pid must be true only for WHEEL_INTERFACE \
             and FFB_JOYSTICK (pid={:#06x})",
            pid
        );
    }

    /// max_input_channels must not exceed 32 (USB HID joystick button limit)
    /// for any recognised device, regardless of which PID resolves it.
    #[test]
    fn prop_button_count_does_not_exceed_max(pid in any::<u16>()) {
        if let Some(device) = leo_bodnar::LeoBodnarDevice::from_product_id(pid) {
            prop_assert!(
                device.max_input_channels() <= 32,
                "{:?} reports {} channels, must be ≤ 32",
                device,
                device.max_input_channels()
            );
        }
    }

    /// Axis values are within the valid 16-bit range: WHEEL_ENCODER_CPR ≤ u16::MAX.
    /// Verified as an invariant on every proptest iteration to guard against
    /// accidental constant changes.
    #[test]
    fn prop_axis_encoder_cpr_within_u16_range(_x in any::<u8>()) {
        prop_assert!(
            leo_bodnar::WHEEL_ENCODER_CPR <= u32::from(u16::MAX),
            "WHEEL_ENCODER_CPR={} must fit in a u16 axis range",
            leo_bodnar::WHEEL_ENCODER_CPR
        );
    }

    /// WHEEL_DEFAULT_MAX_TORQUE_NM is always a positive, finite value.
    #[test]
    fn prop_default_torque_is_positive_and_finite(_x in any::<u8>()) {
        prop_assert!(
            leo_bodnar::WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0,
            "WHEEL_DEFAULT_MAX_TORQUE_NM must be > 0.0, got {}",
            leo_bodnar::WHEEL_DEFAULT_MAX_TORQUE_NM
        );
        prop_assert!(
            leo_bodnar::WHEEL_DEFAULT_MAX_TORQUE_NM.is_finite(),
            "WHEEL_DEFAULT_MAX_TORQUE_NM must be finite, got {}",
            leo_bodnar::WHEEL_DEFAULT_MAX_TORQUE_NM
        );
    }

    /// is_leo_bodnar with the wrong VID must never return true, regardless of PID.
    #[test]
    fn prop_wrong_vid_never_recognised(
        vid in any::<u16>().prop_filter("not Leo Bodnar VID", |v| *v != 0x1DD2),
        pid in any::<u16>(),
    ) {
        prop_assert!(
            !leo_bodnar::is_leo_bodnar(vid, pid),
            "VID {:#06x} must not be recognised as Leo Bodnar for any PID",
            vid
        );
    }

    /// is_leo_bodnar with the correct VID must agree with is_leo_bodnar_device.
    #[test]
    fn prop_is_leo_bodnar_with_correct_vid_agrees_with_device_check(pid in any::<u16>()) {
        prop_assert_eq!(
            leo_bodnar::is_leo_bodnar(leo_bodnar::VENDOR_ID, pid),
            leo_bodnar::is_leo_bodnar_device(pid),
            "is_leo_bodnar(VENDOR_ID, pid) must agree with is_leo_bodnar_device(pid) \
             for PID {:#06x}",
            pid
        );
    }
}
