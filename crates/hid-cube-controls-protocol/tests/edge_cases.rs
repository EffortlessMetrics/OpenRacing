//! Comprehensive edge-case, boundary-value, and property tests for
//! the Cube Controls protocol crate.
//!
//! Covers VID/PID constants, model classification, display names,
//! torque safety, provisional flags, and cross-function consistency.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Constant golden values
// ---------------------------------------------------------------------------

#[test]
fn vendor_id_golden() {
    assert_eq!(CUBE_CONTROLS_VENDOR_ID, 0x0483);
}

#[test]
fn pid_constants_golden() {
    assert_eq!(CUBE_CONTROLS_GT_PRO_PID, 0x0C73);
    assert_eq!(CUBE_CONTROLS_FORMULA_PRO_PID, 0x0C74);
    assert_eq!(CUBE_CONTROLS_CSX3_PID, 0x0C75);
}

#[test]
fn all_pids_non_zero_and_distinct() {
    let pids = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for &pid in &pids {
        assert_ne!(pid, 0, "PID must not be zero");
    }
    let mut sorted = pids.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), pids.len(), "all PIDs must be distinct");
}

#[test]
fn pids_are_consecutive() {
    assert_eq!(CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID + 1);
    assert_eq!(CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID + 1);
}

// ---------------------------------------------------------------------------
// Product recognition
// ---------------------------------------------------------------------------

#[test]
fn is_cube_controls_product_known_pids() {
    assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
}

#[test]
fn is_cube_controls_product_boundary_pids() {
    // Just outside the known PID range
    assert!(!is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID - 1));
    assert!(!is_cube_controls_product(CUBE_CONTROLS_CSX3_PID + 1));
}

#[test]
fn is_cube_controls_product_edge_values() {
    assert!(!is_cube_controls_product(0x0000));
    assert!(!is_cube_controls_product(0xFFFF));
    assert!(!is_cube_controls_product(0x0001));
    assert!(!is_cube_controls_product(CUBE_CONTROLS_VENDOR_ID)); // VID is not a PID
}

// ---------------------------------------------------------------------------
// Model classification
// ---------------------------------------------------------------------------

#[test]
fn model_from_product_id_known() {
    assert_eq!(
        CubeControlsModel::from_product_id(CUBE_CONTROLS_GT_PRO_PID),
        CubeControlsModel::GtPro
    );
    assert_eq!(
        CubeControlsModel::from_product_id(CUBE_CONTROLS_FORMULA_PRO_PID),
        CubeControlsModel::FormulaPro
    );
    assert_eq!(
        CubeControlsModel::from_product_id(CUBE_CONTROLS_CSX3_PID),
        CubeControlsModel::Csx3
    );
}

#[test]
fn model_from_product_id_unknown() {
    assert_eq!(
        CubeControlsModel::from_product_id(0x0000),
        CubeControlsModel::Unknown
    );
    assert_eq!(
        CubeControlsModel::from_product_id(0xFFFF),
        CubeControlsModel::Unknown
    );
    assert_eq!(
        CubeControlsModel::from_product_id(0x0001),
        CubeControlsModel::Unknown
    );
}

// ---------------------------------------------------------------------------
// Display names
// ---------------------------------------------------------------------------

#[test]
fn display_names_contain_brand() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
    ];
    for model in &models {
        assert!(
            model.display_name().starts_with("Cube Controls"),
            "{model:?} name must start with 'Cube Controls'"
        );
    }
}

#[test]
fn display_names_all_unique() {
    let all_models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    let mut names: Vec<&str> = all_models.iter().map(|m| m.display_name()).collect();
    let len_before = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), len_before, "all display names must be unique");
}

#[test]
fn display_names_all_non_empty() {
    let all_models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &all_models {
        assert!(
            !model.display_name().is_empty(),
            "{model:?} name must be non-empty"
        );
    }
}

// ---------------------------------------------------------------------------
// Torque safety – input-only devices must return 0.0
// ---------------------------------------------------------------------------

#[test]
fn all_models_torque_zero() {
    let all_models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &all_models {
        assert!(
            (model.max_torque_nm() - 0.0).abs() < f32::EPSILON,
            "{model:?} torque must be exactly 0.0 (input-only device)"
        );
    }
}

#[test]
fn torque_is_not_negative() {
    let all_models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &all_models {
        assert!(
            model.max_torque_nm() >= 0.0,
            "{model:?} torque must not be negative"
        );
    }
}

// ---------------------------------------------------------------------------
// Provisional status
// ---------------------------------------------------------------------------

#[test]
fn all_models_are_provisional() {
    let all_models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &all_models {
        assert!(model.is_provisional(), "{model:?} must be provisional");
    }
}

// ---------------------------------------------------------------------------
// Trait derivations
// ---------------------------------------------------------------------------

#[test]
fn model_debug_format_non_empty() {
    for model in [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ] {
        assert!(!format!("{model:?}").is_empty());
    }
}

#[test]
fn model_clone_and_copy() {
    let model = CubeControlsModel::GtPro;
    let cloned = model;
    assert_eq!(model, cloned);
}

#[test]
fn model_eq_reflexive() {
    let all_models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &all_models {
        assert_eq!(model, model);
    }
}

#[test]
fn model_variants_distinct() {
    assert_ne!(CubeControlsModel::GtPro, CubeControlsModel::FormulaPro);
    assert_ne!(CubeControlsModel::GtPro, CubeControlsModel::Csx3);
    assert_ne!(CubeControlsModel::GtPro, CubeControlsModel::Unknown);
    assert_ne!(CubeControlsModel::FormulaPro, CubeControlsModel::Csx3);
    assert_ne!(CubeControlsModel::FormulaPro, CubeControlsModel::Unknown);
    assert_ne!(CubeControlsModel::Csx3, CubeControlsModel::Unknown);
}

// ---------------------------------------------------------------------------
// Cross-function consistency
// ---------------------------------------------------------------------------

#[test]
fn from_product_id_agrees_with_is_cube_controls_product() {
    let pids = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for &pid in &pids {
        let model = CubeControlsModel::from_product_id(pid);
        assert_ne!(model, CubeControlsModel::Unknown);
        assert!(is_cube_controls_product(pid));
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// For any u16 PID, from_product_id returns non-Unknown iff is_cube_controls_product is true.
    #[test]
    fn prop_from_pid_consistent_with_is_product(pid in any::<u16>()) {
        let model = CubeControlsModel::from_product_id(pid);
        let recognised = is_cube_controls_product(pid);
        prop_assert_eq!(
            model != CubeControlsModel::Unknown,
            recognised,
            "from_product_id and is_cube_controls_product must agree for PID 0x{:04X}",
            pid
        );
    }

    /// from_product_id is deterministic.
    #[test]
    fn prop_from_product_id_deterministic(pid in any::<u16>()) {
        let a = CubeControlsModel::from_product_id(pid);
        let b = CubeControlsModel::from_product_id(pid);
        prop_assert_eq!(a, b);
    }

    /// display_name is deterministic and non-empty for any resolved model.
    #[test]
    fn prop_display_name_deterministic(pid in any::<u16>()) {
        let model = CubeControlsModel::from_product_id(pid);
        let n1 = model.display_name();
        let n2 = model.display_name();
        prop_assert_eq!(n1, n2);
        prop_assert!(!n1.is_empty());
    }

    /// max_torque_nm is always exactly 0.0 for any PID.
    #[test]
    fn prop_torque_always_zero(pid in any::<u16>()) {
        let model = CubeControlsModel::from_product_id(pid);
        prop_assert!((model.max_torque_nm() - 0.0).abs() < f32::EPSILON);
    }

    /// is_provisional is always true for any model.
    #[test]
    fn prop_always_provisional(pid in any::<u16>()) {
        let model = CubeControlsModel::from_product_id(pid);
        prop_assert!(model.is_provisional());
    }

    /// display_name contains no control characters.
    #[test]
    fn prop_display_name_no_control_chars(pid in any::<u16>()) {
        let model = CubeControlsModel::from_product_id(pid);
        for ch in model.display_name().chars() {
            prop_assert!(!ch.is_control(), "display_name must not contain control chars");
        }
    }
}
