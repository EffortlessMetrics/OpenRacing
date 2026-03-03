//! Property-based tests for Cammus device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - All known PIDs are unique (no duplicates)
//! - CammusModel classification and max_torque_nm bounds
//! - Round-trip: PID → model → name is consistent

use proptest::prelude::*;
use racing_wheel_hid_cammus_protocol::{
    CammusModel, PRODUCT_C5, PRODUCT_C12, PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, VENDOR_ID,
    is_cammus, product_name,
};

/// All known Cammus product IDs.
const ALL_PIDS: [u16; 4] = [
    PRODUCT_C5,
    PRODUCT_C12,
    PRODUCT_CP5_PEDALS,
    PRODUCT_LC100_PEDALS,
];

/// Wheelbase-only PIDs.
const WHEELBASE_PIDS: [u16; 2] = [PRODUCT_C5, PRODUCT_C12];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(VENDOR_ID != 0,
            "VENDOR_ID must not be zero");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_pids_unique(idx_a in 0usize..4usize, idx_b in 0usize..4usize) {
        if idx_a != idx_b {
            prop_assert!(ALL_PIDS[idx_a] != ALL_PIDS[idx_b],
                "PIDs at index {idx_a} and {idx_b} must differ, both are {:#06x}",
                ALL_PIDS[idx_a]);
        }
    }

    /// VENDOR_ID must match expected Cammus vendor value (0x3416).
    #[test]
    fn prop_vendor_id_value(_unused: u8) {
        prop_assert_eq!(VENDOR_ID, 0x3416,
            "VENDOR_ID must be 0x3416");
    }

    /// CammusModel::from_pid must return Some for all known PIDs.
    #[test]
    fn prop_known_pid_resolves_to_model(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        let model = CammusModel::from_pid(pid);
        prop_assert!(model.is_some(),
            "CammusModel::from_pid({pid:#06x}) must return Some");
    }

    /// CammusModel::max_torque_nm must be non-negative and finite for known models.
    #[test]
    fn prop_torque_non_negative_and_finite(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        if let Some(model) = CammusModel::from_pid(pid) {
            let torque = model.max_torque_nm();
            prop_assert!(torque >= 0.0,
                "{model:?} must have non-negative max_torque_nm, got {torque}");
            prop_assert!(torque.is_finite(),
                "{model:?} must have finite max_torque_nm, got {torque}");
        }
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_wheelbase_torque_positive(idx in 0usize..2usize) {
        let pid = WHEELBASE_PIDS[idx];
        if let Some(model) = CammusModel::from_pid(pid) {
            let torque = model.max_torque_nm();
            prop_assert!(torque > 0.0,
                "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
        }
    }

    /// product_name must return Some for all known PIDs.
    #[test]
    fn prop_product_name_known(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        let name = product_name(pid);
        prop_assert!(name.is_some(),
            "product_name({pid:#06x}) must return Some");
    }

    /// product_name must contain "Cammus" for all known PIDs.
    #[test]
    fn prop_product_name_contains_brand(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        if let Some(name) = product_name(pid) {
            prop_assert!(name.contains("Cammus"),
                "product_name({pid:#06x}) must contain 'Cammus', got '{name}'");
        }
    }

    /// is_cammus must return true for all known PIDs with the correct VID.
    #[test]
    fn prop_is_cammus_known_devices(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(is_cammus(VENDOR_ID, pid),
            "is_cammus(VENDOR_ID, {pid:#06x}) must return true");
    }

    /// is_cammus must return false for wrong VID.
    #[test]
    fn prop_is_cammus_wrong_vid(vid in 0u16..=u16::MAX) {
        prop_assume!(vid != VENDOR_ID);
        for &pid in &ALL_PIDS {
            prop_assert!(!is_cammus(vid, pid),
                "is_cammus({vid:#06x}, {pid:#06x}) must return false for wrong VID");
        }
    }

    /// Round-trip: PID → model → name must be non-empty for known PIDs.
    #[test]
    fn prop_roundtrip_pid_model_name(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        if let Some(model) = CammusModel::from_pid(pid) {
            let name = model.name();
            prop_assert!(!name.is_empty(),
                "CammusModel name for PID {pid:#06x} must not be empty");
        }
    }
}
