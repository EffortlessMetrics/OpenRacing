//! Cross-reference tests for VRS DirectForce Pro VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_vrs_protocol::{VRS_PRODUCT_ID, VRS_VENDOR_ID};

/// VRS VID must be 0x0483 (STMicroelectronics, shared with Simagic legacy).
///
/// Source: USB VID registry; VRS DirectForce Pro community reports.
#[test]
fn vendor_id_is_0483() {
    assert_eq!(
        VRS_VENDOR_ID, 0x0483,
        "VRS VID changed — update ids.rs and SOURCES.md"
    );
}

/// VRS DirectForce Pro PID must be 0xA355.
///
/// Source: JacKeTUs/linux-steering-wheels (Platinum support entry).
#[test]
fn directforce_pro_pid_is_a355() {
    assert_eq!(
        VRS_PRODUCT_ID, 0xA355,
        "VRS DirectForce Pro PID changed — update ids.rs and SOURCES.md"
    );
}
