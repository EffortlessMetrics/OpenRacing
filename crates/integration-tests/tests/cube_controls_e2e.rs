//! BDD end-to-end tests for the Cube Controls protocol crate.
//!
//! Cube Controls products are steering wheels (input-only button boxes), not
//! wheelbases. They do not produce force feedback. These tests verify model
//! identification, PID recognition, display names, provisional status, and
//! safe torque defaults.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};

// ─── Scenario 1: Model identification from known product IDs ──────────────────

#[test]
fn scenario_model_given_gt_pro_pid_when_from_product_id_then_returns_gt_pro() {
    // Given: the GT Pro product ID
    let pid = CUBE_CONTROLS_GT_PRO_PID;

    // When: resolving the model
    let model = CubeControlsModel::from_product_id(pid);

    // Then: returns GtPro variant
    assert_eq!(model, CubeControlsModel::GtPro);
}

#[test]
fn scenario_model_given_formula_pro_pid_when_from_product_id_then_returns_formula_pro() {
    let pid = CUBE_CONTROLS_FORMULA_PRO_PID;
    let model = CubeControlsModel::from_product_id(pid);
    assert_eq!(model, CubeControlsModel::FormulaPro);
}

#[test]
fn scenario_model_given_csx3_pid_when_from_product_id_then_returns_csx3() {
    let pid = CUBE_CONTROLS_CSX3_PID;
    let model = CubeControlsModel::from_product_id(pid);
    assert_eq!(model, CubeControlsModel::Csx3);
}

// ─── Scenario 2: Product ID recognition ───────────────────────────────────────

#[test]
fn scenario_recognition_given_known_pids_when_is_cube_controls_product_then_returns_true() {
    // Given/When/Then: all known PIDs are recognised
    assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
}

#[test]
fn scenario_recognition_given_unknown_pid_when_is_cube_controls_product_then_returns_false() {
    // Given: PIDs that do not belong to Cube Controls
    assert!(!is_cube_controls_product(0x0000));
    assert!(!is_cube_controls_product(0x0001));
    assert!(!is_cube_controls_product(0x0522)); // Simagic legacy PID
    assert!(!is_cube_controls_product(0xFFFF));
}

// ─── Scenario 3: Display names per model ──────────────────────────────────────

#[test]
fn scenario_display_given_gt_pro_when_display_name_then_returns_expected_string() {
    let name = CubeControlsModel::GtPro.display_name();
    assert_eq!(name, "Cube Controls GT Pro");
}

#[test]
fn scenario_display_given_formula_pro_when_display_name_then_returns_expected_string() {
    let name = CubeControlsModel::FormulaPro.display_name();
    assert_eq!(name, "Cube Controls Formula Pro");
}

#[test]
fn scenario_display_given_csx3_when_display_name_then_returns_expected_string() {
    let name = CubeControlsModel::Csx3.display_name();
    assert_eq!(name, "Cube Controls CSX3");
}

#[test]
fn scenario_display_given_unknown_when_display_name_then_returns_non_empty_fallback() {
    let name = CubeControlsModel::Unknown.display_name();
    assert!(
        !name.is_empty(),
        "Unknown model must still have a display name"
    );
    assert_eq!(name, "Cube Controls (unknown model)");
}

// ─── Scenario 4: Max torque per model (input-only devices → 0 Nm) ─────────────

#[test]
fn scenario_torque_given_all_models_when_max_torque_then_returns_zero() {
    // Cube Controls products are steering wheels (input-only), not FFB devices.
    // All models must return 0.0 Nm to prevent accidental FFB scaling.
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];

    for model in models {
        let torque = model.max_torque_nm();
        assert!(
            torque.abs() < f32::EPSILON,
            "{} must report 0.0 Nm torque, got {torque}",
            model.display_name()
        );
    }
}

// ─── Scenario 5: Provisional PID status ───────────────────────────────────────

#[test]
fn scenario_provisional_given_all_known_models_when_is_provisional_then_returns_true() {
    // All Cube Controls PIDs are currently unverified from hardware captures.
    assert!(CubeControlsModel::GtPro.is_provisional());
    assert!(CubeControlsModel::FormulaPro.is_provisional());
    assert!(CubeControlsModel::Csx3.is_provisional());
}

#[test]
fn scenario_provisional_given_unknown_model_when_is_provisional_then_returns_true() {
    assert!(CubeControlsModel::Unknown.is_provisional());
}

// ─── Scenario 6: Vendor ID constant verification ─────────────────────────────

#[test]
fn scenario_vendor_id_given_constant_when_checked_then_matches_stm_shared_vid() {
    // Cube Controls uses the STMicroelectronics shared VID (0x0483).
    assert_eq!(CUBE_CONTROLS_VENDOR_ID, 0x0483);
}

#[test]
fn scenario_vendor_id_given_pid_constants_when_checked_then_are_in_expected_range() {
    // Provisional PIDs are in the 0x0C73–0x0C75 range.
    assert_eq!(CUBE_CONTROLS_GT_PRO_PID, 0x0C73);
    assert_eq!(CUBE_CONTROLS_FORMULA_PRO_PID, 0x0C74);
    assert_eq!(CUBE_CONTROLS_CSX3_PID, 0x0C75);
}

// ─── Scenario 7: Unknown product ID handling ──────────────────────────────────

#[test]
fn scenario_unknown_given_unrecognised_pid_when_from_product_id_then_returns_unknown() {
    let model = CubeControlsModel::from_product_id(0xFFFF);
    assert_eq!(model, CubeControlsModel::Unknown);
}

#[test]
fn scenario_unknown_given_zero_pid_when_from_product_id_then_returns_unknown() {
    let model = CubeControlsModel::from_product_id(0x0000);
    assert_eq!(model, CubeControlsModel::Unknown);
}

#[test]
fn scenario_unknown_given_adjacent_pid_when_from_product_id_then_returns_unknown() {
    // PIDs just outside the known range should not match
    let below = CubeControlsModel::from_product_id(CUBE_CONTROLS_GT_PRO_PID - 1);
    let above = CubeControlsModel::from_product_id(CUBE_CONTROLS_CSX3_PID + 1);
    assert_eq!(below, CubeControlsModel::Unknown);
    assert_eq!(above, CubeControlsModel::Unknown);
}

#[test]
fn scenario_unknown_given_unknown_model_when_display_and_torque_then_returns_safe_defaults() {
    // Unknown model must still provide safe API responses
    let model = CubeControlsModel::from_product_id(0x9999);
    assert_eq!(model, CubeControlsModel::Unknown);
    assert!(!model.display_name().is_empty());
    assert!(model.max_torque_nm().abs() < f32::EPSILON);
    assert!(model.is_provisional());
}
