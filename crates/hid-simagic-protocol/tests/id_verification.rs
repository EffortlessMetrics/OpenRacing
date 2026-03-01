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
        product_ids::HANDBRAKE, 0x0A04,
        "Simagic Handbrake PID changed — update ids.rs and SOURCES.md"
    );
}
