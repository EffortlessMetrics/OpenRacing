//! Deep protocol tests for Cube Controls HID protocol crate.

use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn vendor_id_is_stm_shared_vid() {
    assert_eq!(CUBE_CONTROLS_VENDOR_ID, 0x0483);
}

#[test]
fn all_known_pids_are_nonzero_and_unique() {
    let pids = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for pid in pids {
        assert_ne!(pid, 0, "PID must not be zero");
    }
    assert_ne!(CUBE_CONTROLS_GT_PRO_PID, CUBE_CONTROLS_FORMULA_PRO_PID);
    assert_ne!(CUBE_CONTROLS_GT_PRO_PID, CUBE_CONTROLS_CSX3_PID);
    assert_ne!(CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_CSX3_PID);
}

#[test]
fn all_known_pids_are_recognised() {
    assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
}

#[test]
fn unknown_pids_not_recognised() {
    let unknowns: &[u16] = &[0x0000, 0x0001, 0x0C72, 0x0C76, 0xFFFF, 0x0522];
    for &pid in unknowns {
        assert!(
            !is_cube_controls_product(pid),
            "PID 0x{pid:04X} should not be recognised"
        );
    }
}

#[test]
fn pids_do_not_overlap_with_simagic_legacy() {
    // Simagic legacy PID on the same VID
    assert!(!is_cube_controls_product(0x0522));
}

// ── Model classification ─────────────────────────────────────────────────────

#[test]
fn model_from_pid_maps_correctly() {
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
fn unknown_pid_maps_to_unknown_model() {
    assert_eq!(
        CubeControlsModel::from_product_id(0x0000),
        CubeControlsModel::Unknown
    );
    assert_eq!(
        CubeControlsModel::from_product_id(0xFFFF),
        CubeControlsModel::Unknown
    );
}

// ── Display names ────────────────────────────────────────────────────────────

#[test]
fn display_names_are_correct() {
    assert_eq!(
        CubeControlsModel::GtPro.display_name(),
        "Cube Controls GT Pro"
    );
    assert_eq!(
        CubeControlsModel::FormulaPro.display_name(),
        "Cube Controls Formula Pro"
    );
    assert_eq!(CubeControlsModel::Csx3.display_name(), "Cube Controls CSX3");
}

#[test]
fn unknown_model_has_nonempty_display_name() {
    let name = CubeControlsModel::Unknown.display_name();
    assert!(!name.is_empty());
    assert!(name.contains("Cube Controls"));
}

#[test]
fn all_display_names_contain_brand() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        assert!(
            model.display_name().contains("Cube Controls"),
            "{model:?} display name '{}' must contain 'Cube Controls'",
            model.display_name()
        );
    }
}

// ── Torque (input-only devices) ──────────────────────────────────────────────

#[test]
fn torque_is_zero_for_all_models() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        assert!(
            (model.max_torque_nm() - 0.0).abs() < f32::EPSILON,
            "{model:?} torque must be 0.0 for input-only device"
        );
    }
}

#[test]
fn torque_is_non_negative_for_all_models() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        assert!(
            model.max_torque_nm() >= 0.0,
            "{model:?} torque must be non-negative"
        );
    }
}

// ── Provisional status ───────────────────────────────────────────────────────

#[test]
fn all_models_are_provisional() {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        assert!(
            model.is_provisional(),
            "{model:?} should be provisional while PIDs are unconfirmed"
        );
    }
}

// ── Cross-consistency ────────────────────────────────────────────────────────

#[test]
fn every_recognised_pid_resolves_to_non_unknown_model() {
    let known = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for pid in known {
        let model = CubeControlsModel::from_product_id(pid);
        assert_ne!(
            model,
            CubeControlsModel::Unknown,
            "PID 0x{pid:04X} must resolve to a known model"
        );
    }
}

#[test]
fn is_cube_controls_product_consistent_with_model() {
    let known = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for pid in known {
        assert!(is_cube_controls_product(pid));
        assert_ne!(
            CubeControlsModel::from_product_id(pid),
            CubeControlsModel::Unknown
        );
    }
}

#[test]
fn unrecognised_pid_not_in_product_check() {
    let unknowns: &[u16] = &[0x0000, 0x0001, 0x0C72, 0x0C76, 0xFFFF];
    for &pid in unknowns {
        assert!(!is_cube_controls_product(pid));
        assert_eq!(
            CubeControlsModel::from_product_id(pid),
            CubeControlsModel::Unknown
        );
    }
}

#[test]
fn model_copy_and_eq() {
    let a = CubeControlsModel::GtPro;
    let b = a;
    assert_eq!(a, b);
}

#[test]
fn model_debug_is_nonempty() {
    let s = format!("{:?}", CubeControlsModel::Csx3);
    assert!(!s.is_empty());
}

#[test]
fn pids_are_in_provisional_range() {
    // All provisional PIDs are in the 0x0C73-0x0C75 range
    assert_eq!(CUBE_CONTROLS_GT_PRO_PID, 0x0C73);
    assert_eq!(CUBE_CONTROLS_FORMULA_PRO_PID, 0x0C74);
    assert_eq!(CUBE_CONTROLS_CSX3_PID, 0x0C75);
}
