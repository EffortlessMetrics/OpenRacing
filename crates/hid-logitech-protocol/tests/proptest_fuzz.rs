//! Fuzz-style property tests for Logitech protocol edge cases.
//!
//! Feeds arbitrary byte sequences and extreme values into all parse and encode
//! entry points to ensure no panics occur and all outputs are well-formed.

use proptest::prelude::*;
use racing_wheel_hid_logitech_protocol::{
    LogitechConstantForceEncoder, LogitechModel, is_wheel_product, parse_input_report,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Arbitrary-bytes fuzz ─────────────────────────────────────────────

    /// Feeding any byte sequence of any length to parse_input_report must
    /// never panic.
    #[test]
    fn fuzz_input_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = parse_input_report(&data);
    }

    // ── Boundary lengths ─────────────────────────────────────────────────

    /// Input report requires 10+ bytes with report ID 0x01. Lengths 0–9
    /// must return None.
    #[test]
    fn fuzz_input_report_short_lengths(len in 0usize..=9) {
        let mut data = vec![0x01u8; len.max(1)];
        data.resize(len, 0xFF);
        if !data.is_empty() {
            data[0] = 0x01;
        }
        prop_assert!(parse_input_report(&data).is_none(),
            "input report with len={len} must return None");
    }

    /// Exactly 10 bytes with correct ID must succeed (minimum valid).
    #[test]
    fn fuzz_input_report_exact_minimum(
        tail in proptest::collection::vec(any::<u8>(), 9usize),
    ) {
        let mut data = vec![0x01u8];
        data.extend_from_slice(&tail);
        prop_assert!(parse_input_report(&data).is_some(),
            "10-byte report with ID 0x01 must parse");
    }

    // ── Wrong report IDs ─────────────────────────────────────────────────

    /// A valid-length report with any non-0x01 report ID must return None.
    #[test]
    fn fuzz_input_report_wrong_id(
        id in 0x02u8..=0xFF,
        tail in proptest::collection::vec(any::<u8>(), 11usize),
    ) {
        let mut data = vec![id];
        data.extend_from_slice(&tail);
        prop_assert!(parse_input_report(&data).is_none(),
            "report with ID 0x{:02X} must return None", id);
    }

    /// Report ID 0x00 must also return None.
    #[test]
    fn fuzz_input_report_id_zero(
        tail in proptest::collection::vec(any::<u8>(), 11usize),
    ) {
        let mut data = vec![0x00u8];
        data.extend_from_slice(&tail);
        prop_assert!(parse_input_report(&data).is_none(),
            "report with ID 0x00 must return None");
    }

    // ── Parse output ranges ──────────────────────────────────────────────

    /// When parse succeeds, all normalised axes must be finite and within
    /// their documented range.
    #[test]
    fn fuzz_input_report_output_ranges(
        data in proptest::collection::vec(any::<u8>(), 10..=64),
    ) {
        let mut buf = data;
        buf[0] = 0x01;
        if let Some(s) = parse_input_report(&buf) {
            prop_assert!(s.steering.is_finite() && s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of range", s.steering);
            prop_assert!(s.throttle.is_finite() && s.throttle >= 0.0 && s.throttle <= 1.0,
                "throttle {} out of range", s.throttle);
            prop_assert!(s.brake.is_finite() && s.brake >= 0.0 && s.brake <= 1.0,
                "brake {} out of range", s.brake);
            prop_assert!(s.clutch.is_finite() && s.clutch >= 0.0 && s.clutch <= 1.0,
                "clutch {} out of range", s.clutch);
            prop_assert!(s.hat <= 0x0F, "hat 0x{:02X} > 0x0F", s.hat);
            prop_assert!(s.paddles <= 0x03, "paddles 0x{:02X} > 0x03", s.paddles);
        }
    }

    // ── LogitechModel::from_product_id ───────────────────────────────────

    /// from_product_id must never panic and must be deterministic.
    #[test]
    fn fuzz_model_from_product_id(pid: u16) {
        let a = LogitechModel::from_product_id(pid);
        let b = LogitechModel::from_product_id(pid);
        prop_assert_eq!(a, b, "from_product_id must be deterministic for 0x{:04X}", pid);
    }

    /// is_wheel_product must never panic and must be deterministic.
    #[test]
    fn fuzz_is_wheel_product(pid: u16) {
        let a = is_wheel_product(pid);
        let b = is_wheel_product(pid);
        prop_assert_eq!(a, b, "is_wheel_product must be deterministic for 0x{:04X}", pid);
    }

    // ── Encoder: NaN / Inf / extreme values ──────────────────────────────

    /// Encoding NaN must not panic.
    #[test]
    fn fuzz_encoder_nan(_dummy in 0u8..=1) {
        let encoder = LogitechConstantForceEncoder::new(10.0);
        let mut out = [0u8; 4];
        let _len = encoder.encode(f32::NAN, &mut out);
    }

    /// Encoding ±Inf must not panic.
    #[test]
    fn fuzz_encoder_inf(positive: bool) {
        let val = if positive { f32::INFINITY } else { f32::NEG_INFINITY };
        let encoder = LogitechConstantForceEncoder::new(10.0);
        let mut out = [0u8; 4];
        let _len = encoder.encode(val, &mut out);
    }

    /// Encoding with max_torque=0.0 must not divide by zero or panic.
    #[test]
    fn fuzz_encoder_zero_max_torque(torque in -100.0f32..=100.0) {
        let encoder = LogitechConstantForceEncoder::new(0.0);
        let mut out = [0u8; 4];
        let _len = encoder.encode(torque, &mut out);
    }

    /// Encoding with extremely large torque values must not overflow.
    #[test]
    fn fuzz_encoder_extreme_torque(torque in -1e10f32..=1e10f32) {
        let encoder = LogitechConstantForceEncoder::new(10.0);
        let mut out = [0u8; 4];
        let _len = encoder.encode(torque, &mut out);
    }
}
