//! Cross-reference tests for Fanatec VID/PID constants against
//! the community Linux kernel driver `gotzl/hid-fanatecff` (`hid-ftec.h`).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_fanatec_protocol::{FANATEC_VENDOR_ID, product_ids};

/// Fanatec VID must be 0x0EB7 (Endor AG / Fanatec).
///
/// Source: USB VID registry; gotzl/hid-fanatecff `hid-ftec.h`.
#[test]
fn vendor_id_is_0eb7() {
    assert_eq!(
        FANATEC_VENDOR_ID, 0x0EB7,
        "Fanatec VID changed — update ids.rs and SOURCES.md"
    );
}

// ── Product IDs — verified against gotzl/hid-fanatecff hid-ftec.h ──────────

#[test]
fn clubsport_v2_pid_is_0001() {
    assert_eq!(product_ids::CLUBSPORT_V2, 0x0001);
}

#[test]
fn clubsport_v2_5_pid_is_0004() {
    assert_eq!(product_ids::CLUBSPORT_V2_5, 0x0004);
}

#[test]
fn csl_elite_ps4_pid_is_0005() {
    assert_eq!(product_ids::CSL_ELITE_PS4, 0x0005);
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
fn csr_elite_pid_is_0011() {
    assert_eq!(product_ids::CSR_ELITE, 0x0011);
}

#[test]
fn csl_dd_pid_is_0020() {
    assert_eq!(product_ids::CSL_DD, 0x0020);
}

#[test]
fn csl_elite_pid_is_0e03() {
    assert_eq!(product_ids::CSL_ELITE, 0x0E03);
}

#[test]
fn csl_elite_pedals_pid_is_6204() {
    assert_eq!(product_ids::CSL_ELITE_PEDALS, 0x6204);
}

#[test]
fn clubsport_pedals_v3_pid_is_183b() {
    assert_eq!(product_ids::CLUBSPORT_PEDALS_V3, 0x183B);
}

#[test]
fn csl_pedals_lc_pid_is_6205() {
    assert_eq!(product_ids::CSL_PEDALS_LC, 0x6205);
}

#[test]
fn csl_pedals_v2_pid_is_6206() {
    assert_eq!(product_ids::CSL_PEDALS_V2, 0x6206);
}
