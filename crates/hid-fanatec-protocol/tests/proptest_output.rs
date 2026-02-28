//! Property-based tests for Fanatec output report encoding.
//!
//! Uses proptest with 500 cases to verify correctness properties of the
//! constant-force FFB encoder independent of specific numeric values.

use proptest::prelude::*;
use racing_wheel_hid_fanatec_protocol::{CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded raw i16 value.
    #[test]
    fn prop_sign_preserved(
        torque in -50.0f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        if torque > 0.01 {
            prop_assert!(raw > 0,
                "positive torque {torque} (max {max_torque}) encoded as non-positive {raw}");
        } else if torque < -0.01 {
            prop_assert!(raw < 0,
                "negative torque {torque} (max {max_torque}) encoded as non-negative {raw}");
        }
    }

    /// Encoded report length must always equal CONSTANT_FORCE_REPORT_LEN (8).
    #[test]
    fn prop_report_length(
        torque in -1000.0f32..=1000.0f32,
        max_torque in 0.0f32..=100.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let len = encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(len, CONSTANT_FORCE_REPORT_LEN,
            "encode() must return CONSTANT_FORCE_REPORT_LEN={}, got {}", CONSTANT_FORCE_REPORT_LEN, len);
    }

    /// Report byte 0 must always be the FFB output report ID (0x01) and
    /// byte 1 must always be the CONSTANT_FORCE command (0x01).
    #[test]
    fn prop_report_header(
        torque in -1000.0f32..=1000.0f32,
        max_torque in 0.0f32..=100.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(out[0], 0x01, "byte 0 must be FFB report ID 0x01");
        prop_assert_eq!(out[1], 0x01, "byte 1 must be CONSTANT_FORCE command 0x01");
    }

    /// Torque within ±max_torque must round-trip through the i16 encoding
    /// with at most (max_torque / 32767) Nm of error (1-LSB tolerance).
    #[test]
    fn prop_round_trip_accuracy(
        torque in -50.0f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        // Only test torques within the valid range.
        let clamped = torque.clamp(-max_torque, max_torque);
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(clamped, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        // Reconstruct: raw / i16::MAX * max_torque (positive side).
        let decoded = if raw >= 0 {
            raw as f32 / i16::MAX as f32 * max_torque
        } else {
            raw as f32 / (-(i16::MIN as f32)) * max_torque
        };
        let tolerance = max_torque / i16::MAX as f32 + 1e-4;
        let error = (clamped - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {clamped} round-trips as {decoded} (error {error} > tolerance {tolerance})"
        );
    }

    /// Larger absolute torque values must produce larger absolute raw values
    /// (monotonicity), within the in-range region.
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..=50.0f32,
        t2 in 0.0f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out1 = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut out2 = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(t1.min(max_torque), 0, &mut out1);
        encoder.encode(t2.min(max_torque), 0, &mut out2);
        let r1 = i16::from_le_bytes([out1[2], out1[3]]);
        let r2 = i16::from_le_bytes([out2[2], out2[3]]);
        if t1 < t2 - 0.01 {
            prop_assert!(
                r1 <= r2,
                "t1={t1} → {r1} should be ≤ t2={t2} → {r2} (max_torque={max_torque})"
            );
        }
    }

    /// Positive and negative torques of equal magnitude must produce raw values
    /// that are mirror images (|pos_raw| ≈ |neg_raw|, within 1 LSB).
    #[test]
    fn prop_sign_symmetry(
        torque in 0.01f32..=50.0f32,
        max_torque in 0.1f32..=50.0f32,
    ) {
        let clamped = torque.min(max_torque);
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut pos_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut neg_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(clamped, 0, &mut pos_out);
        encoder.encode(-clamped, 0, &mut neg_out);
        let pos_raw = i16::from_le_bytes([pos_out[2], pos_out[3]]);
        let neg_raw = i16::from_le_bytes([neg_out[2], neg_out[3]]);
        let diff = (pos_raw as i32 + neg_raw as i32).unsigned_abs();
        prop_assert!(
            diff <= 1,
            "pos_raw={pos_raw} and neg_raw={neg_raw} should be symmetric (diff={diff})"
        );
    }

    /// Reserved bytes 4–7 must always be zero.
    #[test]
    fn prop_reserved_bytes_zero(
        torque in -1000.0f32..=1000.0f32,
        max_torque in 0.0f32..=100.0f32,
    ) {
        let encoder = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
        encoder.encode(torque, 0, &mut out);
        prop_assert_eq!(out[4], 0x00, "reserved byte 4 must be zero");
        prop_assert_eq!(out[5], 0x00, "reserved byte 5 must be zero");
        prop_assert_eq!(out[6], 0x00, "reserved byte 6 must be zero");
        prop_assert_eq!(out[7], 0x00, "reserved byte 7 must be zero");
    }
}
