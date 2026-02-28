//! Cross-reference tests for Logitech VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_logitech_protocol::{LOGITECH_VENDOR_ID, product_ids};

/// Logitech VID must be 0x046D (Logitech Inc.).
///
/// Source: USB VID registry; Linux kernel hid-ids.h; oversteer project.
#[test]
fn vendor_id_is_046d() {
    assert_eq!(
        LOGITECH_VENDOR_ID, 0x046D,
        "Logitech VID changed — update ids.rs and SOURCES.md"
    );
}

// ── Product IDs — verified against Linux kernel hid-ids.h and oversteer ──────

#[test]
fn g25_pid_is_c299() {
    assert_eq!(product_ids::G25, 0xC299);
}

#[test]
fn g27_pid_is_c29b() {
    assert_eq!(product_ids::G27, 0xC29B);
}

#[test]
fn g29_ps_pid_is_c24f() {
    assert_eq!(product_ids::G29_PS, 0xC24F);
}

#[test]
fn g920_pid_is_c262() {
    assert_eq!(product_ids::G920, 0xC262);
}

#[test]
fn g923_ps_pid_is_c267() {
    assert_eq!(product_ids::G923_PS, 0xC267);
}

#[test]
fn g923_native_pid_is_c266() {
    assert_eq!(product_ids::G923, 0xC266);
}

#[test]
fn g923_xbox_pid_is_c26e() {
    assert_eq!(product_ids::G923_XBOX, 0xC26E);
}

#[test]
fn g_pro_pid_is_c268() {
    assert_eq!(product_ids::G_PRO, 0xC268);
}
