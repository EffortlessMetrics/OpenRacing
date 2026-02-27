//! Property-based tests for the Fanatec HID protocol encoding.
//!
//! Uses proptest with 500 cases to verify invariants on FFB encoding,
//! mode-byte values, gain clamping, and LED/rumble report fields.

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{
    FanatecConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN, build_display_report, build_led_report,
    build_mode_switch_report, build_rumble_report, build_set_gain_report, build_stop_all_report,
};

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// Report ID (byte 0) and command byte (byte 1) must always be correct.
    #[test]
    fn prop_report_id_and_command_bytes(
        torque in -100.0f32..100.0f32,
        max_torque in 0.01f32..50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(out[0], 0x01, "byte 0 must be FFB report ID 0x01");
        prop_assert_eq!(out[1], 0x01, "byte 1 must be CONSTANT_FORCE command 0x01");
    }

    /// The encoded raw i16 value must remain within the valid i16 range.
    #[test]
    fn prop_ffb_scalar_bounds(
        torque in -200.0f32..200.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(
            raw >= i16::MIN && raw <= i16::MAX,
            "raw value {} must fit in i16",
            raw
        );
    }

    /// A positive torque (> 0) must produce a non-negative raw value.
    #[test]
    fn prop_positive_torque_produces_nonneg_raw(
        torque in 0.01f32..50.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(
            raw >= 0,
            "positive torque {torque} with max {max_torque} must give raw >= 0, got {raw}"
        );
    }

    /// A negative torque (< 0) must produce a non-positive raw value.
    #[test]
    fn prop_negative_torque_produces_nonpos_raw(
        torque in -50.0f32..-0.01f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(
            raw <= 0,
            "negative torque {torque} with max {max_torque} must give raw <= 0, got {raw}"
        );
    }

    /// Torque at 10× max_torque must saturate to i16::MAX / i16::MIN.
    #[test]
    fn prop_over_range_torque_saturates(max_torque in 0.1f32..50.0f32) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

        encoder.encode(max_torque * 10.0, 0, &mut out);
        let raw_pos = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(raw_pos, i16::MAX, "over-positive must saturate to i16::MAX");

        encoder.encode(-max_torque * 10.0, 0, &mut out);
        let raw_neg = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(raw_neg, i16::MIN, "over-negative must saturate to i16::MIN");
    }

    /// Reserved bytes (4–7) must always be zero regardless of inputs.
    #[test]
    fn prop_reserved_bytes_always_zero(
        torque in -100.0f32..100.0f32,
        max_torque in 0.1f32..50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(out[4], 0, "byte 4 (reserved) must be zero");
        prop_assert_eq!(out[5], 0, "byte 5 (reserved) must be zero");
        prop_assert_eq!(out[6], 0, "byte 6 (reserved) must be zero");
        prop_assert_eq!(out[7], 0, "byte 7 (reserved) must be zero");
    }

    /// `encode_zero` must always produce a zero force (bytes 2–3 zero).
    #[test]
    fn prop_encode_zero_always_clears_force(max_torque in 0.1f32..50.0f32) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode_zero(&mut out);
        prop_assert_eq!(out[2], 0, "encode_zero must clear force low byte");
        prop_assert_eq!(out[3], 0, "encode_zero must clear force high byte");
    }

    /// Set-gain report must clamp any u8 gain value to [0, 100].
    #[test]
    fn prop_gain_report_clamped(gain: u8) {
        let report = build_set_gain_report(gain);
        prop_assert_eq!(report[0], 0x01, "gain report ID must be 0x01");
        prop_assert_eq!(report[1], 0x10, "set-gain command byte must be 0x10");
        prop_assert!(
            report[2] <= 100,
            "gain byte must be clamped to 100, got {}",
            report[2]
        );
        if gain <= 100 {
            prop_assert_eq!(report[2], gain, "gain ≤100 must pass through unchanged");
        } else {
            prop_assert_eq!(report[2], 100, "gain >100 must be clamped to 100");
        }
    }

    /// LED report must round-trip the 16-bit bitmask and brightness correctly.
    #[test]
    fn prop_led_bitmask_roundtrip(bitmask: u16, brightness: u8) {
        let report = build_led_report(bitmask, brightness);
        prop_assert_eq!(report[0], 0x08, "LED report ID must be 0x08");
        prop_assert_eq!(report[1], 0x80, "REV_LIGHTS command must be 0x80");
        let recovered = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(recovered, bitmask, "bitmask must round-trip via LE bytes");
        prop_assert_eq!(report[4], brightness, "brightness must be preserved");
        prop_assert_eq!(&report[5..], &[0u8; 3], "LED reserved bytes must be zero");
    }

    /// Rumble report must preserve left/right motor intensities and duration.
    #[test]
    fn prop_rumble_parameters_preserved(left: u8, right: u8, duration: u8) {
        let report = build_rumble_report(left, right, duration);
        prop_assert_eq!(report[0], 0x08, "rumble report ID must be 0x08");
        prop_assert_eq!(report[1], 0x82, "RUMBLE command must be 0x82");
        prop_assert_eq!(report[2], left, "left motor intensity must be preserved");
        prop_assert_eq!(report[3], right, "right motor intensity must be preserved");
        prop_assert_eq!(report[4], duration, "duration must be preserved");
        prop_assert_eq!(&report[5..], &[0u8; 3], "rumble reserved bytes must be zero");
    }

    /// Display report must preserve mode, digits, and brightness.
    #[test]
    fn prop_display_report_fields(mode: u8, d0: u8, d1: u8, d2: u8, brightness: u8) {
        let report = build_display_report(mode, [d0, d1, d2], brightness);
        prop_assert_eq!(report[0], 0x08, "display report ID must be 0x08");
        prop_assert_eq!(report[1], 0x81, "DISPLAY command must be 0x81");
        prop_assert_eq!(report[2], mode, "mode byte must be preserved");
        prop_assert_eq!(report[3], d0, "digit 0 must be preserved");
        prop_assert_eq!(report[4], d1, "digit 1 must be preserved");
        prop_assert_eq!(report[5], d2, "digit 2 must be preserved");
        prop_assert_eq!(report[6], brightness, "brightness must be preserved");
        prop_assert_eq!(report[7], 0, "display reserved byte must be zero");
    }
}

/// Mode-switch report bytes are constants — verify once.
#[test]
fn test_mode_switch_report_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_mode_switch_report();
    assert_eq!(report[0], 0x01, "mode-switch report ID must be 0x01");
    assert_eq!(report[1], 0x01, "Set Mode command must be 0x01");
    assert_eq!(report[2], 0x03, "Advanced/PC mode byte must be 0x03");
    assert_eq!(&report[3..], &[0u8; 5], "mode-switch reserved bytes must be zero");
    Ok(())
}

/// Stop-all command byte is a constant — verify once.
#[test]
fn test_stop_all_command_byte() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_stop_all_report();
    assert_eq!(report[0], 0x01, "stop-all report ID must be 0x01");
    assert_eq!(report[1], 0x0F, "stop-all command byte must be 0x0F");
    assert_eq!(&report[2..], &[0u8; 6], "stop-all reserved bytes must be zero");
    Ok(())
}
