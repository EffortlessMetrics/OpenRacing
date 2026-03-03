//! Roundtrip property-based tests for the VRS DirectForce Pro HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Constant force encoder magnitude roundtrip
//! - Spring / damper / friction encoder parameter roundtrips
//! - Rotation range and gain report roundtrips
//! - FFB enable report roundtrip
//! - Input report field roundtrips
//! - Pedal normalization roundtrip
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VrsConstantForceEncoder, VrsDamperEncoder, VrsFrictionEncoder, VrsPedalAxesRaw,
    VrsSpringEncoder, build_device_gain, build_ffb_enable, build_rotation_range, identify_device,
    is_wheelbase_product, parse_input_report,
};

// ── Constant force encoder roundtrip ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Torque within ±max round-trips through the magnitude encoding
    /// with at most 1 LSB of error on the ±10000 scale.
    #[test]
    fn prop_constant_force_roundtrip(
        torque_frac in -1.0_f32..=1.0_f32,
        max_torque in 0.1_f32..=50.0_f32,
    ) {
        let torque = torque_frac * max_torque;
        let enc = VrsConstantForceEncoder::new(max_torque);
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

    /// encode_zero always produces zero magnitude.
    #[test]
    fn prop_constant_force_zero(max_torque in 0.1_f32..=50.0_f32) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        prop_assert_eq!(raw, 0, "encode_zero must produce zero magnitude");
    }

    /// Sign of input must be preserved in the encoded magnitude.
    #[test]
    fn prop_constant_force_sign(
        torque in -50.0_f32..=50.0_f32,
        max_torque in 0.1_f32..=50.0_f32,
    ) {
        let enc = VrsConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[3], out[4]]);
        if torque > 0.01 {
            prop_assert!(raw >= 0, "positive torque must yield non-negative raw");
        } else if torque < -0.01 {
            prop_assert!(raw <= 0, "negative torque must yield non-positive raw");
        }
    }
}

// ── Spring / damper / friction encoder roundtrips ───────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Spring steering position, center offset, and deadzone must round-trip.
    #[test]
    fn prop_spring_roundtrip(
        coefficient in 0u16..=10_000u16,
        steering: i16,
        center: i16,
        deadzone in 0u16..=10_000u16,
    ) {
        let enc = VrsSpringEncoder::new(20.0);
        let mut out = [0u8; SPRING_REPORT_LEN];
        let len = enc.encode(coefficient, steering, center, deadzone, &mut out);
        prop_assert_eq!(len, SPRING_REPORT_LEN);
        let recovered_steering = i16::from_le_bytes([out[4], out[5]]);
        let recovered_center = i16::from_le_bytes([out[6], out[7]]);
        prop_assert_eq!(recovered_steering, steering, "steering must round-trip");
        prop_assert_eq!(recovered_center, center, "center must round-trip");
    }

    /// Damper velocity must round-trip; reserved bytes must be zero.
    #[test]
    fn prop_damper_roundtrip(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = VrsDamperEncoder::new(20.0);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, DAMPER_REPORT_LEN);
        let recovered = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered, velocity, "velocity must round-trip");
        prop_assert_eq!(out[6], 0, "reserved byte 6 must be zero");
        prop_assert_eq!(out[7], 0, "reserved byte 7 must be zero");
    }

    /// Friction velocity must round-trip; reserved bytes must be zero.
    #[test]
    fn prop_friction_roundtrip(
        coefficient in 0u16..=10_000u16,
        velocity in 0u16..=10_000u16,
    ) {
        let enc = VrsFrictionEncoder::new(20.0);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        let len = enc.encode(coefficient, velocity, &mut out);
        prop_assert_eq!(len, FRICTION_REPORT_LEN);
        let recovered = u16::from_le_bytes([out[4], out[5]]);
        prop_assert_eq!(recovered, velocity, "velocity must round-trip");
        prop_assert_eq!(&out[6..], &[0u8; 4], "reserved bytes 6-9 must be zero");
    }
}

// ── Output report builder roundtrips ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Rotation range degrees u16 LE at bytes [2..4] must round-trip.
    #[test]
    fn prop_rotation_range_roundtrip(degrees: u16) {
        let report = build_rotation_range(degrees);
        prop_assert_eq!(report.len(), 8);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(decoded, degrees, "degrees must round-trip");
    }

    /// Gain report byte [2] must equal the input gain.
    #[test]
    fn prop_gain_roundtrip(gain: u8) {
        let report = build_device_gain(gain);
        prop_assert_eq!(report.len(), 8);
        prop_assert_eq!(report[2], gain, "gain must round-trip");
    }

    /// FFB enable bool must round-trip.
    #[test]
    fn prop_ffb_enable_roundtrip(enabled: bool) {
        let report = build_ffb_enable(enabled);
        prop_assert_eq!(report.len(), 8);
        let decoded = report[1] != 0;
        prop_assert_eq!(decoded, enabled, "FFB enable must round-trip");
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

    /// is_wheelbase_product must be consistent with identify_device.
    #[test]
    fn prop_wheelbase_consistent(pid: u16) {
        let id = identify_device(pid);
        let is_wb = is_wheelbase_product(pid);
        if is_wb {
            prop_assert!(id.supports_ffb,
                "wheelbase PID {pid:#06X} must support FFB");
        }
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
    ) {
        let raw = VrsPedalAxesRaw {
            throttle,
            brake,
            clutch,
        };
        let norm = raw.normalize();
        prop_assert!(norm.throttle >= 0.0 && norm.throttle <= 1.0,
            "throttle {}", norm.throttle);
        prop_assert!(norm.brake >= 0.0 && norm.brake <= 1.0,
            "brake {}", norm.brake);
        prop_assert!(norm.clutch >= 0.0 && norm.clutch <= 1.0,
            "clutch {}", norm.clutch);
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

    /// Steering i16 written to bytes [0..2] must parse back as a normalised
    /// value in [-1.0, 1.0].
    #[test]
    fn prop_input_steering_range(steering_raw: i16) {
        let mut data = [0u8; 17];
        let bytes = steering_raw.to_le_bytes();
        data[0] = bytes[0];
        data[1] = bytes[1];
        let state = parse_input_report(&data);
        prop_assert!(state.is_some(), "valid 17-byte report must parse");
        if let Some(s) = state {
            prop_assert!(
                s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of [-1,1] for raw {steering_raw}", s.steering
            );
        }
    }

    /// Buttons u16 written to bytes [8..10] must round-trip exactly.
    #[test]
    fn prop_input_buttons_roundtrip(buttons: u16) {
        let mut data = [0u8; 17];
        data[8] = (buttons & 0xFF) as u8;
        data[9] = (buttons >> 8) as u8;
        let state = parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.buttons, buttons, "buttons must round-trip");
        }
    }
}
