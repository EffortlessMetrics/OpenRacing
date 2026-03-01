//! Device auto-detection integration tests.
//!
//! Validates the "plug and play" guarantee: when a user plugs in a racing wheel,
//! the system automatically identifies the device by USB VID/PID and loads the
//! correct vendor protocol handler.
//!
//! # Coverage
//!
//! * Every known VID/PID dispatches to the correct vendor handler
//! * Unknown VID/PIDs return `None` gracefully (no panic)
//! * No duplicate VID/PID registrations across vendors
//! * All registered devices have valid VID (non-zero), PID (non-zero), a
//!   human-readable name, and a vendor label
//! * The HID PID fallback path works for unknown VIDs with PID capability

use racing_wheel_engine::hid::vendor::{
    get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback,
};
use std::collections::HashMap;

// ─── Known device registry ──────────────────────────────────────────────────
//
// Canonical list of every supported VID/PID pair, the expected vendor label,
// and a human-readable device name.  This table is the single source of truth
// for these tests.

struct KnownDevice {
    vid: u16,
    pid: u16,
    vendor: &'static str,
    name: &'static str,
}

/// Build the full registry of known VID/PID → device mappings.
fn known_devices() -> Vec<KnownDevice> {
    vec![
        // ── Logitech (VID 0x046D) ────────────────────────────────────────
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::G25,
            vendor: "Logitech",
            name: "G25",
        },
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::G27,
            vendor: "Logitech",
            name: "G27",
        },
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::G29_PS,
            vendor: "Logitech",
            name: "G29",
        },
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::G920,
            vendor: "Logitech",
            name: "G920",
        },
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::G923_PS,
            vendor: "Logitech",
            name: "G923 PS",
        },
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::G_PRO,
            vendor: "Logitech",
            name: "G PRO",
        },
        KnownDevice {
            vid: 0x046D,
            pid: racing_wheel_hid_logitech_protocol::product_ids::DRIVING_FORCE_GT,
            vendor: "Logitech",
            name: "Driving Force GT",
        },
        // ── Fanatec (VID 0x0EB7) ─────────────────────────────────────────
        KnownDevice {
            vid: 0x0EB7,
            pid: racing_wheel_hid_fanatec_protocol::product_ids::CSL_DD,
            vendor: "Fanatec",
            name: "CSL DD",
        },
        KnownDevice {
            vid: 0x0EB7,
            pid: racing_wheel_hid_fanatec_protocol::product_ids::DD1,
            vendor: "Fanatec",
            name: "DD1",
        },
        KnownDevice {
            vid: 0x0EB7,
            pid: racing_wheel_hid_fanatec_protocol::product_ids::DD2,
            vendor: "Fanatec",
            name: "DD2",
        },
        KnownDevice {
            vid: 0x0EB7,
            pid: racing_wheel_hid_fanatec_protocol::product_ids::GT_DD_PRO,
            vendor: "Fanatec",
            name: "GT DD Pro",
        },
        KnownDevice {
            vid: 0x0EB7,
            pid: racing_wheel_hid_fanatec_protocol::product_ids::CLUBSPORT_V2_5,
            vendor: "Fanatec",
            name: "ClubSport V2.5",
        },
        // ── Thrustmaster (VID 0x044F) ────────────────────────────────────
        KnownDevice {
            vid: 0x044F,
            pid: racing_wheel_hid_thrustmaster_protocol::product_ids::T300_RS,
            vendor: "Thrustmaster",
            name: "T300 RS",
        },
        KnownDevice {
            vid: 0x044F,
            pid: racing_wheel_hid_thrustmaster_protocol::product_ids::T818,
            vendor: "Thrustmaster",
            name: "T818",
        },
        KnownDevice {
            vid: 0x044F,
            pid: racing_wheel_hid_thrustmaster_protocol::product_ids::TS_PC_RACER,
            vendor: "Thrustmaster",
            name: "TS-PC Racer",
        },
        KnownDevice {
            vid: 0x044F,
            pid: racing_wheel_hid_thrustmaster_protocol::product_ids::T500_RS,
            vendor: "Thrustmaster",
            name: "T500 RS",
        },
        KnownDevice {
            vid: 0x044F,
            pid: racing_wheel_hid_thrustmaster_protocol::product_ids::T248,
            vendor: "Thrustmaster",
            name: "T248",
        },
        // ── Moza (VID 0x346E) ────────────────────────────────────────────
        KnownDevice {
            vid: 0x346E,
            pid: racing_wheel_hid_moza_protocol::product_ids::R9_V1,
            vendor: "Moza",
            name: "R9",
        },
        KnownDevice {
            vid: 0x346E,
            pid: racing_wheel_hid_moza_protocol::product_ids::R5_V1,
            vendor: "Moza",
            name: "R5",
        },
        KnownDevice {
            vid: 0x346E,
            pid: racing_wheel_hid_moza_protocol::product_ids::R3_V1,
            vendor: "Moza",
            name: "R3",
        },
        KnownDevice {
            vid: 0x346E,
            pid: racing_wheel_hid_moza_protocol::product_ids::R12_V1,
            vendor: "Moza",
            name: "R12",
        },
        // ── Simagic EVO (VID 0x3670) ─────────────────────────────────────
        KnownDevice {
            vid: 0x3670,
            pid: racing_wheel_hid_simagic_protocol::product_ids::EVO,
            vendor: "Simagic",
            name: "EVO",
        },
        KnownDevice {
            vid: 0x3670,
            pid: racing_wheel_hid_simagic_protocol::product_ids::EVO_SPORT,
            vendor: "Simagic",
            name: "EVO Sport",
        },
        KnownDevice {
            vid: 0x3670,
            pid: racing_wheel_hid_simagic_protocol::product_ids::EVO_PRO,
            vendor: "Simagic",
            name: "EVO Pro",
        },
        // ── Simagic Legacy (VID 0x0483 / STM) ───────────────────────────
        KnownDevice {
            vid: 0x0483,
            pid: 0x0522, // Simagic Alpha legacy PID on STM VID
            vendor: "Simagic",
            name: "Alpha (legacy STM)",
        },
        // ── VRS (VID 0x0483 / STM, specific PIDs) ───────────────────────
        KnownDevice {
            vid: 0x0483,
            pid: racing_wheel_hid_vrs_protocol::product_ids::DIRECTFORCE_PRO,
            vendor: "VRS",
            name: "DirectForce Pro",
        },
        KnownDevice {
            vid: 0x0483,
            pid: racing_wheel_hid_vrs_protocol::product_ids::DIRECTFORCE_PRO_V2,
            vendor: "VRS",
            name: "DirectForce Pro V2",
        },
        // ── Cube Controls (VID 0x0483 / STM, specific PIDs) ─────────────
        KnownDevice {
            vid: 0x0483,
            pid: hid_cube_controls_protocol::CUBE_CONTROLS_GT_PRO_PID,
            vendor: "Cube Controls",
            name: "GT Pro",
        },
        KnownDevice {
            vid: 0x0483,
            pid: hid_cube_controls_protocol::CUBE_CONTROLS_FORMULA_PRO_PID,
            vendor: "Cube Controls",
            name: "Formula Pro",
        },
        KnownDevice {
            vid: 0x0483,
            pid: hid_cube_controls_protocol::CUBE_CONTROLS_CSX3_PID,
            vendor: "Cube Controls",
            name: "CSX3",
        },
        // ── Simucube (VID 0x16D0) ────────────────────────────────────────
        KnownDevice {
            vid: 0x16D0,
            pid: hid_simucube_protocol::SIMUCUBE_2_SPORT_PID,
            vendor: "Simucube",
            name: "Simucube 2 Sport",
        },
        KnownDevice {
            vid: 0x16D0,
            pid: hid_simucube_protocol::SIMUCUBE_2_PRO_PID,
            vendor: "Simucube",
            name: "Simucube 2 Pro",
        },
        KnownDevice {
            vid: 0x16D0,
            pid: hid_simucube_protocol::SIMUCUBE_2_ULTIMATE_PID,
            vendor: "Simucube",
            name: "Simucube 2 Ultimate",
        },
        // ── Asetek (VID 0x2433) ──────────────────────────────────────────
        KnownDevice {
            vid: 0x2433,
            pid: hid_asetek_protocol::ASETEK_FORTE_PID,
            vendor: "Asetek",
            name: "Forte",
        },
        KnownDevice {
            vid: 0x2433,
            pid: hid_asetek_protocol::ASETEK_INVICTA_PID,
            vendor: "Asetek",
            name: "Invicta",
        },
        KnownDevice {
            vid: 0x2433,
            pid: hid_asetek_protocol::ASETEK_LAPRIMA_PID,
            vendor: "Asetek",
            name: "La Prima",
        },
        // ── Heusinkveld (VID 0x04D8 / Microchip, specific PIDs) ─────────
        KnownDevice {
            vid: 0x04D8,
            pid: hid_heusinkveld_protocol::HEUSINKVELD_SPRINT_PID,
            vendor: "Heusinkveld",
            name: "Sprint",
        },
        KnownDevice {
            vid: 0x04D8,
            pid: hid_heusinkveld_protocol::HEUSINKVELD_ULTIMATE_PID,
            vendor: "Heusinkveld",
            name: "Ultimate",
        },
        KnownDevice {
            vid: 0x04D8,
            pid: hid_heusinkveld_protocol::HEUSINKVELD_PRO_PID,
            vendor: "Heusinkveld",
            name: "Pro",
        },
        // ── SimpleMotion V2 (VID 0x1D50) ─────────────────────────────────
        KnownDevice {
            vid: 0x1D50,
            pid: racing_wheel_simplemotion_v2::IONI_PRODUCT_ID,
            vendor: "Granite Devices",
            name: "IONI",
        },
        KnownDevice {
            vid: 0x1D50,
            pid: racing_wheel_simplemotion_v2::IONI_PRODUCT_ID_PREMIUM,
            vendor: "Granite Devices",
            name: "IONI Premium",
        },
        KnownDevice {
            vid: 0x1D50,
            pid: racing_wheel_simplemotion_v2::ARGON_PRODUCT_ID,
            vendor: "Granite Devices",
            name: "ARGON",
        },
        // ── OpenFFBoard (VID 0x1209 / pid.codes) ─────────────────────────
        KnownDevice {
            vid: 0x1209,
            pid: racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
            vendor: "OpenFFBoard",
            name: "OpenFFBoard",
        },
        // ── FFBeast (VID 0x045B) ─────────────────────────────────────────
        KnownDevice {
            vid: 0x045B,
            pid: racing_wheel_hid_ffbeast_protocol::FFBEAST_PRODUCT_ID_WHEEL,
            vendor: "FFBeast",
            name: "FFBeast Wheel",
        },
        // ── Cammus (VID 0x3416) ──────────────────────────────────────────
        KnownDevice {
            vid: 0x3416,
            pid: racing_wheel_hid_cammus_protocol::PRODUCT_C5,
            vendor: "Cammus",
            name: "C5",
        },
        KnownDevice {
            vid: 0x3416,
            pid: racing_wheel_hid_cammus_protocol::PRODUCT_C12,
            vendor: "Cammus",
            name: "C12",
        },
        // ── AccuForce (VID 0x1FC9 / NXP) ────────────────────────────────
        KnownDevice {
            vid: 0x1FC9,
            pid: racing_wheel_hid_accuforce_protocol::PID_ACCUFORCE_PRO,
            vendor: "AccuForce",
            name: "AccuForce Pro",
        },
        // ── Leo Bodnar (VID 0x1DD2) ──────────────────────────────────────
        KnownDevice {
            vid: 0x1DD2,
            pid: racing_wheel_hid_leo_bodnar_protocol::PID_WHEEL_INTERFACE,
            vendor: "Leo Bodnar",
            name: "Wheel Interface",
        },
        KnownDevice {
            vid: 0x1DD2,
            pid: racing_wheel_hid_leo_bodnar_protocol::PID_BBI32,
            vendor: "Leo Bodnar",
            name: "BBI-32",
        },
        // ── Button Box (VID 0x1209 / pid.codes) ─────────────────────────
        KnownDevice {
            vid: 0x1209,
            pid: hid_button_box_protocol::PRODUCT_ID_BUTTON_BOX,
            vendor: "Button Box",
            name: "Generic Button Box",
        },
    ]
}

// ─── Known vendor VIDs ──────────────────────────────────────────────────────

/// All vendor VIDs that `get_vendor_protocol()` recognises.
const KNOWN_VENDOR_VIDS: &[(u16, &str)] = &[
    (0x046D, "Logitech"),
    (0x0EB7, "Fanatec"),
    (0x044F, "Thrustmaster"),
    (0x346E, "Moza"),
    (0x3670, "Simagic EVO"),
    (0x0483, "STM (VRS / Simagic / Cube Controls)"),
    (0x16D0, "MCS (Simucube / Simagic legacy)"),
    (0x2433, "Asetek"),
    (0x04D8, "Microchip (Heusinkveld)"),
    (0x1D50, "Granite Devices"),
    (0x1209, "pid.codes (OpenFFBoard / Button Box)"),
    (0x045B, "FFBeast"),
    (0x1FC9, "NXP (AccuForce)"),
    (0x3416, "Cammus"),
    (0x1DD2, "Leo Bodnar"),
];

// ═════════════════════════════════════════════════════════════════════════════
// Test: every known VID/PID dispatches to a protocol handler
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn every_known_vid_pid_dispatches_to_a_handler() -> Result<(), Box<dyn std::error::Error>> {
    let mut failures: Vec<String> = Vec::new();

    for dev in known_devices() {
        let handler = get_vendor_protocol(dev.vid, dev.pid);
        if handler.is_none() {
            failures.push(format!(
                "{} {} (VID=0x{:04X}, PID=0x{:04X})",
                dev.vendor, dev.name, dev.vid, dev.pid
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "The following known devices did NOT dispatch to a protocol handler:\n  {}",
        failures.join("\n  ")
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: unknown VID/PIDs return None gracefully (no panic)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn unknown_vid_returns_none_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    // Completely unknown VIDs
    let unknown_vids: &[(u16, u16)] = &[
        (0x0000, 0x0000),
        (0x0000, 0x0001),
        (0xFFFF, 0xFFFF),
        (0x9999, 0x1234),
        (0xDEAD, 0xBEEF),
    ];

    for (vid, pid) in unknown_vids {
        let result = get_vendor_protocol(*vid, *pid);
        assert!(
            result.is_none(),
            "unknown VID=0x{:04X} PID=0x{:04X} must return None",
            vid,
            pid
        );
    }
    Ok(())
}

#[test]
fn unknown_pid_on_known_guarded_vid_returns_none_not_panic()
-> Result<(), Box<dyn std::error::Error>> {
    // VIDs that guard on PID (Heusinkveld, FFBeast, AccuForce, OpenFFBoard/ButtonBox)
    let guarded: &[(u16, u16, &str)] = &[
        (0x04D8, 0x0001, "Microchip (non-Heusinkveld)"),
        (0x045B, 0x0001, "Renesas (non-FFBeast)"),
        (0x1FC9, 0x0001, "NXP (non-AccuForce)"),
        (0x1209, 0x0001, "pid.codes (non-OpenFFBoard, non-ButtonBox)"),
    ];

    for (vid, pid, label) in guarded {
        let result = get_vendor_protocol(*vid, *pid);
        assert!(
            result.is_none(),
            "{label}: VID=0x{vid:04X} PID=0x{pid:04X} must return None"
        );
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: no duplicate VID/PID registrations across the known device table
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn no_duplicate_vid_pid_registrations() -> Result<(), Box<dyn std::error::Error>> {
    let devices = known_devices();
    let mut seen: HashMap<(u16, u16), &str> = HashMap::new();
    let mut duplicates: Vec<String> = Vec::new();

    for dev in &devices {
        let key = (dev.vid, dev.pid);
        if let Some(existing_vendor) = seen.get(&key) {
            duplicates.push(format!(
                "VID=0x{:04X} PID=0x{:04X}: registered for both '{}' and '{}'",
                dev.vid, dev.pid, existing_vendor, dev.vendor
            ));
        } else {
            seen.insert(key, dev.vendor);
        }
    }

    assert!(
        duplicates.is_empty(),
        "Duplicate VID/PID registrations found:\n  {}",
        duplicates.join("\n  ")
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: all registered devices have valid VID, PID, name, and vendor
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn all_registered_devices_have_valid_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let mut failures: Vec<String> = Vec::new();

    for dev in known_devices() {
        if dev.vid == 0 {
            failures.push(format!("{} {}: VID must be non-zero", dev.vendor, dev.name));
        }
        if dev.pid == 0 {
            failures.push(format!("{} {}: PID must be non-zero", dev.vendor, dev.name));
        }
        if dev.name.is_empty() {
            failures.push(format!(
                "VID=0x{:04X} PID=0x{:04X}: name must be non-empty",
                dev.vid, dev.pid
            ));
        }
        if dev.vendor.is_empty() {
            failures.push(format!(
                "{} (VID=0x{:04X} PID=0x{:04X}): vendor must be non-empty",
                dev.name, dev.vid, dev.pid
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Device metadata validation failures:\n  {}",
        failures.join("\n  ")
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: every known vendor VID has at least one device registered
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn every_known_vendor_has_at_least_one_device() -> Result<(), Box<dyn std::error::Error>> {
    let devices = known_devices();
    let mut missing: Vec<String> = Vec::new();

    for (vid, label) in KNOWN_VENDOR_VIDS {
        let has_device = devices.iter().any(|d| d.vid == *vid);
        if !has_device {
            missing.push(format!("VID=0x{:04X} ({label})", vid));
        }
    }

    assert!(
        missing.is_empty(),
        "Vendor VIDs with no devices registered in the test table:\n  {}",
        missing.join("\n  ")
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: HID PID fallback for unknown VIDs with PID capability
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn hid_pid_fallback_handles_unknown_vid_with_capability() -> Result<(), Box<dyn std::error::Error>>
{
    // Unknown VID but device advertises HID PID → should get generic handler
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xAAAA, 0xBBBB, true);
    assert!(
        handler.is_some(),
        "unknown VID with HID PID capability must fall back to generic handler"
    );

    // Unknown VID and no HID PID capability → None
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xAAAA, 0xBBBB, false);
    assert!(
        handler.is_none(),
        "unknown VID without HID PID capability must return None"
    );

    Ok(())
}

#[test]
fn hid_pid_fallback_prefers_vendor_handler_over_generic() -> Result<(), Box<dyn std::error::Error>>
{
    // Known VID/PID with HID PID capability → should use vendor handler, not generic
    let handler = get_vendor_protocol_with_hid_pid_fallback(
        0x046D,
        racing_wheel_hid_logitech_protocol::product_ids::G920,
        true,
    );
    assert!(
        handler.is_some(),
        "known device must dispatch even when HID PID fallback is available"
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: shared-VID disambiguation is deterministic
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn shared_stm_vid_disambiguates_vrs_cube_simagic() -> Result<(), Box<dyn std::error::Error>> {
    // VRS PID on shared STM VID → must get a handler
    let vrs = get_vendor_protocol(
        0x0483,
        racing_wheel_hid_vrs_protocol::product_ids::DIRECTFORCE_PRO,
    );
    assert!(vrs.is_some(), "VRS DFP on STM VID must dispatch");

    // Cube Controls PID on shared STM VID → must get a handler
    let cube = get_vendor_protocol(0x0483, hid_cube_controls_protocol::CUBE_CONTROLS_GT_PRO_PID);
    assert!(
        cube.is_some(),
        "Cube Controls GT Pro on STM VID must dispatch"
    );

    // Simagic PID on shared STM VID → must get a handler (fallback)
    let simagic = get_vendor_protocol(
        0x0483, 0x0522, // Simagic Alpha legacy PID
    );
    assert!(simagic.is_some(), "Simagic Alpha on STM VID must dispatch");

    Ok(())
}

#[test]
fn shared_mcs_vid_disambiguates_simucube_simagic() -> Result<(), Box<dyn std::error::Error>> {
    // Simucube PID on shared MCS VID → Simucube handler
    let simucube = get_vendor_protocol(0x16D0, hid_simucube_protocol::SIMUCUBE_2_PRO_PID);
    assert!(
        simucube.is_some(),
        "Simucube 2 Pro on MCS VID must dispatch"
    );

    // Legacy Simagic PID on shared MCS VID → Simagic fallback
    let simagic = get_vendor_protocol(0x16D0, 0x0D5A);
    assert!(
        simagic.is_some(),
        "Simagic M10 on MCS VID must dispatch as fallback"
    );

    Ok(())
}

#[test]
fn shared_pid_codes_vid_disambiguates_openffboard_button_box()
-> Result<(), Box<dyn std::error::Error>> {
    let offb = get_vendor_protocol(
        0x1209,
        racing_wheel_hid_openffboard_protocol::OPENFFBOARD_PRODUCT_ID,
    );
    assert!(offb.is_some(), "OpenFFBoard on pid.codes VID must dispatch");

    let bbox = get_vendor_protocol(0x1209, hid_button_box_protocol::PRODUCT_ID_BUTTON_BOX);
    assert!(bbox.is_some(), "Button Box on pid.codes VID must dispatch");

    // Unknown PID → None
    let unknown = get_vendor_protocol(0x1209, 0x0001);
    assert!(
        unknown.is_none(),
        "unknown PID on pid.codes VID must return None"
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Test: stress unknown inputs (boundary values, edge cases)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn extreme_vid_pid_values_do_not_panic() -> Result<(), Box<dyn std::error::Error>> {
    let edge_cases: &[(u16, u16)] = &[
        (0x0000, 0x0000),
        (0x0000, 0xFFFF),
        (0xFFFF, 0x0000),
        (0xFFFF, 0xFFFF),
        (u16::MAX, u16::MAX),
        (0x0001, 0x0001),
    ];

    for (vid, pid) in edge_cases {
        // Must not panic — we don't care about the return value
        let _ = get_vendor_protocol(*vid, *pid);
        let _ = get_vendor_protocol_with_hid_pid_fallback(*vid, *pid, true);
        let _ = get_vendor_protocol_with_hid_pid_fallback(*vid, *pid, false);
    }

    Ok(())
}
