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
