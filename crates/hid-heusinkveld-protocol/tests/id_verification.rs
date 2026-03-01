//! Cross-reference tests for Heusinkveld VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_heusinkveld_protocol::{
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
};

/// Heusinkveld VID must be 0x04D8 (Microchip Technology).
///
/// ✅ Confirmed by: the-sz.com, devicehunt.com (Microchip Technology, Inc.).
/// Note: Heusinkveld does not own this VID — it is Microchip's shared VID.
#[test]
fn vendor_id_is_04d8() {
    assert_eq!(
        HEUSINKVELD_VENDOR_ID, 0x04D8,
        "Heusinkveld VID changed — update ids.rs and SOURCES.md"
    );
}

/// Heusinkveld Sprint PID must be 0xF6D0.
///
/// 🔶 Community-sourced — OpenFlight `sprint-pedals.yaml`.
/// Not present in any public USB ID database or Linux kernel driver.
#[test]
fn sprint_pid_is_f6d0() {
    assert_eq!(HEUSINKVELD_SPRINT_PID, 0xF6D0);
}

/// Heusinkveld Ultimate+ PID must be 0xF6D2.
///
/// 🔶 Community-sourced — OpenFlight `ultimate-pedals-0241.yaml`.
/// Not present in any public USB ID database or Linux kernel driver.
#[test]
fn ultimate_pid_is_f6d2() {
    assert_eq!(HEUSINKVELD_ULTIMATE_PID, 0xF6D2);
}

/// Heusinkveld Pro PID must be 0xF6D3.
///
/// ⚠ Estimated — sequential after 0xF6D2; Pro is discontinued.
/// **Zero external evidence.** Needs hardware USB descriptor dump.
#[test]
fn pro_pid_is_f6d3() {
    assert_eq!(HEUSINKVELD_PRO_PID, 0xF6D3);
}
