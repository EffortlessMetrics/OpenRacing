//! Property-based tests for Logitech device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - Model classification determinism
//! - max_torque_nm is non-negative and finite for every model
//! - max_rotation_deg is positive for every model
//! - Consistency between LogitechModel and is_wheel_product

use proptest::prelude::*;
use racing_wheel_hid_logitech_protocol::{
    LOGITECH_VENDOR_ID, LogitechModel, is_wheel_product, product_ids,
};

/// All known Logitech wheel product IDs.
const WHEEL_PIDS: [u16; 17] = [
    product_ids::MOMO,
    product_ids::MOMO_2,
    product_ids::WINGMAN_FORMULA_FORCE_GP,
    product_ids::VIBRATION_WHEEL,
    product_ids::DRIVING_FORCE_EX,
    product_ids::DRIVING_FORCE_PRO,
    product_ids::DRIVING_FORCE_GT,
    product_ids::SPEED_FORCE_WIRELESS,
    product_ids::G25,
    product_ids::G27,
    product_ids::G29_PS,
    product_ids::G920,
    product_ids::G923,
    product_ids::G923_PS,
    product_ids::G923_XBOX,
    product_ids::G_PRO,
    product_ids::G_PRO_XBOX,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// LOGITECH_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(LOGITECH_VENDOR_ID != 0,
            "LOGITECH_VENDOR_ID must not be zero");
    }

    /// Every known wheel PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..17usize) {
        let pid = WHEEL_PIDS[idx];
        prop_assert!(pid != 0,
            "wheel PID at index {idx} must not be zero");
    }

    /// LogitechModel::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = LogitechModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheel models must have strictly positive torque.
    #[test]
    fn prop_known_model_torque_positive(idx in 0usize..17usize) {
        let pid = WHEEL_PIDS[idx];
        let model = LogitechModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
    }

    /// LogitechModel::max_rotation_deg must be positive for any PID.
    #[test]
    fn prop_max_rotation_positive(pid: u16) {
        let model = LogitechModel::from_product_id(pid);
        let deg = model.max_rotation_deg();
        prop_assert!(deg > 0,
            "{model:?} must have positive max_rotation_deg, got {deg}");
    }

    /// LogitechModel::from_product_id must be deterministic for any PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = LogitechModel::from_product_id(pid);
        let b = LogitechModel::from_product_id(pid);
        prop_assert_eq!(a, b,
            "LogitechModel::from_product_id must be deterministic for pid={:#06x}", pid);
    }

    /// A recognised PID must not resolve to LogitechModel::Unknown.
    #[test]
    fn prop_recognised_pid_not_unknown(idx in 0usize..17usize) {
        let pid = WHEEL_PIDS[idx];
        let model = LogitechModel::from_product_id(pid);
        prop_assert_ne!(model, LogitechModel::Unknown,
            "recognised PID {:#06x} must not resolve to Unknown", pid);
    }

    /// LogitechModel classification must be consistent with is_wheel_product.
    #[test]
    fn prop_model_consistent_with_is_wheel(pid: u16) {
        let is_known = is_wheel_product(pid);
        let model = LogitechModel::from_product_id(pid);
        prop_assert_eq!(
            is_known,
            model != LogitechModel::Unknown,
            "is_wheel_product and from_product_id must agree for pid={:#06x}", pid
        );
    }
}
