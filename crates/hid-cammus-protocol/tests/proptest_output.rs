//! Property-based tests for Cammus FFB output report encoding.
//!
//! Uses proptest with 500 cases to verify correctness properties independent of
//! specific numeric values: sign preservation, round-trip fidelity, report
//! structure (size + report ID), monotonicity, and encode_stop identity.

use proptest::prelude::*;
use racing_wheel_hid_cammus_protocol::{FFB_REPORT_ID, FFB_REPORT_LEN, encode_stop, encode_torque};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded i16 raw value.
    ///
    /// Inputs strictly within (-1.0, 0.0) or (0.0, 1.0) must produce a raw
    /// value with the matching sign.
    #[test]
    fn prop_sign_preserved(torque in -0.999f32..=0.999f32) {
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        if torque > 0.001 {
            prop_assert!(raw > 0,
                "positive torque {torque} encoded as non-positive {raw}");
        } else if torque < -0.001 {
            prop_assert!(raw < 0,
                "negative torque {torque} encoded as non-negative {raw}");
        }
    }

    /// Torque within [-1.0, +1.0] must round-trip through the i16 encoding
    /// with at most 1 LSB of error (i.e. < 1/i16::MAX ≈ 0.00004).
    #[test]
    fn prop_round_trip(torque in -1.0f32..=1.0f32) {
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let decoded = raw as f32 / i16::MAX as f32;
        let error = (torque - decoded).abs();
        // One LSB of i16::MAX is approximately 3e-5; allow a small margin.
        prop_assert!(error < 0.0001,
            "torque {torque} round-trips as {decoded} (error {error})");
    }

    /// The report must always be exactly FFB_REPORT_LEN bytes; byte 0 must be
    /// FFB_REPORT_ID, byte 3 must be MODE_GAME (0x01), bytes 4–7 must be zero.
    #[test]
    fn prop_report_structure(torque in -100.0f32..=100.0f32) {
        let report = encode_torque(torque);
        prop_assert_eq!(report.len(), FFB_REPORT_LEN,
            "report length must equal FFB_REPORT_LEN");
        prop_assert_eq!(report[0], FFB_REPORT_ID,
            "byte 0 must be FFB_REPORT_ID (0x01)");
        prop_assert_eq!(report[3], 0x01u8, "byte 3 (mode) must be MODE_GAME (0x01)");
        prop_assert_eq!(&report[4..], &[0u8; 4], "reserved bytes 4-7 must be zero");
    }

    /// Encoding must be monotone: larger input torque must produce a larger (or
    /// equal) raw value within the valid range.
    #[test]
    fn prop_monotone(
        a in -1.0f32..=1.0f32,
        b in -1.0f32..=1.0f32,
    ) {
        let ra = encode_torque(a);
        let rb = encode_torque(b);
        let raw_a = i16::from_le_bytes([ra[1], ra[2]]);
        let raw_b = i16::from_le_bytes([rb[1], rb[2]]);
        if a < b - 0.001 {
            prop_assert!(raw_a <= raw_b,
                "monotone: torque {a} < {b} but raw {raw_a} > {raw_b}");
        } else if a > b + 0.001 {
            prop_assert!(raw_a >= raw_b,
                "monotone: torque {a} > {b} but raw {raw_a} < {raw_b}");
        }
    }

    /// `encode_stop()` must always be identical to `encode_torque(0.0)`.
    #[test]
    fn prop_encode_stop_equals_zero_torque(_dummy in 0u8..=1u8) {
        let stop = encode_stop();
        let zero = encode_torque(0.0);
        prop_assert_eq!(stop, zero,
            "encode_stop() must equal encode_torque(0.0)");
    }
}
