//! Cross-reference tests for Leo Bodnar VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_leo_bodnar_protocol::ids::{
    PID_BBI32, PID_FFB_JOYSTICK, PID_SLI_M, PID_USB_JOYSTICK, PID_WHEEL_INTERFACE, VENDOR_ID,
};

/// Leo Bodnar VID must be 0x1DD2 (Leo Bodnar Electronics Ltd).
///
/// Source: USB VID registry (the-sz.com); community USB captures.
#[test]
fn vendor_id_is_1dd2() {
    assert_eq!(
        VENDOR_ID, 0x1DD2,
        "Leo Bodnar VID changed — update ids.rs and SOURCES.md"
    );
}

/// USB Joystick PID must be 0x0001.
#[test]
fn usb_joystick_pid_is_0001() {
    assert_eq!(PID_USB_JOYSTICK, 0x0001);
}

/// BBI-32 Button Box PID must be 0x000C.
#[test]
fn bbi32_pid_is_000c() {
    assert_eq!(PID_BBI32, 0x000C);
}

/// USB Sim Racing Wheel Interface PID must be 0x000E.
#[test]
fn wheel_interface_pid_is_000e() {
    assert_eq!(PID_WHEEL_INTERFACE, 0x000E);
}

/// FFB Joystick PID must be 0x000F.
#[test]
fn ffb_joystick_pid_is_000f() {
    assert_eq!(PID_FFB_JOYSTICK, 0x000F);
}

/// SLI-Pro Shift Light Indicator PID must be 0x1301.
///
/// NOTE: 0x1301 is a community estimate (see ids.rs doc comment).
/// This test pins the current value; update if a real PID is discovered.
#[test]
fn sli_m_pid_is_community_estimate() {
    assert_eq!(PID_SLI_M, 0x1301);
}
