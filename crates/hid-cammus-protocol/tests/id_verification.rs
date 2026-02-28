//! Cross-reference tests for Cammus VID/PID constants against the golden values
//! recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_cammus_protocol::{PRODUCT_C12, PRODUCT_C5, VENDOR_ID};

/// Cammus VID must be 0x3416 (Shenzhen Cammus Electronic Technology Co., Ltd.).
///
/// Source: USB VID registry (the-sz.com); JacKeTUs/linux-steering-wheels.
#[test]
fn vendor_id_is_3416() {
    assert_eq!(
        VENDOR_ID, 0x3416,
        "Cammus VID changed — update ids.rs and SOURCES.md"
    );
}

/// Cammus C5 (5 Nm) PID must be 0x0301.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn c5_pid_is_0301() {
    assert_eq!(PRODUCT_C5, 0x0301);
}

/// Cammus C12 (12 Nm) PID must be 0x0302.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn c12_pid_is_0302() {
    assert_eq!(PRODUCT_C12, 0x0302);
}
