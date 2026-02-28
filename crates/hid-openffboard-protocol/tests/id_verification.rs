//! Cross-reference tests for OpenFFBoard VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_openffboard_protocol::ids::{
    OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
};

/// OpenFFBoard VID must be 0x1209 (pid.codes open hardware VID).
///
/// Source: <https://pid.codes/1209/FFB0/>; OpenFFBoard GitHub project.
#[test]
fn vendor_id_is_1209() {
    assert_eq!(
        OPENFFBOARD_VENDOR_ID, 0x1209,
        "OpenFFBoard VID changed — update ids.rs and SOURCES.md"
    );
}

/// OpenFFBoard main firmware PID must be 0xFFB0.
///
/// Source: <https://pid.codes/1209/FFB0/>; official pid.codes registry entry.
#[test]
fn main_pid_is_ffb0() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID, 0xFFB0);
}

/// OpenFFBoard alternate firmware PID must be 0xFFB1.
///
/// NOTE: 0xFFB1 is **not** registered on pid.codes and is absent from the
/// official configurator and driver.  Pinned here to track the current value;
/// update if an authoritative source confirms or replaces it.
#[test]
fn alt_pid_is_ffb1() {
    assert_eq!(OPENFFBOARD_PRODUCT_ID_ALT, 0xFFB1);
}
