//! Cross-reference tests for Fanatec VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_fanatec_protocol::{FANATEC_VENDOR_ID, product_ids};

/// Fanatec VID must be 0x0EB7 (Endor AG / Fanatec).
///
/// Source: USB VID registry; JacKeTUs/linux-steering-wheels.
#[test]
fn vendor_id_is_0eb7() {
    assert_eq!(
        FANATEC_VENDOR_ID, 0x0EB7,
        "Fanatec VID changed — update ids.rs and SOURCES.md"
    );
}

// ── Product IDs — verified against linux-steering-wheels + Fanatec community ─

#[test]
fn csl_elite_base_pid_is_0004() {
    assert_eq!(product_ids::CSL_ELITE_BASE, 0x0004);
}

#[test]
fn dd1_pid_is_0006() {
    assert_eq!(product_ids::DD1, 0x0006);
}

#[test]
fn dd2_pid_is_0007() {
    assert_eq!(product_ids::DD2, 0x0007);
}

#[test]
fn csl_dd_pid_is_0020() {
    assert_eq!(product_ids::CSL_DD, 0x0020);
}
