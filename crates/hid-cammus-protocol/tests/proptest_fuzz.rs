//! Fuzz-style property tests for Cammus protocol edge cases.
//!
//! Feeds arbitrary byte sequences and extreme values into all parse and encode
//! entry points to ensure no panics occur and all outputs are well-formed.

use proptest::prelude::*;
use racing_wheel_hid_cammus_protocol::{
    CammusModel, FFB_REPORT_LEN, ParseError, encode_stop, encode_torque, parse,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ── Arbitrary-bytes fuzz ─────────────────────────────────────────────

    /// Feeding any byte sequence to parse must never panic.
    #[test]
    fn fuzz_parse_arbitrary_bytes(
        data in proptest::collection::vec(any::<u8>(), 0..=128),
    ) {
        let _ = parse(&data);
    }

    /// Short buffers (< 12 bytes) must return Err(TooShort), not panic.
    #[test]
    fn fuzz_parse_short_returns_too_short(len in 0usize..12) {
        let data = vec![0xFFu8; len];
        match parse(&data) {
            Err(ParseError::TooShort { got, need }) => {
                prop_assert_eq!(got, len, "got must equal buffer length");
                prop_assert_eq!(need, 12, "need must be 12");
            }
            Ok(_) => {
                prop_assert!(false, "short buffer (len={len}) must not parse successfully");
            }
        }
    }

    /// Exactly 12 bytes must succeed.
    #[test]
    fn fuzz_parse_exact_minimum(
        data in proptest::collection::vec(any::<u8>(), 12usize..=12),
    ) {
        prop_assert!(parse(&data).is_ok(),
            "12-byte buffer must parse successfully");
    }

    // ── Parse output ranges ──────────────────────────────────────────────

    /// When parse succeeds, all normalised axes must be finite and in
    /// their documented range.
    #[test]
    fn fuzz_parse_output_ranges(
        data in proptest::collection::vec(any::<u8>(), 12..=128),
    ) {
        if let Ok(report) = parse(&data) {
            prop_assert!(report.steering.is_finite()
                && report.steering >= -1.0 && report.steering <= 1.0,
                "steering {} out of [-1.0, 1.0]", report.steering);
            prop_assert!(report.throttle.is_finite()
                && report.throttle >= 0.0 && report.throttle <= 1.0,
                "throttle {} out of [0.0, 1.0]", report.throttle);
            prop_assert!(report.brake.is_finite()
                && report.brake >= 0.0 && report.brake <= 1.0,
                "brake {} out of [0.0, 1.0]", report.brake);
            prop_assert!(report.clutch.is_finite()
                && report.clutch >= 0.0 && report.clutch <= 1.0,
                "clutch {} out of [0.0, 1.0]", report.clutch);
            prop_assert!(report.handbrake.is_finite()
                && report.handbrake >= 0.0 && report.handbrake <= 1.0,
                "handbrake {} out of [0.0, 1.0]", report.handbrake);
        }
    }

    // ── encode_torque → decode roundtrip ─────────────────────────────────

    /// Encoding a normalised torque and decoding the i16 from the result
    /// must preserve the sign and approximate magnitude.
    #[test]
    fn fuzz_encode_torque_roundtrip(torque in -1.0f32..=1.0) {
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let decoded = raw as f32 / i16::MAX as f32;
        // 1-LSB tolerance
        let tolerance = 1.0 / i16::MAX as f32 + 1e-5;
        let error = (torque - decoded).abs();
        prop_assert!(error <= tolerance,
            "torque {torque} → raw {raw} → decoded {decoded} (error {error} > tol {tolerance})");
    }

    // ── encode_torque: NaN / Inf / extreme values ────────────────────────

    /// Encoding NaN must not panic and must produce FFB_REPORT_LEN bytes.
    #[test]
    fn fuzz_encode_torque_nan(_dummy in 0u8..=1) {
        let report = encode_torque(f32::NAN);
        prop_assert_eq!(report.len(), FFB_REPORT_LEN);
    }

    /// Encoding ±Inf must not panic. The result must clamp to ±i16::MAX.
    #[test]
    fn fuzz_encode_torque_inf(positive: bool) {
        let val = if positive { f32::INFINITY } else { f32::NEG_INFINITY };
        let report = encode_torque(val);
        prop_assert_eq!(report.len(), FFB_REPORT_LEN);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        if positive {
            prop_assert_eq!(raw, i16::MAX,
                "+Inf must saturate to i16::MAX, got {}", raw);
        } else {
            prop_assert_eq!(raw, -i16::MAX,
                "-Inf must saturate to -i16::MAX, got {}", raw);
        }
    }

    /// Encoding values outside [-1.0, 1.0] must clamp, not overflow.
    #[test]
    fn fuzz_encode_torque_out_of_range(torque in -1e6f32..=1e6f32) {
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let max_report = encode_torque(1.0);
        let max_raw = i16::from_le_bytes([max_report[1], max_report[2]]);
        let min_report = encode_torque(-1.0);
        let min_raw = i16::from_le_bytes([min_report[1], min_report[2]]);
        prop_assert!(raw >= min_raw && raw <= max_raw,
            "raw {raw} outside [{min_raw}, {max_raw}] for torque {torque}");
    }

    // ── encode_stop consistency ──────────────────────────────────────────

    /// encode_stop must always equal encode_torque(0.0).
    #[test]
    fn fuzz_encode_stop_is_zero(_dummy in 0u8..=1) {
        prop_assert_eq!(encode_stop(), encode_torque(0.0));
    }

    // ── CammusModel::from_pid ────────────────────────────────────────────

    /// from_pid must never panic for any u16 and must be deterministic.
    #[test]
    fn fuzz_model_from_pid(pid: u16) {
        let a = CammusModel::from_pid(pid);
        let b = CammusModel::from_pid(pid);
        prop_assert_eq!(a, b, "from_pid must be deterministic for 0x{:04X}", pid);
    }

    // ── Button round-trip ────────────────────────────────────────────────

    /// Buttons encoded in bytes 6-7 must round-trip through parse.
    #[test]
    fn fuzz_button_roundtrip(buttons: u16) {
        let mut data = [0u8; 12];
        data[6] = (buttons & 0xFF) as u8;
        data[7] = (buttons >> 8) as u8;
        let report = parse(&data);
        prop_assert!(report.is_ok(), "12-byte buffer must parse");
        if let Ok(r) = report {
            prop_assert_eq!(r.buttons, buttons,
                "buttons must round-trip: expected 0x{:04X}, got 0x{:04X}", buttons, r.buttons);
        }
    }
}
