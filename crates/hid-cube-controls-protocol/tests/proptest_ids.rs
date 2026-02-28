//! Property-based tests for Cube Controls device identification and classification.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants and device detection
//! - CubeControlsModel classification determinism and metadata consistency
//! - is_cube_controls_product correctness for arbitrary PIDs

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VENDOR_ID must always equal 0x0483 (STMicroelectronics shared VID).
    #[test]
    fn prop_vendor_id_constant_is_stm(_unused: u8) {
        prop_assert_eq!(CUBE_CONTROLS_VENDOR_ID, 0x0483u16,
            "CUBE_CONTROLS_VENDOR_ID must always be 0x0483 (STMicroelectronics)");
    }

    /// All three confirmed PIDs must always be recognised by is_cube_controls_product.
    #[test]
    fn prop_all_known_pids_recognised(idx in 0usize..3usize) {
        let pids = [
            CUBE_CONTROLS_GT_PRO_PID,
            CUBE_CONTROLS_FORMULA_PRO_PID,
            CUBE_CONTROLS_CSX3_PID,
        ];
        prop_assert!(is_cube_controls_product(pids[idx]),
            "PID {:#06x} must be recognised as a Cube Controls product", pids[idx]);
    }

    /// is_cube_controls_product must return true only for the three known PIDs.
    #[test]
    fn prop_is_cube_controls_product_only_known_pids(pid: u16) {
        let expected = pid == CUBE_CONTROLS_GT_PRO_PID
            || pid == CUBE_CONTROLS_FORMULA_PRO_PID
            || pid == CUBE_CONTROLS_CSX3_PID;
        prop_assert_eq!(
            is_cube_controls_product(pid),
            expected,
            "is_cube_controls_product must be true only for known PIDs (pid={:#06x})", pid
        );
    }

    /// CubeControlsModel::from_product_id must be deterministic for any PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = CubeControlsModel::from_product_id(pid);
        let b = CubeControlsModel::from_product_id(pid);
        prop_assert_eq!(a, b,
            "CubeControlsModel::from_product_id must be deterministic for pid={:#06x}", pid);
    }

    /// A recognised PID must not resolve to CubeControlsModel::Unknown.
    #[test]
    fn prop_recognised_pid_not_unknown(pid: u16) {
        if is_cube_controls_product(pid) {
            prop_assert_ne!(CubeControlsModel::from_product_id(pid), CubeControlsModel::Unknown,
                "recognised PID {:#06x} must not resolve to Unknown", pid);
        }
    }

    /// An unrecognised PID must always resolve to CubeControlsModel::Unknown.
    #[test]
    fn prop_unrecognised_pid_resolves_to_unknown(pid: u16) {
        if !is_cube_controls_product(pid) {
            prop_assert_eq!(CubeControlsModel::from_product_id(pid), CubeControlsModel::Unknown,
                "unrecognised PID {:#06x} must resolve to Unknown", pid);
        }
    }

    /// CubeControlsModel::from_product_id and is_cube_controls_product must agree:
    /// product is recognised iff model is not Unknown.
    #[test]
    fn prop_from_pid_consistent_with_is_product(pid: u16) {
        let is_known = is_cube_controls_product(pid);
        let model = CubeControlsModel::from_product_id(pid);
        prop_assert_eq!(
            is_known,
            model != CubeControlsModel::Unknown,
            "is_cube_controls_product and from_product_id must agree for pid={:#06x}", pid
        );
    }

    /// CubeControlsModel::max_torque_nm must always be strictly positive and finite.
    #[test]
    fn prop_max_torque_positive_and_finite(pid: u16) {
        let model = CubeControlsModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} must have positive max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// CubeControlsModel::display_name must never be empty for any PID.
    #[test]
    fn prop_display_name_non_empty(pid: u16) {
        let model = CubeControlsModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty(),
            "{model:?} must have a non-empty display_name");
    }

    /// All models must currently report as provisional.
    #[test]
    fn prop_all_models_provisional(pid: u16) {
        let model = CubeControlsModel::from_product_id(pid);
        prop_assert!(model.is_provisional(),
            "{model:?} must be provisional while VID/PIDs are unconfirmed");
    }

    /// display_name must contain "Cube Controls" for all models.
    #[test]
    fn prop_display_name_contains_brand(pid: u16) {
        let model = CubeControlsModel::from_product_id(pid);
        prop_assert!(model.display_name().contains("Cube Controls"),
            "{model:?} display_name must contain 'Cube Controls', got '{}'",
            model.display_name());
    }

    /// max_torque_nm for known models must be exactly 20.0 Nm (current rating).
    #[test]
    fn prop_known_models_rated_at_20nm(idx in 0usize..3usize) {
        let pids = [
            CUBE_CONTROLS_GT_PRO_PID,
            CUBE_CONTROLS_FORMULA_PRO_PID,
            CUBE_CONTROLS_CSX3_PID,
        ];
        let model = CubeControlsModel::from_product_id(pids[idx]);
        let torque = model.max_torque_nm();
        prop_assert!(
            (torque - 20.0).abs() < f32::EPSILON,
            "{model:?} must be rated at 20.0 Nm, got {torque}"
        );
    }
}
