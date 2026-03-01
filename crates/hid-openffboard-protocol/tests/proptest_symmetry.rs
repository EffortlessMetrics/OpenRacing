//! Property-based tests for OpenFFBoard torque encoding symmetry and determinism.
//!
//! Complements property_tests.rs and proptest_output.rs by testing antisymmetry,
//! determinism, monotonicity in the negative range, and Default encoder parity.

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::{
    OpenFFBoardTorqueEncoder, CONSTANT_FORCE_REPORT_ID,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Encoding must be antisymmetric: raw(t) == -raw(-t) for t in [-1, 1].
    #[test]
    fn prop_antisymmetry(torque in -1.0f32..=1.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let pos_report = enc.encode(torque);
        let neg_report = enc.encode(-torque);
        let pos_raw = i16::from_le_bytes([pos_report[1], pos_report[2]]);
        let neg_raw = i16::from_le_bytes([neg_report[1], neg_report[2]]);
        prop_assert_eq!(
            pos_raw, -neg_raw,
            "encode({}) raw={} should equal -encode({}) raw={}",
            torque, pos_raw, -torque, neg_raw
        );
    }

    /// Encoding must be deterministic: same input always produces same output.
    #[test]
    fn prop_encode_deterministic(torque in -2.0f32..=2.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let a = enc.encode(torque);
        let b = enc.encode(torque);
        prop_assert_eq!(a, b,
            "encode({}) must be deterministic", torque);
    }

    /// Monotonicity in the negative range: more negative torque → smaller raw value.
    #[test]
    fn prop_monotone_negative(
        t1 in -1.0f32..0.0f32,
        t2 in -1.0f32..0.0f32,
    ) {
        let enc = OpenFFBoardTorqueEncoder;
        let r1 = i16::from_le_bytes({ let r = enc.encode(t1); [r[1], r[2]] });
        let r2 = i16::from_le_bytes({ let r = enc.encode(t2); [r[1], r[2]] });
        if t1 < t2 - 0.0001 {
            prop_assert!(r1 <= r2,
                "t1={t1} -> {r1} should be <= t2={t2} -> {r2} (monotone in negative range)");
        }
    }

    /// The Default encoder must produce the same results as a manually constructed one.
    #[test]
    fn prop_default_encoder_consistent(torque in -2.0f32..=2.0f32) {
        let default_enc = OpenFFBoardTorqueEncoder::default();
        let manual_enc = OpenFFBoardTorqueEncoder;
        prop_assert_eq!(
            default_enc.encode(torque),
            manual_enc.encode(torque),
            "Default and manual encoders must produce identical output"
        );
    }

    /// Encoding zero torque must produce all-zero payload bytes.
    #[test]
    fn prop_zero_torque_always_zero(_unused: u8) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(0.0);
        prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID);
        prop_assert_eq!(report[1], 0, "torque lo byte must be 0");
        prop_assert_eq!(report[2], 0, "torque hi byte must be 0");
        prop_assert_eq!(report[3], 0, "reserved byte 3 must be 0");
        prop_assert_eq!(report[4], 0, "reserved byte 4 must be 0");
    }

    /// Full-scale torque (±1.0) must saturate at ±10000.
    #[test]
    fn prop_full_scale_saturates(sign in prop::bool::ANY) {
        let enc = OpenFFBoardTorqueEncoder;
        let input = if sign { 1.0f32 } else { -1.0f32 };
        let report = enc.encode(input);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let expected = if sign { 10_000i16 } else { -10_000i16 };
        prop_assert_eq!(raw, expected,
            "full-scale input {} must encode as {}, got {}", input, expected, raw);
    }
}
