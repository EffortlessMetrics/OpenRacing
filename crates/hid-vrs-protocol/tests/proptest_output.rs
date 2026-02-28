//! Property-based tests for VRS DirectForce Pro FFB output report encoding.
//!
//! Uses proptest with 500 cases to verify correctness properties independent of
//! specific numeric values: sign preservation, round-trip fidelity, report
//! structure (size + report ID), monotonicity, and coefficient round-trips
//! for spring, damper, and friction encoders.

use proptest::prelude::*;
use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VrsConstantForceEncoder, VrsDamperEncoder, VrsFrictionEncoder, VrsSpringEncoder,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded magnitude.
    #[test]
    fn prop_sign_preserved(
        torque in -200.0f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
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
    /// with at most one LSB of the ±10000 scale.
    #[test]
    fn prop_round_trip(
        torque_frac in -1.0f32..=1.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let torque = torque_frac * max_torque;
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        let decoded = raw as f32 / 10_000.0 * max_torque;
        let error = (torque - decoded).abs();
        prop_assert!(error < max_torque / 10_000.0 + 0.001,
            "torque {torque} round-trips as {decoded} (error {error}, max_torque {max_torque})");
    }

    /// The output buffer must be exactly CONSTANT_FORCE_REPORT_LEN bytes; byte 0
    /// must be 0x11 (CONSTANT_FORCE report ID) and byte 1 must be effect block 1.
    #[test]
    fn prop_report_structure(
        torque in -200.0f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = enc.encode(torque, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN,
            "returned length must equal CONSTANT_FORCE_REPORT_LEN");
        prop_assert_eq!(out[0], 0x11u8, "byte 0 must be 0x11 (CONSTANT_FORCE report ID)");
        prop_assert_eq!(out[1], 1u8,    "byte 1 must be effect block index 1");
    }

    /// Larger positive torque must produce a larger (or equal) encoded magnitude.
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..50.0f32,
        t2 in 0.0f32..50.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
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

    /// Spring steering position written at bytes 4–5 must be recoverable unchanged.
    #[test]
    fn prop_spring_steering_round_trip(
        coefficient in 0u16..=10_000u16,
        steering in i16::MIN..=i16::MAX,
        center in i16::MIN..=i16::MAX,
        deadzone in 0u16..=10_000u16,
    ) {
        let enc = VrsSpringEncoder::new(20.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        let len = enc.encode(coefficient, steering, center, deadzone, &mut out);
        prop_assert_eq!(len, SPRING_REPORT_LEN,
            "returned length must equal SPRING_REPORT_LEN");
        prop_assert_eq!(out[0], 0x19u8, "byte 0 must be 0x19 (SPRING_EFFECT report ID)");
        let recovered_steering = i16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered_steering, steering, "steering position must round-trip");
        let recovered_center = i16::from_le_bytes([out[6], out[7]]);
        prop_assert_eq!(recovered_center, center, "center offset must round-trip");
    }

    /// Damper velocity written at bytes 4–5 must be recoverable unchanged, and
    /// reserved bytes 6–7 must remain zero.
    #[test]
    fn prop_damper_velocity_round_trip(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = VrsDamperEncoder::new(20.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, DAMPER_REPORT_LEN,
            "returned length must equal DAMPER_REPORT_LEN");
        prop_assert_eq!(out[0], 0x1Au8, "byte 0 must be 0x1A (DAMPER_EFFECT report ID)");
        let recovered_velocity = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered_velocity, velocity, "damper velocity must round-trip");
        prop_assert_eq!(out[6], 0u8, "reserved byte 6 must be zero");
        prop_assert_eq!(out[7], 0u8, "reserved byte 7 must be zero");
    }

    /// Friction velocity written at bytes 4–5 must be recoverable unchanged, and
    /// reserved bytes 6–9 must remain zero.
    #[test]
    fn prop_friction_velocity_round_trip(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = VrsFrictionEncoder::new(20.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, FRICTION_REPORT_LEN,
            "returned length must equal FRICTION_REPORT_LEN");
        prop_assert_eq!(out[0], 0x1Bu8, "byte 0 must be 0x1B (FRICTION_EFFECT report ID)");
        let recovered_velocity = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered_velocity, velocity, "friction velocity must round-trip");
        prop_assert_eq!(&out[6..], &[0u8; 4], "reserved bytes 6-9 must be zero");
    }
}
