//! Cross-reference tests for Simucube VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_simucube_protocol::{
    SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_VENDOR_ID,
};

/// Simucube VID must be 0x16D0 (MCS Electronics / OpenMoko).
///
/// Source: USB VID registry; JacKeTUs/linux-steering-wheels.
#[test]
fn vendor_id_is_16d0() {
    assert_eq!(
        SIMUCUBE_VENDOR_ID, 0x16D0,
        "Simucube VID changed — update ids.rs and SOURCES.md"
    );
}

/// Simucube 2 Ultimate (35 Nm) PID must be 0x0D5F.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn sc2_ultimate_pid_is_0d5f() {
    assert_eq!(SIMUCUBE_2_ULTIMATE_PID, 0x0D5F);
}

/// Simucube 2 Pro (25 Nm) PID must be 0x0D60.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn sc2_pro_pid_is_0d60() {
    assert_eq!(SIMUCUBE_2_PRO_PID, 0x0D60);
}

/// Simucube 2 Sport (15 Nm) PID must be 0x0D61.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn sc2_sport_pid_is_0d61() {
    assert_eq!(SIMUCUBE_2_SPORT_PID, 0x0D61);
}
