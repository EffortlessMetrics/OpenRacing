//! Cross-reference tests for FFBeast VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_ffbeast_protocol::ids::{
    FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL,
    FFBEAST_VENDOR_ID,
};

/// FFBeast VID must be 0x045B (`USB_VENDOR_ID_FFBEAST` in the Linux kernel).
///
/// Source: Linux kernel `hid-ids.h`; <https://github.com/HF-Robotics/FFBeast>.
#[test]
fn vendor_id_is_045b() {
    assert_eq!(
        FFBEAST_VENDOR_ID, 0x045B,
        "FFBeast VID changed — update ids.rs and SOURCES.md"
    );
}

/// FFBeast joystick PID must be 0x58F9.
#[test]
fn joystick_pid_is_58f9() {
    assert_eq!(FFBEAST_PRODUCT_ID_JOYSTICK, 0x58F9);
}

/// FFBeast rudder PID must be 0x5968.
#[test]
fn rudder_pid_is_5968() {
    assert_eq!(FFBEAST_PRODUCT_ID_RUDDER, 0x5968);
}

/// FFBeast wheel PID must be 0x59D7.
#[test]
fn wheel_pid_is_59d7() {
    assert_eq!(FFBEAST_PRODUCT_ID_WHEEL, 0x59D7);
}
