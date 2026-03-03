//! Additional insta snapshot tests for Cube Controls protocol.
//!
//! Complements `snapshot_tests.rs` with boundary PID lookup, model equality,
//! and a combined capabilities table.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};
use insta::assert_debug_snapshot;

#[test]
fn snapshot_boundary_pids() {
    let pids: &[u16] = &[
        CUBE_CONTROLS_GT_PRO_PID - 1,
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
        CUBE_CONTROLS_CSX3_PID + 1,
        0x0000,
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|pid| {
            let model = CubeControlsModel::from_product_id(*pid);
            let matched = is_cube_controls_product(*pid);
            format!("PID={pid:#06x}: model={:?}, matched={matched}", model)
        })
        .collect();
    assert_debug_snapshot!(summary);
}

#[test]
fn snapshot_all_models_capabilities_table() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    let table: Vec<String> = models
        .iter()
        .map(|m| {
            format!(
                "{:?}: display='{}', torque={:.1}Nm, provisional={}",
                m,
                m.display_name(),
                m.max_torque_nm(),
                m.is_provisional()
            )
        })
        .collect();
    assert_debug_snapshot!(table);
}

#[test]
fn snapshot_vendor_id_shared_stm32() {
    assert_debug_snapshot!(format!(
        "VID={:#06x} (STMicroelectronics shared), is_cube_gt={}, is_cube_formula={}, is_cube_csx3={}",
        CUBE_CONTROLS_VENDOR_ID,
        is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID),
        is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID),
        is_cube_controls_product(CUBE_CONTROLS_CSX3_PID)
    ));
}

#[test]
fn snapshot_model_equality() {
    let a = CubeControlsModel::from_product_id(CUBE_CONTROLS_GT_PRO_PID);
    let b = CubeControlsModel::GtPro;
    assert_debug_snapshot!(format!("from_pid={a:?}, literal={b:?}, equal={}", a == b));
}
