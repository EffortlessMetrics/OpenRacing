//! Protocol verification tests for the Cube Controls HID protocol implementation.
//!
//! These tests document the **provisional** status of all Cube Controls VID/PIDs
//! and verify that the crate correctly marks them as unconfirmed.
//!
//! ## Sources cited
//!
//! | # | Source | What it confirms |
//! |---|--------|------------------|
//! | 1 | STMicroelectronics USB-IF registration | VID `0x0483` = STMicroelectronics (generic shared VID) |
//! | 2 | JacKeTUs/linux-steering-wheels | **No** Cube Controls entries (checked 2025-07) |
//! | 3 | JacKeTUs/simracing-hwdb | **No** Cube Controls hwdb file (checked 2025-07) |
//! | 4 | Linux kernel `hid-ids.h` | **No** Cube Controls entries (checked 2025-07) |
//! | 5 | cubecontrols.com product pages | No USB VID/PID published |
//! | 6 | devicehunt.com VID `0x0483` | PIDs `0x0C73`–`0x0C75` not registered |
//!
//! ## Status
//!
//! All PIDs are **fabricated placeholders** with no hardware confirmation.
//! Cube Controls products are steering wheels (input-only button boxes), not
//! wheelbases — they do not produce force feedback.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};

// ════════════════════════════════════════════════════════════════════════════
// § 1. VID verification
// ════════════════════════════════════════════════════════════════════════════

/// VID `0x0483` = STMicroelectronics (generic shared VID for STM32 devices).
/// Source [1]: USB-IF vendor list — STMicroelectronics
/// Note: this VID is shared with VRS, legacy Simagic, and many non-sim devices.
#[test]
fn vid_is_stmicroelectronics_shared() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        CUBE_CONTROLS_VENDOR_ID, 0x0483,
        "Cube Controls VID must be 0x0483 (STMicroelectronics shared VID)"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 2. Provisional PID verification
// ════════════════════════════════════════════════════════════════════════════

/// All three PIDs (GT Pro, Formula Pro, CSX3) are provisional placeholders.
/// Source [6]: devicehunt.com shows no entries for PIDs 0x0C73–0x0C75 under VID 0x0483.
/// Source [2]: No Cube Controls in linux-steering-wheels.
/// Source [3]: No Cube Controls in simracing-hwdb.
/// Source [4]: No Cube Controls in Linux kernel hid-ids.h.
#[test]
fn pids_are_placeholder_values() -> Result<(), Box<dyn std::error::Error>> {
    // These are sequential placeholder values — verify they match the code
    assert_eq!(
        CUBE_CONTROLS_GT_PRO_PID, 0x0C73,
        "GT Pro provisional PID"
    );
    assert_eq!(
        CUBE_CONTROLS_FORMULA_PRO_PID, 0x0C74,
        "Formula Pro provisional PID"
    );
    assert_eq!(CUBE_CONTROLS_CSX3_PID, 0x0C75, "CSX3 provisional PID");
    Ok(())
}

/// All provisional PIDs must be recognized by `is_cube_controls_product`.
#[test]
fn provisional_pids_are_recognised() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
    Ok(())
}

/// Unknown PIDs must not be recognised.
#[test]
fn unknown_pids_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_cube_controls_product(0x0001),
        "arbitrary PID must be rejected"
    );
    assert!(
        !is_cube_controls_product(0xFFFF),
        "0xFFFF must be rejected"
    );
    // VRS DFP PID shares VID 0x0483 but must not match as Cube Controls
    assert!(
        !is_cube_controls_product(0xA355),
        "VRS DFP PID must not match as Cube Controls"
    );
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 3. All models must be marked provisional
// ════════════════════════════════════════════════════════════════════════════

/// Every `CubeControlsModel` variant must return `is_provisional() == true`.
/// This guards against accidentally shipping unconfirmed PIDs as "verified".
#[test]
fn all_models_are_provisional() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &models {
        assert!(
            model.is_provisional(),
            "{model:?} must be provisional"
        );
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 4. Torque must be zero (input-only devices)
// ════════════════════════════════════════════════════════════════════════════

/// Cube Controls products are steering wheels (button boxes), not wheelbases.
/// They do not produce force feedback. `max_torque_nm()` must return 0.0
/// for safety (prevents accidentally scaling FFB output through these devices).
#[test]
fn torque_is_zero_for_all_models() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in &models {
        assert!(
            model.max_torque_nm().abs() < f32::EPSILON,
            "{model:?} must have 0 Nm torque (input-only device)"
        );
    }
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
// § 5. Model resolution
// ════════════════════════════════════════════════════════════════════════════

/// Known provisional PIDs must resolve to named model variants.
#[test]
fn known_pids_resolve_to_named_models() -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

/// Unknown PIDs resolve to `CubeControlsModel::Unknown`.
#[test]
fn unknown_pid_resolves_to_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        CubeControlsModel::from_product_id(0xFFFF),
        CubeControlsModel::Unknown
    );
    Ok(())
}

/// Display names must be non-empty and mention "Cube Controls".
#[test]
fn display_names_are_descriptive() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
    ];
    for model in &models {
        let name = model.display_name();
        assert!(
            name.contains("Cube Controls"),
            "{model:?} display name must mention 'Cube Controls', got: {name}"
        );
    }
    Ok(())
}
