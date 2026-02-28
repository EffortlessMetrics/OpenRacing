//! Property-based tests for Fanatec device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - Model classification determinism and consistency with is_wheelbase_product
//! - max_torque_nm is non-negative and finite for every model
//! - encoder_cpr is always positive

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{
    FANATEC_VENDOR_ID, FanatecModel, is_wheelbase_product, product_ids,
};

/// All known Fanatec wheelbase product IDs.
const WHEELBASE_PIDS: [u16; 10] = [
    product_ids::CLUBSPORT_V2,
    product_ids::CLUBSPORT_V2_5,
    product_ids::CSL_ELITE_PS4,
    product_ids::CSL_ELITE,
    product_ids::DD1,
    product_ids::DD2,
    product_ids::CSR_ELITE,
    product_ids::CSL_DD,
    product_ids::GT_DD_PRO,
    product_ids::CLUBSPORT_DD,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// FANATEC_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(FANATEC_VENDOR_ID != 0,
            "FANATEC_VENDOR_ID must not be zero");
    }

    /// Every known wheelbase PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..10usize) {
        let pid = WHEELBASE_PIDS[idx];
        prop_assert!(pid != 0,
            "wheelbase PID at index {idx} must not be zero");
    }

    /// FanatecModel::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = FanatecModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_known_model_torque_positive(idx in 0usize..10usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = FanatecModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
    }

    /// FanatecModel::encoder_cpr must always be positive for any PID.
    #[test]
    fn prop_encoder_cpr_positive(pid: u16) {
        let model = FanatecModel::from_product_id(pid);
        let cpr = model.encoder_cpr();
        prop_assert!(cpr > 0,
            "{model:?} must have positive encoder_cpr, got {cpr}");
    }

    /// FanatecModel::from_product_id must be deterministic for any PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = FanatecModel::from_product_id(pid);
        let b = FanatecModel::from_product_id(pid);
        prop_assert_eq!(a, b,
            "FanatecModel::from_product_id must be deterministic for pid={:#06x}", pid);
    }

    /// A recognised PID must not resolve to FanatecModel::Unknown.
    #[test]
    fn prop_recognised_pid_not_unknown(idx in 0usize..10usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = FanatecModel::from_product_id(pid);
        prop_assert_ne!(model, FanatecModel::Unknown,
            "recognised PID {:#06x} must not resolve to Unknown", pid);
    }

    /// FanatecModel classification must be consistent with is_wheelbase_product.
    #[test]
    fn prop_model_consistent_with_is_wheelbase(pid: u16) {
        let model = FanatecModel::from_product_id(pid);
        let is_wb = is_wheelbase_product(pid);
        if is_wb {
            prop_assert_ne!(model, FanatecModel::Unknown,
                "is_wheelbase_product({:#06x})=true but model is Unknown", pid);
        }
    }
}
