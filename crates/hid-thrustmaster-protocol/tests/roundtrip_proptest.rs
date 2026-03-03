//! Roundtrip property-based tests for the Thrustmaster HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Constant force encoder magnitude roundtrip
//! - Input report field roundtrips
//! - Spring / damper / friction effect roundtrips
//! - T150 range and gain roundtrips
//! - Set-range report roundtrip
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, ThrustmasterConstantForceEncoder, build_damper_effect,
    build_friction_effect, build_set_range_report, build_spring_effect, encode_gain_t150,
    encode_range_t150, parse_input_report,
};

// ── Constant force encoder roundtrip ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Torque within ±max round-trips through the magnitude encoding
    /// with at most 1 LSB of error on the ±10000 scale.
    #[test]
    fn prop_constant_force_roundtrip(
        torque_frac in -1.0_f32..=1.0_f32,
        max_torque in 0.1_f32..=20.0_f32,
    ) {
        let torque = torque_frac * max_torque;
        let enc = ThrustmasterConstantForceEncoder::new(max_torque);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        let decoded = raw as f32 / 10_000.0 * max_torque;
        let tolerance = max_torque / 10_000.0 + 0.001;
        let error = (torque - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {torque} roundtrips as {decoded} (error {error} > tol {tolerance})"
        );
    }

    /// encode_zero always produces zero raw magnitude.
    #[test]
    fn prop_constant_force_zero(max_torque in 0.1_f32..=20.0_f32) {
        let enc = ThrustmasterConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; EFFECT_REPORT_LEN];
        enc.encode_zero(&mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(raw, 0, "encode_zero must produce zero magnitude");
    }

    /// Report length from encode must always equal EFFECT_REPORT_LEN.
    #[test]
    fn prop_constant_force_length(
        torque in -100.0_f32..=100.0_f32,
        max_torque in 0.1_f32..=20.0_f32,
    ) {
        let enc = ThrustmasterConstantForceEncoder::new(max_torque);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        let len = enc.encode(torque, &mut out);
        prop_assert_eq!(len, EFFECT_REPORT_LEN);
    }
}

// ── Input report roundtrip ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Steering u16 written to bytes [1..3] must parse back with a value in [-1,1].
    #[test]
    fn prop_input_steering_range(steering_raw: u16) {
        let mut data = [0u8; 10];
        data[0] = 0x01;
        data[1] = (steering_raw & 0xFF) as u8;
        data[2] = (steering_raw >> 8) as u8;
        let state = parse_input_report(&data);
        prop_assert!(state.is_some(), "valid 10-byte report must parse");
        if let Some(s) = state {
            prop_assert!(
                s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of [-1,1]", s.steering
            );
        }
    }

    /// Buttons u16 written to bytes [6..8] must round-trip exactly.
    #[test]
    fn prop_input_buttons_roundtrip(buttons: u16) {
        let mut data = [0u8; 10];
        data[0] = 0x01;
        data[1] = 0x00; data[2] = 0x80; // center
        data[6] = (buttons & 0xFF) as u8;
        data[7] = (buttons >> 8) as u8;
        let state = parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.buttons, buttons, "buttons must round-trip");
        }
    }

    /// Short reports must return None, not panic.
    #[test]
    fn prop_input_short_no_panic(len in 0usize..=9usize) {
        let data = vec![0x01u8; len];
        let state = parse_input_report(&data);
        if len < 10 {
            prop_assert!(state.is_none(), "report of len {len} must return None");
        }
    }
}

// ── Spring / damper / friction effect roundtrips ────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Spring center i16 and stiffness u16 must round-trip through the encoded report.
    #[test]
    fn prop_spring_effect_roundtrip(center: i16, stiffness: u16) {
        let report = build_spring_effect(center, stiffness);
        prop_assert_eq!(report.len(), EFFECT_REPORT_LEN);
        let decoded_center = i16::from_le_bytes([report[3], report[4]]);
        let decoded_stiffness = u16::from_le_bytes([report[5], report[6]]);
        prop_assert_eq!(decoded_center, center, "center must round-trip");
        prop_assert_eq!(decoded_stiffness, stiffness, "stiffness must round-trip");
    }

    /// Damper damping u16 must round-trip through the encoded report.
    #[test]
    fn prop_damper_effect_roundtrip(damping: u16) {
        let report = build_damper_effect(damping);
        prop_assert_eq!(report.len(), EFFECT_REPORT_LEN);
        let decoded = u16::from_le_bytes([report[3], report[4]]);
        prop_assert_eq!(decoded, damping, "damping must round-trip");
    }

    /// Friction min/max u16 must round-trip.
    #[test]
    fn prop_friction_effect_roundtrip(minimum: u16, maximum: u16) {
        let report = build_friction_effect(minimum, maximum);
        prop_assert_eq!(report.len(), EFFECT_REPORT_LEN);
        let decoded_min = u16::from_le_bytes([report[3], report[4]]);
        let decoded_max = u16::from_le_bytes([report[5], report[6]]);
        prop_assert_eq!(decoded_min, minimum, "minimum must round-trip");
        prop_assert_eq!(decoded_max, maximum, "maximum must round-trip");
    }
}

// ── T300/T150 output report roundtrips ──────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Set-range degrees encode→decode roundtrip.
    #[test]
    fn prop_set_range_roundtrip(degrees: u16) {
        let report = build_set_range_report(degrees);
        prop_assert_eq!(report.len(), 7);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(decoded, degrees, "degrees must round-trip");
    }

    /// T150 range encode→decode roundtrip: u16 LE at bytes [2..4].
    #[test]
    fn prop_t150_range_roundtrip(range_value: u16) {
        let report = encode_range_t150(range_value);
        prop_assert_eq!(report.len(), 4);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(decoded, range_value, "T150 range must round-trip");
        prop_assert_eq!(report[0], 0x40, "byte 0 must be 0x40");
        prop_assert_eq!(report[1], 0x11, "byte 1 must be 0x11");
    }

    /// T150 gain encode→decode roundtrip: gain at byte [1].
    #[test]
    fn prop_t150_gain_roundtrip(gain: u8) {
        let report = encode_gain_t150(gain);
        prop_assert_eq!(report.len(), 2);
        prop_assert_eq!(report[0], 0x43, "byte 0 must be 0x43");
        prop_assert_eq!(report[1], gain, "gain must round-trip");
    }
}
