//! Cross-reference tests for PXN / Lite Star VID/PID constants against
//! the golden values from the Linux kernel `hid-ids.h` (mainline ≥6.15).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_pxn_protocol::{
    PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2, VENDOR_ID,
};

/// PXN / Lite Star VID must be 0x11FF.
///
/// Source: Linux kernel `hid-ids.h` (`USB_VENDOR_ID_LITE_STAR = 0x11ff`).
#[test]
fn vendor_id_is_11ff() {
    assert_eq!(
        VENDOR_ID, 0x11FF,
        "PXN/Lite Star VID changed — update ids.rs and SOURCES.md"
    );
}

/// PXN V10 PID must be 0x3245.
///
/// Source: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V10 = 0x3245`).
#[test]
fn pxn_v10_pid_is_3245() {
    assert_eq!(
        PRODUCT_V10, 0x3245,
        "PXN V10 PID changed — update ids.rs and SOURCES.md"
    );
}

/// PXN V12 PID must be 0x1212.
///
/// Source: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V12 = 0x1212`).
#[test]
fn pxn_v12_pid_is_1212() {
    assert_eq!(
        PRODUCT_V12, 0x1212,
        "PXN V12 PID changed — update ids.rs and SOURCES.md"
    );
}

/// PXN V12 Lite PID must be 0x1112.
///
/// Source: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V12_LITE = 0x1112`).
#[test]
fn pxn_v12_lite_pid_is_1112() {
    assert_eq!(
        PRODUCT_V12_LITE, 0x1112,
        "PXN V12 Lite PID changed — update ids.rs and SOURCES.md"
    );
}

/// PXN V12 Lite variant 2 PID must be 0x1211.
///
/// Source: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V12_LITE_2 = 0x1211`).
#[test]
fn pxn_v12_lite_2_pid_is_1211() {
    assert_eq!(
        PRODUCT_V12_LITE_2, 0x1211,
        "PXN V12 Lite 2 PID changed — update ids.rs and SOURCES.md"
    );
}

/// Lite Star GT987 PID must be 0x2141.
///
/// Source: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_LITE_STAR_GT987 = 0x2141`).
#[test]
fn lite_star_gt987_pid_is_2141() {
    assert_eq!(
        PRODUCT_GT987, 0x2141,
        "Lite Star GT987 PID changed — update ids.rs and SOURCES.md"
    );
}
