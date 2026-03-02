//! Cross-reference tests for Heusinkveld VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_heusinkveld_protocol::{
    HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
    HEUSINKVELD_HANDBRAKE_V2_PID, HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_ULTIMATE_PID,
    HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID, HEUSINKVELD_SHIFTER_PID,
    HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_VENDOR_ID,
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
