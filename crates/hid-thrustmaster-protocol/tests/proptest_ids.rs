//! Property-based tests for Thrustmaster device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - Model classification determinism
//! - max_torque_nm is non-negative and finite for every model
//! - name() is non-empty for every model
//! - Consistency between Model and is_wheel_product / identify_device

use proptest::prelude::*;
use racing_wheel_hid_thrustmaster_protocol::{
    Model, THRUSTMASTER_VENDOR_ID, identify_device, is_wheel_product, product_ids,
};

/// All known Thrustmaster wheelbase product IDs (those reachable via from_product_id).
const WHEELBASE_PIDS: [u16; 13] = [
    product_ids::T150,
    product_ids::TMX,
    product_ids::T300_RS,
    product_ids::T300_RS_PS4,
    product_ids::T300_RS_GT,
    product_ids::TX_RACING,
    product_ids::T500_RS,
    product_ids::T248,
    product_ids::T248X,
    product_ids::TS_PC_RACER,
    product_ids::TS_XW,
    product_ids::TS_XW_GIP,
    product_ids::T818,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// THRUSTMASTER_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(THRUSTMASTER_VENDOR_ID != 0,
            "THRUSTMASTER_VENDOR_ID must not be zero");
    }

    /// Every known wheelbase PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..13usize) {
        let pid = WHEELBASE_PIDS[idx];
        prop_assert!(pid != 0,
            "wheelbase PID at index {idx} must not be zero");
    }

    /// Model::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = Model::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_known_model_torque_positive(idx in 0usize..13usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = Model::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
    }

    /// Model::name() must never be empty for any PID.
    #[test]
    fn prop_name_non_empty(pid: u16) {
        let model = Model::from_product_id(pid);
        prop_assert!(!model.name().is_empty(),
            "{model:?} must have a non-empty name");
    }

    /// Model::name() must contain "Thrustmaster" for all models.
    #[test]
    fn prop_name_contains_brand(pid: u16) {
        let model = Model::from_product_id(pid);
        prop_assert!(model.name().contains("Thrustmaster"),
            "{model:?} name must contain 'Thrustmaster', got '{}'", model.name());
    }

    /// Model::from_product_id must be deterministic for any PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = Model::from_product_id(pid);
        let b = Model::from_product_id(pid);
        prop_assert_eq!(a, b,
            "Model::from_product_id must be deterministic for pid={:#06x}", pid);
    }

    /// A recognised wheelbase PID must not resolve to Model::Unknown.
    #[test]
    fn prop_recognised_pid_not_unknown(idx in 0usize..13usize) {
        let pid = WHEELBASE_PIDS[idx];
        let model = Model::from_product_id(pid);
        prop_assert_ne!(model, Model::Unknown,
            "recognised PID {:#06x} must not resolve to Unknown", pid);
    }

    /// identify_device name must be non-empty for any PID.
    #[test]
    fn prop_identify_device_name_non_empty(pid: u16) {
        let identity = identify_device(pid);
        prop_assert!(!identity.name.is_empty(),
            "identify_device({pid:#06x}).name must not be empty");
    }

    /// identify_device must agree with is_wheel_product.
    #[test]
    fn prop_identify_device_consistent_with_is_wheel(pid: u16) {
        use racing_wheel_hid_thrustmaster_protocol::ThrustmasterDeviceCategory;
        let identity = identify_device(pid);
        let is_wheel = is_wheel_product(pid);
        let category_is_wheel = matches!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        prop_assert_eq!(is_wheel, category_is_wheel,
            "is_wheel_product and identify_device must agree for pid={:#06x}", pid);
    }
}
