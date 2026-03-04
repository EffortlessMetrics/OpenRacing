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

/// T818 constant retains 0xB69B for backward compatibility, but real T818
/// hardware shares PID 0xB696 with T248/T128 ("Thrustmaster Advanced Mode
/// Racer"). See hid-tmff2 issues #58 and #97.
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

// ── Product IDs — legacy wheels and peripherals ─────────────────────────────

#[test]
fn t_gt_ii_gt_pid_is_b681() {
    assert_eq!(product_ids::T_GT_II_GT, 0xB681);
}

#[test]
fn nascar_pro_ff2_pid_is_b605() {
    assert_eq!(product_ids::NASCAR_PRO_FF2, 0xB605);
}

#[test]
fn fgt_rumble_force_pid_is_b651() {
    assert_eq!(product_ids::FGT_RUMBLE_FORCE, 0xB651);
}

#[test]
fn rgt_ff_clutch_pid_is_b653() {
    assert_eq!(product_ids::RGT_FF_CLUTCH, 0xB653);
}

#[test]
fn fgt_force_feedback_pid_is_b654() {
    assert_eq!(product_ids::FGT_FORCE_FEEDBACK, 0xB654);
}

#[test]
fn f430_force_feedback_pid_is_b65a() {
    assert_eq!(product_ids::F430_FORCE_FEEDBACK, 0xB65A);
}

#[test]
fn tpr_pedals_pid_is_b68f() {
    assert_eq!(product_ids::TPR_PEDALS, 0xB68F);
}

#[test]
fn t_lcm_pid_is_b371() {
    assert_eq!(product_ids::T_LCM, 0xB371);
}
