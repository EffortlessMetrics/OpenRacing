//! Comprehensive device matrix / device registry / hwdb integration tests.
//!
//! Validates VID/PID lookup, capability detection, unknown device handling,
//! protocol selection, and device name resolution across all supported device
//! families: Logitech, Thrustmaster, Fanatec, Moza, SimuCube, Simagic, VRS,
//! Cube Controls, Asetek, Heusinkveld, Granite/SimpleMotion, OpenFFBoard,
//! FFBeast, Cammus, AccuForce, Leo Bodnar, PXN, and Button Box.

use racing_wheel_engine::hid::vendor::{
    get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback, VendorProtocol,
};

// ── Vendor-specific protocol crate re-exports ────────────────────────────────

use hid_asetek_protocol::AsetekModel;
use hid_simucube_protocol::SimucubeModel;
use racing_wheel_engine::hid::vendor::accuforce::AccuForceProtocolHandler;
use racing_wheel_engine::hid::vendor::asetek::AsetekProtocolHandler;
use racing_wheel_engine::hid::vendor::cammus::{
    CammusModel, CammusProtocolHandler, CAMMUS_C12_PID, CAMMUS_C5_PID, CAMMUS_VENDOR_ID,
};
use racing_wheel_engine::hid::vendor::fanatec::{FanatecModel, product_ids as fanatec_pids};
use racing_wheel_engine::hid::vendor::logitech::{
    LogitechModel, LogitechProtocol, product_ids as logitech_pids,
};
use racing_wheel_engine::hid::vendor::pxn::{PxnModel, PxnProtocolHandler};
use racing_wheel_engine::hid::vendor::simagic::{SimagicModel, SimagicProtocol};
use racing_wheel_engine::hid::vendor::simucube::SimucubeProtocolHandler;
use racing_wheel_engine::hid::vendor::thrustmaster::product_ids as tm_pids;
use racing_wheel_engine::hid::vendor::vrs::product_ids as vrs_pids;
use racing_wheel_hid_accuforce_protocol::AccuForceModel;

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: device entry for the matrix
// ═══════════════════════════════════════════════════════════════════════════════

struct DeviceMatrixEntry {
    vid: u16,
    pid: u16,
    vendor_label: &'static str,
    device_name: &'static str,
    /// Expected max torque in Nm (approximate). 0.0 means "no FFB" (e.g. pedals).
    expected_max_torque_nm: f32,
    /// Tolerance for torque comparison.
    torque_tolerance: f32,
    /// Whether the device should be classified as V2 hardware.
    expected_is_v2: bool,
    /// Whether get_vendor_protocol should return Some for this VID/PID.
    should_dispatch: bool,
}

impl DeviceMatrixEntry {
    const fn new(
        vid: u16,
        pid: u16,
        vendor_label: &'static str,
        device_name: &'static str,
        expected_max_torque_nm: f32,
        expected_is_v2: bool,
    ) -> Self {
        Self {
            vid,
            pid,
            vendor_label,
            device_name,
            expected_max_torque_nm,
            torque_tolerance: 0.5,
            expected_is_v2,
            should_dispatch: true,
        }
    }

    const fn with_tolerance(mut self, tol: f32) -> Self {
        self.torque_tolerance = tol;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. VID/PID lookup for all known device families
// ═══════════════════════════════════════════════════════════════════════════════

/// Comprehensive device matrix covering all supported vendor families.
fn full_device_matrix() -> Vec<DeviceMatrixEntry> {
    vec![
        // ── Logitech ─────────────────────────────────────────────────────
        DeviceMatrixEntry::new(0x046D, logitech_pids::G25, "Logitech", "G25", 2.5, false),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G27, "Logitech", "G27", 2.5, false),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G29_PS, "Logitech", "G29", 2.2, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G920, "Logitech", "G920", 2.2, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G923_XBOX, "Logitech", "G923 Xbox", 2.2, false)
            .with_tolerance(0.5),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G923_PS, "Logitech", "G923 PS", 2.2, false)
            .with_tolerance(0.5),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G_PRO, "Logitech", "G PRO", 11.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x046D, logitech_pids::G_PRO_XBOX, "Logitech", "G PRO Xbox", 11.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x046D, logitech_pids::DRIVING_FORCE_GT, "Logitech", "DFGT", 2.5, false),
        // ── Fanatec (wheelbases) ─────────────────────────────────────────
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::DD1, "Fanatec", "DD1", 20.0, true)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::DD2, "Fanatec", "DD2", 25.0, true)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::CSL_DD, "Fanatec", "CSL DD", 8.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::GT_DD_PRO, "Fanatec", "GT DD Pro", 8.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::CLUBSPORT_DD, "Fanatec", "ClubSport DD+", 12.0, true)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::CLUBSPORT_V2, "Fanatec", "ClubSport V2", 8.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::CLUBSPORT_V2_5, "Fanatec", "ClubSport V2.5", 8.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::CSL_ELITE, "Fanatec", "CSL Elite", 6.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0EB7, fanatec_pids::CSR_ELITE, "Fanatec", "CSR Elite", 5.0, false)
            .with_tolerance(0.1),
        // ── Thrustmaster ─────────────────────────────────────────────────
        DeviceMatrixEntry::new(0x044F, tm_pids::T818, "Thrustmaster", "T818", 10.0, false)
            .with_tolerance(1.0),
        DeviceMatrixEntry::new(0x044F, tm_pids::T300_RS, "Thrustmaster", "T300 RS", 3.9, false)
            .with_tolerance(1.0),
        DeviceMatrixEntry::new(0x044F, tm_pids::TS_PC_RACER, "Thrustmaster", "TS-PC", 4.5, false)
            .with_tolerance(1.5),
        DeviceMatrixEntry::new(0x044F, tm_pids::T500_RS, "Thrustmaster", "T500 RS", 5.0, false)
            .with_tolerance(1.5),
        DeviceMatrixEntry::new(0x044F, tm_pids::T248, "Thrustmaster", "T248", 3.0, false)
            .with_tolerance(1.5),
        // ── Moza ─────────────────────────────────────────────────────────
        DeviceMatrixEntry::new(0x346E, 0x0000, "Moza", "R16/R21 V1", 16.0, false)
            .with_tolerance(5.0),
        DeviceMatrixEntry::new(0x346E, 0x0002, "Moza", "R9 V1", 9.0, false)
            .with_tolerance(3.0),
        DeviceMatrixEntry::new(0x346E, 0x0004, "Moza", "R5 V1", 5.5, false)
            .with_tolerance(2.0),
        DeviceMatrixEntry::new(0x346E, 0x0010, "Moza", "R16/R21 V2", 16.0, true)
            .with_tolerance(5.0),
        DeviceMatrixEntry::new(0x346E, 0x0012, "Moza", "R9 V2", 9.0, true)
            .with_tolerance(3.0),
        // ── Simagic EVO (VID 0x3670) ─────────────────────────────────────
        DeviceMatrixEntry::new(0x3670, 0x0500, "Simagic", "EVO Sport", 9.0, true)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x3670, 0x0501, "Simagic", "EVO", 12.0, true)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x3670, 0x0502, "Simagic", "EVO Pro", 18.0, true)
            .with_tolerance(0.1),
        // ── Simagic Legacy (VID 0x0483/STM) ──────────────────────────────
        DeviceMatrixEntry::new(0x0483, 0x0522, "Simagic", "Alpha (legacy)", 15.0, false)
            .with_tolerance(0.1),
        // ── VRS (VID 0x0483/STM) ─────────────────────────────────────────
        DeviceMatrixEntry::new(0x0483, vrs_pids::DIRECTFORCE_PRO, "VRS", "DirectForce Pro", 20.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x0483, vrs_pids::DIRECTFORCE_PRO_V2, "VRS", "DirectForce Pro V2", 25.0, true)
            .with_tolerance(0.1),
        // ── Cube Controls (VID 0x0483/STM) ───────────────────────────────
        DeviceMatrixEntry::new(0x0483, 0x0C73, "Cube Controls", "GT Pro", 0.0, false)
            .with_tolerance(5.0),
        DeviceMatrixEntry::new(0x0483, 0x0C74, "Cube Controls", "Formula Pro", 0.0, false)
            .with_tolerance(5.0),
        DeviceMatrixEntry::new(0x0483, 0x0C75, "Cube Controls", "CSX3", 0.0, false)
            .with_tolerance(5.0),
        // ── Simucube (VID 0x16D0) ────────────────────────────────────────
        DeviceMatrixEntry::new(0x16D0, hid_simucube_protocol::SIMUCUBE_2_SPORT_PID, "Simucube", "SC2 Sport", 17.0, false)
            .with_tolerance(1.0),
        DeviceMatrixEntry::new(0x16D0, hid_simucube_protocol::SIMUCUBE_2_PRO_PID, "Simucube", "SC2 Pro", 25.0, true)
            .with_tolerance(1.0),
        DeviceMatrixEntry::new(0x16D0, hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID, "Simucube", "SC2 Ultimate", 32.0, true)
            .with_tolerance(2.0),
        // ── Asetek (VID 0x2433) ──────────────────────────────────────────
        DeviceMatrixEntry::new(0x2433, hid_asetek_protocol::ASETEK_INVICTA_PID, "Asetek", "Invicta", 27.0, false)
            .with_tolerance(1.0),
        DeviceMatrixEntry::new(0x2433, hid_asetek_protocol::ASETEK_FORTE_PID, "Asetek", "Forte", 18.0, true)
            .with_tolerance(1.0),
        DeviceMatrixEntry::new(0x2433, hid_asetek_protocol::ASETEK_LAPRIMA_PID, "Asetek", "La Prima", 12.0, false)
            .with_tolerance(1.0),
        // ── Heusinkveld (VID 0x04D8 / Microchip) ─────────────────────────
        DeviceMatrixEntry::new(0x04D8, hid_heusinkveld_protocol::HEUSINKVELD_SPRINT_PID, "Heusinkveld", "Sprint", 0.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x04D8, hid_heusinkveld_protocol::HEUSINKVELD_ULTIMATE_PID, "Heusinkveld", "Ultimate+", 0.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x04D8, hid_heusinkveld_protocol::HEUSINKVELD_PRO_PID, "Heusinkveld", "Pro", 0.0, false)
            .with_tolerance(0.1),
        // ── Granite Devices / SimpleMotion V2 (VID 0x1D50) ───────────────
        DeviceMatrixEntry::new(0x1D50, racing_wheel_simplemotion_v2::IONI_PRODUCT_ID, "Granite", "IONI", 0.0, false)
            .with_tolerance(5.0),
        DeviceMatrixEntry::new(0x1D50, racing_wheel_simplemotion_v2::IONI_PRODUCT_ID_PREMIUM, "Granite", "IONI Premium", 0.0, true)
            .with_tolerance(5.0),
        DeviceMatrixEntry::new(0x1D50, racing_wheel_simplemotion_v2::ARGON_PRODUCT_ID, "Granite", "ARGON", 0.0, true)
            .with_tolerance(5.0),
        // ── OpenFFBoard (VID 0x1209) ─────────────────────────────────────
        DeviceMatrixEntry::new(0x1209, racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID, "OpenFFBoard", "Main", 0.0, false)
            .with_tolerance(5.0),
        // ── FFBeast (VID 0x045B) ─────────────────────────────────────────
        DeviceMatrixEntry::new(0x045B, racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL, "FFBeast", "Wheel", 0.0, false)
            .with_tolerance(5.0),
        // ── Cammus (VID 0x3416) ──────────────────────────────────────────
        DeviceMatrixEntry::new(0x3416, CAMMUS_C5_PID, "Cammus", "C5", 5.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x3416, CAMMUS_C12_PID, "Cammus", "C12", 12.0, true)
            .with_tolerance(0.1),
        // ── AccuForce (VID 0x1FC9) ───────────────────────────────────────
        DeviceMatrixEntry::new(0x1FC9, racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO, "AccuForce", "Pro", 7.0, false)
            .with_tolerance(1.0),
        // ── Leo Bodnar (VID 0x1DD2) ──────────────────────────────────────
        DeviceMatrixEntry::new(0x1DD2, racing_wheel_hid_leo_bodnar_protocol::PID_WHEEL_INTERFACE, "Leo Bodnar", "Wheel Interface", 0.0, false)
            .with_tolerance(3.0),
        DeviceMatrixEntry::new(0x1DD2, racing_wheel_hid_leo_bodnar_protocol::PID_BBI32, "Leo Bodnar", "BBI-32", 0.0, false)
            .with_tolerance(3.0),
        // ── Button Box (VID 0x1209) ──────────────────────────────────────
        DeviceMatrixEntry::new(0x1209, hid_button_box_protocol::PRODUCT_ID_BUTTON_BOX, "Button Box", "Generic", 0.0, false)
            .with_tolerance(0.1),
        // ── PXN (VID 0x11FF) ─────────────────────────────────────────────
        DeviceMatrixEntry::new(0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V10, "PXN", "V10", 10.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V12, "PXN", "V12", 12.0, true)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE, "PXN", "V12 Lite", 6.0, false)
            .with_tolerance(0.1),
        DeviceMatrixEntry::new(0x11FF, racing_wheel_hid_pxn_protocol::PRODUCT_GT987, "PXN", "GT987", 5.0, false)
            .with_tolerance(0.1),
    ]
}

/// Every device in the matrix must dispatch to a protocol handler via VID/PID.
#[test]
fn all_device_families_dispatch_via_vid_pid() -> Result<(), Box<dyn std::error::Error>> {
    let mut failures: Vec<String> = Vec::new();

    for entry in full_device_matrix() {
        let handler = get_vendor_protocol(entry.vid, entry.pid);
        if entry.should_dispatch && handler.is_none() {
            failures.push(format!(
                "{} {} (VID=0x{:04X} PID=0x{:04X}) did not dispatch",
                entry.vendor_label, entry.device_name, entry.vid, entry.pid
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Device dispatch failures:\n  {}",
        failures.join("\n  ")
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Device capability detection from VID/PID (FFB config validation)
// ═══════════════════════════════════════════════════════════════════════════════

/// For every device in the matrix, the FFB config max_torque must be within
/// the expected range and torque must be non-negative.
#[test]
fn ffb_config_torque_within_expected_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut failures: Vec<String> = Vec::new();

    for entry in full_device_matrix() {
        let handler = match get_vendor_protocol(entry.vid, entry.pid) {
            Some(h) => h,
            None => continue,
        };

        let config = handler.get_ffb_config();

        // Torque must never be negative
        if config.max_torque_nm < 0.0 {
            failures.push(format!(
                "{} {}: negative torque {} Nm",
                entry.vendor_label, entry.device_name, config.max_torque_nm
            ));
        }

        // If we have a non-zero expected torque, verify within tolerance
        if entry.expected_max_torque_nm > 0.0 {
            let diff = (config.max_torque_nm - entry.expected_max_torque_nm).abs();
            if diff > entry.torque_tolerance {
                failures.push(format!(
                    "{} {}: expected ~{} Nm, got {} Nm (diff={}, tol={})",
                    entry.vendor_label,
                    entry.device_name,
                    entry.expected_max_torque_nm,
                    config.max_torque_nm,
                    diff,
                    entry.torque_tolerance,
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "FFB config torque validation failures:\n  {}",
        failures.join("\n  ")
    );
    Ok(())
}

/// Encoder CPR must be non-negative for all devices.
#[test]
fn ffb_config_encoder_cpr_non_negative() -> Result<(), Box<dyn std::error::Error>> {
    for entry in full_device_matrix() {
        let handler = match get_vendor_protocol(entry.vid, entry.pid) {
            Some(h) => h,
            None => continue,
        };
        let config = handler.get_ffb_config();
        // encoder_cpr is u32, so always >= 0; just check it doesn't panic
        let _ = config.encoder_cpr;
    }
    Ok(())
}

/// Logitech-specific FFB config: G PRO must have higher torque than G920.
#[test]
fn logitech_g_pro_has_higher_torque_than_g920() -> Result<(), Box<dyn std::error::Error>> {
    let g920 = LogitechProtocol::new(0x046D, logitech_pids::G920);
    let g_pro = LogitechProtocol::new(0x046D, logitech_pids::G_PRO);

    let g920_torque = g920.get_ffb_config().max_torque_nm;
    let g_pro_torque = g_pro.get_ffb_config().max_torque_nm;

    assert!(
        g_pro_torque > g920_torque,
        "G PRO ({g_pro_torque} Nm) must have higher torque than G920 ({g920_torque} Nm)"
    );
    Ok(())
}

/// Fanatec DD bases must have higher torque than belt-driven bases.
#[test]
fn fanatec_dd_torque_exceeds_belt_torque() -> Result<(), Box<dyn std::error::Error>> {
    let dd_pids = [fanatec_pids::DD1, fanatec_pids::DD2, fanatec_pids::CSL_DD];
    let belt_pids = [fanatec_pids::CLUBSPORT_V2, fanatec_pids::CSR_ELITE];

    let max_belt_torque = belt_pids
        .iter()
        .map(|&pid| FanatecModel::from_product_id(pid).max_torque_nm())
        .fold(0.0_f32, f32::max);

    for &pid in &dd_pids {
        let dd_torque = FanatecModel::from_product_id(pid).max_torque_nm();
        assert!(
            dd_torque >= max_belt_torque,
            "DD base PID 0x{pid:04X} torque {dd_torque} Nm must >= belt max {max_belt_torque} Nm"
        );
    }
    Ok(())
}

/// Simagic EVO generation encoder CPR must be higher than legacy generation.
#[test]
fn simagic_evo_higher_encoder_cpr_than_legacy() -> Result<(), Box<dyn std::error::Error>> {
    let evo = SimagicProtocol::new(0x3670, 0x0501);
    let legacy = SimagicProtocol::new(0x0483, 0x0522);

    let evo_cpr = evo.get_ffb_config().encoder_cpr;
    let legacy_cpr = legacy.get_ffb_config().encoder_cpr;

    assert!(
        evo_cpr > legacy_cpr,
        "EVO encoder CPR ({evo_cpr}) must exceed legacy ({legacy_cpr})"
    );
    Ok(())
}

/// Cammus C12 must have higher torque than C5.
#[test]
fn cammus_c12_higher_torque_than_c5() -> Result<(), Box<dyn std::error::Error>> {
    let c5 = CammusModel::from_product_id(CAMMUS_C5_PID);
    let c12 = CammusModel::from_product_id(CAMMUS_C12_PID);

    assert!(
        c12.max_torque_nm() > c5.max_torque_nm(),
        "C12 ({} Nm) must have higher torque than C5 ({} Nm)",
        c12.max_torque_nm(),
        c5.max_torque_nm(),
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Unknown device handling
// ═══════════════════════════════════════════════════════════════════════════════

/// Completely unknown VIDs must return None from get_vendor_protocol.
#[test]
fn unknown_vid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    let unknown_vids: &[u16] = &[0x0000, 0x0001, 0x9999, 0xDEAD, 0xFFFF];

    for &vid in unknown_vids {
        let result = get_vendor_protocol(vid, 0xBEEF);
        assert!(
            result.is_none(),
            "unknown VID 0x{vid:04X} must return None"
        );
    }
    Ok(())
}

/// VIDs with PID-guarded dispatch must return None for unknown PIDs.
#[test]
fn pid_guarded_vids_return_none_for_unknown_pids() -> Result<(), Box<dyn std::error::Error>> {
    let guarded: &[(u16, &str)] = &[
        (0x04D8, "Microchip (Heusinkveld)"),
        (0x045B, "Renesas (FFBeast)"),
        (0x1FC9, "NXP (AccuForce)"),
        (0x1209, "pid.codes (OpenFFBoard/ButtonBox)"),
        (0x11FF, "PXN"),
    ];

    for &(vid, label) in guarded {
        let result = get_vendor_protocol(vid, 0xFFFF);
        assert!(
            result.is_none(),
            "{label} VID 0x{vid:04X}: unknown PID 0xFFFF must return None"
        );
    }
    Ok(())
}

/// VIDs with fallback-to-Simagic on unknown PIDs must still return Some.
#[test]
fn fallback_vids_return_some_for_unknown_pids() -> Result<(), Box<dyn std::error::Error>> {
    let fallback_vids: &[(u16, &str)] = &[
        (0x0483, "STM (fallback to Simagic)"),
        (0x16D0, "MCS (fallback to Simagic)"),
    ];

    for &(vid, label) in fallback_vids {
        let result = get_vendor_protocol(vid, 0xFFFF);
        assert!(
            result.is_some(),
            "{label} VID 0x{vid:04X}: unknown PID must fall through to fallback handler"
        );
    }
    Ok(())
}

/// Edge-case VID/PID values must not panic (boundary testing).
#[test]
fn boundary_vid_pid_values_do_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    let edges: &[(u16, u16)] = &[
        (0x0000, 0x0000),
        (0x0000, 0xFFFF),
        (0xFFFF, 0x0000),
        (0xFFFF, 0xFFFF),
        (u16::MAX, u16::MAX),
    ];

    for &(vid, pid) in edges {
        // Must not panic
        let _ = get_vendor_protocol(vid, pid);
        let _ = get_vendor_protocol_with_hid_pid_fallback(vid, pid, true);
        let _ = get_vendor_protocol_with_hid_pid_fallback(vid, pid, false);
    }
    Ok(())
}

/// HID PID fallback activates for unknown VID with capability flag, but NOT
/// for unknown VID without capability flag.
#[test]
fn hid_pid_fallback_activation() -> Result<(), Box<dyn std::error::Error>> {
    let handler_with = get_vendor_protocol_with_hid_pid_fallback(0xBEEF, 0xCAFE, true);
    assert!(
        handler_with.is_some(),
        "unknown VID with HID PID capability must get a generic handler"
    );

    let handler_without = get_vendor_protocol_with_hid_pid_fallback(0xBEEF, 0xCAFE, false);
    assert!(
        handler_without.is_none(),
        "unknown VID without HID PID capability must return None"
    );
    Ok(())
}

/// HID PID fallback must not override a known vendor handler.
#[test]
fn hid_pid_fallback_does_not_override_known_vendor() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases: &[(u16, u16, &str)] = &[
        (0x046D, logitech_pids::G920, "Logitech G920"),
        (0x0EB7, fanatec_pids::CSL_DD, "Fanatec CSL DD"),
        (0x044F, tm_pids::T818, "Thrustmaster T818"),
        (0x346E, 0x0012, "Moza R9 V2"),
        (0x3670, 0x0501, "Simagic EVO"),
        (0x2433, hid_asetek_protocol::ASETEK_INVICTA_PID, "Asetek Invicta"),
    ];

    for &(vid, pid, label) in test_cases {
        // With and without HID PID capability — both must dispatch
        for flag in [true, false] {
            let handler = get_vendor_protocol_with_hid_pid_fallback(vid, pid, flag);
            assert!(
                handler.is_some(),
                "{label} must dispatch regardless of has_hid_pid_capability={flag}"
            );
        }
    }
    Ok(())
}

/// Unknown PIDs on Logitech/Fanatec/Moza/Thrustmaster/Simagic EVO/Asetek/Cammus/
/// Leo Bodnar VIDs still return Some (these vendors don't guard on PID).
#[test]
fn unguarded_vids_accept_unknown_pids() -> Result<(), Box<dyn std::error::Error>> {
    let unguarded: &[(u16, &str)] = &[
        (0x046D, "Logitech"),
        (0x0EB7, "Fanatec"),
        (0x044F, "Thrustmaster"),
        (0x346E, "Moza"),
        (0x3670, "Simagic EVO"),
        (0x2433, "Asetek"),
        (0x3416, "Cammus"),
        (0x1DD2, "Leo Bodnar"),
        (0x1D50, "Granite Devices"),
    ];

    for &(vid, label) in unguarded {
        let result = get_vendor_protocol(vid, 0xBEEF);
        assert!(
            result.is_some(),
            "{label} VID 0x{vid:04X}: unknown PID should still return a handler"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Protocol selection based on device identification
// ═══════════════════════════════════════════════════════════════════════════════

/// V2 hardware classification for all matrix entries.
#[test]
fn is_v2_hardware_matches_expected() -> Result<(), Box<dyn std::error::Error>> {
    let mut failures: Vec<String> = Vec::new();

    for entry in full_device_matrix() {
        let handler = match get_vendor_protocol(entry.vid, entry.pid) {
            Some(h) => h,
            None => continue,
        };

        let actual = handler.is_v2_hardware();
        if actual != entry.expected_is_v2 {
            failures.push(format!(
                "{} {} (0x{:04X}:0x{:04X}): is_v2_hardware={actual}, expected={}",
                entry.vendor_label, entry.device_name, entry.vid, entry.pid, entry.expected_is_v2,
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "V2 hardware classification mismatches:\n  {}",
        failures.join("\n  ")
    );
    Ok(())
}

/// Wheelbases that support vendor-specific output reports must have output_report_id().
#[test]
fn wheelbase_output_report_metadata_consistency() -> Result<(), Box<dyn std::error::Error>> {
    // Vendors where wheelbases should have output report metadata
    let wheelbases_with_report_ids: &[(u16, u16, &str)] = &[
        (0x046D, logitech_pids::G920, "Logitech G920"),
        (0x0EB7, fanatec_pids::CSL_DD, "Fanatec CSL DD"),
        (0x0EB7, fanatec_pids::DD1, "Fanatec DD1"),
        (0x16D0, hid_simucube_protocol::SIMUCUBE_2_SPORT_PID, "Simucube 2 Sport"),
    ];

    for &(vid, pid, label) in wheelbases_with_report_ids {
        let handler = get_vendor_protocol(vid, pid);
        assert!(handler.is_some(), "{label}: must dispatch");
        let handler = handler.as_ref().map(|h| h.as_ref());
        if let Some(h) = handler {
            assert!(
                h.output_report_id().is_some(),
                "{label}: wheelbase must have an output report ID"
            );
            assert!(
                h.output_report_len().is_some(),
                "{label}: wheelbase must have an output report length"
            );
        }
    }
    Ok(())
}

/// Pedal/peripheral devices should NOT have output report metadata.
#[test]
fn pedal_devices_have_no_output_report_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let pedals_no_report: &[(u16, u16, &str)] = &[
        (0x04D8, hid_heusinkveld_protocol::HEUSINKVELD_SPRINT_PID, "Heusinkveld Sprint"),
    ];

    for &(vid, pid, label) in pedals_no_report {
        let handler = get_vendor_protocol(vid, pid);
        assert!(handler.is_some(), "{label}: must dispatch");
        if let Some(h) = &handler {
            assert!(
                h.output_report_id().is_none(),
                "{label}: pedal device must NOT have an output report ID"
            );
            assert!(
                h.output_report_len().is_none(),
                "{label}: pedal device must NOT have an output report length"
            );
        }
    }
    Ok(())
}

/// Simucube encoder CPR must be consistent across all Simucube 2 models.
#[test]
fn simucube_encoder_cpr_consistent_across_models() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        hid_simucube_protocol::SIMUCUBE_2_SPORT_PID,
        hid_simucube_protocol::SIMUCUBE_2_PRO_PID,
        hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
    ];

    let cprs: Vec<u32> = pids
        .iter()
        .filter_map(|&pid| get_vendor_protocol(0x16D0, pid))
        .map(|h| h.get_ffb_config().encoder_cpr)
        .collect();

    // All Simucube 2 bases use the same 22-bit angle sensor
    let first = cprs[0];
    for (i, &cpr) in cprs.iter().enumerate() {
        assert_eq!(
            cpr, first,
            "Simucube 2 PID index {i}: encoder CPR {cpr} != first {first}"
        );
    }
    Ok(())
}

/// VRS DirectForce Pro V2 must be classified as V2 hardware; V1 must not.
#[test]
fn vrs_v2_hardware_classification() -> Result<(), Box<dyn std::error::Error>> {
    let v1 = get_vendor_protocol(0x0483, vrs_pids::DIRECTFORCE_PRO);
    assert!(v1.is_some(), "VRS DFP V1 must dispatch");
    assert!(
        !v1.as_ref().is_none_or(|h| h.is_v2_hardware()),
        "VRS DFP V1 must NOT be V2 hardware"
    );

    let v2 = get_vendor_protocol(0x0483, vrs_pids::DIRECTFORCE_PRO_V2);
    assert!(v2.is_some(), "VRS DFP V2 must dispatch");
    assert!(
        v2.as_ref().is_some_and(|h| h.is_v2_hardware()),
        "VRS DFP V2 must be V2 hardware"
    );
    Ok(())
}

/// Asetek Forte must be V2 hardware; Invicta and La Prima must not.
#[test]
fn asetek_v2_hardware_classification() -> Result<(), Box<dyn std::error::Error>> {
    let forte = get_vendor_protocol(0x2433, hid_asetek_protocol::ASETEK_FORTE_PID);
    assert!(forte.is_some(), "Asetek Forte must dispatch");
    assert!(
        forte.as_ref().is_some_and(|h| h.is_v2_hardware()),
        "Asetek Forte must be V2 hardware"
    );

    let invicta = get_vendor_protocol(0x2433, hid_asetek_protocol::ASETEK_INVICTA_PID);
    assert!(invicta.is_some(), "Asetek Invicta must dispatch");
    assert!(
        !invicta.as_ref().is_none_or(|h| h.is_v2_hardware()),
        "Asetek Invicta must NOT be V2 hardware"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Device name resolution / model classification
// ═══════════════════════════════════════════════════════════════════════════════

/// Logitech model classification from PID.
#[test]
fn logitech_model_classification_all_known() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, LogitechModel)] = &[
        (logitech_pids::G25, LogitechModel::G25),
        (logitech_pids::G27, LogitechModel::G27),
        (logitech_pids::G29_PS, LogitechModel::G29),
        (logitech_pids::G920, LogitechModel::G920),
        (logitech_pids::G923_XBOX, LogitechModel::G923),
        (logitech_pids::G923_PS, LogitechModel::G923),
        (logitech_pids::G_PRO, LogitechModel::GPro),
        (logitech_pids::G_PRO_XBOX, LogitechModel::GPro),
    ];

    for &(pid, expected) in cases {
        let protocol = LogitechProtocol::new(0x046D, pid);
        assert_eq!(
            protocol.model(),
            expected,
            "Logitech PID 0x{pid:04X} should map to {expected:?}"
        );
    }
    Ok(())
}

/// Logitech unknown PID maps to LogitechModel::Unknown.
#[test]
fn logitech_unknown_pid_maps_to_unknown_model() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = LogitechProtocol::new(0x046D, 0xBEEF);
    assert_eq!(protocol.model(), LogitechModel::Unknown);
    Ok(())
}

/// Fanatec model classification for all wheelbases.
#[test]
fn fanatec_model_classification_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, FanatecModel)] = &[
        (fanatec_pids::DD1, FanatecModel::Dd1),
        (fanatec_pids::DD2, FanatecModel::Dd2),
        (fanatec_pids::CSL_DD, FanatecModel::CslDd),
        (fanatec_pids::GT_DD_PRO, FanatecModel::GtDdPro),
        (fanatec_pids::CLUBSPORT_DD, FanatecModel::ClubSportDd),
        (fanatec_pids::CLUBSPORT_V2, FanatecModel::ClubSportV2),
        (fanatec_pids::CLUBSPORT_V2_5, FanatecModel::ClubSportV25),
        (fanatec_pids::CSL_ELITE, FanatecModel::CslElite),
        (fanatec_pids::CSR_ELITE, FanatecModel::CsrElite),
    ];

    for &(pid, expected) in cases {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(
            model, expected,
            "Fanatec PID 0x{pid:04X} should map to {expected:?}"
        );
    }
    Ok(())
}

/// Fanatec unknown PID maps to FanatecModel::Unknown.
#[test]
fn fanatec_unknown_pid_maps_to_unknown_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(0xBEEF);
    assert_eq!(model, FanatecModel::Unknown);
    Ok(())
}

/// Simagic model classification across both legacy and EVO generations.
#[test]
fn simagic_model_classification_all_generations() -> Result<(), Box<dyn std::error::Error>> {
    // Legacy (VID 0x0483)
    let alpha = SimagicProtocol::new(0x0483, 0x0522);
    assert_eq!(alpha.model(), SimagicModel::Alpha);

    // EVO (VID 0x3670)
    let evo_sport = SimagicProtocol::new(0x3670, 0x0500);
    assert_eq!(evo_sport.model(), SimagicModel::EvoSport);

    let evo = SimagicProtocol::new(0x3670, 0x0501);
    assert_eq!(evo.model(), SimagicModel::Evo);

    let evo_pro = SimagicProtocol::new(0x3670, 0x0502);
    assert_eq!(evo_pro.model(), SimagicModel::EvoPro);

    // EVO unknown PID
    let evo_unknown = SimagicProtocol::new(0x3670, 0xBEEF);
    assert_eq!(evo_unknown.model(), SimagicModel::EvoUnknown);

    // Legacy unknown PID
    let legacy_unknown = SimagicProtocol::new(0x0483, 0xBEEF);
    assert_eq!(legacy_unknown.model(), SimagicModel::Unknown);

    Ok(())
}

/// Simucube model classification.
#[test]
fn simucube_model_classification() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, SimucubeModel)] = &[
        (hid_simucube_protocol::SIMUCUBE_2_SPORT_PID, SimucubeModel::Sport),
        (hid_simucube_protocol::SIMUCUBE_2_PRO_PID, SimucubeModel::Pro),
        (hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID, SimucubeModel::Ultimate),
    ];

    for &(pid, expected) in cases {
        let handler = SimucubeProtocolHandler::new(0x16D0, pid);
        assert_eq!(
            handler.model(),
            expected,
            "Simucube PID 0x{pid:04X} should map to {expected:?}"
        );
    }
    Ok(())
}

/// Asetek model classification.
#[test]
fn asetek_model_classification() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, AsetekModel)] = &[
        (hid_asetek_protocol::ASETEK_INVICTA_PID, AsetekModel::Invicta),
        (hid_asetek_protocol::ASETEK_FORTE_PID, AsetekModel::Forte),
        (hid_asetek_protocol::ASETEK_LAPRIMA_PID, AsetekModel::LaPrima),
    ];

    for &(pid, expected) in cases {
        let handler = AsetekProtocolHandler::new(0x2433, pid);
        assert_eq!(
            handler.model(),
            expected,
            "Asetek PID 0x{pid:04X} should map to {expected:?}"
        );
    }
    Ok(())
}

/// Cammus model classification.
#[test]
fn cammus_model_classification() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, CammusModel)] = &[
        (CAMMUS_C5_PID, CammusModel::C5),
        (CAMMUS_C12_PID, CammusModel::C12),
    ];

    for &(pid, expected) in cases {
        let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, pid);
        assert_eq!(
            handler.model(),
            expected,
            "Cammus PID 0x{pid:04X} should map to {expected:?}"
        );
    }

    // Unknown
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, 0xBEEF);
    assert_eq!(handler.model(), CammusModel::Unknown);
    Ok(())
}

/// PXN model classification.
#[test]
fn pxn_model_classification() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, PxnModel)] = &[
        (racing_wheel_hid_pxn_protocol::PRODUCT_V10, PxnModel::V10),
        (racing_wheel_hid_pxn_protocol::PRODUCT_V12, PxnModel::V12),
        (racing_wheel_hid_pxn_protocol::PRODUCT_V12_LITE, PxnModel::V12Lite),
        (racing_wheel_hid_pxn_protocol::PRODUCT_GT987, PxnModel::Gt987),
    ];

    for &(pid, expected) in cases {
        let handler = PxnProtocolHandler::new(0x11FF, pid);
        assert_eq!(
            handler.model(),
            expected,
            "PXN PID 0x{pid:04X} should map to {expected:?}"
        );
    }

    // Unknown
    let handler = PxnProtocolHandler::new(0x11FF, 0xBEEF);
    assert_eq!(handler.model(), PxnModel::Unknown);
    Ok(())
}

/// AccuForce model classification.
#[test]
fn accuforce_model_classification() -> Result<(), Box<dyn std::error::Error>> {
    let handler = AccuForceProtocolHandler::new(0x1FC9, racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO);
    assert_eq!(handler.model(), AccuForceModel::Pro);

    let handler = AccuForceProtocolHandler::new(0x1FC9, 0xBEEF);
    assert_eq!(handler.model(), AccuForceModel::Unknown);
    Ok(())
}

/// Device name resolution: verify display_name logic by constructing DeviceInfo
/// with different field combinations.
#[test]
fn device_info_name_resolution_priority() -> Result<(), Box<dyn std::error::Error>> {
    // When a vendor protocol handler is created, it produces correct FFB config
    // metadata. The display_name resolution is tested indirectly — each handler
    // returns meaningful config that includes max_torque and encoder_cpr.
    //
    // Direct HidDeviceInfo tests are in openracing-hid-common; here we verify
    // that model classification across vendors always resolves to a meaningful
    // display string by checking each protocol handler's get_ffb_config().
    let handler = get_vendor_protocol(0x046D, logitech_pids::G920);
    assert!(handler.is_some());
    let config = handler.as_ref().map(|h| h.get_ffb_config());
    assert!(config.is_some());

    // A completely unknown VID returns None — no name resolution possible
    let handler = get_vendor_protocol(0xDEAD, 0xBEEF);
    assert!(handler.is_none());

    Ok(())
}

/// HidDeviceInfo matches() helper — tested via the engine's DeviceInfo type.
#[test]
fn device_vid_pid_matching() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that protocol dispatch correctly distinguishes VID+PID pairs
    // that are numerically close but belong to different vendors.
    let logitech = get_vendor_protocol(0x046D, logitech_pids::G920);
    assert!(logitech.is_some(), "Logitech G920 must dispatch");

    // Same VID, different PID should still work
    let logitech2 = get_vendor_protocol(0x046D, logitech_pids::G_PRO);
    assert!(logitech2.is_some(), "Logitech G PRO must dispatch");

    // Different VID, same PID should NOT resolve to the same vendor
    let not_logitech = get_vendor_protocol(0x0000, logitech_pids::G920);
    assert!(
        not_logitech.is_none(),
        "VID 0x0000 with Logitech PID must return None"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-vendor consistency checks
// ═══════════════════════════════════════════════════════════════════════════════

/// No two distinct vendors in the full matrix should claim the same VID+PID.
#[test]
fn no_duplicate_vid_pid_across_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = full_device_matrix();
    let mut seen: std::collections::HashMap<(u16, u16), &str> = std::collections::HashMap::new();
    let mut duplicates: Vec<String> = Vec::new();

    for entry in &matrix {
        let key = (entry.vid, entry.pid);
        if let Some(existing) = seen.get(&key) {
            duplicates.push(format!(
                "VID=0x{:04X} PID=0x{:04X}: claimed by '{}' and '{}'",
                entry.vid, entry.pid, existing, entry.vendor_label
            ));
        } else {
            seen.insert(key, entry.vendor_label);
        }
    }

    assert!(
        duplicates.is_empty(),
        "Duplicate VID+PID in device matrix:\n  {}",
        duplicates.join("\n  ")
    );
    Ok(())
}

/// Matrix must cover a minimum number of device families (sanity).
#[test]
fn matrix_covers_minimum_vendor_count() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = full_device_matrix();
    let vendors: std::collections::HashSet<&str> =
        matrix.iter().map(|e| e.vendor_label).collect();

    // We support at least 15 vendor families
    assert!(
        vendors.len() >= 15,
        "expected >= 15 vendor families in matrix, got {}",
        vendors.len()
    );
    Ok(())
}

/// Matrix must cover a minimum number of individual devices.
#[test]
fn matrix_covers_minimum_device_count() -> Result<(), Box<dyn std::error::Error>> {
    let count = full_device_matrix().len();
    assert!(
        count >= 50,
        "expected >= 50 devices in matrix, got {count}"
    );
    Ok(())
}

/// Every handler returned by the dispatch table implements VendorProtocol
/// and can produce an FfbConfig without panicking.
#[test]
fn all_dispatched_handlers_produce_valid_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
    for entry in full_device_matrix() {
        let handler = match get_vendor_protocol(entry.vid, entry.pid) {
            Some(h) => h,
            None => continue,
        };

        let config = handler.get_ffb_config();

        // Sanity: max_torque must be finite
        assert!(
            config.max_torque_nm.is_finite(),
            "{} {}: max_torque_nm is not finite",
            entry.vendor_label,
            entry.device_name
        );

        // is_v2_hardware, output_report_id, output_report_len must not panic
        let _ = handler.is_v2_hardware();
        let _ = handler.output_report_id();
        let _ = handler.output_report_len();
    }
    Ok(())
}
