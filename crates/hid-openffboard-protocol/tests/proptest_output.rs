//! Property-based tests for OpenFFBoard torque encoding and output report generation.
//!
//! Uses proptest with 500 cases to verify correctness properties independent of
//! specific numeric values.

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::output::MAX_TORQUE_SCALE;
use racing_wheel_hid_openffboard_protocol::{
    OpenFFBoardTorqueEncoder, CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded raw value.
    #[test]
    fn prop_sign_preserved(torque in -1.0f32..=1.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        if torque > 0.0001 {
            prop_assert!(raw > 0,
                "positive torque {torque} encoded as non-positive {raw}");
        } else if torque < -0.0001 {
            prop_assert!(raw < 0,
                "negative torque {torque} encoded as non-negative {raw}");
        }
    }

    /// Encoded raw value must always stay within [-MAX_TORQUE_SCALE, MAX_TORQUE_SCALE],
    /// even for inputs well outside [-1.0, 1.0].
    #[test]
    fn prop_no_overflow(torque in -1000.0f32..=1000.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(
            (-MAX_TORQUE_SCALE..=MAX_TORQUE_SCALE).contains(&raw),
            "raw {raw} is outside [-{MAX_TORQUE_SCALE}, {MAX_TORQUE_SCALE}]"
        );
    }

    /// Torque within [-1.0, 1.0] must round-trip through the i16 encoding
    /// with at most 1/MAX_TORQUE_SCALE of error.
    #[test]
    fn prop_round_trip(torque in -1.0f32..=1.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
        let error = (torque - decoded).abs();
        prop_assert!(
            error < 1.0 / MAX_TORQUE_SCALE as f32,
            "torque {torque} round-trips as {decoded} (error {error})"
        );
    }

    /// For any torque input, the report must have exactly CONSTANT_FORCE_REPORT_LEN bytes
    /// and byte 0 must equal CONSTANT_FORCE_REPORT_ID.
    #[test]
    fn prop_report_structure(torque in -2.0f32..=2.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        prop_assert_eq!(
            report.len(), CONSTANT_FORCE_REPORT_LEN,
            "report.len()={} != CONSTANT_FORCE_REPORT_LEN={}",
            report.len(), CONSTANT_FORCE_REPORT_LEN
        );
        prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID,
            "byte 0 must be CONSTANT_FORCE_REPORT_ID ({:#04x})", CONSTANT_FORCE_REPORT_ID);
    }

    /// Larger positive torque values must produce larger raw values (monotonicity).
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..1.0f32,
        t2 in 0.0f32..1.0f32,
    ) {
        let enc = OpenFFBoardTorqueEncoder;
        let r1 = i16::from_le_bytes({let r = enc.encode(t1); [r[1], r[2]]});
        let r2 = i16::from_le_bytes({let r = enc.encode(t2); [r[1], r[2]]});
        if t1 < t2 - 0.0001 {
            prop_assert!(r1 <= r2,
                "t1={t1} -> {r1} should be <= t2={t2} -> {r2}");
        }
    }
}
