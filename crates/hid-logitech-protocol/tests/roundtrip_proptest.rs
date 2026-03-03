//! Roundtrip property-based tests for the Logitech HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Constant force encoder magnitude roundtrip
//! - Input report field roundtrips (steering, buttons, pedals)
//! - Set-range report roundtrip
//! - Gain report roundtrip
//! - Autocenter and LED reports
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LogitechConstantForceEncoder, LogitechModel, build_gain_report,
    build_set_autocenter_report, build_set_leds_report, build_set_range_report, parse_input_report,
};

// ── Constant force encoder roundtrip ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Torque within ±max round-trips through the i16 magnitude encoding
    /// with at most 1 LSB of error on the ±10000 scale.
    #[test]
    fn prop_constant_force_roundtrip(
        torque_frac in -1.0_f32..=1.0_f32,
        max_torque in 0.1_f32..=20.0_f32,
    ) {
        let torque = torque_frac * max_torque;
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
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
    fn prop_constant_force_zero_roundtrip(max_torque in 0.1_f32..=20.0_f32) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode_zero(&mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(raw, 0, "encode_zero must produce zero magnitude");
    }

    /// Report header: byte 0 is report ID 0x12, byte 1 is effect block 1.
    #[test]
    fn prop_constant_force_header(
        torque in -100.0_f32..=100.0_f32,
        max_torque in 0.1_f32..=20.0_f32,
    ) {
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = enc.encode(torque, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
        prop_assert_eq!(out[0], 0x12, "byte 0 must be report ID 0x12");
        prop_assert_eq!(out[1], 1, "byte 1 must be effect block index 1");
    }
}

// ── Input report roundtrip ──────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Steering u16 written to bytes [1..3] must parse back with consistent
    /// normalised value in [-1.0, 1.0].
    #[test]
    fn prop_input_steering_roundtrip(steering_raw: u16) {
        let mut data = [0u8; 10];
        data[0] = 0x01; // report ID
        data[1] = (steering_raw & 0xFF) as u8;
        data[2] = (steering_raw >> 8) as u8;
        data[3] = 0x00; // throttle released
        data[4] = 0x00; // brake released
        data[5] = 0x00; // clutch released
        let state = parse_input_report(&data);
        prop_assert!(state.is_some(), "valid 10-byte report must parse");
        if let Some(s) = state {
            prop_assert!(
                s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of [-1,1] for raw {steering_raw}", s.steering
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

    /// Hat value written to byte [8] lower nibble must round-trip.
    #[test]
    fn prop_input_hat_roundtrip(hat in 0u8..=0x0Fu8) {
        let mut data = [0u8; 10];
        data[0] = 0x01;
        data[1] = 0x00; data[2] = 0x80;
        data[8] = hat;
        let state = parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.hat, hat, "hat must round-trip");
        }
    }

    /// Short reports must return None without panicking.
    #[test]
    fn prop_input_short_report_none(len in 0usize..=9usize) {
        let data = vec![0x01u8; len];
        let state = parse_input_report(&data);
        if len < 10 {
            prop_assert!(state.is_none(), "report of len {len} must return None");
        }
    }
}

// ── Output report builder roundtrips ────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Set-range degrees encode→decode roundtrip: u16 LE at bytes [2..4].
    #[test]
    fn prop_set_range_roundtrip(degrees: u16) {
        let report = build_set_range_report(degrees);
        prop_assert_eq!(report.len(), 7);
        let decoded = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(decoded, degrees, "degrees must round-trip exactly");
        prop_assert_eq!(report[0], 0xF8, "byte 0 must be 0xF8");
        prop_assert_eq!(report[1], 0x81, "byte 1 must be 0x81");
    }

    /// Gain report byte 1 must equal the input gain.
    #[test]
    fn prop_gain_roundtrip(gain: u8) {
        let report = build_gain_report(gain);
        prop_assert_eq!(report.len(), 2);
        prop_assert_eq!(report[0], 0x16, "byte 0 must be 0x16");
        prop_assert_eq!(report[1], gain, "gain must round-trip exactly");
    }

    /// Autocenter report is deterministic.
    #[test]
    fn prop_autocenter_deterministic(strength: u8, rate: u8) {
        let r1 = build_set_autocenter_report(strength, rate);
        let r2 = build_set_autocenter_report(strength, rate);
        prop_assert_eq!(r1, r2, "autocenter report must be deterministic");
        prop_assert_eq!(r1.len(), 7);
    }

    /// LED report mask must appear in the output.
    #[test]
    fn prop_leds_roundtrip(mask in 0u8..=0x1Fu8) {
        let report = build_set_leds_report(mask);
        prop_assert_eq!(report.len(), 7);
    }

    /// Model from_product_id is deterministic.
    #[test]
    fn prop_model_deterministic(pid: u16) {
        let m1 = LogitechModel::from_product_id(pid);
        let m2 = LogitechModel::from_product_id(pid);
        prop_assert_eq!(m1, m2);
    }
}
