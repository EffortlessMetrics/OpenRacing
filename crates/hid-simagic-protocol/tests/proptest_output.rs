//! Property-based tests for Simagic FFB output report encoding.
//!
//! Uses proptest with 500 cases to verify correctness properties independent of
//! specific numeric values: sign preservation, round-trip fidelity, report
//! structure (size + report ID), monotonicity, and coefficient round-trips
//! for spring, damper, and friction encoders.

use proptest::prelude::*;
use racing_wheel_hid_simagic_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicFrictionEncoder,
    SimagicSpringEncoder,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded magnitude.
    #[test]
    fn prop_sign_preserved(
        torque in -200.0f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        if torque > 0.01 {
            prop_assert!(raw >= 0,
                "positive torque {torque} (max {max_torque}) encoded as negative {raw}");
        } else if torque < -0.01 {
            prop_assert!(raw <= 0,
                "negative torque {torque} (max {max_torque}) encoded as positive {raw}");
        }
    }

    /// Torque within ±max_torque must round-trip through the magnitude encoding
    /// with at most 1/10000 relative error (one LSB of the ±10000 scale).
    #[test]
    fn prop_round_trip(
        torque_frac in -1.0f32..=1.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let torque = torque_frac * max_torque;
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        let decoded = raw as f32 / 10_000.0 * max_torque;
        let error = (torque - decoded).abs();
        // One LSB of the 10000-count scale is max_torque/10000.
        prop_assert!(error < max_torque / 10_000.0 + 0.001,
            "torque {torque} round-trips as {decoded} (error {error}, max_torque {max_torque})");
    }

    /// The output buffer must be exactly CONSTANT_FORCE_REPORT_LEN bytes and
    /// byte 0 must always be the CONSTANT_FORCE report ID (0x11).
    #[test]
    fn prop_report_structure(
        torque in -200.0f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = enc.encode(torque, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN,
            "returned length must equal CONSTANT_FORCE_REPORT_LEN");
        prop_assert_eq!(out[0], 0x11u8, "byte 0 must be 0x11 (CONSTANT_FORCE report ID)");
        prop_assert_eq!(out[1], 1u8,    "byte 1 must be effect block index 1");
    }

    /// Larger positive torque must produce a larger (or equal) magnitude.
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..50.0f32,
        t2 in 0.0f32..50.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out1 = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut out2 = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(t1, &mut out1);
        enc.encode(t2, &mut out2);
        let r1 = i16::from_le_bytes([out1[3], out1[4]]);
        let r2 = i16::from_le_bytes([out2[3], out2[4]]);
        if t1 < t2 - 0.001 {
            prop_assert!(r1 <= r2,
                "t1={t1} -> {r1} must be <= t2={t2} -> {r2} (max {max_torque})");
        }
    }

    /// Spring coefficient written at bytes 2–3 must be recoverable unchanged.
    #[test]
    fn prop_spring_coefficient_round_trip(
        strength in 0u16..=1000u16,
        steering in i16::MIN..=i16::MAX,
        center in i16::MIN..=i16::MAX,
        deadzone in 0u16..=1000u16,
    ) {
        let enc = SimagicSpringEncoder::new(15.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        let len = enc.encode(strength, steering, center, deadzone, &mut out);
        prop_assert_eq!(len, SPRING_REPORT_LEN,
            "returned length must equal SPRING_REPORT_LEN");
        prop_assert_eq!(out[0], 0x12u8, "byte 0 must be 0x12 (SPRING_EFFECT report ID)");
        let recovered_strength = u16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(recovered_strength, strength, "spring strength must round-trip");
        let recovered_steering = i16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered_steering, steering, "steering position must round-trip");
    }

    /// Damper coefficient and velocity written at bytes 2–5 must be recoverable.
    #[test]
    fn prop_damper_coefficient_round_trip(
        strength in 0u16..=1000u16,
        velocity in 0u16..=10000u16,
    ) {
        let enc = SimagicDamperEncoder::new(15.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        let len = enc.encode(strength, velocity, &mut out);
        prop_assert_eq!(len, DAMPER_REPORT_LEN,
            "returned length must equal DAMPER_REPORT_LEN");
        prop_assert_eq!(out[0], 0x13u8, "byte 0 must be 0x13 (DAMPER_EFFECT report ID)");
        let recovered_strength = u16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(recovered_strength, strength, "damper strength must round-trip");
        let recovered_velocity = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered_velocity, velocity, "damper velocity must round-trip");
        prop_assert_eq!(out[6], 0u8, "reserved byte 6 must be zero");
        prop_assert_eq!(out[7], 0u8, "reserved byte 7 must be zero");
    }

    /// Friction coefficient and velocity written at bytes 2–5 must be recoverable.
    #[test]
    fn prop_friction_coefficient_round_trip(
        coefficient in 0u16..=1000u16,
        velocity in 0u16..=10000u16,
    ) {
        let enc = SimagicFrictionEncoder::new(15.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, FRICTION_REPORT_LEN,
            "returned length must equal FRICTION_REPORT_LEN");
        prop_assert_eq!(out[0], 0x14u8, "byte 0 must be 0x14 (FRICTION_EFFECT report ID)");
        let recovered_coeff = u16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(recovered_coeff, coefficient, "friction coefficient must round-trip");
        let recovered_velocity = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered_velocity, velocity, "friction velocity must round-trip");
        prop_assert_eq!(&out[6..], &[0u8; 4], "reserved bytes 6-9 must be zero");
    }
}
