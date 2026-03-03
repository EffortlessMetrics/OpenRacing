//! Property tests for the AccuForce HID protocol.
//!
//! Verifies invariants across a wide range of inputs using `proptest`.

use proptest::prelude::*;
use racing_wheel_hid_accuforce_protocol as accuforce;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// is_accuforce returns true for the official VID + known PIDs.
    #[test]
    fn prop_is_accuforce_correct(pid in prop_oneof![
        Just(accuforce::PID_ACCUFORCE_PRO),
    ]) {
        prop_assert!(accuforce::is_accuforce(accuforce::VENDOR_ID, pid));
    }

    /// is_accuforce returns false for any non-AccuForce VID.
    #[test]
    fn prop_is_accuforce_wrong_vid(vid in 0u16..=u16::MAX, pid in 0u16..=u16::MAX) {
        if vid != accuforce::VENDOR_ID {
            prop_assert!(
                !accuforce::is_accuforce(vid, pid),
                "non-AccuForce VID {vid:#06X} must not be recognised"
            );
        }
    }

    /// is_accuforce returns false for unknown PIDs even with the correct VID.
    #[test]
    fn prop_is_accuforce_unknown_pid(pid in 0u16..=u16::MAX) {
        if pid != accuforce::PID_ACCUFORCE_PRO {
            prop_assert!(!accuforce::is_accuforce(accuforce::VENDOR_ID, pid));
        }
    }

    /// AccuForceModel::from_product_id returns a known variant for known PIDs.
    #[test]
    fn prop_model_from_known_pid(pid in prop_oneof![
        Just(accuforce::PID_ACCUFORCE_PRO),
    ]) {
        let model = accuforce::AccuForceModel::from_product_id(pid);
        prop_assert_ne!(model, accuforce::AccuForceModel::Unknown);
    }

    /// AccuForceModel::from_product_id returns Unknown for unrecognised PIDs.
    #[test]
    fn prop_model_from_unknown_pid(pid in 0u16..=u16::MAX) {
        if pid != accuforce::PID_ACCUFORCE_PRO {
            prop_assert_eq!(
                accuforce::AccuForceModel::from_product_id(pid),
                accuforce::AccuForceModel::Unknown
            );
        }
    }

    /// max_torque_nm is positive and finite for all known models.
    #[test]
    fn prop_model_max_torque_positive(pid in prop_oneof![
        Just(accuforce::PID_ACCUFORCE_PRO),
    ]) {
        let model = accuforce::AccuForceModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0, "torque must be positive, got {torque}");
        prop_assert!(torque.is_finite(), "torque must be finite, got {torque}");
    }

    /// display_name is non-empty for all known models.
    #[test]
    fn prop_model_name_non_empty(pid in prop_oneof![
        Just(accuforce::PID_ACCUFORCE_PRO),
    ]) {
        let model = accuforce::AccuForceModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty());
    }

    /// display_name for known models must contain "AccuForce".
    #[test]
    fn prop_known_model_name_contains_accuforce(pid in prop_oneof![
        Just(accuforce::PID_ACCUFORCE_PRO),
    ]) {
        let model = accuforce::AccuForceModel::from_product_id(pid);
        prop_assert!(
            model.display_name().contains("AccuForce"),
            "display_name {:?} must contain 'AccuForce'", model.display_name()
        );
    }

    /// is_accuforce_pid agrees with is_accuforce when using the correct VID.
    #[test]
    fn prop_is_accuforce_pid_agrees(pid in 0u16..=u16::MAX) {
        prop_assert_eq!(
            accuforce::is_accuforce(accuforce::VENDOR_ID, pid),
            accuforce::is_accuforce_pid(pid),
            "is_accuforce(VENDOR_ID, {:#06X}) must equal is_accuforce_pid({:#06X})",
            pid, pid
        );
    }

    /// DeviceInfo preserves VID and PID for any input pair.
    #[test]
    fn prop_device_info_preserves_vid_pid(vid in 0u16..=u16::MAX, pid in 0u16..=u16::MAX) {
        let info = accuforce::DeviceInfo::from_vid_pid(vid, pid);
        prop_assert_eq!(info.vendor_id, vid);
        prop_assert_eq!(info.product_id, pid);
    }

    /// DeviceInfo model agrees with AccuForceModel::from_product_id.
    #[test]
    fn prop_device_info_model_consistent(vid in 0u16..=u16::MAX, pid in 0u16..=u16::MAX) {
        let info = accuforce::DeviceInfo::from_vid_pid(vid, pid);
        let expected = accuforce::AccuForceModel::from_product_id(pid);
        prop_assert_eq!(info.model, expected);
    }

    /// MAX_REPORT_BYTES must not exceed the USB full-speed HID limit (64 bytes).
    #[test]
    fn prop_max_report_within_usb_limit(_unused: u8) {
        prop_assert!(accuforce::MAX_REPORT_BYTES <= 64);
    }

    /// RECOMMENDED_B_INTERVAL_MS must be positive.
    #[test]
    fn prop_recommended_interval_positive(_unused: u8) {
        prop_assert!(accuforce::RECOMMENDED_B_INTERVAL_MS > 0);
    }

    /// HID_PID_USAGE_PAGE must be 0x000F (Physical Interface Device).
    #[test]
    fn prop_hid_pid_usage_page_value(_unused: u8) {
        prop_assert_eq!(accuforce::HID_PID_USAGE_PAGE, 0x000Fu16);
    }
}
