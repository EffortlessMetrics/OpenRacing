//! Cross-crate validation of protocol constants.
//!
//! Ensures that VID/PID constants defined in individual HID protocol crates
//! are consistent with the engine's device dispatch tables, that product IDs
//! are unique within each vendor, that telemetry adapter game IDs match the
//! config/registry, and that device names are non-empty and non-placeholder.

use std::collections::{HashMap, HashSet};

use racing_wheel_engine::hid::vendor::get_vendor_protocol;
use racing_wheel_telemetry_adapters::adapter_factories;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ═════════════════════════════════════════════════════════════════════════════
// 1. Protocol VID constants match engine dispatch tables
// ═════════════════════════════════════════════════════════════════════════════

/// Every VID/PID pair exported by protocol crates must be routable through
/// `get_vendor_protocol()`. A mismatch means a supported device would be
/// silently ignored at runtime.
#[test]
fn protocol_vid_pid_pairs_dispatch_through_engine() -> TestResult {
    // (VID, PID, label) — one representative per vendor + key products
    let cases: &[(u16, u16, &str)] = &[
        // Logitech
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            racing_wheel_hid_logitech_protocol::product_ids::G920,
            "Logitech G920",
        ),
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
            "Logitech G29",
        ),
        (
            racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID,
            racing_wheel_hid_logitech_protocol::product_ids::G_PRO,
            "Logitech G PRO",
        ),
        // Fanatec
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
            "Fanatec CSL DD",
        ),
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            racing_wheel_hid_fanatec_protocol::product_ids::DD1,
            "Fanatec DD1",
        ),
        (
            racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID,
            racing_wheel_hid_fanatec_protocol::product_ids::DD2,
            "Fanatec DD2",
        ),
        // Thrustmaster
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
            "Thrustmaster T300 RS",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            racing_wheel_hid_thrustmaster_protocol::product_ids::T818,
            "Thrustmaster T818",
        ),
        (
            racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID,
            racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER,
            "Thrustmaster TS-PC Racer",
        ),
        // Simagic (modern VID)
        (
            racing_wheel_hid_simagic_protocol::SIMAGIC_VENDOR_ID,
            racing_wheel_hid_simagic_protocol::product_ids::EVO,
            "Simagic EVO",
        ),
        // Moza
        (
            racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID,
            racing_wheel_hid_moza_protocol::product_ids::R9_V1,
            "Moza R9",
        ),
        // PXN
        (
            racing_wheel_hid_pxn_protocol::VENDOR_ID,
            racing_wheel_hid_pxn_protocol::PRODUCT_V10,
            "PXN V10",
        ),
        (
            racing_wheel_hid_pxn_protocol::VENDOR_ID,
            racing_wheel_hid_pxn_protocol::PRODUCT_V12,
            "PXN V12",
        ),
        // Cammus
        (
            racing_wheel_hid_cammus_protocol::VENDOR_ID,
            racing_wheel_hid_cammus_protocol::PRODUCT_C5,
            "Cammus C5",
        ),
        (
            racing_wheel_hid_cammus_protocol::VENDOR_ID,
            racing_wheel_hid_cammus_protocol::PRODUCT_C12,
            "Cammus C12",
        ),
        // Simucube
        (
            hid_simucube_protocol::VENDOR_ID,
            hid_simucube_protocol::SIMUCUBE_2_SPORT_PID,
            "Simucube 2 Sport",
        ),
        (
            hid_simucube_protocol::VENDOR_ID,
            hid_simucube_protocol::SIMUCUBE_2_PRO_PID,
            "Simucube 2 Pro",
        ),
        (
            hid_simucube_protocol::VENDOR_ID,
            hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
            "Simucube 2 Ultimate",
        ),
        // VRS (shared STM VID)
        (
            racing_wheel_hid_vrs_protocol::VRS_VENDOR_ID,
            racing_wheel_hid_vrs_protocol::VRS_PRODUCT_ID,
            "VRS DirectForce Pro",
        ),
        // Asetek
        (
            hid_asetek_protocol::VENDOR_ID,
            hid_asetek_protocol::PRODUCT_ID_FORTE,
            "Asetek Forte",
        ),
        (
            hid_asetek_protocol::VENDOR_ID,
            hid_asetek_protocol::PRODUCT_ID_INVICTA,
            "Asetek Invicta",
        ),
        // AccuForce
        (
            racing_wheel_hid_accuforce_protocol::VENDOR_ID,
            racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO,
            "AccuForce Pro",
        ),
        // Leo Bodnar
        (
            racing_wheel_hid_leo_bodnar_protocol::VENDOR_ID,
            racing_wheel_hid_leo_bodnar_protocol::PID_WHEEL_INTERFACE,
            "Leo Bodnar Wheel Interface",
        ),
        // FFBeast
        (
            racing_wheel_hid_ffbeast_protocol::FFBEAST_VENDOR_ID,
            racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL,
            "FFBeast Wheel",
        ),
        // OpenFFBoard
        (
            racing_wheel_hid_openffboard_protocol::OPENFFBOARD_VENDOR_ID,
            racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
            "OpenFFBoard",
        ),
        // Cube Controls (shared STM VID)
        (
            hid_cube_controls_protocol::CUBE_CONTROLS_VENDOR_ID,
            hid_cube_controls_protocol::CUBE_CONTROLS_GT_PRO_PID,
            "Cube Controls GT Pro",
        ),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (vid, pid, label) in cases {
        if get_vendor_protocol(*vid, *pid).is_none() {
            failures.push(format!(
                "{label} (VID 0x{vid:04X}, PID 0x{pid:04X}) not dispatched by engine"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Protocol VID/PID constants not routable through engine dispatch:\n  {}",
        failures.join("\n  ")
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. PID uniqueness within each vendor (no collisions)
// ═════════════════════════════════════════════════════════════════════════════

/// All product IDs within a single vendor must be unique. Duplicate PIDs
/// would cause silent mis-identification of hardware at runtime.
#[test]
fn protocol_pids_unique_within_vendor() -> TestResult {
    // Collect (VID, PID, label) for all known products
    let all_products: &[(u16, u16, &str)] = &[
        // Logitech (VID 0x046D)
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::MOMO, "MOMO"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_PRO, "DFP"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_GT, "DFGT"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::SPEED_FORCE_WIRELESS, "SFW"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::MOMO_2, "MOMO 2"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::WINGMAN_FORMULA_FORCE_GP, "WFFGP"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::WINGMAN_FORMULA_FORCE, "WFF"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::VIBRATION_WHEEL, "Vibration"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G25, "G25"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_EX, "DF-EX"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G27, "G27"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G29_PS, "G29 PS"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G920, "G920"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G923, "G923"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G923_PS, "G923 PS"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G923_XBOX, "G923 Xbox"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G923_XBOX_ALT, "G923 Xbox Alt"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G_PRO, "G PRO"),
        (0x046D, racing_wheel_hid_logitech_protocol::product_ids::G_PRO_XBOX, "G PRO Xbox"),
        // Fanatec (VID 0x0EB7)
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2, "CS V2"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2_5, "CS V2.5"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE_PS4, "CSL Elite PS4"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::DD1, "DD1"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::DD2, "DD2"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CSR_ELITE, "CSR Elite"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD, "CSL DD"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::GT_DD_PRO, "GT DD Pro"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CSL_ELITE, "CSL Elite"),
        (0x0EB7, racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_DD, "CS DD"),
        // Thrustmaster (VID 0x044F)
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T150, "T150"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS, "T300 RS"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS_PS4, "T300 RS PS4"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS_GT, "T300 RS GT"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::TX_RACING, "TX Racing"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T500_RS, "T500 RS"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T248, "T248"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T818, "T818"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER, "TS-PC Racer"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::TS_XW, "TS-XW"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::TMX, "TMX"),
        (0x044F, racing_wheel_hid_thrustmaster_protocol::product_ids::T248X, "T248X"),
        // PXN (VID 0x11FF)
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V10, "V10"),
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V12, "V12"),
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE, "V12 Lite"),
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE_2, "V12 Lite 2"),
        (0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_GT987, "GT987"),
        // Cammus (VID 0x3416)
        (0x3416, racing_wheel_hid_cammus_protocol::PRODUCT_C5, "C5"),
        (0x3416, racing_wheel_hid_cammus_protocol::PRODUCT_C12, "C12"),
        (0x3416, racing_wheel_hid_cammus_protocol::PRODUCT_CP5_PEDALS, "CP5 Pedals"),
        (0x3416, racing_wheel_hid_cammus_protocol::PRODUCT_LC100_PEDALS, "LC100 Pedals"),
        // Simucube (VID 0x16D0)
        (0x16D0, hid_simucube_protocol::SIMUCUBE_1_PID, "Simucube 1"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_2_SPORT_PID, "Sport"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_2_PRO_PID, "Pro"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID, "Ultimate"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_ACTIVE_PEDAL_PID, "ActivePedal"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_WIRELESS_WHEEL_PID, "Wireless Wheel"),
        // Simagic EVO (VID 0x3670)
        (0x3670, racing_wheel_hid_simagic_protocol::product_ids::EVO_SPORT, "EVO Sport"),
        (0x3670, racing_wheel_hid_simagic_protocol::product_ids::EVO, "EVO"),
        (0x3670, racing_wheel_hid_simagic_protocol::product_ids::EVO_PRO, "EVO Pro"),
        // FFBeast (VID 0x045B)
        (0x045B, racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_JOYSTICK, "Joystick"),
        (0x045B, racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_RUDDER, "Rudder"),
        (0x045B, racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL, "Wheel"),
        // Asetek (VID 0x2433)
        (0x2433, hid_asetek_protocol::ASETEK_INVICTA_PID, "Invicta"),
        (0x2433, hid_asetek_protocol::ASETEK_FORTE_PID, "Forte"),
        (0x2433, hid_asetek_protocol::ASETEK_LAPRIMA_PID, "La Prima"),
        (0x2433, hid_asetek_protocol::ASETEK_TONY_KANAAN_PID, "Tony Kanaan"),
    ];

    // Group by VID and check for PID collisions
    let mut vid_pids: HashMap<u16, Vec<(u16, &str)>> = HashMap::new();
    for (vid, pid, label) in all_products {
        vid_pids.entry(*vid).or_default().push((*pid, label));
    }

    let mut collisions: Vec<String> = Vec::new();
    for (vid, entries) in &vid_pids {
        let mut seen: HashMap<u16, &str> = HashMap::new();
        for (pid, label) in entries {
            if let Some(existing) = seen.get(pid) {
                collisions.push(format!(
                    "VID 0x{vid:04X}: PID 0x{pid:04X} claimed by both '{existing}' and '{label}'"
                ));
            } else {
                seen.insert(*pid, label);
            }
        }
    }

    assert!(
        collisions.is_empty(),
        "PID collisions detected within vendor:\n  {}",
        collisions.join("\n  ")
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Telemetry adapter game IDs match config/registry
// ═════════════════════════════════════════════════════════════════════════════

/// Every game ID returned by `adapter_factories()` must appear in both
/// telemetry YAML config files. This is a cross-crate consistency check
/// between the telemetry-adapters, telemetry-config, and telemetry-support
/// crates.
#[test]
fn telemetry_adapter_game_ids_in_config_registry() -> TestResult {
    let config_ids = racing_wheel_telemetry_config::matrix_game_id_set()?;
    let support_ids = racing_wheel_telemetry_support::matrix_game_id_set()?;

    let mut missing: Vec<String> = Vec::new();

    for (game_id, _) in adapter_factories() {
        if !config_ids.contains(*game_id) {
            missing.push(format!("'{game_id}' missing from telemetry-config matrix"));
        }
        if !support_ids.contains(*game_id) {
            missing.push(format!("'{game_id}' missing from telemetry-support matrix"));
        }
    }

    assert!(
        missing.is_empty(),
        "Telemetry adapter game IDs not found in config/registry:\n  {}",
        missing.join("\n  ")
    );

    Ok(())
}

/// Adapter factory game IDs must be non-empty and contain no whitespace-only
/// entries.
#[test]
fn telemetry_adapter_game_ids_are_non_empty() -> TestResult {
    let mut invalid: Vec<String> = Vec::new();

    for (game_id, _) in adapter_factories() {
        if game_id.is_empty() || game_id.trim().is_empty() {
            invalid.push("empty or whitespace-only game ID found".to_string());
        }
    }

    assert!(
        invalid.is_empty(),
        "Invalid telemetry adapter game IDs:\n  {}",
        invalid.join("\n  ")
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Device names: no empty strings or placeholder text
// ═════════════════════════════════════════════════════════════════════════════

/// Device display names must not be empty, whitespace-only, or contain
/// common placeholder text (TODO, FIXME, placeholder, TBD, unknown device).
#[test]
fn protocol_device_names_are_valid() -> TestResult {
    let placeholder_patterns: &[&str] = &[
        "todo", "fixme", "placeholder", "tbd", "xxx", "n/a",
    ];

    // Collect (name, source) pairs from all protocol crates that expose
    // display_name / product_name / name functions.
    let names: Vec<(&str, &str)> = vec![
        // Thrustmaster Model::name()
        (racing_wheel_hid_thrustmaster_protocol::Model::T150.name(), "Thrustmaster T150"),
        (racing_wheel_hid_thrustmaster_protocol::Model::T300RS.name(), "Thrustmaster T300RS"),
        (racing_wheel_hid_thrustmaster_protocol::Model::T818.name(), "Thrustmaster T818"),
        (racing_wheel_hid_thrustmaster_protocol::Model::TSPCRacer.name(), "Thrustmaster TSPCRacer"),
        (racing_wheel_hid_thrustmaster_protocol::Model::T80.name(), "Thrustmaster T80"),
        // Simucube SimucubeModel::display_name()
        (hid_simucube_protocol::SimucubeModel::Simucube1.display_name(), "Simucube 1"),
        (hid_simucube_protocol::SimucubeModel::Sport.display_name(), "Simucube Sport"),
        (hid_simucube_protocol::SimucubeModel::Pro.display_name(), "Simucube Pro"),
        (hid_simucube_protocol::SimucubeModel::Ultimate.display_name(), "Simucube Ultimate"),
        // Asetek AsetekModel::display_name()
        (hid_asetek_protocol::AsetekModel::Forte.display_name(), "Asetek Forte"),
        (hid_asetek_protocol::AsetekModel::Invicta.display_name(), "Asetek Invicta"),
        (hid_asetek_protocol::AsetekModel::LaPrima.display_name(), "Asetek LaPrima"),
        (hid_asetek_protocol::AsetekModel::TonyKanaan.display_name(), "Asetek TonyKanaan"),
        // Heusinkveld HeusinkveldModel::display_name()
        (hid_heusinkveld_protocol::HeusinkveldModel::Sprint.display_name(), "Heusinkveld Sprint"),
        (hid_heusinkveld_protocol::HeusinkveldModel::Ultimate.display_name(), "Heusinkveld Ultimate"),
        (hid_heusinkveld_protocol::HeusinkveldModel::Pro.display_name(), "Heusinkveld Pro"),
        // Cube Controls CubeControlsModel::display_name()
        (hid_cube_controls_protocol::CubeControlsModel::GtPro.display_name(), "CC GT Pro"),
        (hid_cube_controls_protocol::CubeControlsModel::FormulaPro.display_name(), "CC Formula Pro"),
        (hid_cube_controls_protocol::CubeControlsModel::Csx3.display_name(), "CC CSX3"),
        // OpenFFBoard OpenFFBoardVariant::name()
        (racing_wheel_hid_openffboard_protocol::OpenFFBoardVariant::Main.name(), "OpenFFBoard Main"),
        (racing_wheel_hid_openffboard_protocol::OpenFFBoardVariant::Alternate.name(), "OpenFFBoard Alt"),
        // PXN product_name()
        (
            racing_wheel_hid_pxn_protocol::product_name(racing_wheel_hid_pxn_protocol::PRODUCT_V10)
                .unwrap_or(""),
            "PXN V10",
        ),
        (
            racing_wheel_hid_pxn_protocol::product_name(racing_wheel_hid_pxn_protocol::PRODUCT_V12)
                .unwrap_or(""),
            "PXN V12",
        ),
        // Cammus product_name()
        (
            racing_wheel_hid_cammus_protocol::product_name(racing_wheel_hid_cammus_protocol::PRODUCT_C5)
                .unwrap_or(""),
            "Cammus C5",
        ),
        (
            racing_wheel_hid_cammus_protocol::product_name(racing_wheel_hid_cammus_protocol::PRODUCT_C12)
                .unwrap_or(""),
            "Cammus C12",
        ),
    ];

    let mut issues: Vec<String> = Vec::new();
    for (name, source) in &names {
        if name.is_empty() || name.trim().is_empty() {
            issues.push(format!("{source}: device name is empty or whitespace-only"));
            continue;
        }
        let lower = name.to_lowercase();
        for pattern in placeholder_patterns {
            if lower.contains(pattern) {
                issues.push(format!(
                    "{source}: device name '{name}' contains placeholder text '{pattern}'"
                ));
            }
        }
    }

    assert!(
        issues.is_empty(),
        "Invalid device names detected:\n  {}",
        issues.join("\n  ")
    );

    Ok(())
}

/// VID constants themselves must be non-zero (0x0000 is not a valid USB VID).
#[test]
fn protocol_vendor_ids_are_non_zero() -> TestResult {
    let vids: &[(u16, &str)] = &[
        (racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID, "Logitech"),
        (racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID, "Fanatec"),
        (racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID, "Thrustmaster"),
        (racing_wheel_hid_simagic_protocol::SIMAGIC_VENDOR_ID, "Simagic"),
        (racing_wheel_hid_moza_protocol::MOZA_VENDOR_ID, "Moza"),
        (racing_wheel_hid_pxn_protocol::VENDOR_ID, "PXN"),
        (racing_wheel_hid_cammus_protocol::VENDOR_ID, "Cammus"),
        (hid_simucube_protocol::VENDOR_ID, "Simucube"),
        (racing_wheel_hid_vrs_protocol::VRS_VENDOR_ID, "VRS"),
        (hid_asetek_protocol::VENDOR_ID, "Asetek"),
        (racing_wheel_hid_accuforce_protocol::VENDOR_ID, "AccuForce"),
        (racing_wheel_hid_leo_bodnar_protocol::VENDOR_ID, "Leo Bodnar"),
        (racing_wheel_hid_ffbeast_protocol::FFBEAST_VENDOR_ID, "FFBeast"),
        (racing_wheel_hid_openffboard_protocol::OPENFFBOARD_VENDOR_ID, "OpenFFBoard"),
        (hid_cube_controls_protocol::CUBE_CONTROLS_VENDOR_ID, "Cube Controls"),
        (hid_heusinkveld_protocol::HEUSINKVELD_VENDOR_ID, "Heusinkveld"),
    ];

    let mut bad: Vec<&str> = Vec::new();
    for (vid, label) in vids {
        if *vid == 0 {
            bad.push(label);
        }
    }

    assert!(
        bad.is_empty(),
        "Vendor IDs must be non-zero. Invalid vendors: {:?}",
        bad
    );

    Ok(())
}

/// Cross-vendor PID collision guard: when multiple vendors share VID `0x0483`
/// (STMicroelectronics), their PIDs must not overlap.
#[test]
fn shared_stm_vid_pids_do_not_collide() -> TestResult {
    // VRS PIDs on shared STM VID
    let vrs_pids: &[(u16, &str)] = &[
        (racing_wheel_hid_vrs_protocol::VRS_PRODUCT_ID, "VRS DFP"),
        (racing_wheel_hid_vrs_protocol::product_ids::DIRECTFORCE_PRO_V2, "VRS DFP V2"),
        (racing_wheel_hid_vrs_protocol::product_ids::R295, "VRS R295"),
        (racing_wheel_hid_vrs_protocol::product_ids::PEDALS, "VRS Pedals"),
    ];

    // Cube Controls PIDs on shared STM VID
    let cc_pids: &[(u16, &str)] = &[
        (hid_cube_controls_protocol::CUBE_CONTROLS_GT_PRO_PID, "CC GT Pro"),
        (hid_cube_controls_protocol::CUBE_CONTROLS_FORMULA_PRO_PID, "CC Formula Pro"),
        (hid_cube_controls_protocol::CUBE_CONTROLS_CSX3_PID, "CC CSX3"),
    ];

    // Simagic legacy PID on shared STM VID
    let simagic_pids: &[(u16, &str)] = &[
        (racing_wheel_hid_simagic_protocol::ids::SIMAGIC_LEGACY_PID, "Simagic Legacy"),
    ];

    let mut all_pids: HashMap<u16, Vec<&str>> = HashMap::new();
    for group in [vrs_pids, cc_pids, simagic_pids] {
        for (pid, label) in group {
            all_pids.entry(*pid).or_default().push(label);
        }
    }

    let mut collisions: Vec<String> = Vec::new();
    for (pid, owners) in &all_pids {
        if owners.len() > 1 {
            collisions.push(format!(
                "PID 0x{pid:04X} claimed by: {}",
                owners.join(", ")
            ));
        }
    }

    assert!(
        collisions.is_empty(),
        "PID collisions on shared STM VID 0x0483:\n  {}",
        collisions.join("\n  ")
    );

    Ok(())
}

/// Telemetry adapter game IDs must be unique (no two adapters claim the
/// same game ID).
#[test]
fn telemetry_adapter_game_ids_are_unique() -> TestResult {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut duplicates: Vec<&str> = Vec::new();

    for (game_id, _) in adapter_factories() {
        if !seen.insert(game_id) {
            duplicates.push(game_id);
        }
    }

    assert!(
        duplicates.is_empty(),
        "Duplicate telemetry adapter game IDs: {:?}",
        duplicates
    );

    Ok(())
}
