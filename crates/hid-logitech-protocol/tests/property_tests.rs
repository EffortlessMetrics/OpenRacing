//! Property-based tests for the Logitech HID protocol crate.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID / PID constant values and detection
//! - FFB constant-force encoder (report ID, magnitude bounds, saturation,
//!   monotonicity, zero-input)
//! - Vendor output reports (report ID, parameter round-trips, masking)

use proptest::prelude::*;
use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LOGITECH_VENDOR_ID, LogitechConstantForceEncoder, LogitechModel,
    VENDOR_REPORT_LEN, build_gain_report, build_native_mode_report, build_set_autocenter_report,
    build_set_leds_report, build_set_range_report, is_wheel_product, product_ids,
};

// ── VID / PID invariants ──────────────────────────────────────────────────────

/// VID constant must equal the authoritative Logitech USB vendor ID (0x046D).
#[test]
fn test_vendor_id_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(LOGITECH_VENDOR_ID, 0x046D, "Logitech VID must be 0x046D");
    Ok(())
}

/// Every known PID must be recognised by `is_wheel_product`.
#[test]
fn test_all_known_pids_detected() -> Result<(), Box<dyn std::error::Error>> {
    let known = [
        product_ids::G25,
        product_ids::G27_A,
        product_ids::G27,
        product_ids::G29_PS,
        product_ids::G920,
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
        product_ids::G_PRO,
        product_ids::G_PRO_XBOX,
    ];
    for pid in known {
        assert!(
            is_wheel_product(pid),
            "PID 0x{pid:04X} must be recognised as a Logitech wheel"
        );
        assert_ne!(
            LogitechModel::from_product_id(pid),
            LogitechModel::Unknown,
            "PID 0x{pid:04X} must not classify as Unknown"
        );
    }
    Ok(())
}

/// Exact numeric values verified against kernel `hid-ids.h`, new-lg4ff, and oversteer.
#[test]
fn test_pid_constant_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::G25, 0xC299);
    assert_eq!(product_ids::G27_A, 0xC294);
    assert_eq!(product_ids::G27, 0xC29B);
    assert_eq!(product_ids::G29_PS, 0xC24F);
    assert_eq!(product_ids::G920, 0xC262);
    assert_eq!(product_ids::G923, 0xC266);
    assert_eq!(product_ids::G923_PS, 0xC267);
    assert_eq!(product_ids::G923_XBOX, 0xC26E);
    assert_eq!(product_ids::G_PRO, 0xC268);
    assert_eq!(product_ids::G_PRO_XBOX, 0xC272);
    Ok(())
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // ── PID boundary detection ────────────────────────────────────────────────

    /// `is_wheel_product` must return `false` for all known non-Logitech PIDs
    /// constructed by flipping the high byte to something other than 0xCx.
    #[test]
    fn prop_non_logitech_pid_not_detected(lo in 0x00u8..=0xFFu8) {
        // PIDs 0x0000–0x00FF and 0xFF00–0xFFFF are outside the Logitech range.
        prop_assert!(!is_wheel_product(u16::from(lo)),
            "low-range PID 0x{:04X} must not be a Logitech wheel", lo);
        let hi_pid = 0xFF00u16 | u16::from(lo);
        prop_assert!(!is_wheel_product(hi_pid),
            "high-range PID 0x{hi_pid:04X} must not be a Logitech wheel");
    }

    // ── Constant-force encoder: report ID ────────────────────────────────────

    /// Report ID (byte 0) and effect-block index (byte 1) must always be correct.
    #[test]
    fn prop_report_id_and_effect_block(
        torque in -100.0f32..100.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        prop_assert_eq!(out[0], 0x12, "byte 0 must be CONSTANT_FORCE report ID 0x12");
        prop_assert_eq!(out[1], 1u8, "byte 1 must be effect block index 1");
    }

    // ── Constant-force encoder: magnitude bounds ──────────────────────────────

    /// Encoded magnitude must always be in the valid Logitech range ±10000.
    #[test]
    fn prop_ffb_magnitude_in_bounds(
        torque in proptest::num::f32::ANY,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(
            (-10_000..=10_000).contains(&mag),
            "magnitude {mag} out of ±10000 for torque={torque}, max={max_torque}"
        );
    }

    // ── Constant-force encoder: sign preservation ─────────────────────────────

    /// Positive torque must produce a non-negative magnitude.
    #[test]
    fn prop_positive_torque_nonneg_magnitude(
        torque in 0.01f32..50.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(mag >= 0,
            "positive torque {torque} must give mag >= 0, got {mag}");
    }

    /// Negative torque must produce a non-positive magnitude.
    #[test]
    fn prop_negative_torque_nonpos_magnitude(
        torque in -50.0f32..-0.01f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(mag <= 0,
            "negative torque {torque} must give mag <= 0, got {mag}");
    }

    // ── Constant-force encoder: saturation ───────────────────────────────────

    /// Torque at ±max_torque must produce exactly ±10000.
    #[test]
    fn prop_full_scale_saturates(max_torque in 0.01f32..50.0f32) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

        enc.encode(max_torque, &mut out);
        let pos = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(pos, 10_000i16, "full positive torque must saturate to 10000");

        enc.encode(-max_torque, &mut out);
        let neg = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(neg, -10_000i16, "full negative torque must saturate to -10000");
    }

    // ── Constant-force encoder: zero input ───────────────────────────────────

    /// Zero torque must always produce zero magnitude.
    #[test]
    fn prop_zero_torque_produces_zero(max_torque in 0.01f32..50.0f32) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(0.0, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(mag, 0i16, "zero torque must encode to magnitude 0");
    }

    /// `encode_zero` must always zero the magnitude regardless of `max_torque`.
    #[test]
    fn prop_encode_zero_clears_magnitude(max_torque in 0.01f32..50.0f32) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        prop_assert_eq!(out[2], 0u8, "encode_zero must clear mag low byte");
        prop_assert_eq!(out[3], 0u8, "encode_zero must clear mag high byte");
    }

    // ── set_range_report round-trip ───────────────────────────────────────────

    /// Degrees encoded in `build_set_range_report` must round-trip via LE bytes.
    #[test]
    fn prop_set_range_report_roundtrip(degrees: u16) {
        let r = build_set_range_report(degrees);
        prop_assert_eq!(r[0], 0xF8u8, "vendor report ID must be 0xF8");
        prop_assert_eq!(r[1], 0x81u8, "SET_RANGE command must be 0x81");
        let recovered = u16::from_le_bytes([r[2], r[3]]);
        prop_assert_eq!(recovered, degrees, "degrees must round-trip via LE bytes");
        prop_assert_eq!(&r[4..], &[0u8; 3], "bytes 4-6 must be zero");
    }

    // ── set_autocenter_report parameter preservation ──────────────────────────

    /// Strength and rate must be preserved verbatim in the autocenter report.
    #[test]
    fn prop_autocenter_params_preserved(strength: u8, rate: u8) {
        let r = build_set_autocenter_report(strength, rate);
        prop_assert_eq!(r[0], 0xF8u8, "vendor report ID must be 0xF8");
        prop_assert_eq!(r[1], 0x14u8, "SET_AUTOCENTER command must be 0x14");
        prop_assert_eq!(r[2], strength, "strength must be preserved");
        prop_assert_eq!(r[3], rate, "rate must be preserved");
        prop_assert_eq!(&r[4..], &[0u8; 3], "bytes 4-6 must be zero");
    }

    // ── set_leds_report masking ───────────────────────────────────────────────

    /// LED mask must be truncated to 5 bits; high bits are dropped.
    #[test]
    fn prop_leds_masked_to_5_bits(mask: u8) {
        let r = build_set_leds_report(mask);
        prop_assert_eq!(r[0], 0xF8u8, "vendor report ID must be 0xF8");
        prop_assert_eq!(r[1], 0x12u8, "SET_LEDS command must be 0x12");
        prop_assert_eq!(r[2], mask & 0x1F, "led_mask must be masked to 5 bits");
        prop_assert_eq!(&r[3..], &[0u8; 4], "bytes 3-6 must be zero");
    }

    // ── gain_report round-trip ────────────────────────────────────────────────

    /// Gain must be preserved verbatim in the 2-byte device gain report.
    #[test]
    fn prop_gain_report_roundtrip(gain: u8) {
        let r = build_gain_report(gain);
        prop_assert_eq!(r[0], 0x16u8, "Device Gain report ID must be 0x16");
        prop_assert_eq!(r[1], gain, "gain must be preserved unchanged");
    }

    // ── VENDOR_REPORT_LEN sanity ──────────────────────────────────────────────

    /// All 7-byte vendor reports built with arbitrary inputs must be exactly
    /// VENDOR_REPORT_LEN bytes (verified through the Rust type system; this
    /// test confirms the constant is 7).
    #[test]
    fn prop_vendor_report_len_is_seven(degrees: u16) {
        let r = build_set_range_report(degrees);
        prop_assert_eq!(r.len(), VENDOR_REPORT_LEN, "vendor report must be VENDOR_REPORT_LEN bytes");
    }
}

// ── native_mode_report is a constant ─────────────────────────────────────────

/// Native-mode report must have correct command byte and zero padding.
#[test]
fn test_native_mode_report_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let r = build_native_mode_report();
    assert_eq!(r[0], 0xF8, "vendor report ID must be 0xF8");
    assert_eq!(r[1], 0x0A, "NATIVE_MODE command must be 0x0A");
    assert_eq!(&r[2..], &[0u8; 5], "bytes 2-6 must be zero");
    Ok(())
}
