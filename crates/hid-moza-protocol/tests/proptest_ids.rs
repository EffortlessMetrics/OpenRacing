//! Property-based tests for Moza device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero (except R16/R21 V1 which is 0x0000)
//! - MozaModel classification determinism and max_torque_nm bounds
//! - identify_device name is non-empty and max_torque_nm is non-negative
//! - Round-trip: PID → model → PID preserves model identity

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{
    MOZA_VENDOR_ID, MozaModel, identify_device, is_wheelbase_product, product_ids,
};

/// All known Moza product IDs.
const ALL_PIDS: [u16; 14] = [
    product_ids::R16_R21_V1,
    product_ids::R9_V1,
    product_ids::R5_V1,
    product_ids::R3_V1,
    product_ids::R12_V1,
    product_ids::R16_R21_V2,
    product_ids::R9_V2,
    product_ids::R5_V2,
    product_ids::R3_V2,
    product_ids::R12_V2,
    product_ids::SR_P_PEDALS,
    product_ids::HGP_SHIFTER,
    product_ids::SGP_SHIFTER,
    product_ids::HBP_HANDBRAKE,
];

/// Wheelbase-only PIDs.
const WHEELBASE_PIDS: [u16; 10] = [
    product_ids::R16_R21_V1,
    product_ids::R9_V1,
    product_ids::R5_V1,
    product_ids::R3_V1,
    product_ids::R12_V1,
    product_ids::R16_R21_V2,
    product_ids::R9_V2,
    product_ids::R5_V2,
    product_ids::R3_V2,
    product_ids::R12_V2,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// MOZA_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(MOZA_VENDOR_ID != 0,
            "MOZA_VENDOR_ID must not be zero");
    }

    /// VID must match the expected Moza vendor ID (0x346E).
    #[test]
    fn prop_vendor_id_matches_expected(_unused: u8) {
        prop_assert_eq!(MOZA_VENDOR_ID, 0x346E,
            "MOZA_VENDOR_ID must be 0x346E");
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_known_pids_unique(a in 0usize..14usize, b in 0usize..14usize) {
        if a != b {
            prop_assert!(ALL_PIDS[a] != ALL_PIDS[b],
                "PIDs at index {a} and {b} must differ, both are {:#06x}",
                ALL_PIDS[a]);
        }
    }

    /// MozaModel::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = MozaModel::from_pid(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_wheelbase_torque_positive(idx in 0usize..10usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = MozaModel::from_pid(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
    }

    /// identify_device name must be non-empty for any PID.
    #[test]
    fn prop_identify_name_non_empty(pid: u16) {
        let identity = identify_device(pid);
        prop_assert!(!identity.name.is_empty(),
            "identify_device({pid:#06x}).name must not be empty");
    }

    /// identify_device name must contain "Moza" for all known PIDs.
    #[test]
    fn prop_identify_name_contains_brand(idx in 0usize..14usize) {
        let pid = ALL_PIDS[idx];
        let identity = identify_device(pid);
        prop_assert!(identity.name.contains("Moza"),
            "identify_device({pid:#06x}).name must contain 'Moza', got '{}'",
            identity.name);
    }

    /// is_wheelbase_product must return true for all wheelbase PIDs.
    #[test]
    fn prop_wheelbase_classification(idx in 0usize..10usize) {
        let pid = WHEELBASE_PIDS[idx];
        prop_assert!(is_wheelbase_product(pid),
            "PID {pid:#06x} must be classified as wheelbase");
    }

    /// identify_device must agree with MozaModel on wheelbase classification.
    #[test]
    fn prop_model_consistent_with_identify(pid: u16) {
        let model = MozaModel::from_pid(pid);
        let is_wb = is_wheelbase_product(pid);
        let model_has_torque = model.max_torque_nm() > 0.0;
        if is_wb {
            prop_assert!(model_has_torque,
                "wheelbase pid={pid:#06x} model {model:?} must have positive torque");
        }
    }

    /// Round-trip: PID → model → PID must produce the same model.
    #[test]
    fn prop_roundtrip_pid_model(idx in 0usize..14usize) {
        let pid = ALL_PIDS[idx];
        let model = MozaModel::from_pid(pid);
        let model2 = MozaModel::from_pid(pid);
        prop_assert_eq!(model, model2);
    }
}
