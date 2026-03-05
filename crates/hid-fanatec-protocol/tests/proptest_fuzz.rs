//! Fuzz-style property tests for Fanatec protocol edge cases.
//!
//! Feeds arbitrary byte sequences and extreme values into all parse and encode
//! entry points to ensure no panics occur and all outputs are well-formed.

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, FanatecModel, FanatecRimId,
    parse_extended_report, parse_pedal_report, parse_standard_report,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Arbitrary-bytes fuzz: every parse entry point ────────────────────

    /// Feeding any byte sequence to parse_standard_report must never panic.
    #[test]
    fn fuzz_standard_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = parse_standard_report(&data);
    }

    /// Feeding any byte sequence to parse_extended_report must never panic.
    #[test]
    fn fuzz_extended_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = parse_extended_report(&data);
    }

    /// Feeding any byte sequence to parse_pedal_report must never panic.
    #[test]
    fn fuzz_pedal_report_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = parse_pedal_report(&data);
    }

    // ── Boundary lengths ─────────────────────────────────────────────────

    /// Standard report requires 10+ bytes with report ID 0x01. Lengths 0–9
    /// must return None regardless of content.
    #[test]
    fn fuzz_standard_report_short_lengths(len in 0usize..=9) {
        let mut data = vec![0x01u8; len.max(1)];
        data.resize(len, 0xFF);
        if !data.is_empty() {
            data[0] = 0x01;
        }
        prop_assert!(parse_standard_report(&data).is_none(),
            "standard report with len={len} must return None");
    }

    /// Extended report requires 11+ bytes with report ID 0x02. Lengths 0–10
    /// must return None regardless of content.
    #[test]
    fn fuzz_extended_report_short_lengths(len in 0usize..=10) {
        let mut data = vec![0x02u8; len.max(1)];
        data.resize(len, 0xFF);
        if !data.is_empty() {
            data[0] = 0x02;
        }
        prop_assert!(parse_extended_report(&data).is_none(),
            "extended report with len={len} must return None");
    }

    /// Pedal report requires 5+ bytes with report ID 0x01. Lengths 0–4
    /// must return None regardless of content.
    #[test]
    fn fuzz_pedal_report_short_lengths(len in 0usize..=4) {
        let mut data = vec![0x01u8; len.max(1)];
        data.resize(len, 0xFF);
        if !data.is_empty() {
            data[0] = 0x01;
        }
        prop_assert!(parse_pedal_report(&data).is_none(),
            "pedal report with len={len} must return None");
    }

    // ── Wrong report IDs ─────────────────────────────────────────────────

    /// A valid-length standard report with any non-0x01 report ID must return
    /// None.
    #[test]
    fn fuzz_standard_report_wrong_id(
        id in 0x02u8..=0xFF,
        tail in proptest::collection::vec(any::<u8>(), 63usize),
    ) {
        let mut data = vec![id];
        data.extend_from_slice(&tail);
        prop_assert!(parse_standard_report(&data).is_none(),
            "standard report with ID 0x{:02X} must return None", id);
    }

    /// A valid-length extended report with any non-0x02 report ID must return
    /// None.
    #[test]
    fn fuzz_extended_report_wrong_id(
        id in (0u8..=0xFF).prop_filter("not 0x02", |&id| id != 0x02),
        tail in proptest::collection::vec(any::<u8>(), 63usize),
    ) {
        let mut data = vec![id];
        data.extend_from_slice(&tail);
        prop_assert!(parse_extended_report(&data).is_none(),
            "extended report with ID 0x{:02X} must return None", id);
    }

    // ── FanatecRimId::from_byte ──────────────────────────────────────────

    /// from_byte must never panic for any u8 value and must be deterministic.
    #[test]
    fn fuzz_rim_id_from_byte(byte: u8) {
        let a = FanatecRimId::from_byte(byte);
        let b = FanatecRimId::from_byte(byte);
        prop_assert_eq!(a, b, "from_byte must be deterministic for 0x{:02X}", byte);
    }

    /// from_product_id must never panic for any u16 value and must be
    /// deterministic.
    #[test]
    fn fuzz_model_from_product_id(pid: u16) {
        let a = FanatecModel::from_product_id(pid);
        let b = FanatecModel::from_product_id(pid);
        prop_assert_eq!(a, b, "from_product_id must be deterministic for 0x{:04X}", pid);
    }

    // ── Encoder: NaN / Inf / extreme values ──────────────────────────────

    /// Encoding NaN must not panic and must produce a valid-length report.
    #[test]
    fn fuzz_encoder_nan(_dummy in 0u8..=1) {
        let encoder = FanatecConstantForceEncoder::new(20.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(f32::NAN, 0, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    }

    /// Encoding ±Inf must not panic and must produce a valid-length report.
    #[test]
    fn fuzz_encoder_inf(positive: bool) {
        let val = if positive { f32::INFINITY } else { f32::NEG_INFINITY };
        let encoder = FanatecConstantForceEncoder::new(20.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(val, 0, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    }

    /// Encoding with max_torque=0.0 must not panic or divide by zero.
    #[test]
    fn fuzz_encoder_zero_max_torque(torque in -100.0f32..=100.0) {
        let encoder = FanatecConstantForceEncoder::new(0.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    }

    // ── Parse output ranges ──────────────────────────────────────────────

    /// When parse_standard_report succeeds, all normalised axes must be finite
    /// and within their documented range.
    #[test]
    fn fuzz_standard_report_output_ranges(
        data in proptest::collection::vec(any::<u8>(), 10..=64),
    ) {
        let mut buf = data;
        buf[0] = 0x01;
        if let Some(s) = parse_standard_report(&buf) {
            prop_assert!(s.steering.is_finite() && s.steering >= -1.0 && s.steering <= 1.0,
                "steering {} out of range", s.steering);
            prop_assert!(s.throttle.is_finite() && s.throttle >= 0.0 && s.throttle <= 1.0,
                "throttle {} out of range", s.throttle);
            prop_assert!(s.brake.is_finite() && s.brake >= 0.0 && s.brake <= 1.0,
                "brake {} out of range", s.brake);
            prop_assert!(s.clutch.is_finite() && s.clutch >= 0.0 && s.clutch <= 1.0,
                "clutch {} out of range", s.clutch);
        }
    }

    /// When parse_pedal_report succeeds, raw values must be 12-bit (≤ 0x0FFF).
    #[test]
    fn fuzz_pedal_report_output_ranges(
        data in proptest::collection::vec(any::<u8>(), 5..=64),
    ) {
        let mut buf = data;
        buf[0] = 0x01;
        if let Some(s) = parse_pedal_report(&buf) {
            prop_assert!(s.throttle_raw <= 0x0FFF,
                "throttle_raw 0x{:04X} > 0x0FFF", s.throttle_raw);
            prop_assert!(s.brake_raw <= 0x0FFF,
                "brake_raw 0x{:04X} > 0x0FFF", s.brake_raw);
            prop_assert!(s.clutch_raw <= 0x0FFF,
                "clutch_raw 0x{:04X} > 0x0FFF", s.clutch_raw);
            prop_assert!(s.axis_count == 2 || s.axis_count == 3,
                "axis_count {} must be 2 or 3", s.axis_count);
        }
    }
}
