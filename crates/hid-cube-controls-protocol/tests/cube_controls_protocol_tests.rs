//! Comprehensive Cube Controls protocol hardening tests.
//!
//! Covers VID/PID validation, model classification, input-only safety
//! (zero torque), provisional status, and proptest fuzzing.

use hid_cube_controls_protocol::*;

// ─── VID / PID golden values ────────────────────────────────────────────

#[test]
fn vid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        CUBE_CONTROLS_VENDOR_ID, 0x0483,
        "STMicroelectronics shared VID"
    );
    Ok(())
}

#[test]
fn pid_golden_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CUBE_CONTROLS_GT_PRO_PID, 0x0C73);
    assert_eq!(CUBE_CONTROLS_FORMULA_PRO_PID, 0x0C74);
    assert_eq!(CUBE_CONTROLS_CSX3_PID, 0x0C75);
    Ok(())
}

#[test]
fn pids_are_consecutive() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID + 1);
    assert_eq!(CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID + 1);
    Ok(())
}

#[test]
fn all_pids_nonzero_and_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for &pid in &pids {
        assert_ne!(pid, 0);
    }
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j]);
        }
    }
    Ok(())
}

// ─── Product matching ───────────────────────────────────────────────────

#[test]
fn is_cube_controls_product_known() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
    Ok(())
}

#[test]
fn is_cube_controls_product_rejects_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_cube_controls_product(0x0000));
    assert!(!is_cube_controls_product(0xFFFF));
    assert!(!is_cube_controls_product(0x0522)); // Simagic legacy
    assert!(!is_cube_controls_product(0xA355)); // VRS DFP
    // Boundary values
    assert!(!is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID - 1));
    assert!(!is_cube_controls_product(CUBE_CONTROLS_CSX3_PID + 1));
    Ok(())
}

// ─── Model classification ───────────────────────────────────────────────

#[test]
fn model_from_product_id_known() -> Result<(), Box<dyn std::error::Error>> {
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

#[test]
fn model_from_product_id_unknown() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        CubeControlsModel::from_product_id(0x0000),
        CubeControlsModel::Unknown
    );
    assert_eq!(
        CubeControlsModel::from_product_id(0xFFFF),
        CubeControlsModel::Unknown
    );
    assert_eq!(
        CubeControlsModel::from_product_id(0x0522),
        CubeControlsModel::Unknown
    );
    Ok(())
}

#[test]
fn display_names_contain_brand() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        let name = model.display_name();
        assert!(!name.is_empty(), "{model:?} name must not be empty");
        assert!(
            name.contains("Cube Controls"),
            "{model:?} name must contain brand"
        );
    }
    Ok(())
}

#[test]
fn display_names_unique() -> Result<(), Box<dyn std::error::Error>> {
    let names: Vec<&str> = [
        CubeControlsModel::GtPro.display_name(),
        CubeControlsModel::FormulaPro.display_name(),
        CubeControlsModel::Csx3.display_name(),
        CubeControlsModel::Unknown.display_name(),
    ]
    .to_vec();
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            assert_ne!(names[i], names[j], "names at [{i}] and [{j}] must differ");
        }
    }
    Ok(())
}

// ─── Input-only safety: zero torque ─────────────────────────────────────

#[test]
fn all_models_torque_zero() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        let torque = model.max_torque_nm();
        assert!(
            (torque - 0.0).abs() < f32::EPSILON,
            "{model:?} torque must be exactly 0.0 (input-only device), got {torque}"
        );
    }
    Ok(())
}

#[test]
fn torque_not_negative() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        assert!(
            model.max_torque_nm() >= 0.0,
            "{model:?} torque must not be negative"
        );
    }
    Ok(())
}

// ─── Provisional status ─────────────────────────────────────────────────

#[test]
fn all_models_provisional() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        assert!(
            model.is_provisional(),
            "{model:?} must be provisional (unverified PIDs)"
        );
    }
    Ok(())
}

// ─── Model trait consistency ────────────────────────────────────────────

#[test]
fn model_copy_clone_eq() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        let cloned = model;
        assert_eq!(model, cloned, "{model:?} Copy must preserve value");
    }
    // Distinct variants are not equal
    assert_ne!(CubeControlsModel::GtPro, CubeControlsModel::FormulaPro);
    assert_ne!(CubeControlsModel::FormulaPro, CubeControlsModel::Csx3);
    assert_ne!(CubeControlsModel::Csx3, CubeControlsModel::Unknown);
    Ok(())
}

#[test]
fn model_debug_nonempty() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        CubeControlsModel::GtPro,
        CubeControlsModel::FormulaPro,
        CubeControlsModel::Csx3,
        CubeControlsModel::Unknown,
    ];
    for model in models {
        let debug = format!("{model:?}");
        assert!(
            !debug.is_empty(),
            "Debug output for {model:?} must not be empty"
        );
    }
    Ok(())
}

// ─── Cross-consistency ──────────────────────────────────────────────────

#[test]
fn from_product_id_agrees_with_is_product() -> Result<(), Box<dyn std::error::Error>> {
    let known_pids = [
        CUBE_CONTROLS_GT_PRO_PID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
        CUBE_CONTROLS_CSX3_PID,
    ];
    for pid in known_pids {
        assert!(is_cube_controls_product(pid));
        assert_ne!(
            CubeControlsModel::from_product_id(pid),
            CubeControlsModel::Unknown
        );
    }
    // Unknown PIDs
    let unknown_pids = [
        0x0000u16,
        0xFFFF,
        0x0522,
        CUBE_CONTROLS_GT_PRO_PID - 1,
        CUBE_CONTROLS_CSX3_PID + 1,
    ];
    for pid in unknown_pids {
        assert!(!is_cube_controls_product(pid));
        assert_eq!(
            CubeControlsModel::from_product_id(pid),
            CubeControlsModel::Unknown
        );
    }
    Ok(())
}

// ─── Shared VID disambiguation ──────────────────────────────────────────

#[test]
fn shared_vid_does_not_collide_with_known_vrs_pids() -> Result<(), Box<dyn std::error::Error>> {
    // VRS and Cube Controls share VID 0x0483 but must have distinct PIDs
    let vrs_pids = [
        0xA355u16, 0xA356, 0xA44C, 0xA3BE, 0xA357, 0xA358, 0xA359, 0xA35A,
    ];
    for pid in vrs_pids {
        assert!(
            !is_cube_controls_product(pid),
            "VRS PID 0x{pid:04X} must not match as Cube Controls"
        );
    }
    Ok(())
}

#[test]
fn shared_vid_does_not_collide_with_simagic_legacy() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        !is_cube_controls_product(0x0522),
        "Simagic legacy PID must not match"
    );
    Ok(())
}

// ─── Proptest fuzzing ───────────────────────────────────────────────────

mod proptest_cube_controls {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_is_product_only_known_pids(pid in 0u16..=0xFFFF) {
            if is_cube_controls_product(pid) {
                let known = [CUBE_CONTROLS_GT_PRO_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_CSX3_PID];
                prop_assert!(known.contains(&pid), "accepted unknown PID 0x{pid:04X}");
            }
        }

        #[test]
        fn prop_model_from_pid_consistent_with_is_product(pid in 0u16..=0xFFFF) {
            let is_known = is_cube_controls_product(pid);
            let model = CubeControlsModel::from_product_id(pid);
            if is_known {
                prop_assert_ne!(model, CubeControlsModel::Unknown);
            } else {
                prop_assert_eq!(model, CubeControlsModel::Unknown);
            }
        }

        #[test]
        fn prop_torque_always_zero(pid in 0u16..=0xFFFF) {
            let model = CubeControlsModel::from_product_id(pid);
            let torque = model.max_torque_nm();
            prop_assert!((torque - 0.0).abs() < f32::EPSILON,
                "torque for PID 0x{pid:04X} must be 0.0, got {torque}");
        }

        #[test]
        fn prop_all_models_provisional(pid in 0u16..=0xFFFF) {
            let model = CubeControlsModel::from_product_id(pid);
            prop_assert!(model.is_provisional());
        }

        #[test]
        fn prop_display_name_nonempty(pid in 0u16..=0xFFFF) {
            let model = CubeControlsModel::from_product_id(pid);
            prop_assert!(!model.display_name().is_empty());
        }

        #[test]
        fn prop_display_name_contains_brand(pid in 0u16..=0xFFFF) {
            let model = CubeControlsModel::from_product_id(pid);
            prop_assert!(model.display_name().contains("Cube Controls"));
        }

        #[test]
        fn prop_model_deterministic(pid in 0u16..=0xFFFF) {
            let a = CubeControlsModel::from_product_id(pid);
            let b = CubeControlsModel::from_product_id(pid);
            prop_assert_eq!(a, b);
        }
    }
}
