//! Property-based tests for AccuForce device identification and classification.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants and device detection functions
//! - Model classification determinism and metadata consistency
//! - DeviceInfo construction preserves VID/PID and agrees with AccuForceModel

use proptest::prelude::*;
use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, PID_ACCUFORCE_PRO, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VENDOR_ID must always equal 0x1FC9 (NXP Semiconductors).
    #[test]
    fn prop_vendor_id_constant_is_nxp(_unused: u8) {
        prop_assert_eq!(VENDOR_ID, 0x1FC9u16,
            "VENDOR_ID must always be 0x1FC9 (NXP Semiconductors)");
    }

    /// is_accuforce with the correct VID must agree with is_accuforce_pid for any PID.
    #[test]
    fn prop_is_accuforce_with_vendor_id_agrees_with_pid_check(pid: u16) {
        prop_assert_eq!(
            is_accuforce(VENDOR_ID, pid),
            is_accuforce_pid(pid),
            "is_accuforce(VENDOR_ID, {:#06x}) must equal is_accuforce_pid({:#06x})", pid, pid
        );
    }

    /// is_accuforce with any VID other than VENDOR_ID must always return false.
    #[test]
    fn prop_wrong_vid_never_recognised(
        vid in any::<u16>().prop_filter("not AccuForce VID", |v| *v != VENDOR_ID),
        pid: u16,
    ) {
        prop_assert!(!is_accuforce(vid, pid),
            "VID {vid:#06x} must not be recognised as AccuForce for any PID");
    }

    /// is_accuforce_pid(PID_ACCUFORCE_PRO) must always be true.
    #[test]
    fn prop_known_pid_always_recognised(_unused: u8) {
        prop_assert!(is_accuforce_pid(PID_ACCUFORCE_PRO),
            "PID_ACCUFORCE_PRO ({PID_ACCUFORCE_PRO:#06x}) must always be recognised");
    }

    /// AccuForceModel::from_product_id must be deterministic for any PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = AccuForceModel::from_product_id(pid);
        let b = AccuForceModel::from_product_id(pid);
        prop_assert_eq!(a, b,
            "AccuForceModel::from_product_id must be deterministic for pid={:#06x}", pid);
    }

    /// A recognised PID must not resolve to AccuForceModel::Unknown.
    #[test]
    fn prop_recognised_pid_not_unknown(pid: u16) {
        if is_accuforce_pid(pid) {
            prop_assert_ne!(AccuForceModel::from_product_id(pid), AccuForceModel::Unknown,
                "recognised PID {:#06x} must not resolve to Unknown", pid);
        }
    }

    /// An unrecognised PID must always resolve to AccuForceModel::Unknown.
    #[test]
    fn prop_unrecognised_pid_resolves_to_unknown(pid: u16) {
        if !is_accuforce_pid(pid) {
            prop_assert_eq!(AccuForceModel::from_product_id(pid), AccuForceModel::Unknown,
                "unrecognised PID {:#06x} must resolve to Unknown", pid);
        }
    }

    /// AccuForceModel::max_torque_nm must always be strictly positive and finite.
    #[test]
    fn prop_max_torque_positive_and_finite(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} must have positive max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// AccuForceModel::display_name must never be empty for any PID.
    #[test]
    fn prop_display_name_non_empty(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty(),
            "{model:?} must have a non-empty display_name");
    }

    /// DeviceInfo::from_vid_pid must preserve both the VID and PID exactly.
    #[test]
    fn prop_device_info_preserves_vid_pid(vid: u16, pid: u16) {
        let info = DeviceInfo::from_vid_pid(vid, pid);
        prop_assert_eq!(info.vendor_id, vid,
            "DeviceInfo must preserve vendor_id={:#06x}", vid);
        prop_assert_eq!(info.product_id, pid,
            "DeviceInfo must preserve product_id={:#06x}", pid);
    }

    /// DeviceInfo model must agree with AccuForceModel::from_product_id for the same PID.
    #[test]
    fn prop_device_info_model_agrees_with_from_pid(vid: u16, pid: u16) {
        let info = DeviceInfo::from_vid_pid(vid, pid);
        let expected = AccuForceModel::from_product_id(pid);
        prop_assert_eq!(info.model, expected,
            "DeviceInfo model must agree with AccuForceModel::from_product_id \
             for pid={:#06x}", pid);
    }
}
