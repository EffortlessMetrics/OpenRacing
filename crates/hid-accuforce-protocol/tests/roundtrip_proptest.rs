//! Roundtrip property-based tests for the AccuForce HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - AccuForceModel from_product_id roundtrip
//! - DeviceInfo from_vid_pid roundtrip
//! - Known PID classification
//! - Display name and torque invariants
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, PID_ACCUFORCE_PRO, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// ── Model identification roundtrip ──────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// from_product_id is deterministic: calling it twice yields the same model.
    #[test]
    fn prop_model_deterministic(pid: u16) {
        let m1 = AccuForceModel::from_product_id(pid);
        let m2 = AccuForceModel::from_product_id(pid);
        prop_assert_eq!(m1, m2);
    }

    /// Known PID must yield Pro model; unknown PIDs must yield Unknown.
    #[test]
    fn prop_model_known_vs_unknown(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        if pid == PID_ACCUFORCE_PRO {
            prop_assert_eq!(model, AccuForceModel::Pro,
                "PID_ACCUFORCE_PRO must map to Pro");
        } else {
            prop_assert_eq!(model, AccuForceModel::Unknown,
                "unknown PID must map to Unknown");
        }
    }

    /// display_name must never be empty for any model.
    #[test]
    fn prop_display_name_nonempty(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty(),
            "display_name for {:?} must not be empty", model);
    }

    /// max_torque_nm must always be positive for any model.
    #[test]
    fn prop_max_torque_positive(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(model.max_torque_nm() > 0.0,
            "max_torque_nm for {:?} must be > 0, got {}", model, model.max_torque_nm());
    }
}

// ── DeviceInfo roundtrip ────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// DeviceInfo from_vid_pid must preserve vid and pid fields.
    #[test]
    fn prop_device_info_vid_pid_preserved(vid: u16, pid: u16) {
        let info = DeviceInfo::from_vid_pid(vid, pid);
        prop_assert_eq!(info.vendor_id, vid);
        prop_assert_eq!(info.product_id, pid);
    }

    /// DeviceInfo model must match AccuForceModel::from_product_id for the same PID.
    #[test]
    fn prop_device_info_model_consistent(vid: u16, pid: u16) {
        let info = DeviceInfo::from_vid_pid(vid, pid);
        let direct_model = AccuForceModel::from_product_id(pid);
        prop_assert_eq!(info.model, direct_model,
            "DeviceInfo model must match direct from_product_id");
    }

    /// is_accuforce must return true only for correct VID+PID combination.
    #[test]
    fn prop_is_accuforce_correct(vid: u16, pid: u16) {
        let expected = vid == VENDOR_ID && pid == PID_ACCUFORCE_PRO;
        prop_assert_eq!(is_accuforce(vid, pid), expected);
    }

    /// is_accuforce_pid must return true only for known PIDs.
    #[test]
    fn prop_is_accuforce_pid_correct(pid: u16) {
        let expected = pid == PID_ACCUFORCE_PRO;
        prop_assert_eq!(is_accuforce_pid(pid), expected);
    }
}

// ── Boundary tests ──────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Edge-case PIDs (0, u16::MAX, VENDOR_ID as PID) must not panic.
    #[test]
    fn prop_edge_pids_no_panic(
        pid in prop_oneof![
            Just(0u16),
            Just(u16::MAX),
            Just(VENDOR_ID),
            Just(PID_ACCUFORCE_PRO),
            Just(PID_ACCUFORCE_PRO.wrapping_add(1)),
            Just(PID_ACCUFORCE_PRO.wrapping_sub(1)),
            any::<u16>(),
        ]
    ) {
        let _ = AccuForceModel::from_product_id(pid);
        let _ = DeviceInfo::from_vid_pid(VENDOR_ID, pid);
        let _ = is_accuforce(VENDOR_ID, pid);
        let _ = is_accuforce_pid(pid);
    }
}
