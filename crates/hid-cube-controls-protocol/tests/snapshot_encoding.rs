//! Extended snapshot tests for Cube Controls wire-format encoding.
//!
//! These tests supplement `snapshot_tests.rs` by covering hex-formatted
//! constant values, model identification from boundary PIDs, and
//! the provisional status of all models.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};
use insta::assert_snapshot;

// ── Hex-formatted ID constants ───────────────────────────────────────────────

#[test]
fn test_snapshot_vendor_id_hex() {
    assert_snapshot!(format!("0x{CUBE_CONTROLS_VENDOR_ID:04X}"));
}

#[test]
fn test_snapshot_all_pids_hex() {
    let pids = [
        ("GT Pro", CUBE_CONTROLS_GT_PRO_PID),
        ("Formula Pro", CUBE_CONTROLS_FORMULA_PRO_PID),
        ("CSX3", CUBE_CONTROLS_CSX3_PID),
    ];
    let formatted: Vec<String> = pids
        .iter()
        .map(|(name, pid)| format!("{name}: 0x{pid:04X}"))
        .collect();
    assert_snapshot!(formatted.join(", "));
}

// ── Product identification boundary values ───────────────────────────────────

#[test]
fn test_snapshot_is_product_boundary_pids() {
    let test_pids: Vec<String> = [
        0x0000u16,
        CUBE_CONTROLS_GT_PRO_PID - 1,
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_GT_PRO_PID + 1,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
        CUBE_CONTROLS_CSX3_PID + 1,
        0xFFFF,
    ]
    .iter()
    .map(|&pid| format!("0x{pid:04X}={}", is_cube_controls_product(pid)))
    .collect();
    assert_snapshot!(test_pids.join(", "));
}

// ── Model classification from all PIDs ───────────────────────────────────────

#[test]
fn test_snapshot_model_from_all_known_pids() {
    let models: Vec<String> = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ]
    .iter()
    .map(|&pid| {
        let model = CubeControlsModel::from_product_id(pid);
        format!(
            "0x{pid:04X}: {:?}, name={}, torque={:.1}, provisional={}",
            model,
            model.display_name(),
            model.max_torque_nm(),
            model.is_provisional()
        )
    })
    .collect();
    assert_snapshot!(models.join("\n"));
}

#[test]
fn test_snapshot_model_unknown_pid_formatted() {
    let model = CubeControlsModel::from_product_id(0xFFFF);
    assert_snapshot!(format!(
        "model={:?}, name={}, torque={:.1}, provisional={}",
        model,
        model.display_name(),
        model.max_torque_nm(),
        model.is_provisional()
    ));
}

// ── Torque is zero for all models (input-only devices) ───────────────────────

#[test]
fn test_snapshot_torque_zero_all_models() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    let results: Vec<String> = models
        .iter()
        .map(|m| format!("{:?}: {:.1}Nm", m, m.max_torque_nm()))
        .collect();
    assert_snapshot!(results.join(", "));
}
