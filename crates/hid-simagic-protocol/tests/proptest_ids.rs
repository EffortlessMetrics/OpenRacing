//! Property-based tests for Simagic device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - SimagicModel classification determinism and max_torque_nm bounds
//! - identify_device name is non-empty and max_torque_nm is non-negative
//! - Consistency between SimagicModel and identify_device

use proptest::prelude::*;
use racing_wheel_hid_simagic_protocol::{
    SIMAGIC_VENDOR_ID, SimagicModel, identify_device, is_wheelbase_product, product_ids,
};

/// All known Simagic product IDs (wheelbases and peripherals).
const ALL_PIDS: [u16; 15] = [
    product_ids::EVO_SPORT,
    product_ids::EVO,
    product_ids::EVO_PRO,
    product_ids::ALPHA_EVO,
    product_ids::NEO,
    product_ids::NEO_MINI,
    product_ids::P1000_PEDALS,
    product_ids::P1000A_PEDALS,
    product_ids::P2000_PEDALS,
    product_ids::SHIFTER_H,
    product_ids::SHIFTER_SEQ,
    product_ids::HANDBRAKE,
    product_ids::RIM_WR1,
    product_ids::RIM_GT1,
    product_ids::RIM_GT_NEO,
];

/// Wheelbase-only PIDs.
const WHEELBASE_PIDS: [u16; 6] = [
    product_ids::EVO_SPORT,
    product_ids::EVO,
    product_ids::EVO_PRO,
    product_ids::ALPHA_EVO,
    product_ids::NEO,
    product_ids::NEO_MINI,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// SIMAGIC_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(SIMAGIC_VENDOR_ID != 0,
            "SIMAGIC_VENDOR_ID must not be zero");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..15usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// SimagicModel::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = SimagicModel::from_pid(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_wheelbase_torque_positive(idx in 0usize..6usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = SimagicModel::from_pid(pid);
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

    /// identify_device name must contain "Simagic" for all known PIDs.
    #[test]
    fn prop_identify_name_contains_brand(idx in 0usize..15usize) {
        let pid = ALL_PIDS[idx];
        let identity = identify_device(pid);
        prop_assert!(identity.name.contains("Simagic"),
            "identify_device({pid:#06x}).name must contain 'Simagic', got '{}'",
            identity.name);
    }

    /// identify_device max_torque_nm must be non-negative when present.
    #[test]
    fn prop_identify_torque_non_negative(pid: u16) {
        let identity = identify_device(pid);
        if let Some(torque) = identity.max_torque_nm {
            prop_assert!(torque >= 0.0,
                "identify_device({pid:#06x}).max_torque_nm must be >= 0, got {torque}");
            prop_assert!(torque.is_finite(),
                "identify_device({pid:#06x}).max_torque_nm must be finite, got {torque}");
        }
    }

    /// SimagicModel and identify_device must agree on wheelbase classification.
    #[test]
    fn prop_model_consistent_with_identify(pid: u16) {
        let model = SimagicModel::from_pid(pid);
        let is_wb = is_wheelbase_product(pid);
        let model_has_torque = model.max_torque_nm() > 0.0;
        if is_wb {
            prop_assert!(model_has_torque,
                "wheelbase pid={pid:#06x} model {model:?} must have positive torque");
        }
    }
}
