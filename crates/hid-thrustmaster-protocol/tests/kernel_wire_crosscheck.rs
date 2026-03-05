//! Cross-check tests for Thrustmaster T300RS-family wire-format constants
//! against the community Linux kernel driver `Kimplul/hid-tmff2`.
//!
//! These tests pin the T300RS opcodes, effect constants, and waveform IDs
//! to the values documented in the kernel driver source. If any assertion
//! fails, the crate constants have drifted from the authoritative source.

use racing_wheel_hid_thrustmaster_protocol::effects;
use racing_wheel_hid_thrustmaster_protocol::ids::{THRUSTMASTER_VENDOR_ID, product_ids};

// ── Vendor ID ──────────────────────────────────────────────────────────────

/// Thrustmaster VID = 0x044F.
#[test]
fn vendor_id_is_044f() {
    assert_eq!(THRUSTMASTER_VENDOR_ID, 0x044F);
}

// ── Core product IDs (hid-tmff2 defines) ───────────────────────────────────

/// TMT300RS_PS4_NORM_ID = 0xb66d
#[test]
fn t300_rs_ps4_matches_kernel() {
    assert_eq!(product_ids::T300_RS_PS4, 0xB66D, "TMT300RS_PS4_NORM_ID");
}

/// TMT300RS_PS3_NORM_ID = 0xb66e
#[test]
fn t300_rs_matches_kernel() {
    assert_eq!(product_ids::T300_RS, 0xB66E, "TMT300RS_PS3_NORM_ID");
}

/// TMT300RS_PS3_ADV_ID = 0xb66f
#[test]
fn t300_rs_gt_matches_kernel() {
    assert_eq!(product_ids::T300_RS_GT, 0xB66F, "TMT300RS_PS3_ADV_ID");
}

/// TX_ACTIVE = 0xb669
#[test]
fn tx_racing_matches_kernel() {
    assert_eq!(product_ids::TX_RACING, 0xB669, "TX_ACTIVE");
}

/// TMT248_PC_ID = 0xb696
#[test]
fn t248_matches_kernel() {
    assert_eq!(product_ids::T248, 0xB696, "TMT248_PC_ID");
}

/// TMTS_PC_RACER_ID = 0xb689
#[test]
fn ts_pc_racer_matches_kernel() {
    assert_eq!(product_ids::TS_PC_RACER, 0xB689, "TMTS_PC_RACER_ID");
}

/// TSXW_ACTIVE = 0xb692
#[test]
fn ts_xw_matches_kernel() {
    assert_eq!(product_ids::TS_XW, 0xB692, "TSXW_ACTIVE");
}

/// FFB_WHEEL_GENERIC (pre-init) = 0xb65d
/// Source: hid-tminit / hid-thrustmaster.c
#[test]
fn ffb_wheel_generic_matches_kernel() {
    assert_eq!(product_ids::FFB_WHEEL_GENERIC, 0xB65D);
}

// ── Effect constants (from hid-tmff2 source) ───────────────────────────────

/// Max simultaneous effects = 16.
#[test]
fn max_effects_is_16() {
    assert_eq!(effects::MAX_EFFECTS, 16);
}

/// Normal-mode buffer = 63 bytes (USB mode, Report ID 0x60).
#[test]
fn norm_buffer_length_is_63() {
    assert_eq!(effects::NORM_BUFFER_LENGTH, 63);
}

/// PS4-mode buffer = 31 bytes (Report ID 0x05).
#[test]
fn ps4_buffer_length_is_31() {
    assert_eq!(effects::PS4_BUFFER_LENGTH, 31);
}

/// Timing start marker = 0x4f.
#[test]
fn timing_start_marker_is_0x4f() {
    assert_eq!(effects::TIMING_START_MARKER, 0x4F);
}

/// Infinite duration = 0xFFFF.
#[test]
fn infinite_duration_is_0xffff() {
    assert_eq!(effects::INFINITE_DURATION, 0xFFFF);
}

/// Spring max saturation = 0x6aa6 (hid-tmt300rs.c: t300rs_condition_max_saturation).
#[test]
fn spring_max_saturation_matches_kernel() {
    assert_eq!(effects::SPRING_MAX_SATURATION, 0x6AA6);
}

/// Condition hardcoded values from the kernel driver.
#[test]
fn condition_hardcoded_matches_kernel() {
    assert_eq!(
        effects::CONDITION_HARDCODED,
        [0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff],
        "condition_values[] from hid-tmt300rs.c"
    );
}

// ── Waveform IDs (FF_* - 0x57) ────────────────────────────────────────────

/// Kernel: periodic.waveform - 0x57 for each FF_* type.
#[test]
fn waveform_ids_match_kernel_computation() {
    assert_eq!(effects::Waveform::Square as u8, 0x01, "FF_SQUARE - 0x57");
    assert_eq!(
        effects::Waveform::Triangle as u8,
        0x02,
        "FF_TRIANGLE - 0x57"
    );
    assert_eq!(effects::Waveform::Sine as u8, 0x03, "FF_SINE - 0x57");
    assert_eq!(effects::Waveform::SawUp as u8, 0x04, "FF_SAW_UP - 0x57");
    assert_eq!(effects::Waveform::SawDown as u8, 0x05, "FF_SAW_DOWN - 0x57");
}

/// Condition sub-types.
#[test]
fn condition_types_match_kernel() {
    assert_eq!(effects::ConditionType::Spring as u8, 0x00);
    assert_eq!(effects::ConditionType::Other as u8, 0x01);
}

// ── T300RS module constants ────────────────────────────────────────────────

/// T300RS report sizes match USB descriptor.
#[test]
fn t300rs_report_sizes() {
    use racing_wheel_hid_thrustmaster_protocol::t300rs;
    assert_eq!(t300rs::T300RS_REPORT_SIZE, 64);
    assert_eq!(t300rs::T300RS_REPORT_SIZE_PS4, 32);
}

/// T300RS header byte = 0x60.
#[test]
fn t300rs_header_byte() {
    use racing_wheel_hid_thrustmaster_protocol::t300rs;
    assert_eq!(t300rs::HEADER_BYTE, 0x60);
}

/// T300RS effect opcodes match hid-tmff2 FFBEFFECTS.md.
#[test]
fn t300rs_effect_opcodes_match_kernel() {
    use racing_wheel_hid_thrustmaster_protocol::t300rs::effect_op;
    assert_eq!(effect_op::NEW_CONSTANT, 0x6A);
    assert_eq!(effect_op::NEW_RAMP, 0x6B);
    assert_eq!(effect_op::MODIFY_CONSTANT, 0x0A);
    assert_eq!(effect_op::MODIFY_ENVELOPE, 0x29);
    assert_eq!(effect_op::MODIFY_DURATION, 0x49);
    assert_eq!(effect_op::PLAY_CONTROL, 0x89);
}

/// T300RS settings sub-commands.
#[test]
fn t300rs_settings_subcmds() {
    use racing_wheel_hid_thrustmaster_protocol::t300rs::settings;
    assert_eq!(settings::ROTATION_ANGLE, 0x11);
    assert_eq!(settings::AUTOCENTER_FORCE, 0x03);
    assert_eq!(settings::AUTOCENTER_ENABLE, 0x04);
}

/// T300RS command categories.
#[test]
fn t300rs_cmd_categories() {
    use racing_wheel_hid_thrustmaster_protocol::t300rs::cmd;
    assert_eq!(cmd::EFFECT, 0x00);
    assert_eq!(cmd::OPEN_CLOSE, 0x01);
    assert_eq!(cmd::GAIN, 0x02);
    assert_eq!(cmd::SETTINGS, 0x08);
}

/// Range scale factor = 0x3c (degrees * 0x3c).
#[test]
fn range_scale_matches_kernel() {
    assert_eq!(effects::RANGE_SCALE, 0x3C);
}
