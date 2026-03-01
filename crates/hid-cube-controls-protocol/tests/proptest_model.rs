//! Property-based tests for CubeControlsModel method invariants.
//!
//! Complements proptest_ids.rs by testing model-level property combinations
//! and cross-method consistency that go beyond individual ID recognition.

use hid_cube_controls_protocol::CubeControlsModel;
use proptest::prelude::*;

/// Strategy producing all four CubeControlsModel variants.
fn arb_model() -> impl Strategy<Value = CubeControlsModel> {
    prop_oneof![
        Just(CubeControlsModel::GtPro),
        Just(CubeControlsModel::FormulaPro),
        Just(CubeControlsModel::Csx3),
        Just(CubeControlsModel::Unknown),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Display names for distinct known models must differ.
    #[test]
    fn prop_known_model_display_names_unique(
        a in arb_model(),
        b in arb_model(),
    ) {
        if a != b && a != CubeControlsModel::Unknown && b != CubeControlsModel::Unknown {
            prop_assert_ne!(
                a.display_name(),
                b.display_name(),
                "distinct known models must have distinct display names: {:?} vs {:?}",
                a, b
            );
        }
    }

    /// max_torque_nm must be exactly 0.0 for every variant (input-only devices).
    #[test]
    fn prop_torque_exactly_zero_all_variants(model in arb_model()) {
        let torque = model.max_torque_nm();
        prop_assert!(
            (torque - 0.0).abs() < f32::EPSILON,
            "{model:?} must have max_torque_nm == 0.0, got {torque}"
        );
    }

    /// Debug formatting must be non-empty for every variant.
    #[test]
    fn prop_debug_format_non_empty(model in arb_model()) {
        let debug = format!("{model:?}");
        prop_assert!(!debug.is_empty(),
            "Debug output for {model:?} must be non-empty");
    }

    /// display_name must start with "Cube Controls" for every variant.
    #[test]
    fn prop_display_name_starts_with_brand(model in arb_model()) {
        prop_assert!(
            model.display_name().starts_with("Cube Controls"),
            "{model:?} display_name must start with 'Cube Controls', got '{}'",
            model.display_name()
        );
    }

    /// display_name must not contain any control characters.
    #[test]
    fn prop_display_name_no_control_chars(model in arb_model()) {
        let name = model.display_name();
        prop_assert!(
            !name.chars().any(|c| c.is_control()),
            "{model:?} display_name contains control characters: {name:?}"
        );
    }

    /// Two copies of the same model must be equal (Copy semantics).
    #[test]
    fn prop_copy_equals_original(model in arb_model()) {
        let copy = model;
        prop_assert_eq!(model, copy,
            "copied model must equal original");
    }

    /// is_provisional and max_torque_nm must be consistent across repeated calls.
    #[test]
    fn prop_method_results_stable(model in arb_model()) {
        let prov1 = model.is_provisional();
        let prov2 = model.is_provisional();
        prop_assert_eq!(prov1, prov2, "is_provisional must be stable");

        let t1 = model.max_torque_nm();
        let t2 = model.max_torque_nm();
        prop_assert!((t1 - t2).abs() < f32::EPSILON,
            "max_torque_nm must be stable: {t1} vs {t2}");
    }

    /// display_name must be valid UTF-8 and have a reasonable length (< 100 chars).
    #[test]
    fn prop_display_name_reasonable_length(model in arb_model()) {
        let name = model.display_name();
        prop_assert!(name.len() < 100,
            "{model:?} display_name is unreasonably long ({} chars)", name.len());
        prop_assert!(name.len() > 5,
            "{model:?} display_name is too short ({} chars)", name.len());
    }
}
