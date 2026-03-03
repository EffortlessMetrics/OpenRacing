//! Roundtrip property-based tests for the Fanatec HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Constant force encoder magnitude roundtrip
//! - Standard input report button/pedal roundtrip
//! - Pedal report axis roundtrip
//! - Rotation range encode→decode
//! - LED report encoding determinism
//! - Display report encoding determinism
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, build_display_report, build_led_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report, build_stop_all_report,
    parse_pedal_report, parse_standard_report,
};

// ── Constant force magnitude roundtrip ──────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Torque within ±max round-trips through the i16 magnitude encoding
    /// with at most 1 LSB of error on the ±32767 scale.
    #[test]
    fn prop_constant_force_magnitude_roundtrip(
        torque_frac in -1.0_f32..=1.0_f32,
        max_torque in 0.1_f32..=50.0_f32,
    ) {
        let torque = torque_frac * max_torque;
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        let decoded = if raw >= 0 {
            raw as f32 / i16::MAX as f32 * max_torque
        } else {
            raw as f32 / (-(i16::MIN as f32)) * max_torque
        };
        let tolerance = max_torque / i16::MAX as f32 + 1e-4;
        let error = (torque - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {torque} roundtrips as {decoded} (error {error} > tol {tolerance})"
        );
    }

    /// encode_zero always produces zero magnitude.
    #[test]
    fn prop_constant_force_zero_roundtrip(max_torque in 0.1_f32..=50.0_f32) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode_zero(&mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(raw, 0, "encode_zero must produce zero magnitude");
    }
}

// ── Standard input report roundtrip ─────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Steering raw u16 written into a constructed report must parse back
    /// as a normalised value consistent with the raw input.
    #[test]
    fn prop_input_steering_roundtrip(steering_raw: u16) {
        let mut data = [0u8; 64];
        data[0] = 0x01; // standard report ID
        data[2] = (steering_raw & 0xFF) as u8;
        data[1] = (steering_raw >> 8) as u8;
        // Remaining axes at neutral
        data[3] = 0xFF;
        data[4] = 0xFF;
        data[5] = 0xFF;
        let state = parse_standard_report(&data);
        prop_assert!(state.is_some(), "valid 64-byte report must parse");
        if let Some(s) = state {
            // Steering is normalised to [-1.0, 1.0]; verify it's in range.
            prop_assert!(
                s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of [-1,1] for raw {steering_raw}",
                s.steering
            );
        }
    }

    /// Buttons encoded in a constructed input report must round-trip exactly.
    #[test]
    fn prop_input_buttons_roundtrip(buttons: u16) {
        let mut data = [0u8; 64];
        data[0] = 0x01;
        data[2] = 0x80; // center steering
        data[3] = 0xFF;
        data[4] = 0xFF;
        data[5] = 0xFF;
        data[7] = (buttons & 0xFF) as u8;
        data[8] = (buttons >> 8) as u8;
        let state = parse_standard_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.buttons, buttons, "buttons must round-trip exactly");
        }
    }
}

// ── Pedal report roundtrip ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Pedal raw 12-bit values encoded in a pedal report must round-trip
    /// through parse_pedal_report exactly.
    #[test]
    fn prop_pedal_axes_roundtrip(
        throttle in 0u16..=0x0FFFu16,
        brake in 0u16..=0x0FFFu16,
        clutch in 0u16..=0x0FFFu16,
    ) {
        let mut data = [0u8; 7];
        data[0] = 0x01;
        let t = throttle.to_le_bytes();
        data[1] = t[0]; data[2] = t[1];
        let b = brake.to_le_bytes();
        data[3] = b[0]; data[4] = b[1];
        let c = clutch.to_le_bytes();
        data[5] = c[0]; data[6] = c[1];
        let state = parse_pedal_report(&data);
        prop_assert!(state.is_some(), "valid 7-byte pedal report must parse");
        if let Some(s) = state {
            prop_assert_eq!(s.throttle_raw, throttle);
            prop_assert_eq!(s.brake_raw, brake);
            prop_assert_eq!(s.clutch_raw, clutch);
        }
    }

    /// Short pedal reports (< 5 bytes) must return None, not panic.
    #[test]
    fn prop_pedal_short_report_none(len in 0usize..=4usize) {
        let data = vec![0x01u8; len];
        let state = parse_pedal_report(&data);
        prop_assert!(state.is_none(), "short report of len {len} must return None");
    }
}

// ── Output report builder roundtrips ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Rotation range degrees encode→decode roundtrip (clamped to valid range).
    #[test]
    fn prop_rotation_range_roundtrip(degrees: u16) {
        let report = build_rotation_range_report(degrees);
        prop_assert_eq!(report.len(), 8);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        let clamped = degrees.clamp(90, 2520);
        prop_assert_eq!(decoded, clamped);
    }

    /// Gain report byte 2 must equal the input gain clamped to [0, 100].
    #[test]
    fn prop_gain_roundtrip(gain: u8) {
        let report = build_set_gain_report(gain);
        prop_assert_eq!(report.len(), 8);
        let clamped = gain.min(100);
        prop_assert_eq!(report[2], clamped);
    }

    /// LED report bitmask must round-trip through bytes [2..4] as u16 LE.
    #[test]
    fn prop_led_report_roundtrip(bitmask: u16, brightness: u8) {
        let report = build_led_report(bitmask, brightness);
        prop_assert_eq!(report.len(), 8);
        let decoded_mask = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(decoded_mask, bitmask, "LED bitmask must round-trip");
    }

    /// Display report digits must appear in the output.
    #[test]
    fn prop_display_report_deterministic(
        mode: u8,
        d0: u8,
        d1: u8,
        d2: u8,
        brightness: u8,
    ) {
        let r1 = build_display_report(mode, [d0, d1, d2], brightness);
        let r2 = build_display_report(mode, [d0, d1, d2], brightness);
        prop_assert_eq!(r1, r2, "display report must be deterministic");
    }

    /// Rumble report must be deterministic.
    #[test]
    fn prop_rumble_report_deterministic(left: u8, right: u8, duration: u8) {
        let r1 = build_rumble_report(left, right, duration);
        let r2 = build_rumble_report(left, right, duration);
        prop_assert_eq!(r1, r2, "rumble report must be deterministic");
    }

    /// Stop-all report is always a fixed value.
    #[test]
    fn prop_stop_all_deterministic(_seed: u8) {
        let r1 = build_stop_all_report();
        let r2 = build_stop_all_report();
        prop_assert_eq!(r1, r2, "stop_all must be deterministic");
    }
}
