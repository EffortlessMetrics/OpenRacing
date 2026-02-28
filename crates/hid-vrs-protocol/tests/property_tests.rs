//! Property-based tests for the VRS DirectForce Pro HID protocol crate.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID / PID constant values and device detection
//! - FFB constant-force encoder (report ID, magnitude bounds, saturation,
//!   sign preservation, zero-input)
//! - Condition effect encoders (spring, damper, friction) report IDs
//! - Vendor output reports (rotation range, device gain, FFB enable)

use proptest::prelude::*;
use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VRS_VENDOR_ID, VrsConstantForceEncoder, VrsDamperEncoder, VrsFrictionEncoder, VrsSpringEncoder,
    build_device_gain, build_ffb_enable, build_rotation_range, identify_device,
    is_wheelbase_product, product_ids,
};

// ── VID / PID invariants ──────────────────────────────────────────────────────

/// VID constant must equal the authoritative VRS USB vendor ID (0x0483).
#[test]
fn test_vendor_id_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VRS_VENDOR_ID, 0x0483, "VRS VID must be 0x0483");
    Ok(())
}

/// Every known VRS wheelbase PID must be recognised by `is_wheelbase_product`.
#[test]
fn test_wheelbase_pids_detected() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbase_pids = [
        product_ids::DIRECTFORCE_PRO,
        product_ids::DIRECTFORCE_PRO_V2,
    ];
    for pid in wheelbase_pids {
        assert!(
            is_wheelbase_product(pid),
            "PID 0x{pid:04X} must be recognised as a VRS wheelbase"
        );
        let identity = identify_device(pid);
        assert!(identity.supports_ffb, "PID 0x{pid:04X} must support FFB");
        assert!(
            identity.max_torque_nm.is_some(),
            "PID 0x{pid:04X} must have a max_torque_nm value"
        );
    }
    Ok(())
}

/// Non-wheelbase VRS PIDs must not be recognised as wheelbases.
#[test]
fn test_non_wheelbase_pids_not_detected() -> Result<(), Box<dyn std::error::Error>> {
    let non_wheelbase = [
        product_ids::PEDALS_V1,
        product_ids::PEDALS_V2,
        product_ids::HANDBRAKE,
        product_ids::SHIFTER,
    ];
    for pid in non_wheelbase {
        assert!(
            !is_wheelbase_product(pid),
            "PID 0x{pid:04X} must not be recognised as a VRS wheelbase"
        );
    }
    Ok(())
}

/// Exact numeric values verified against VRS USB descriptors.
#[test]
fn test_pid_constant_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::DIRECTFORCE_PRO, 0xA355);
    assert_eq!(product_ids::DIRECTFORCE_PRO_V2, 0xA356);
    assert_eq!(product_ids::PEDALS_V1, 0xA357);
    assert_eq!(product_ids::PEDALS_V2, 0xA358);
    assert_eq!(product_ids::HANDBRAKE, 0xA359);
    assert_eq!(product_ids::SHIFTER, 0xA35A);
    Ok(())
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // ── PID boundary detection ────────────────────────────────────────────────

    /// `is_wheelbase_product` must return `false` for PIDs in ranges far from
    /// the known VRS range (0xA355–0xA35A).
    #[test]
    fn prop_out_of_range_pid_not_wheelbase(lo in 0x00u8..=0xFFu8) {
        let pid_low = u16::from(lo);
        prop_assert!(!is_wheelbase_product(pid_low),
            "low-range PID 0x{:04X} must not be a VRS wheelbase", pid_low);
        let pid_high = 0xFF00u16 | u16::from(lo);
        prop_assert!(!is_wheelbase_product(pid_high),
            "high-range PID 0x{pid_high:04X} must not be a VRS wheelbase");
    }

    // ── Constant-force encoder: report ID ────────────────────────────────────

    /// Report ID (byte 0) and effect-block index (byte 1) must always be correct.
    #[test]
    fn prop_constant_force_report_id_and_effect_block(
        torque in -100.0f32..100.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        prop_assert_eq!(out[0], 0x11u8, "byte 0 must be CONSTANT_FORCE report ID 0x11");
        prop_assert_eq!(out[1], 1u8,    "byte 1 must be effect block index 1");
    }

    // ── Constant-force encoder: magnitude bounds ──────────────────────────────

    /// Encoded magnitude (bytes 3–4, LE) must always be in ±10000.
    #[test]
    fn prop_constant_force_magnitude_in_bounds(
        torque in proptest::num::f32::ANY,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
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
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        prop_assert!(mag >= 0,
            "positive torque {torque} must give mag >= 0, got {mag}");
    }

    /// Negative torque must produce a non-positive magnitude.
    #[test]
    fn prop_negative_torque_nonpos_magnitude(
        torque in -50.0f32..-0.01f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        prop_assert!(mag <= 0,
            "negative torque {torque} must give mag <= 0, got {mag}");
    }

    // ── Constant-force encoder: saturation ───────────────────────────────────

    /// Torque at ±max_torque must produce exactly ±10000.
    #[test]
    fn prop_full_scale_saturates(max_torque in 0.01f32..50.0f32) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

        enc.encode(max_torque, &mut out);
        let pos = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(pos, 10_000i16, "full positive torque must saturate to 10000");

        enc.encode(-max_torque, &mut out);
        let neg = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(neg, -10_000i16, "full negative torque must saturate to -10000");
    }

    // ── Constant-force encoder: zero input ───────────────────────────────────

    /// Zero torque must always produce zero magnitude.
    #[test]
    fn prop_zero_torque_produces_zero(max_torque in 0.01f32..50.0f32) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(0.0, &mut out);
        let mag = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(mag, 0i16, "zero torque must encode to magnitude 0");
    }

    /// `encode_zero` must always zero the magnitude regardless of `max_torque`.
    #[test]
    fn prop_encode_zero_clears_magnitude(max_torque in 0.01f32..50.0f32) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        prop_assert_eq!(out[3], 0u8, "encode_zero must clear mag low byte (out[3])");
        prop_assert_eq!(out[4], 0u8, "encode_zero must clear mag high byte (out[4])");
    }

    // ── Spring encoder: report ID and coefficient round-trip ─────────────────

    /// Spring report ID (byte 0) must be 0x19 and effect block index (byte 1) = 1.
    #[test]
    fn prop_spring_report_id_preserved(
        coefficient in 0u16..=10_000u16,
        steering in i16::MIN..=i16::MAX,
        center in i16::MIN..=i16::MAX,
        deadzone in 0u16..=10_000u16,
    ) {
        let enc = VrsSpringEncoder::new(20.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        enc.encode(coefficient, steering, center, deadzone, &mut out);
        prop_assert_eq!(out[0], 0x19u8, "byte 0 must be SPRING_EFFECT report ID 0x19");
        prop_assert_eq!(out[1], 1u8,    "byte 1 must be effect block index 1");
        let recovered = u16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(recovered, coefficient, "spring coefficient must round-trip");
    }

    // ── Damper encoder: report ID ─────────────────────────────────────────────

    /// Damper report ID (byte 0) must be 0x1A.
    #[test]
    fn prop_damper_report_id_preserved(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = VrsDamperEncoder::new(20.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(out[0], 0x1Au8, "byte 0 must be DAMPER_EFFECT report ID 0x1A");
        prop_assert_eq!(out[1], 1u8,    "byte 1 must be effect block index 1");
        let recovered = u16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(recovered, coefficient, "damper coefficient must round-trip");
    }

    // ── Friction encoder: report ID ───────────────────────────────────────────

    /// Friction report ID (byte 0) must be 0x1B.
    #[test]
    fn prop_friction_report_id_preserved(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = VrsFrictionEncoder::new(20.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(out[0], 0x1Bu8, "byte 0 must be FRICTION_EFFECT report ID 0x1B");
        prop_assert_eq!(out[1], 1u8,    "byte 1 must be effect block index 1");
        let recovered = u16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(recovered, coefficient, "friction coefficient must round-trip");
    }

    // ── build_rotation_range round-trip ──────────────────────────────────────

    /// Degrees encoded in `build_rotation_range` must round-trip via LE bytes.
    #[test]
    fn prop_rotation_range_roundtrip(degrees: u16) {
        let r = build_rotation_range(degrees);
        prop_assert_eq!(r[0], 0x0Cu8, "SET_REPORT ID must be 0x0C");
        let recovered = u16::from_le_bytes([r[2], r[3]]);
        prop_assert_eq!(recovered, degrees, "degrees must round-trip via LE bytes");
        prop_assert_eq!(&r[4..], &[0u8; 4], "bytes 4-7 must be zero");
    }

    // ── build_device_gain round-trip ─────────────────────────────────────────

    /// Gain must be preserved verbatim in `build_device_gain`.
    #[test]
    fn prop_device_gain_roundtrip(gain: u8) {
        let r = build_device_gain(gain);
        prop_assert_eq!(r[0], 0x0Cu8, "SET_REPORT ID must be 0x0C");
        prop_assert_eq!(r[2], gain,   "gain must be preserved at byte 2");
    }

    // ── build_ffb_enable ─────────────────────────────────────────────────────

    /// FFB enable/disable report must encode the boolean correctly.
    #[test]
    fn prop_ffb_enable_byte_matches(enable: bool) {
        let r = build_ffb_enable(enable);
        prop_assert_eq!(r[0], 0x0Bu8, "DEVICE_CONTROL report ID must be 0x0B");
        let expected = if enable { 1u8 } else { 0u8 };
        prop_assert_eq!(r[1], expected, "enable byte must match boolean");
        prop_assert_eq!(&r[2..], &[0u8; 6], "bytes 2-7 must be zero");
    }
}
