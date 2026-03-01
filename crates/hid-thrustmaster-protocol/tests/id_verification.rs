//! Cross-reference tests for Thrustmaster VID/PID constants against
//! the golden values recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_thrustmaster_protocol::{THRUSTMASTER_VENDOR_ID, product_ids};

/// Thrustmaster VID must be 0x044F (Guillemot / Thrustmaster).
///
/// Source: USB VID registry; Linux kernel hid-ids.h; JacKeTUs/linux-steering-wheels.
#[test]
fn vendor_id_is_044f() {
    assert_eq!(
        THRUSTMASTER_VENDOR_ID, 0x044F,
        "Thrustmaster VID changed — update ids.rs and SOURCES.md"
    );
}

// ── Product IDs — verified against linux-steering-wheels + Linux kernel ──────

#[test]
fn t150_pid_is_b677() {
    assert_eq!(product_ids::T150, 0xB677);
}

#[test]
fn t300_rs_pid_is_b66e() {
    assert_eq!(product_ids::T300_RS, 0xB66E);
}

#[test]
fn t300_rs_ps4_pid_is_b66d() {
    assert_eq!(product_ids::T300_RS_PS4, 0xB66D);
}

#[test]
fn tmx_pid_is_b67f() {
    assert_eq!(product_ids::TMX, 0xB67F);
}

#[test]
fn t248_pid_is_b696() {
    assert_eq!(product_ids::T248, 0xB696);
}

#[test]
fn ts_xw_pid_is_b692() {
    assert_eq!(product_ids::TS_XW, 0xB692);
}

#[test]
fn ts_xw_gip_pid_is_b691() {
    assert_eq!(product_ids::TS_XW_GIP, 0xB691);
}

#[test]
fn t248x_pid_is_b69a() {
    assert_eq!(product_ids::T248X, 0xB69A);
}

#[test]
fn t500_rs_pid_is_b65e() {
    assert_eq!(product_ids::T500_RS, 0xB65E);
}

#[test]
fn t300_rs_gt_pid_is_b66f() {
    assert_eq!(product_ids::T300_RS_GT, 0xB66F);
}

#[test]
fn tx_racing_pid_is_b669() {
    assert_eq!(product_ids::TX_RACING, 0xB669);
}

#[test]
fn ts_pc_racer_pid_is_b689() {
    assert_eq!(product_ids::TS_PC_RACER, 0xB689);
}

#[test]
fn t818_pid_is_b69b() {
    assert_eq!(product_ids::T818, 0xB69B);
}

#[test]
fn ffb_wheel_generic_pid_is_b65d() {
    assert_eq!(product_ids::FFB_WHEEL_GENERIC, 0xB65D);
}

#[test]
fn tx_racing_orig_pid_is_b664() {
    assert_eq!(product_ids::TX_RACING_ORIG, 0xB664);
}

#[test]
fn t80_pid_is_b668() {
    assert_eq!(product_ids::T80, 0xB668);
}

#[test]
fn t80_ferrari_488_pid_is_b66a() {
    assert_eq!(product_ids::T80_FERRARI_488, 0xB66A);
}
