//! Cross-reference tests for Heusinkveld VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_heusinkveld_protocol::{
    HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID,
    HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_ULTIMATE_PID, HEUSINKVELD_LEGACY_VENDOR_ID,
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SHIFTER_PID, HEUSINKVELD_SHIFTER_VENDOR_ID,
    HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
};

/// Current Heusinkveld VID must be 0x30B7.
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb `90-heusinkveld.hwdb`.
#[test]
fn vendor_id_is_30b7() {
    assert_eq!(
        HEUSINKVELD_VENDOR_ID, 0x30B7,
        "Heusinkveld current VID changed — update ids.rs and SOURCES.md"
    );
}

/// Legacy VID must be 0x04D8 (Microchip Technology).
#[test]
fn legacy_vendor_id_is_04d8() {
    assert_eq!(
        HEUSINKVELD_LEGACY_VENDOR_ID, 0x04D8,
        "Heusinkveld legacy VID changed — update ids.rs and SOURCES.md"
    );
}

/// Current Sprint PID must be 0x1001 (VID 0x30B7).
#[test]
fn sprint_pid_is_1001() {
    assert_eq!(HEUSINKVELD_SPRINT_PID, 0x1001);
}

/// Current Ultimate PID must be 0x1003 (VID 0x30B7).
#[test]
fn ultimate_pid_is_1003() {
    assert_eq!(HEUSINKVELD_ULTIMATE_PID, 0x1003);
}

/// Legacy Sprint PID must be 0xF6D0 (VID 0x04D8).
#[test]
fn legacy_sprint_pid_is_f6d0() {
    assert_eq!(HEUSINKVELD_LEGACY_SPRINT_PID, 0xF6D0);
}

/// Legacy Ultimate PID must be 0xF6D2 (VID 0x04D8).
#[test]
fn legacy_ultimate_pid_is_f6d2() {
    assert_eq!(HEUSINKVELD_LEGACY_ULTIMATE_PID, 0xF6D2);
}

/// Pro PID must be 0xF6D3 (legacy/discontinued, VID 0x04D8).
#[test]
fn pro_pid_is_f6d3() {
    assert_eq!(HEUSINKVELD_PRO_PID, 0xF6D3);
}

/// Handbrake V1 PID must be 0x8B82 (VID 0x10C4 Silicon Labs).
#[test]
fn handbrake_v1_pid_is_8b82() {
    assert_eq!(HEUSINKVELD_HANDBRAKE_V1_PID, 0x8B82);
    assert_eq!(HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, 0x10C4);
}

/// Handbrake V2 PID must be 0x1002 (VID 0x30B7).
#[test]
fn handbrake_v2_pid_is_1002() {
    assert_eq!(HEUSINKVELD_HANDBRAKE_V2_PID, 0x1002);
}

/// Sequential Shifter PID must be 0x3142 (VID 0xA020).
#[test]
fn shifter_pid_is_3142() {
    assert_eq!(HEUSINKVELD_SHIFTER_PID, 0x3142);
    assert_eq!(HEUSINKVELD_SHIFTER_VENDOR_ID, 0xA020);
}

/// Cross-check: all known Heusinkveld PIDs resolve to the expected model via
/// `HeusinkveldModel::from_vid_pid`. This is a single table-driven test so
/// any future PID addition that forgets to update the match arm will fail.
#[test]
fn cross_check_all_known_pids() -> Result<(), String> {
    use hid_heusinkveld_protocol::HeusinkveldModel;

    let table: &[(u16, u16, HeusinkveldModel)] = &[
        // Current VID 0x30B7
        (
            HEUSINKVELD_VENDOR_ID,
            HEUSINKVELD_SPRINT_PID,
            HeusinkveldModel::Sprint,
        ),
        (
            HEUSINKVELD_VENDOR_ID,
            HEUSINKVELD_ULTIMATE_PID,
            HeusinkveldModel::Ultimate,
        ),
        (
            HEUSINKVELD_VENDOR_ID,
            HEUSINKVELD_HANDBRAKE_V2_PID,
            HeusinkveldModel::HandbrakeV2,
        ),
        // Legacy VID 0x04D8
        (
            HEUSINKVELD_LEGACY_VENDOR_ID,
            HEUSINKVELD_LEGACY_SPRINT_PID,
            HeusinkveldModel::Sprint,
        ),
        (
            HEUSINKVELD_LEGACY_VENDOR_ID,
            HEUSINKVELD_LEGACY_ULTIMATE_PID,
            HeusinkveldModel::Ultimate,
        ),
        (
            HEUSINKVELD_LEGACY_VENDOR_ID,
            HEUSINKVELD_PRO_PID,
            HeusinkveldModel::Pro,
        ),
        // Peripherals
        (
            HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
            HEUSINKVELD_HANDBRAKE_V1_PID,
            HeusinkveldModel::HandbrakeV1,
        ),
        (
            HEUSINKVELD_SHIFTER_VENDOR_ID,
            HEUSINKVELD_SHIFTER_PID,
            HeusinkveldModel::SequentialShifter,
        ),
    ];

    for &(vid, pid, ref expected) in table {
        let model = HeusinkveldModel::from_vid_pid(vid, pid);
        if model != *expected {
            return Err(format!(
                "VID {vid:#06x} PID {pid:#06x}: expected {expected:?}, got {model:?}"
            ));
        }
        // Also verify is_heusinkveld_device recognises the VID
        if !hid_heusinkveld_protocol::is_heusinkveld_device(vid) {
            return Err(format!(
                "is_heusinkveld_device({vid:#06x}) returned false for known VID"
            ));
        }
    }
    Ok(())
}

/// Cross-check: no two known PIDs (across all VID groups) share the same value,
/// to prevent accidental copy-paste of PID constants.
#[test]
fn all_pids_are_unique() -> Result<(), String> {
    let pids: &[(u16, &str)] = &[
        (HEUSINKVELD_SPRINT_PID, "SPRINT"),
        (HEUSINKVELD_ULTIMATE_PID, "ULTIMATE"),
        (HEUSINKVELD_HANDBRAKE_V2_PID, "HANDBRAKE_V2"),
        (HEUSINKVELD_LEGACY_SPRINT_PID, "LEGACY_SPRINT"),
        (HEUSINKVELD_LEGACY_ULTIMATE_PID, "LEGACY_ULTIMATE"),
        (HEUSINKVELD_PRO_PID, "PRO"),
        (HEUSINKVELD_HANDBRAKE_V1_PID, "HANDBRAKE_V1"),
        (HEUSINKVELD_SHIFTER_PID, "SHIFTER"),
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            if pids[i].0 == pids[j].0 {
                return Err(format!(
                    "PID collision: {} and {} both have value {:#06x}",
                    pids[i].1, pids[j].1, pids[i].0
                ));
            }
        }
    }
    Ok(())
}
