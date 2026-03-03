//! Cross-reference tests for Simagic VID/PID constants against the golden values
//! recorded in `docs/protocols/SOURCES.md` (F-005).
//!
//! If any assertion fails, update `ids.rs` AND the SOURCES.md table together.
//! Do not change only one of the two — they must stay in sync.

use racing_wheel_hid_simagic_protocol::ids::{
    SIMAGIC_LEGACY_PID, SIMAGIC_LEGACY_VENDOR_ID, SIMAGIC_VENDOR_ID, product_ids,
};

/// Simagic EVO-generation VID must be 0x3670 (Shen Zhen Simagic Technology Co., Limited).
///
/// Source: USB VID registry; JacKeTUs/linux-steering-wheels; JacKeTUs/simagic-ff driver.
#[test]
fn evo_vendor_id_is_3670() {
    assert_eq!(
        SIMAGIC_VENDOR_ID, 0x3670,
        "Simagic EVO VID changed — update ids.rs and SOURCES.md"
    );
}

/// Legacy Simagic VID must be 0x0483 (STMicroelectronics, shared with VRS).
///
/// Source: JacKeTUs/simagic-ff kernel driver source; USB VID registry.
#[test]
fn legacy_vendor_id_is_0483() {
    assert_eq!(
        SIMAGIC_LEGACY_VENDOR_ID, 0x0483,
        "Simagic legacy VID changed — update ids.rs and SOURCES.md"
    );
}

/// Legacy PID 0x0522 is shared by Alpha / Alpha Mini / M10 / Alpha Ultimate.
///
/// Source: JacKeTUs/simagic-ff kernel driver source.
#[test]
fn legacy_pid_is_0522() {
    assert_eq!(
        SIMAGIC_LEGACY_PID, 0x0522,
        "Simagic legacy PID changed — update ids.rs and SOURCES.md"
    );
}

// ── EVO generation wheelbase PIDs (Verified) ────────────────────────────────

#[test]
fn evo_sport_pid_is_0500() {
    assert_eq!(product_ids::EVO_SPORT, 0x0500);
}

#[test]
fn evo_pid_is_0501() {
    assert_eq!(product_ids::EVO, 0x0501);
}

#[test]
fn evo_pro_pid_is_0502() {
    assert_eq!(product_ids::EVO_PRO, 0x0502);
}

// ── Accessories (Verified) ──────────────────────────────────────────────────

/// Simagic TB-RS Handbrake PID must be 0x0A04.
///
/// Source: JacKeTUs/simracing-hwdb `90-simagic.hwdb` (`v3670p0A04`).
#[test]
fn handbrake_pid_is_0a04() {
    assert_eq!(
        product_ids::HANDBRAKE,
        0x0A04,
        "Simagic Handbrake PID changed — update ids.rs and SOURCES.md"
    );
}

// ── Estimated PIDs (pin values to detect accidental changes) ────────────────

#[test]
fn alpha_evo_pid_is_0600() {
    assert_eq!(product_ids::ALPHA_EVO, 0x0600);
}

#[test]
fn neo_pid_is_0700() {
    assert_eq!(product_ids::NEO, 0x0700);
}

#[test]
fn neo_mini_pid_is_0701() {
    assert_eq!(product_ids::NEO_MINI, 0x0701);
}

#[test]
fn p1000_pedals_pid_is_1001() {
    assert_eq!(product_ids::P1000_PEDALS, 0x1001);
}

#[test]
fn p2000_pedals_pid_is_1002() {
    assert_eq!(product_ids::P2000_PEDALS, 0x1002);
}

#[test]
fn p1000a_pedals_pid_is_1003() {
    assert_eq!(product_ids::P1000A_PEDALS, 0x1003);
}

#[test]
fn shifter_h_pid_is_2001() {
    assert_eq!(product_ids::SHIFTER_H, 0x2001);
}

#[test]
fn shifter_seq_pid_is_2002() {
    assert_eq!(product_ids::SHIFTER_SEQ, 0x2002);
}

#[test]
fn rim_wr1_pid_is_4001() {
    assert_eq!(product_ids::RIM_WR1, 0x4001);
}

#[test]
fn rim_gt1_pid_is_4002() {
    assert_eq!(product_ids::RIM_GT1, 0x4002);
}

#[test]
fn rim_gt_neo_pid_is_4003() {
    assert_eq!(product_ids::RIM_GT_NEO, 0x4003);
}

#[test]
fn rim_formula_pid_is_4004() {
    assert_eq!(product_ids::RIM_FORMULA, 0x4004);
}

// ── PID uniqueness ──────────────────────────────────────────────────────────

/// All product ID constants must have distinct values — duplicates would cause
/// ambiguous device identification.
#[test]
fn all_product_ids_are_unique() {
    let all_pids: &[(u16, &str)] = &[
        (product_ids::EVO_SPORT, "EVO_SPORT"),
        (product_ids::EVO, "EVO"),
        (product_ids::EVO_PRO, "EVO_PRO"),
        (product_ids::ALPHA_EVO, "ALPHA_EVO"),
        (product_ids::NEO, "NEO"),
        (product_ids::NEO_MINI, "NEO_MINI"),
        (product_ids::P1000_PEDALS, "P1000_PEDALS"),
        (product_ids::P1000A_PEDALS, "P1000A_PEDALS"),
        (product_ids::P2000_PEDALS, "P2000_PEDALS"),
        (product_ids::SHIFTER_H, "SHIFTER_H"),
        (product_ids::SHIFTER_SEQ, "SHIFTER_SEQ"),
        (product_ids::HANDBRAKE, "HANDBRAKE"),
        (product_ids::RIM_WR1, "RIM_WR1"),
        (product_ids::RIM_GT1, "RIM_GT1"),
        (product_ids::RIM_GT_NEO, "RIM_GT_NEO"),
        (product_ids::RIM_FORMULA, "RIM_FORMULA"),
    ];
    for (i, &(pid_a, name_a)) in all_pids.iter().enumerate() {
        for &(pid_b, name_b) in &all_pids[i + 1..] {
            assert_ne!(
                pid_a, pid_b,
                "duplicate PID {pid_a:#06x}: {name_a} and {name_b}"
            );
        }
    }
}

// ── VID/PID domain checks ───────────────────────────────────────────────────

/// Both Simagic VIDs must be distinct from each other.
#[test]
fn vendor_ids_are_distinct() {
    assert_ne!(
        SIMAGIC_VENDOR_ID, SIMAGIC_LEGACY_VENDOR_ID,
        "modern and legacy VIDs must differ"
    );
}

/// The legacy PID must not collide with any modern (VID 0x3670) product ID.
#[test]
fn legacy_pid_not_in_modern_product_ids() {
    let modern_pids = [
        product_ids::EVO_SPORT,
        product_ids::EVO,
        product_ids::EVO_PRO,
        product_ids::ALPHA_EVO,
        product_ids::NEO,
        product_ids::NEO_MINI,
        product_ids::P1000_PEDALS,
        product_ids::P1000A_PEDALS,
        product_ids::P2000_PEDALS,
        product_ids::SHIFTER_H,
        product_ids::SHIFTER_SEQ,
        product_ids::HANDBRAKE,
        product_ids::RIM_WR1,
        product_ids::RIM_GT1,
        product_ids::RIM_GT_NEO,
        product_ids::RIM_FORMULA,
    ];
    for &pid in &modern_pids {
        assert_ne!(
            SIMAGIC_LEGACY_PID, pid,
            "legacy PID {:#06x} collides with modern PID {pid:#06x}",
            SIMAGIC_LEGACY_PID,
        );
    }
}
