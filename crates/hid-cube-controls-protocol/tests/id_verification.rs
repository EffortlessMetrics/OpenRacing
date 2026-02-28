//! Cross-reference tests for Cube Controls VID/PID constants.
//!
//! # Status: PROVISIONAL
//!
//! All values below are provisional best-guesses (see `docs/protocols/SOURCES.md`
//! — "Devices Under Investigation"). Once confirmed from real hardware captures,
//! update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID,
};

/// Cube Controls VID must be 0x0483 (STMicroelectronics shared VID).
///
/// Source: community reports place Cube Controls hardware on this VID.
#[test]
fn vendor_id_is_0483() {
    assert_eq!(
        CUBE_CONTROLS_VENDOR_ID, 0x0483,
        "Cube Controls VID changed — update ids.rs and SOURCES.md"
    );
}

/// GT Pro PID must be 0x0C73 (provisional).
#[test]
fn gt_pro_pid_is_0c73() {
    assert_eq!(
        CUBE_CONTROLS_GT_PRO_PID, 0x0C73,
        "GT Pro PID changed — update ids.rs and SOURCES.md"
    );
}

/// Formula Pro PID must be 0x0C74 (provisional).
#[test]
fn formula_pro_pid_is_0c74() {
    assert_eq!(
        CUBE_CONTROLS_FORMULA_PRO_PID, 0x0C74,
        "Formula Pro PID changed — update ids.rs and SOURCES.md"
    );
}

/// CSX3 PID must be 0x0C75 (provisional).
#[test]
fn csx3_pid_is_0c75() {
    assert_eq!(
        CUBE_CONTROLS_CSX3_PID, 0x0C75,
        "CSX3 PID changed — update ids.rs and SOURCES.md"
    );
}

/// All three known PIDs must be distinct from each other.
#[test]
fn all_pids_are_distinct() {
    assert_ne!(CUBE_CONTROLS_GT_PRO_PID, CUBE_CONTROLS_FORMULA_PRO_PID);
    assert_ne!(CUBE_CONTROLS_GT_PRO_PID, CUBE_CONTROLS_CSX3_PID);
    assert_ne!(CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_CSX3_PID);
}
