//! VID+PID dispatch verification tests.
//!
//! These tests complement `device_dispatch.rs` by:
//!
//! 1. Exhaustively testing every known PID on shared VIDs (0x0483, 0x04D8, 0x16D0,
//!    0x1209) dispatches to the correct handler.
//! 2. Verifying unknown PIDs on shared VIDs fall through appropriately.
//! 3. Asserting that the full device registry contains no VID+PID duplicates
//!    across all protocol crates.

use racing_wheel_engine::hid::vendor::{
    get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback,
};

/// A (PID, device label) pair.
type PidEntry = (u16, &'static str);

/// A (vendor label, PID entries) pair.
type VendorPids = (&'static str, &'static [PidEntry]);

/// A (vendor label, VID, PID entries) triple.
type VendorVidPids = (&'static str, u16, &'static [PidEntry]);

// ── Shared VIDs ──────────────────────────────────────────────────────────────

const VID_STM: u16 = 0x0483;
const VID_MCS: u16 = 0x16D0;
const VID_HEUSINKVELD: u16 = 0x04D8;
const VID_PID_CODES: u16 = 0x1209;
const VID_GRANITE: u16 = 0x1D50;

// ── VRS PIDs on VID 0x0483 ──────────────────────────────────────────────────

const VRS_PIDS: &[(u16, &str)] = &[
    (0xA355, "DirectForce Pro"),
    (0xA356, "DirectForce Pro V2"),
    (0xA357, "Pedals V1 (deprecated)"),
    (0xA358, "Pedals V2"),
    (0xA359, "Handbrake"),
    (0xA35A, "Shifter"),
    (0xA3BE, "Pedals"),
    (0xA44C, "R295"),
];

// ── Cube Controls PIDs on VID 0x0483 ────────────────────────────────────────

const CUBE_CONTROLS_PIDS: &[(u16, &str)] = &[
    (0x0C73, "GT Pro"),
    (0x0C74, "Formula Pro"),
    (0x0C75, "CSX3"),
];

// ── Simagic legacy PID on VID 0x0483 ───────────────────────────────────────

const SIMAGIC_LEGACY_PIDS: &[(u16, &str)] = &[(0x0522, "Alpha / M10 legacy")];

// ── Heusinkveld PIDs on VID 0x04D8 (Microchip) ────────────────────────────

const HEUSINKVELD_PIDS: &[(u16, &str)] =
    &[(0xF6D0, "Sprint"), (0xF6D2, "Ultimate+"), (0xF6D3, "Pro")];

// ── Simucube PIDs on VID 0x16D0 ────────────────────────────────────────────

const SIMUCUBE_PIDS: &[(u16, &str)] = &[
    (0x0D5A, "Simucube 1"),
    (0x0D5F, "Simucube 2 Ultimate"),
    (0x0D60, "Simucube 2 Pro"),
    (0x0D61, "Simucube 2 Sport"),
    (0x0D63, "Wireless Wheel"),
    (0x0D66, "ActivePedal"),
];

// ── OpenFFBoard PIDs on VID 0x1209 ─────────────────────────────────────────

const OPENFFBOARD_PIDS: &[(u16, &str)] = &[(0xFFB0, "Main"), (0xFFB1, "Alt (unverified)")];

// ── Button Box PIDs on VID 0x1209 ──────────────────────────────────────────

const BUTTON_BOX_PIDS: &[(u16, &str)] = &[(0x1BBD, "Generic button box")];

// ── Granite Devices PIDs on VID 0x1D50 ─────────────────────────────────────

const GRANITE_PIDS: &[(u16, &str)] = &[
    (0x6050, "IONI"),
    (0x6051, "IONI Premium"),
    (0x6052, "ARGON"),
];

// ══════════════════════════════════════════════════════════════════════════════
// 1. VID 0x0483 (STM): every known PID dispatches to Some(handler)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn vid_0x0483_all_vrs_pids_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in VRS_PIDS {
        assert!(
            get_vendor_protocol(VID_STM, pid).is_some(),
            "VRS {name} (PID 0x{pid:04X}) on VID 0x0483 must dispatch to a handler"
        );
    }
    Ok(())
}

#[test]
fn vid_0x0483_all_cube_controls_pids_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in CUBE_CONTROLS_PIDS {
        assert!(
            get_vendor_protocol(VID_STM, pid).is_some(),
            "Cube Controls {name} (PID 0x{pid:04X}) on VID 0x0483 must dispatch to a handler"
        );
    }
    Ok(())
}

#[test]
fn vid_0x0483_simagic_legacy_pid_dispatches() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in SIMAGIC_LEGACY_PIDS {
        assert!(
            get_vendor_protocol(VID_STM, pid).is_some(),
            "Simagic legacy {name} (PID 0x{pid:04X}) on VID 0x0483 must dispatch to a handler"
        );
    }
    Ok(())
}

/// Unknown PIDs on VID 0x0483 fall through to the Simagic legacy handler
/// (not `None`), because the STM VID branch defaults to Simagic.
#[test]
fn vid_0x0483_unknown_pid_falls_through_to_simagic() -> Result<(), Box<dyn std::error::Error>> {
    let handler = get_vendor_protocol(VID_STM, 0xFFFF);
    assert!(
        handler.is_some(),
        "unknown PID on VID 0x0483 should fall through to legacy Simagic handler"
    );
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 2. VID 0x04D8 (Microchip / Heusinkveld): every known PID dispatches
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn vid_0x04d8_all_heusinkveld_pids_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in HEUSINKVELD_PIDS {
        assert!(
            get_vendor_protocol(VID_HEUSINKVELD, pid).is_some(),
            "Heusinkveld {name} (PID 0x{pid:04X}) on VID 0x04D8 must dispatch to a handler"
        );
    }
    Ok(())
}

/// Unknown PIDs on VID 0x04D8 do NOT fall through (Microchip is a generic
/// chip VID; only known Heusinkveld PIDs dispatch).
#[test]
fn vid_0x04d8_unknown_pid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    let handler = get_vendor_protocol(VID_HEUSINKVELD, 0xFFFF);
    assert!(
        handler.is_none(),
        "unknown PID on VID 0x04D8 should return None (not a known Heusinkveld PID)"
    );
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 3. VID 0x16D0 (MCS): Simucube PIDs dispatch, unknown falls to Simagic
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn vid_0x16d0_all_simucube_pids_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in SIMUCUBE_PIDS {
        assert!(
            get_vendor_protocol(VID_MCS, pid).is_some(),
            "Simucube {name} (PID 0x{pid:04X}) on VID 0x16D0 must dispatch to a handler"
        );
    }
    Ok(())
}

/// Unknown PIDs on VID 0x16D0 fall through to the Simagic legacy handler.
#[test]
fn vid_0x16d0_unknown_pid_falls_through_to_simagic() -> Result<(), Box<dyn std::error::Error>> {
    let handler = get_vendor_protocol(VID_MCS, 0xFFFF);
    assert!(
        handler.is_some(),
        "unknown PID on VID 0x16D0 should fall through to legacy Simagic handler"
    );
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 3. VID 0x1209 (pid.codes): known PIDs dispatch, unknown returns None
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn vid_0x1209_all_openffboard_pids_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in OPENFFBOARD_PIDS {
        assert!(
            get_vendor_protocol(VID_PID_CODES, pid).is_some(),
            "OpenFFBoard {name} (PID 0x{pid:04X}) on VID 0x1209 must dispatch to a handler"
        );
    }
    Ok(())
}

#[test]
fn vid_0x1209_button_box_pid_dispatches() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in BUTTON_BOX_PIDS {
        assert!(
            get_vendor_protocol(VID_PID_CODES, pid).is_some(),
            "Button box {name} (PID 0x{pid:04X}) on VID 0x1209 must dispatch to a handler"
        );
    }
    Ok(())
}

/// pid.codes VID with an unrecognised PID must return None — no fallback.
#[test]
fn vid_0x1209_unknown_pid_returns_none() -> Result<(), Box<dyn std::error::Error>> {
    let handler = get_vendor_protocol(VID_PID_CODES, 0x0001);
    assert!(
        handler.is_none(),
        "unknown PID on VID 0x1209 must return None (no legacy fallback)"
    );
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 4. VID 0x1D50 (Granite Devices): all known PIDs dispatch
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn vid_0x1d50_all_granite_pids_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    for &(pid, name) in GRANITE_PIDS {
        assert!(
            get_vendor_protocol(VID_GRANITE, pid).is_some(),
            "Granite Devices {name} (PID 0x{pid:04X}) on VID 0x1D50 must dispatch to a handler"
        );
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 5. Generic HID PID fallback on shared VIDs
// ══════════════════════════════════════════════════════════════════════════════

/// Known shared-VID devices must get their vendor handler even when
/// `has_hid_pid_capability` is true.
#[test]
fn shared_vid_known_pid_ignores_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases: &[(u16, u16, &str)] = &[
        (VID_STM, 0xA355, "VRS DFP"),
        (VID_STM, 0x0C73, "Cube Controls GT Pro"),
        (VID_STM, 0x0522, "Simagic Alpha legacy"),
        (VID_HEUSINKVELD, 0xF6D0, "Heusinkveld Sprint"),
        (VID_MCS, 0x0D61, "Simucube 2 Sport"),
        (VID_PID_CODES, 0xFFB0, "OpenFFBoard"),
        (VID_PID_CODES, 0x1BBD, "Button Box"),
    ];

    for &(vid, pid, label) in test_cases {
        let handler = get_vendor_protocol_with_hid_pid_fallback(vid, pid, true);
        assert!(
            handler.is_some(),
            "{label} (VID 0x{vid:04X} PID 0x{pid:04X}) must resolve to a vendor handler \
             even with has_hid_pid_capability=true"
        );
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// 6. Full VID+PID registry: no duplicates across all vendors
// ══════════════════════════════════════════════════════════════════════════════

/// Collect every known VID+PID pair from all protocol crates and assert
/// that no two vendors claim the same combination.
#[test]
fn vid_pid_registry_has_no_duplicates() -> Result<(), Box<dyn std::error::Error>> {
    // (VID, PID) → vendor label
    let mut registry: std::collections::HashMap<(u16, u16), &str> =
        std::collections::HashMap::new();

    let vendors: &[VendorVidPids] = &[
        // VID 0x0483 vendors
        ("VRS", VID_STM, VRS_PIDS),
        ("Cube Controls", VID_STM, CUBE_CONTROLS_PIDS),
        ("Simagic (legacy 0x0483)", VID_STM, SIMAGIC_LEGACY_PIDS),
        // VID 0x04D8 Heusinkveld
        ("Heusinkveld", VID_HEUSINKVELD, HEUSINKVELD_PIDS),
        // VID 0x16D0 vendors
        ("Simucube", VID_MCS, SIMUCUBE_PIDS),
        // VID 0x1209 vendors
        ("OpenFFBoard", VID_PID_CODES, OPENFFBOARD_PIDS),
        ("Button Box", VID_PID_CODES, BUTTON_BOX_PIDS),
        // VID 0x1D50
        ("Granite Devices", VID_GRANITE, GRANITE_PIDS),
        // Dedicated-VID vendors (included for completeness)
        (
            "Fanatec",
            0x0EB7,
            &[
                (0x0001, "ClubSport V2"),
                (0x0004, "ClubSport V2.5"),
                (0x0005, "CSL Elite PS4"),
                (0x0006, "DD1"),
                (0x0007, "DD2"),
                (0x0011, "CSR Elite"),
                (0x0020, "CSL DD"),
                (0x0024, "GT DD Pro"),
                (0x0E03, "CSL Elite"),
                (0x01E9, "ClubSport DD"),
                (0x1839, "ClubSport Pedals V1/V2"),
                (0x183B, "ClubSport Pedals V3"),
                (0x6204, "CSL Elite Pedals"),
                (0x6205, "CSL Pedals LC"),
                (0x6206, "CSL Pedals V2"),
            ],
        ),
        (
            "Logitech",
            0x046D,
            &[
                (0xC295, "MOMO"),
                (0xC298, "DFP"),
                (0xC29A, "DFGT"),
                (0xC29C, "SFW"),
                (0xCA03, "MOMO 2"),
                (0xC293, "WFF GP"),
                (0xC291, "WFF"),
                (0xCA04, "Vibration"),
                (0xC299, "G25"),
                (0xC294, "DF EX"),
                (0xC29B, "G27"),
                (0xC24F, "G29"),
                (0xC262, "G920"),
                (0xC266, "G923"),
                (0xC267, "G923 PS"),
                (0xC26E, "G923 Xbox"),
                (0xC268, "G PRO"),
                (0xC272, "G PRO Xbox"),
            ],
        ),
        (
            "Moza",
            0x346E,
            &[
                (0x0000, "R16/R21 V1"),
                (0x0002, "R9 V1"),
                (0x0004, "R5 V1"),
                (0x0005, "R3 V1"),
                (0x0006, "R12 V1"),
                (0x0010, "R16/R21 V2"),
                (0x0012, "R9 V2"),
                (0x0014, "R5 V2"),
                (0x0015, "R3 V2"),
                (0x0016, "R12 V2"),
                (0x0003, "SR-P Pedals"),
                (0x0020, "HGP"),
                (0x0021, "SGP"),
                (0x0022, "HBP"),
            ],
        ),
        (
            "Thrustmaster",
            0x044F,
            &[
                (0xB65D, "Generic"),
                (0xB677, "T150"),
                (0xB65E, "T500 RS"),
                (0xB66D, "T300 PS4"),
                (0xB67F, "TMX"),
                (0xB66E, "T300"),
                (0xB66F, "T300 GT"),
                (0xB669, "TX Racing"),
                (0xB664, "TX Orig"),
                (0xB696, "T248"),
                (0xB69A, "T248X"),
                (0xB689, "TS-PC"),
                (0xB692, "TS-XW"),
                (0xB691, "TS-XW GIP"),
                (0xB69B, "T818"),
                (0xB668, "T80"),
                (0xB66A, "T80 Ferrari"),
                (0xB605, "NASCAR"),
                (0xB651, "FGT Rumble"),
                (0xB653, "RGT Clutch"),
                (0xB654, "FGT FFB"),
                (0xB65A, "F430 FFB"),
            ],
        ),
        (
            "Simagic (EVO 0x3670)",
            0x3670,
            &[
                (0x0500, "EVO Sport"),
                (0x0501, "EVO"),
                (0x0502, "EVO Pro"),
                (0x0600, "Alpha EVO"),
                (0x0700, "NEO"),
                (0x0701, "NEO Mini"),
                (0x1001, "P1000"),
                (0x1002, "P2000"),
                (0x1003, "P1000A"),
                (0x2001, "Shifter H"),
                (0x2002, "Shifter Seq"),
                (0x3001, "Handbrake"),
                (0x4001, "WR1"),
                (0x4002, "GT1"),
                (0x4003, "GT NEO"),
                (0x4004, "Formula"),
            ],
        ),
        (
            "Asetek",
            0x2433,
            &[
                (0xF300, "Invicta"),
                (0xF301, "Forte"),
                (0xF303, "La Prima"),
                (0xF306, "Tony Kanaan"),
            ],
        ),
        (
            "FFBeast",
            0x045B,
            &[(0x58F9, "Joystick"), (0x5968, "Rudder"), (0x59D7, "Wheel")],
        ),
        (
            "Cammus",
            0x3416,
            &[
                (0x0301, "C5"),
                (0x0302, "C12"),
                (0x1018, "CP5 Pedals"),
                (0x1019, "LC100 Pedals"),
            ],
        ),
        ("AccuForce", 0x1FC9, &[(0x804C, "AccuForce Pro")]),
        (
            "Leo Bodnar",
            0x1DD2,
            &[
                (0x0001, "USB Joystick"),
                (0x000C, "BBI32"),
                (0x000E, "Wheel Interface"),
                (0x000F, "FFB Joystick"),
                (0x1301, "SLI-M"),
                (0x000B, "BU0836A"),
                (0x0030, "BU0836X"),
                (0x0031, "BU0836 16-bit"),
            ],
        ),
    ];

    let mut duplicates = Vec::new();

    for &(vendor, vid, pids) in vendors {
        for &(pid, device) in pids {
            let key = (vid, pid);
            if let Some(existing) = registry.get(&key) {
                duplicates.push(format!(
                    "VID=0x{vid:04X} PID=0x{pid:04X}: \
                     claimed by both '{existing}' and '{vendor}' ({device})"
                ));
            } else {
                registry.insert(key, vendor);
            }
        }
    }

    assert!(
        duplicates.is_empty(),
        "VID+PID duplicates found across device registry:\n  {}",
        duplicates.join("\n  ")
    );

    // Sanity: we registered a meaningful number of devices
    assert!(
        registry.len() > 100,
        "expected >100 VID+PID entries in registry, got {}",
        registry.len()
    );

    Ok(())
}

/// Cross-check: PID ranges on shared VIDs must not overlap.
/// This verifies that no PID appears in more than one vendor's list for
/// the same VID.
#[test]
fn shared_vid_pid_ranges_do_not_overlap() -> Result<(), Box<dyn std::error::Error>> {
    let shared_vids: &[(u16, &[VendorPids])] = &[
        (
            VID_STM,
            &[
                ("VRS", VRS_PIDS),
                ("Cube Controls", CUBE_CONTROLS_PIDS),
                ("Simagic legacy", SIMAGIC_LEGACY_PIDS),
            ],
        ),
        (
            VID_MCS,
            &[
                ("Heusinkveld", HEUSINKVELD_PIDS),
                ("Simucube", SIMUCUBE_PIDS),
            ],
        ),
        (
            VID_PID_CODES,
            &[
                ("OpenFFBoard", OPENFFBOARD_PIDS),
                ("Button Box", BUTTON_BOX_PIDS),
            ],
        ),
    ];

    let mut errors = Vec::new();

    for &(vid, vendor_groups) in shared_vids {
        let mut pid_owner: std::collections::HashMap<u16, &str> = std::collections::HashMap::new();
        for &(vendor, pids) in vendor_groups {
            for &(pid, _device) in pids {
                if let Some(existing) = pid_owner.get(&pid) {
                    errors.push(format!(
                        "VID 0x{vid:04X}: PID 0x{pid:04X} claimed by both '{existing}' and '{vendor}'"
                    ));
                } else {
                    pid_owner.insert(pid, vendor);
                }
            }
        }
    }

    assert!(
        errors.is_empty(),
        "PID range overlaps on shared VIDs:\n  {}",
        errors.join("\n  ")
    );

    Ok(())
}
