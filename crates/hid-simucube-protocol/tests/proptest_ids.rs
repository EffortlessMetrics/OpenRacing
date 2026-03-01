//! Property-based tests for Simucube device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - SimucubeModel max_torque_nm is non-negative and finite
//! - SimucubeModel display_name is non-empty
//! - Model round-trip: from_product_id always yields consistent metadata

use hid_simucube_protocol::{
    SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_1_PID, SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_PRO_PID,
    SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID,
    SIMUCUBE_WIRELESS_WHEEL_PID, SimucubeModel,
};
use proptest::prelude::*;

/// All known Simucube product IDs (normal operation).
const ALL_PIDS: [u16; 6] = [
    SIMUCUBE_1_PID,
    SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_PRO_PID,
    SIMUCUBE_2_ULTIMATE_PID,
    SIMUCUBE_ACTIVE_PEDAL_PID,
    SIMUCUBE_WIRELESS_WHEEL_PID,
];

/// Bootloader/firmware-upgrade PIDs (not used for HID protocol).
const BOOTLOADER_PIDS: [u16; 2] = [SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_2_BOOTLOADER_PID];

/// Wheelbase-only PIDs (those with positive torque).
const WHEELBASE_PIDS: [u16; 4] = [
    SIMUCUBE_1_PID,
    SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_PRO_PID,
    SIMUCUBE_2_ULTIMATE_PID,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// SIMUCUBE_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(SIMUCUBE_VENDOR_ID != 0,
            "SIMUCUBE_VENDOR_ID must not be zero");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..6usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// SimucubeModel::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = SimucubeModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_wheelbase_torque_positive(idx in 0usize..4usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = SimucubeModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
    }

    /// SimucubeModel::display_name must never be empty for any PID.
    #[test]
    fn prop_display_name_non_empty(pid: u16) {
        let model = SimucubeModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty(),
            "{model:?} must have a non-empty display_name");
    }

    /// display_name must contain "Simucube" or "SimuCube" for all known models.
    #[test]
    fn prop_display_name_contains_brand(idx in 0usize..6usize) {
        let pid = ALL_PIDS[idx];
        let model = SimucubeModel::from_product_id(pid);
        let name = model.display_name();
        let has_brand = name.contains("Simucube") || name.contains("SimuCube");
        prop_assert!(has_brand,
            "{model:?} display_name must contain 'Simucube' or 'SimuCube', got '{name}'");
    }

    /// Model round-trip: from_product_id yields consistent display_name and torque.
    #[test]
    fn prop_model_roundtrip_consistent(pid: u16) {
        let m1 = SimucubeModel::from_product_id(pid);
        let m2 = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(m1.display_name(), m2.display_name(),
            "display_name must be consistent for pid={:#06x}", pid);
        prop_assert!(
            (m1.max_torque_nm() - m2.max_torque_nm()).abs() < f32::EPSILON,
            "max_torque_nm must be consistent for pid={:#06x}", pid
        );
    }

    /// Bootloader PIDs must be non-zero and distinct from normal PIDs.
    #[test]
    fn prop_bootloader_pids_nonzero(idx in 0usize..2usize) {
        let pid = BOOTLOADER_PIDS[idx];
        prop_assert!(pid != 0, "bootloader PID at index {idx} must not be zero");
        // Bootloader PIDs must not appear in the normal PID list.
        prop_assert!(
            !ALL_PIDS.contains(&pid),
            "bootloader PID {pid:#06x} must not overlap with normal PIDs"
        );
    }

    /// Bootloader PIDs must resolve to Unknown model (they are not wheelbases).
    #[test]
    fn prop_bootloader_pids_are_unknown(idx in 0usize..2usize) {
        let pid = BOOTLOADER_PIDS[idx];
        let model = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(model, SimucubeModel::Unknown);
    }
}
