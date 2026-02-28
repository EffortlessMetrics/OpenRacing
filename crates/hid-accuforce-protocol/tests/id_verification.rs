//! Cross-reference tests for AccuForce VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_accuforce_protocol::{PID_ACCUFORCE_PRO, VENDOR_ID};

/// AccuForce VID must be 0x1FC9 (NXP Semiconductors USB chip).
///
/// Source: community USB captures; RetroBat Wheels.cs (commit 0a54752).
#[test]
fn vendor_id_is_1fc9() {
    assert_eq!(
        VENDOR_ID, 0x1FC9,
        "AccuForce VID changed — update ids.rs and SOURCES.md"
    );
}

/// AccuForce Pro PID must be 0x804C.
///
/// Source: community USB device captures.
#[test]
fn accuforce_pro_pid_is_804c() {
    assert_eq!(
        PID_ACCUFORCE_PRO, 0x804C,
        "AccuForce Pro PID changed — update ids.rs and SOURCES.md"
    );
}
