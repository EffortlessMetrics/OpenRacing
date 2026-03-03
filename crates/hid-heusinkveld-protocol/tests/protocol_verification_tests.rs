//! Protocol verification tests for Heusinkveld HID devices.
//!
//! Cross-verifies VID/PID constants, report format, and known-good byte
//! sequences against external community and kernel sources.
//!
//! ## Source citations
//!
//! | Source | URL / location | Data used |
//! |--------|---------------|-----------|
//! | JacKeTUs/simracing-hwdb | `90-heusinkveld.hwdb` (GitHub) | VID/PID for Sprint, Ultimate, Handbrake V1/V2, Sequential Shifter |
//! | Linux kernel `hid-ids.h` | `torvalds/linux` (mainline) | VID 0x04D8 = Microchip Technology, VID 0x10C4 = Silicon Labs |
//! | the-sz.com / devicehunt.com | USB VID registries | VID 0x04D8 = Microchip Technology, Inc. |
//! | OpenFlight YAML | `sprint-pedals.yaml`, `ultimate-pedals-0241.yaml` | Legacy PIDs 0xF6D0, 0xF6D2 |
//! | heusinkveld.com | Product pages | Sprint 2-pedal, Ultimate+ 3-pedal 140 kg |

use hid_heusinkveld_protocol::{
    HeusinkveldInputReport, HeusinkveldModel, HEUSINKVELD_HANDBRAKE_V1_PID,
    HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID,
    HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_ULTIMATE_PID,
    HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID, HEUSINKVELD_SHIFTER_PID,
    HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_VENDOR_ID, MAX_LOAD_CELL_VALUE, REPORT_SIZE_INPUT,
};

// ─── VID cross-verification against external sources ────────────────────────

/// VID 0x30B7 — primary Heusinkveld VID.
///
/// Source: JacKeTUs/simracing-hwdb `90-heusinkveld.hwdb`
/// (modalias patterns `v30B7p1001`, `v30B7p1002`, `v30B7p1003`).
#[test]
fn vid_0x30b7_matches_simracing_hwdb() {
    assert_eq!(
        HEUSINKVELD_VENDOR_ID, 0x30B7,
        "Primary VID must match simracing-hwdb v30B7"
    );
}

/// VID 0x04D8 — legacy Microchip Technology VID.
///
/// Source: the-sz.com, devicehunt.com (Microchip Technology, Inc.),
/// Linux kernel `hid-ids.h` (USB_VENDOR_ID_MICROCHIP = 0x04D8).
#[test]
fn vid_0x04d8_matches_microchip_registry() {
    assert_eq!(
        HEUSINKVELD_LEGACY_VENDOR_ID, 0x04D8,
        "Legacy VID must be Microchip Technology 0x04D8"
    );
}

/// VID 0x10C4 — Silicon Labs, used by Handbrake V1.
///
/// Source: JacKeTUs/simracing-hwdb `90-heusinkveld.hwdb`
/// (modalias `v10C4p8B82`).
#[test]
fn vid_0x10c4_matches_silicon_labs() {
    assert_eq!(
        HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, 0x10C4,
        "Handbrake V1 VID must be Silicon Labs 0x10C4"
    );
}

/// VID 0xA020 — Sequential Shifter VID.
///
/// Source: JacKeTUs/simracing-hwdb `90-heusinkveld.hwdb`
/// (modalias `vA020p3142`).
#[test]
fn vid_0xa020_matches_shifter_hwdb() {
    assert_eq!(
        HEUSINKVELD_SHIFTER_VENDOR_ID, 0xA020,
        "Shifter VID must match simracing-hwdb vA020"
    );
}

// ─── PID cross-verification against simracing-hwdb ──────────────────────────

/// Sprint PID 0x1001 (VID 0x30B7).
///
/// Source: simracing-hwdb `90-heusinkveld.hwdb` — `v30B7p1001`.
#[test]
fn sprint_pid_matches_hwdb() {
    assert_eq!(HEUSINKVELD_SPRINT_PID, 0x1001);
}

/// Ultimate PID 0x1003 (VID 0x30B7).
///
/// Source: simracing-hwdb `90-heusinkveld.hwdb` — `v30B7p1003`.
#[test]
fn ultimate_pid_matches_hwdb() {
    assert_eq!(HEUSINKVELD_ULTIMATE_PID, 0x1003);
}

/// Handbrake V2 PID 0x1002 (VID 0x30B7).
///
/// Source: simracing-hwdb `90-heusinkveld.hwdb` — `v30B7p1002`.
#[test]
fn handbrake_v2_pid_matches_hwdb() {
    assert_eq!(HEUSINKVELD_HANDBRAKE_V2_PID, 0x1002);
}

/// Handbrake V1 PID 0x8B82 (VID 0x10C4).
///
/// Source: simracing-hwdb `90-heusinkveld.hwdb` — `v10C4p8B82`.
#[test]
fn handbrake_v1_pid_matches_hwdb() {
    assert_eq!(HEUSINKVELD_HANDBRAKE_V1_PID, 0x8B82);
}

/// Sequential Shifter PID 0x3142 (VID 0xA020).
///
/// Source: simracing-hwdb `90-heusinkveld.hwdb` — `vA020p3142`.
#[test]
fn shifter_pid_matches_hwdb() {
    assert_eq!(HEUSINKVELD_SHIFTER_PID, 0x3142);
}

// ─── Legacy PID cross-verification (OpenFlight) ─────────────────────────────

/// Legacy Sprint PID 0xF6D0 (VID 0x04D8).
///
/// Source: OpenFlight `sprint-pedals.yaml` (community).
#[test]
fn legacy_sprint_pid_matches_openflight() {
    assert_eq!(HEUSINKVELD_LEGACY_SPRINT_PID, 0xF6D0);
}

/// Legacy Ultimate PID 0xF6D2 (VID 0x04D8).
///
/// Source: OpenFlight `ultimate-pedals-0241.yaml` (community).
#[test]
fn legacy_ultimate_pid_matches_openflight() {
    assert_eq!(HEUSINKVELD_LEGACY_ULTIMATE_PID, 0xF6D2);
}

/// Pro PID 0xF6D3 (VID 0x04D8) — estimated, zero external evidence.
///
/// Flagged as ⚠ Estimated. Sequential guess after 0xF6D2.
#[test]
fn pro_pid_is_sequential_estimate() {
    assert_eq!(HEUSINKVELD_PRO_PID, 0xF6D3);
    // Verify it follows the sequential pattern from the legacy range
    assert_eq!(
        HEUSINKVELD_PRO_PID,
        HEUSINKVELD_LEGACY_ULTIMATE_PID + 1,
        "Pro PID should be sequential after Legacy Ultimate PID"
    );
}

// ─── Report format verification ─────────────────────────────────────────────

/// Report size must be 8 bytes: 3× u16 LE (throttle, brake, clutch) + 1× u8
/// status + 1 padding byte.
#[test]
fn report_size_is_8_bytes() {
    assert_eq!(REPORT_SIZE_INPUT, 8);
}

/// Known-good byte sequence: idle pedals (zeros), connected + calibrated.
///
/// Wire format (little-endian):
///   bytes 0-1: throttle = 0x0000
///   bytes 2-3: brake    = 0x0000
///   bytes 4-5: clutch   = 0x0000
///   byte  6:   status   = 0x03 (connected | calibrated)
///   byte  7:   padding  = 0x00
#[test]
fn known_good_idle_report() -> Result<(), Box<dyn std::error::Error>> {
    let idle: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00];
    let report = HeusinkveldInputReport::parse(&idle).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
    assert_eq!(report.status, 0x03);
    assert!(report.is_connected());
    assert!(report.is_calibrated());
    assert!(!report.has_fault());
    Ok(())
}

/// Known-good byte sequence: full-scale throttle.
///
/// Wire format (little-endian):
///   bytes 0-1: throttle = 0xFFFF
///   bytes 2-3: brake    = 0x0000
///   bytes 4-5: clutch   = 0x0000
///   byte  6:   status   = 0x03
///   byte  7:   padding  = 0x00
#[test]
fn known_good_full_throttle_report() -> Result<(), Box<dyn std::error::Error>> {
    let full_throttle: [u8; 8] = [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00];
    let report = HeusinkveldInputReport::parse(&full_throttle).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, 0xFFFF);
    assert_eq!(report.brake, 0);
    assert_eq!(report.clutch, 0);
    assert!((report.throttle_normalized() - 1.0).abs() < f32::EPSILON);
    Ok(())
}

/// Known-good byte sequence: mid-scale brake application.
///
/// Wire format (little-endian):
///   bytes 0-1: throttle = 0x0000
///   bytes 2-3: brake    = 0x0080 (= 32768, ~50%)
///   bytes 4-5: clutch   = 0x0000
///   byte  6:   status   = 0x03
///   byte  7:   padding  = 0x00
#[test]
fn known_good_mid_brake_report() -> Result<(), Box<dyn std::error::Error>> {
    let mid_brake: [u8; 8] = [0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x03, 0x00];
    let report = HeusinkveldInputReport::parse(&mid_brake).map_err(|e| e.to_string())?;
    assert_eq!(report.throttle, 0);
    assert_eq!(report.brake, 0x8000);
    assert_eq!(report.clutch, 0);
    assert!((report.brake_normalized() - 0.5).abs() < 0.001);
    Ok(())
}

/// Known-good byte sequence: fault condition.
///
/// Wire format:
///   byte 6: status = 0x05 (connected | fault, NOT calibrated)
#[test]
fn known_good_fault_report() -> Result<(), Box<dyn std::error::Error>> {
    let fault: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00];
    let report = HeusinkveldInputReport::parse(&fault).map_err(|e| e.to_string())?;
    assert!(report.is_connected());
    assert!(!report.is_calibrated());
    assert!(report.has_fault());
    Ok(())
}

/// Load cell value range: 16-bit unsigned (0–65535).
///
/// Source: Heusinkveld products use load cells with 16-bit ADC resolution.
#[test]
fn load_cell_max_is_u16_max() {
    assert_eq!(MAX_LOAD_CELL_VALUE, u16::MAX);
    assert_eq!(MAX_LOAD_CELL_VALUE, 0xFFFF);
}

// ─── Cross-vendor VID collision checks ──────────────────────────────────────

/// VID 0x04D8 (Microchip) is shared by thousands of devices. Verify that
/// our code requires both VID AND PID to identify a Heusinkveld device.
#[test]
fn legacy_vid_requires_pid_match() {
    // Random PID on Microchip VID should NOT match
    assert_eq!(
        HeusinkveldModel::from_vid_pid(0x04D8, 0x0001),
        HeusinkveldModel::Unknown
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(0x04D8, 0x0000),
        HeusinkveldModel::Unknown
    );
    // But known legacy PIDs do match
    assert_eq!(
        HeusinkveldModel::from_vid_pid(0x04D8, 0xF6D0),
        HeusinkveldModel::Sprint
    );
}

/// VID 0x10C4 (Silicon Labs) is also shared. Verify PID is required.
#[test]
fn silicon_labs_vid_requires_pid_match() {
    assert_eq!(
        HeusinkveldModel::from_vid_pid(0x10C4, 0x0001),
        HeusinkveldModel::Unknown
    );
    assert_eq!(
        HeusinkveldModel::from_vid_pid(0x10C4, 0x8B82),
        HeusinkveldModel::HandbrakeV1
    );
}

// ─── Product metadata cross-verification ────────────────────────────────────

/// Sprint pedals: 2-pedal set.
///
/// Source: heusinkveld.com product page for Sprint pedals.
#[test]
fn sprint_is_2_pedal() {
    assert_eq!(HeusinkveldModel::Sprint.pedal_count(), 2);
}

/// Ultimate+ pedals: 3-pedal set, up to 140 kg brake force.
///
/// Source: heusinkveld.com — "up to 140kg of force".
#[test]
fn ultimate_is_3_pedal_140kg() {
    assert_eq!(HeusinkveldModel::Ultimate.pedal_count(), 3);
    assert_eq!(HeusinkveldModel::Ultimate.max_load_kg(), 140.0);
}

/// Handbrake and Shifter are not pedal devices (pedal_count = 0).
#[test]
fn peripherals_are_not_pedals() {
    assert_eq!(HeusinkveldModel::HandbrakeV1.pedal_count(), 0);
    assert_eq!(HeusinkveldModel::HandbrakeV2.pedal_count(), 0);
    assert_eq!(HeusinkveldModel::SequentialShifter.pedal_count(), 0);
}

// ─── Table-driven full VID/PID cross-check ──────────────────────────────────

/// Exhaustive table-driven test: every known VID/PID pair must resolve to
/// the correct model. Sources cited inline per row.
#[test]
fn table_driven_vid_pid_cross_check() -> Result<(), String> {
    let table: &[(u16, u16, HeusinkveldModel, &str)] = &[
        // simracing-hwdb v30B7p1001
        (0x30B7, 0x1001, HeusinkveldModel::Sprint, "simracing-hwdb"),
        // simracing-hwdb v30B7p1003
        (0x30B7, 0x1003, HeusinkveldModel::Ultimate, "simracing-hwdb"),
        // simracing-hwdb v30B7p1002
        (0x30B7, 0x1002, HeusinkveldModel::HandbrakeV2, "simracing-hwdb"),
        // OpenFlight sprint-pedals.yaml
        (0x04D8, 0xF6D0, HeusinkveldModel::Sprint, "OpenFlight YAML"),
        // OpenFlight ultimate-pedals-0241.yaml
        (0x04D8, 0xF6D2, HeusinkveldModel::Ultimate, "OpenFlight YAML"),
        // Sequential estimate
        (0x04D8, 0xF6D3, HeusinkveldModel::Pro, "estimated (sequential)"),
        // simracing-hwdb v10C4p8B82
        (0x10C4, 0x8B82, HeusinkveldModel::HandbrakeV1, "simracing-hwdb"),
        // simracing-hwdb vA020p3142
        (0xA020, 0x3142, HeusinkveldModel::SequentialShifter, "simracing-hwdb"),
    ];

    for &(vid, pid, ref expected, source) in table {
        let actual = HeusinkveldModel::from_vid_pid(vid, pid);
        if actual != *expected {
            return Err(format!(
                "VID {vid:#06x} PID {pid:#06x} (source: {source}): \
                 expected {expected:?}, got {actual:?}"
            ));
        }
    }
    Ok(())
}

/// Verify no PID collision across all known Heusinkveld PIDs.
#[test]
fn no_pid_collisions_across_vid_groups() -> Result<(), String> {
    let pids: &[(u16, &str)] = &[
        (HEUSINKVELD_SPRINT_PID, "Sprint (0x30B7)"),
        (HEUSINKVELD_ULTIMATE_PID, "Ultimate (0x30B7)"),
        (HEUSINKVELD_HANDBRAKE_V2_PID, "Handbrake V2 (0x30B7)"),
        (HEUSINKVELD_LEGACY_SPRINT_PID, "Legacy Sprint (0x04D8)"),
        (HEUSINKVELD_LEGACY_ULTIMATE_PID, "Legacy Ultimate (0x04D8)"),
        (HEUSINKVELD_PRO_PID, "Pro (0x04D8)"),
        (HEUSINKVELD_HANDBRAKE_V1_PID, "Handbrake V1 (0x10C4)"),
        (HEUSINKVELD_SHIFTER_PID, "Shifter (0xA020)"),
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            if pids[i].0 == pids[j].0 {
                return Err(format!(
                    "PID collision: {} and {} both = {:#06x}",
                    pids[i].1, pids[j].1, pids[i].0
                ));
            }
        }
    }
    Ok(())
}
