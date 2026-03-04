//! Roundtrip property-based tests for the Simagic HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Constant force encoder magnitude roundtrip
//! - Spring / damper / friction effect parameter roundtrips
//! - Rotation range and gain report roundtrips
//! - Input report field roundtrips
//! - Pedal normalization roundtrip
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_simagic_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicFrictionEncoder, SimagicModel,
    SimagicPedalAxesRaw, SimagicSpringEncoder, build_device_gain, build_led_report,
    build_rotation_range, identify_device, parse_input_report,
};

// ── Constant force encoder roundtrip ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Torque within ±max round-trips through the magnitude encoding
    /// with at most 1 LSB of error on the ±10000 scale.
    #[test]
    fn prop_constant_force_roundtrip(
        torque_frac in -1.0_f32..=1.0_f32,
        max_torque in 0.1_f32..=32.0_f32,
    ) {
        let torque = torque_frac * max_torque;
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        let decoded = raw as f32 / 10_000.0 * max_torque;
        let tolerance = max_torque / 10_000.0 + 0.001;
        let error = (torque - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {torque} roundtrips as {decoded} (err {error} > tol {tolerance})"
        );
    }

    /// encode_zero always produces zero raw magnitude.
    #[test]
    fn prop_constant_force_zero(max_torque in 0.1_f32..=32.0_f32) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(raw, 0, "encode_zero must produce zero magnitude");
    }

    /// Sign of the input torque must be preserved.
    #[test]
    fn prop_constant_force_sign(
        torque in -32.0_f32..=32.0_f32,
        max_torque in 0.1_f32..=32.0_f32,
    ) {
        let enc = SimagicConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        if torque > 0.01 {
            prop_assert!(raw >= 0, "positive torque {torque} must yield non-negative raw {raw}");
        } else if torque < -0.01 {
            prop_assert!(raw <= 0, "negative torque {torque} must yield non-positive raw {raw}");
        }
    }
}

// ── Spring / damper / friction encoder roundtrips ───────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Spring steering position and center offset must round-trip.
    #[test]
    fn prop_spring_roundtrip(
        coefficient in 0u16..=10_000u16,
        steering: i16,
        center: i16,
        deadzone in 0u16..=10_000u16,
    ) {
        let enc = SimagicSpringEncoder::new(20.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        let len = enc.encode(coefficient, steering, center, deadzone, &mut out);
        prop_assert_eq!(len, SPRING_REPORT_LEN);
        let recovered_steering = i16::from_le_bytes([out[4], out[5]]);
        let recovered_center = i16::from_le_bytes([out[6], out[7]]);
        prop_assert_eq!(recovered_steering, steering, "steering must round-trip");
        prop_assert_eq!(recovered_center, center, "center must round-trip");
    }

    /// Damper velocity must round-trip.
    #[test]
    fn prop_damper_roundtrip(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = SimagicDamperEncoder::new(20.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, DAMPER_REPORT_LEN);
        let recovered = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered, velocity, "velocity must round-trip");
    }

    /// Friction velocity must round-trip.
    #[test]
    fn prop_friction_roundtrip(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = SimagicFrictionEncoder::new(20.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, FRICTION_REPORT_LEN);
        let recovered = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered, velocity, "velocity must round-trip");
    }
}

// ── Rotation range and gain report roundtrips ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Rotation range degrees u16 LE at bytes [2..4] must round-trip.
    #[test]
    fn prop_rotation_range_roundtrip(degrees: u16) {
        let report = build_rotation_range(degrees);
        prop_assert_eq!(report.len(), 8);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(decoded, degrees, "degrees must round-trip");
        prop_assert_eq!(report[0], 0x20, "byte 0 must be 0x20");
    }

    /// Gain report byte [1] must equal the input gain exactly.
    #[test]
    fn prop_gain_roundtrip(gain: u8) {
        let report = build_device_gain(gain);
        prop_assert_eq!(report.len(), 8);
        prop_assert_eq!(report[0], 0x21, "byte 0 must be 0x21");
        prop_assert_eq!(report[1], gain, "gain must round-trip");
    }

    /// LED report byte [1] must equal the input pattern.
    #[test]
    fn prop_led_roundtrip(pattern: u8) {
        let report = build_led_report(pattern);
        prop_assert_eq!(report.len(), 8);
        prop_assert_eq!(report[1], pattern, "LED pattern must round-trip");
    }

    /// Device identification is deterministic.
    #[test]
    fn prop_identify_deterministic(pid: u16) {
        let id1 = identify_device(pid);
        let id2 = identify_device(pid);
        let names_match = id1.name == id2.name;
        prop_assert!(names_match, "name must be deterministic");
        prop_assert_eq!(id1.supports_ffb, id2.supports_ffb);
    }
}

// ── Input report and pedal normalization roundtrip ──────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Pedal raw values normalize into [0.0, 1.0] without NaN or infinity.
    #[test]
    fn prop_pedal_normalize_bounds(
        throttle: u16,
        brake: u16,
        clutch: u16,
        handbrake: u16,
    ) {
        let raw = SimagicPedalAxesRaw {
            throttle,
            brake,
            clutch,
            handbrake,
        };
        let norm = raw.normalize();
        prop_assert!(norm.throttle >= 0.0 && norm.throttle <= 1.0,
            "throttle {}", norm.throttle);
        prop_assert!(norm.brake >= 0.0 && norm.brake <= 1.0,
            "brake {}", norm.brake);
        prop_assert!(norm.clutch >= 0.0 && norm.clutch <= 1.0,
            "clutch {}", norm.clutch);
        prop_assert!(norm.handbrake >= 0.0 && norm.handbrake <= 1.0,
            "handbrake {}", norm.handbrake);
    }

    /// Short input reports must return None, not panic.
    #[test]
    fn prop_input_short_no_panic(len in 0usize..=16usize) {
        let data = vec![0u8; len];
        let state = parse_input_report(&data);
        if len < 17 {
            prop_assert!(state.is_none(), "report of len {len} must return None");
        }
    }

    /// Model from_pid is deterministic.
    #[test]
    fn prop_model_deterministic(pid: u16) {
        let m1 = SimagicModel::from_pid(pid);
        let m2 = SimagicModel::from_pid(pid);
        prop_assert_eq!(m1, m2);
    }
}
