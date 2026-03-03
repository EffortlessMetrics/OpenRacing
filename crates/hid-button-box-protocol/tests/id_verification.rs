//! Cross-reference tests for generic button-box VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update the constants in `lib.rs` AND the SOURCES.md
//! table together. Do not change only one of the two — they must stay in sync.

use hid_button_box_protocol::{PRODUCT_ID_BUTTON_BOX, VENDOR_ID_GENERIC};

/// Generic button-box VID must be 0x1209 (pid.codes open hardware VID).
///
/// Source: <https://pid.codes/1209/1BBD/>; open hardware community registry.
#[test]
fn vendor_id_is_1209() {
    assert_eq!(
        VENDOR_ID_GENERIC, 0x1209,
        "Button-box VID changed — update lib.rs and SOURCES.md"
    );
}

/// Generic button-box PID must be 0x1BBD.
///
/// Source: <https://pid.codes/1209/1BBD/>; open hardware community registry.
#[test]
fn product_id_is_1bbd() {
    assert_eq!(
        PRODUCT_ID_BUTTON_BOX, 0x1BBD,
        "Button-box PID changed — update lib.rs and SOURCES.md"
    );
}

/// VID and PID must form a unique pair (they must not be equal).
#[test]
fn vid_pid_are_distinct() {
    assert_ne!(
        VENDOR_ID_GENERIC, PRODUCT_ID_BUTTON_BOX,
        "VID and PID must not be identical"
    );
}

/// PID must be non-zero.
#[test]
fn product_id_nonzero() {
    assert_ne!(PRODUCT_ID_BUTTON_BOX, 0, "Button-box PID must not be zero");
}

/// VID must be non-zero.
#[test]
fn vendor_id_nonzero() {
    assert_ne!(VENDOR_ID_GENERIC, 0, "Button-box VID must not be zero");
}

/// VID 0x1209 is the pid.codes open-source hardware vendor ID.
/// Verify it falls in the expected range for community/open-source devices.
#[test]
fn vendor_id_is_pidcodes_range() {
    assert_eq!(
        VENDOR_ID_GENERIC, 0x1209,
        "Button-box VID must be 0x1209 (pid.codes open hardware)"
    );
}

/// PID must be in the valid USB range (not in the reserved low byte range).
#[test]
fn product_id_above_reserved_range() {
    // Compile-time check that PID is above reserved range.
    const _: () = assert!(PRODUCT_ID_BUTTON_BOX > 0x00FF);
    // Runtime assertion with descriptive message.
    let pid = PRODUCT_ID_BUTTON_BOX;
    assert!(
        pid > 0x00FF,
        "Button-box PID {:#06x} must be above reserved range 0x00FF",
        pid
    );
}
