//! Cross-reference tests for PXN VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_pxn_protocol::{
    PRODUCT_GT987_FF, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_SE, VENDOR_ID,
};

/// PXN VID must be 0x11FF (Shenzhen Jinyu Technology Co., Ltd.).
///
/// Source: USB VID registry; community USB captures.
#[test]
fn vendor_id_is_11ff() {
    assert_eq!(
        VENDOR_ID, 0x11FF,
        "PXN VID changed — update ids.rs and SOURCES.md"
    );
}

// ── Product IDs — from community USB device captures ─────────────────────────

#[test]
fn v10_pid_is_3245() {
    assert_eq!(PRODUCT_V10, 0x3245);
}

#[test]
fn v12_pid_is_1212() {
    assert_eq!(PRODUCT_V12, 0x1212);
}

#[test]
fn v12_lite_pid_is_1112() {
    assert_eq!(PRODUCT_V12_LITE, 0x1112);
}

#[test]
fn v12_lite_se_pid_is_1211() {
    assert_eq!(PRODUCT_V12_LITE_SE, 0x1211);
}

#[test]
fn gt987_ff_pid_is_2141() {
    assert_eq!(PRODUCT_GT987_FF, 0x2141);
}
