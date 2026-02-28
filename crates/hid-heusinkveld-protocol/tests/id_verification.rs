//! Cross-reference tests for Heusinkveld VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_heusinkveld_protocol::{
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
};

/// Heusinkveld VID must be 0x16D0 (MCS Electronics / OpenMoko).
///
/// Source: USB VID registry; JacKeTUs/linux-steering-wheels.
#[test]
fn vendor_id_is_16d0() {
    assert_eq!(
        HEUSINKVELD_VENDOR_ID, 0x16D0,
        "Heusinkveld VID changed — update ids.rs and SOURCES.md"
    );
}

/// Heusinkveld Sprint PID must be 0x1156.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn sprint_pid_is_1156() {
    assert_eq!(HEUSINKVELD_SPRINT_PID, 0x1156);
}

/// Heusinkveld Ultimate+ PID must be 0x1157.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn ultimate_pid_is_1157() {
    assert_eq!(HEUSINKVELD_ULTIMATE_PID, 0x1157);
}

/// Heusinkveld Pro PID must be 0x1158.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn pro_pid_is_1158() {
    assert_eq!(HEUSINKVELD_PRO_PID, 0x1158);
}
