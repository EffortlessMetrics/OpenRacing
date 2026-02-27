//! Cross-reference tests for Moza VID/PID constants against the golden values
//! recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_moza_protocol::{MOZA_VENDOR_ID, product_ids};

/// Moza vendor ID must be 0x346E (Moza Racing USB VID).
///
/// Source: USB VID registry; JacKeTUs/linux-steering-wheels; iRacing forum USB captures.
#[test]
fn vendor_id_is_346e() {
    assert_eq!(
        MOZA_VENDOR_ID, 0x346E,
        "Moza VID changed — update ids.rs and SOURCES.md"
    );
}

// ── Wheelbase V1 PIDs ────────────────────────────────────────────────────────

#[test]
fn r16_r21_v1_pid_is_0000() {
    assert_eq!(product_ids::R16_R21_V1, 0x0000);
}

#[test]
fn r9_v1_pid_is_0002() {
    assert_eq!(product_ids::R9_V1, 0x0002);
}

#[test]
fn r5_v1_pid_is_0004() {
    assert_eq!(product_ids::R5_V1, 0x0004);
}

#[test]
fn r3_v1_pid_is_0005() {
    assert_eq!(product_ids::R3_V1, 0x0005);
}

#[test]
fn r12_v1_pid_is_0006() {
    assert_eq!(product_ids::R12_V1, 0x0006);
}

// ── Wheelbase V2 PIDs ────────────────────────────────────────────────────────

#[test]
fn r16_r21_v2_pid_is_0010() {
    assert_eq!(product_ids::R16_R21_V2, 0x0010);
}

#[test]
fn r9_v2_pid_is_0012() {
    assert_eq!(product_ids::R9_V2, 0x0012);
}

#[test]
fn r5_v2_pid_is_0014() {
    assert_eq!(product_ids::R5_V2, 0x0014);
}

#[test]
fn r3_v2_pid_is_0015() {
    assert_eq!(product_ids::R3_V2, 0x0015);
}

#[test]
fn r12_v2_pid_is_0016() {
    assert_eq!(product_ids::R12_V2, 0x0016);
}

// ── Peripheral PIDs ──────────────────────────────────────────────────────────

#[test]
fn sr_p_pedals_pid_is_0003() {
    assert_eq!(product_ids::SR_P_PEDALS, 0x0003);
}

#[test]
fn hgp_shifter_pid_is_0020() {
    assert_eq!(product_ids::HGP_SHIFTER, 0x0020);
}

#[test]
fn sgp_shifter_pid_is_0021() {
    assert_eq!(product_ids::SGP_SHIFTER, 0x0021);
}

#[test]
fn hbp_handbrake_pid_is_0022() {
    assert_eq!(product_ids::HBP_HANDBRAKE, 0x0022);
}
