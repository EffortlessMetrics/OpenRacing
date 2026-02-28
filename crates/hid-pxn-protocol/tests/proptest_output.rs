//! Property-based tests for PXN FFB output report encoding.
//!
//! Uses proptest with 500 cases to verify correctness properties of
//! [`encode_torque`] and [`encode_stop`] independent of specific numeric values.

use proptest::prelude::*;
use racing_wheel_hid_pxn_protocol::{FFB_REPORT_ID, FFB_REPORT_LEN, encode_stop, encode_torque};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded i16 value.
    #[test]
    fn prop_sign_preserved(torque in -1.0f32..=1.0f32) {
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

    /// Any input must produce a report of exactly FFB_REPORT_LEN bytes with the
    /// correct report-ID byte at position 0.
    #[test]
    fn prop_report_structure(torque in -1000.0f32..=1000.0f32) {
        let report = encode_torque(torque);
        prop_assert_eq!(
            report.len(), FFB_REPORT_LEN,
            "report length must be {} for torque={}", FFB_REPORT_LEN, torque
        );
        prop_assert_eq!(
            report[0], FFB_REPORT_ID,
            "byte 0 must be FFB_REPORT_ID ({:#04x}) for torque={}", FFB_REPORT_ID, torque
        );
    }

    /// Reserved bytes [3..FFB_REPORT_LEN] must always be zero.
    #[test]
    fn prop_reserved_bytes_zero(torque in -1000.0f32..=1000.0f32) {
        let report = encode_torque(torque);
        prop_assert_eq!(
            &report[3..], &[0x00u8; 5],
            "reserved bytes must be zero for torque={}", torque
        );
    }

    /// Torque in [−1.0, +1.0] must round-trip through i16 encoding with at most
    /// 1 LSB of quantisation error.
    #[test]
    fn prop_round_trip(torque in -1.0f32..=1.0f32) {
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let decoded = raw as f32 / i16::MAX as f32;
        let max_error = 1.0_f32 / i16::MAX as f32 + f32::EPSILON * 2.0;
        prop_assert!(
            (decoded - torque).abs() <= max_error,
            "torque {torque} round-trips as {decoded} (error {})", (decoded - torque).abs()
        );
    }

    /// Larger absolute torque values must produce larger absolute raw values
    /// (monotonicity), within the normalised [−1.0, +1.0] range.
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..=1.0f32,
        t2 in 0.0f32..=1.0f32,
    ) {
        let r1 = encode_torque(t1);
        let r2 = encode_torque(t2);
        let raw1 = i16::from_le_bytes([r1[1], r1[2]]);
        let raw2 = i16::from_le_bytes([r2[1], r2[2]]);
        if t1 < t2 - 1e-4 {
            prop_assert!(raw1 <= raw2,
                "t1={t1} -> {raw1} should be <= t2={t2} -> {raw2}");
        }
    }

    /// Inputs outside [−1.0, +1.0] must clamp: the encoded i16 must stay
    /// within [−i16::MAX, +i16::MAX].
    #[test]
    fn prop_no_overflow(torque in -1000.0f32..=1000.0f32) {
        let report = encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        // i16 is always <= i16::MAX by type; the meaningful check is the lower bound.
        prop_assert!(raw >= -i16::MAX, "encoded {raw} must not be below -i16::MAX");
    }

    /// encode_stop must always equal encode_torque(0.0).
    #[test]
    fn prop_encode_stop_is_zero(_x in 0u8..=1u8) {
        prop_assert_eq!(encode_stop(), encode_torque(0.0));
    }
}
