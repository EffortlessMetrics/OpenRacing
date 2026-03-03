//! Insta snapshot tests for Cube Controls protocol constants and types.
//!
//! Snapshots are stored in tests/snapshots/. Regenerate with:
//! INSTA_UPDATE=always cargo test -p hid-cube-controls-protocol

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};

// -- IDs ----------------------------------------------------------------------

#[test]
fn snapshot_vendor_id() {
    insta::assert_debug_snapshot!(CUBE_CONTROLS_VENDOR_ID);
}

#[test]
fn snapshot_pid_gt_pro() {
    insta::assert_debug_snapshot!(CUBE_CONTROLS_GT_PRO_PID);
}

#[test]
fn snapshot_pid_formula_pro() {
    insta::assert_debug_snapshot!(CUBE_CONTROLS_FORMULA_PRO_PID);
}

#[test]
fn snapshot_pid_csx3() {
    insta::assert_debug_snapshot!(CUBE_CONTROLS_CSX3_PID);
}

// -- is_cube_controls_product -------------------------------------------------

#[test]
fn snapshot_is_product_gt_pro() {
    insta::assert_debug_snapshot!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
}

#[test]
fn snapshot_is_product_formula_pro() {
    insta::assert_debug_snapshot!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
}

#[test]
fn snapshot_is_product_csx3() {
    insta::assert_debug_snapshot!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
}

#[test]
fn snapshot_is_product_unknown() {
    insta::assert_debug_snapshot!(is_cube_controls_product(0xFFFF));
}

// -- CubeControlsModel --------------------------------------------------------

#[test]
fn snapshot_model_from_gt_pro_pid() {
    insta::assert_debug_snapshot!(CubeControlsModel::from_product_id(CUBE_CONTROLS_GT_PRO_PID));
}

#[test]
fn snapshot_model_from_formula_pro_pid() {
    insta::assert_debug_snapshot!(CubeControlsModel::from_product_id(
        CUBE_CONTROLS_FORMULA_PRO_PID
    ));
}

#[test]
fn snapshot_model_from_csx3_pid() {
    insta::assert_debug_snapshot!(CubeControlsModel::from_product_id(CUBE_CONTROLS_CSX3_PID));
}

#[test]
fn snapshot_model_from_unknown_pid() {
    insta::assert_debug_snapshot!(CubeControlsModel::from_product_id(0xFFFF));
}

// -- display_name -------------------------------------------------------------

#[test]
fn snapshot_gt_pro_display_name() {
    insta::assert_debug_snapshot!(CubeControlsModel::GtPro.display_name());
}

#[test]
fn snapshot_formula_pro_display_name() {
    insta::assert_debug_snapshot!(CubeControlsModel::FormulaPro.display_name());
}

#[test]
fn snapshot_csx3_display_name() {
    insta::assert_debug_snapshot!(CubeControlsModel::Csx3.display_name());
}

#[test]
fn snapshot_unknown_display_name() {
    insta::assert_debug_snapshot!(CubeControlsModel::Unknown.display_name());
}

// -- max_torque_nm ------------------------------------------------------------

#[test]
fn snapshot_gt_pro_max_torque() {
    insta::assert_debug_snapshot!(CubeControlsModel::GtPro.max_torque_nm());
}

#[test]
fn snapshot_formula_pro_max_torque() {
    insta::assert_debug_snapshot!(CubeControlsModel::FormulaPro.max_torque_nm());
}

#[test]
fn snapshot_csx3_max_torque() {
    insta::assert_debug_snapshot!(CubeControlsModel::Csx3.max_torque_nm());
}

#[test]
fn snapshot_unknown_max_torque() {
    insta::assert_debug_snapshot!(CubeControlsModel::Unknown.max_torque_nm());
}

// -- is_provisional (PROVISIONAL status emphasis) -----------------------------

/// Snapshot the provisional status of each known model variant.
/// All Cube Controls PIDs are PROVISIONAL — unverified from hardware captures.
/// These snapshots will change when devices are confirmed from real USB descriptors.

#[test]
fn snapshot_provisional_gt_pro() {
    insta::assert_debug_snapshot!(CubeControlsModel::GtPro.is_provisional());
}

#[test]
fn snapshot_provisional_formula_pro() {
    insta::assert_debug_snapshot!(CubeControlsModel::FormulaPro.is_provisional());
}

#[test]
fn snapshot_provisional_csx3() {
    insta::assert_debug_snapshot!(CubeControlsModel::Csx3.is_provisional());
}

#[test]
fn snapshot_provisional_unknown() {
    insta::assert_debug_snapshot!(CubeControlsModel::Unknown.is_provisional());
}

/// Snapshot summarizing all PROVISIONAL protocol constants.
/// VID 0x0483 is STMicroelectronics shared — used by many STM32-based devices.
/// PIDs 0x0C73–0x0C75 are fabricated placeholders with no hardware evidence.
#[test]
fn snapshot_provisional_constants_summary() {
    let summary = format!(
        "PROVISIONAL Cube Controls constants (VID shared with STMicroelectronics):\n\
         VID={:#06x} (STMicro shared)\n\
         GT_PRO PID={:#06x} (provisional)\n\
         FORMULA_PRO PID={:#06x} (provisional)\n\
         CSX3 PID={:#06x} (provisional)",
        CUBE_CONTROLS_VENDOR_ID,
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    );
    insta::assert_snapshot!(summary);
}

// -- All known PIDs list snapshot ---------------------------------------------

#[test]
fn snapshot_all_known_pids() {
    let pids = [
        ("GT_PRO", CUBE_CONTROLS_GT_PRO_PID),
        ("FORMULA_PRO", CUBE_CONTROLS_FORMULA_PRO_PID),
        ("CSX3", CUBE_CONTROLS_CSX3_PID),
    ];
    let summary: Vec<String> = pids
        .iter()
        .map(|(name, pid)| format!("{}={:#06x}", name, pid))
        .collect();
    insta::assert_debug_snapshot!(summary.join(", "));
}

// -- Model summary snapshot ---------------------------------------------------

#[test]
fn snapshot_model_summary() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    let summary: Vec<String> = models
        .iter()
        .map(|m| {
            format!(
                "{:?}: name='{}', torque={:.1}Nm, provisional={}",
                m,
                m.display_name(),
                m.max_torque_nm(),
                m.is_provisional()
            )
        })
        .collect();
    insta::assert_debug_snapshot!(summary.join("\n"));
}
