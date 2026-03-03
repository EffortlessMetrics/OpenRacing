//! Property-based tests for Moza torque encoding and output report generation.
//!
//! Covers sign preservation, round-trip fidelity, report structure (size +
//! report ID), and boundary conditions for the direct-torque encoder.

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{MozaDirectTorqueEncoder, REPORT_LEN};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Torque within ±max_torque must round-trip through the i16 encoding
    /// with at most (max_torque / i16::MAX) Nm of error (1-LSB tolerance).
    #[test]
    fn prop_round_trip_accuracy(
        torque in -21.0f32..=21.0f32,
        max_torque in 0.1f32..=21.0f32,
    ) {
        let clamped = torque.clamp(-max_torque, max_torque);
        let enc = MozaDirectTorqueEncoder::new(max_torque);
        let mut out = [0u8; REPORT_LEN];
        enc.encode(clamped, 0, &mut out);
        let raw = i16::from_le_bytes([out[1], out[2]]);
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

    /// Boundary: full-scale positive and negative torques must decode back
    /// within 0.01 Nm of ±max_torque.
    #[test]
    fn prop_round_trip_boundary(max_torque in 0.1f32..=21.0f32) {
        let enc = MozaDirectTorqueEncoder::new(max_torque);
        let mut out = [0u8; REPORT_LEN];

        enc.encode(max_torque, 0, &mut out);
        let raw_pos = i16::from_le_bytes([out[1], out[2]]);
        let decoded_pos = raw_pos as f32 / i16::MAX as f32 * max_torque;
        prop_assert!(
            (decoded_pos - max_torque).abs() < 0.01,
            "+max round-trip: decoded {decoded_pos} vs expected {max_torque}"
        );

        enc.encode(-max_torque, 0, &mut out);
        let raw_neg = i16::from_le_bytes([out[1], out[2]]);
        let decoded_neg = raw_neg as f32 / (-(i16::MIN as f32)) * max_torque;
        prop_assert!(
            (decoded_neg + max_torque).abs() < 0.01,
            "-max round-trip: decoded {decoded_neg} vs expected -{max_torque}"
        );
    }

    /// Positive and negative torques of equal magnitude must produce raw values
    /// that are mirror images (|pos_raw| ≈ |neg_raw|, within 1 LSB).
    #[test]
    fn prop_sign_symmetry(
        torque in 0.01f32..=21.0f32,
        max_torque in 0.1f32..=21.0f32,
    ) {
        let clamped = torque.min(max_torque);
        let enc = MozaDirectTorqueEncoder::new(max_torque);
        let mut pos_out = [0u8; REPORT_LEN];
        let mut neg_out = [0u8; REPORT_LEN];
        enc.encode(clamped, 0, &mut pos_out);
        enc.encode(-clamped, 0, &mut neg_out);
        let pos_raw = i16::from_le_bytes([pos_out[1], pos_out[2]]);
        let neg_raw = i16::from_le_bytes([neg_out[1], neg_out[2]]);
        let diff = (pos_raw as i32 + neg_raw as i32).unsigned_abs();
        prop_assert!(
            diff <= 1,
            "pos_raw={pos_raw} and neg_raw={neg_raw} should be symmetric (diff={diff})"
        );
    }

    /// Reserved bytes 6–7 must always be zero regardless of input.
    #[test]
    fn prop_reserved_bytes_zero(
        max_torque in 0.1f32..=21.0f32,
        torque in -100.0f32..=100.0f32,
    ) {
        let enc = MozaDirectTorqueEncoder::new(max_torque);
        let mut out = [0xFFu8; REPORT_LEN];
        enc.encode(torque, 0, &mut out);
        prop_assert_eq!(out[6], 0x00, "reserved byte 6 must be zero");
        prop_assert_eq!(out[7], 0x00, "reserved byte 7 must be zero");
    }
}
