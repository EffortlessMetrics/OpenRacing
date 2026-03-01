//! Cross-reference tests for Asetek SimSports VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use hid_asetek_protocol::{
    ASETEK_FORTE_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID,
    ASETEK_VENDOR_ID,
};

/// Asetek VID must be 0x2433 (Asetek A/S, official USB VID registry entry).
///
/// ✅ Confirmed by: the-sz.com, devicehunt.com, Linux `hid-ids.h`.
#[test]
fn vendor_id_is_2433() {
    assert_eq!(
        ASETEK_VENDOR_ID, 0x2433,
        "Asetek VID changed — update ids.rs and SOURCES.md"
    );
}

/// Asetek Invicta (27 Nm) PID must be 0xF300.
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs.
#[test]
fn invicta_pid_is_f300() {
    assert_eq!(ASETEK_INVICTA_PID, 0xF300);
}

/// Asetek Forte (18 Nm) PID must be 0xF301.
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs.
#[test]
fn forte_pid_is_f301() {
    assert_eq!(ASETEK_FORTE_PID, 0xF301);
}

/// Asetek La Prima (12 Nm) PID must be 0xF303.
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs,
/// moonrail/asetek_wheelbase_cli.
#[test]
fn laprima_pid_is_f303() {
    assert_eq!(ASETEK_LAPRIMA_PID, 0xF303);
}

/// Asetek Tony Kanaan Edition PID must be 0xF306.
///
/// ✅ Confirmed by: Linux `hid-ids.h`, `hid-universal-pidff.c`, JacKeTUs.
#[test]
fn tony_kanaan_pid_is_f306() {
    assert_eq!(ASETEK_TONY_KANAAN_PID, 0xF306);
}
