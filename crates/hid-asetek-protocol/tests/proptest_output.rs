//! Property-based tests for Asetek torque encoding and output report generation.
//!
//! Covers sign preservation, monotonicity, report structure/ID stability,
//! and round-trip accuracy for the output report builder.

use hid_asetek_protocol as asetek;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded cNm value.
    #[test]
    fn prop_sign_preserved(torque in -27.0f32..=27.0f32) {
        let report = asetek::AsetekOutputReport::new(0).with_torque(torque);
        if torque > 0.01 {
            prop_assert!(report.torque_cNm > 0,
                "positive torque {torque} must encode as positive cNm, got {}",
                report.torque_cNm);
        } else if torque < -0.01 {
            prop_assert!(report.torque_cNm < 0,
                "negative torque {torque} must encode as negative cNm, got {}",
                report.torque_cNm);
        }
    }

    /// Zero torque must encode to exactly zero cNm.
    #[test]
    fn prop_zero_torque_is_zero(_x in 0u8..1u8) {
        let report = asetek::AsetekOutputReport::new(0).with_torque(0.0);
        prop_assert_eq!(report.torque_cNm, 0,
            "zero torque must produce zero cNm");
    }

    /// Monotonicity: if t1 < t2, then cNm(t1) <= cNm(t2).
    #[test]
    fn prop_monotone_torque(
        t1 in -20.0f32..=20.0f32,
        t2 in -20.0f32..=20.0f32,
    ) {
        let r1 = asetek::AsetekOutputReport::new(0).with_torque(t1);
        let r2 = asetek::AsetekOutputReport::new(0).with_torque(t2);
        if t1 < t2 - 0.02 {
            prop_assert!(r1.torque_cNm <= r2.torque_cNm,
                "t1={t1} -> {} should be <= t2={t2} -> {}",
                r1.torque_cNm, r2.torque_cNm);
        }
    }

    /// Built output report has exactly REPORT_SIZE_OUTPUT bytes.
    #[test]
    fn prop_report_size_constant(seq in 0u16..=u16::MAX, torque in -50.0f32..=50.0f32) {
        let result = asetek::AsetekOutputReport::new(seq)
            .with_torque(torque)
            .build();
        prop_assert!(result.is_ok(), "build must succeed");
        if let Ok(data) = result {
            prop_assert_eq!(
                data.len(), asetek::REPORT_SIZE_OUTPUT,
                "report must be exactly {} bytes, got {}",
                asetek::REPORT_SIZE_OUTPUT, data.len()
            );
        }
    }

    /// Sequence field in the built report matches the constructor argument
    /// (bytes [0:2] little-endian).
    #[test]
    fn prop_sequence_preserved(seq in 0u16..=u16::MAX) {
        let result = asetek::AsetekOutputReport::new(seq)
            .with_torque(0.0)
            .build();
        prop_assert!(result.is_ok(), "build must succeed");
        if let Ok(data) = result {
            let parsed_seq = u16::from_le_bytes([data[0], data[1]]);
            prop_assert_eq!(parsed_seq, seq,
                "sequence {} not preserved in report bytes", seq);
        }
    }

    /// Torque field in the built report matches the torque_cNm struct field
    /// (bytes [2:4] little-endian i16).
    #[test]
    fn prop_torque_bytes_match_field(torque in -20.0f32..=20.0f32) {
        let report = asetek::AsetekOutputReport::new(0).with_torque(torque);
        let result = report.build();
        prop_assert!(result.is_ok(), "build must succeed");
        if let Ok(data) = result {
            let raw = i16::from_le_bytes([data[2], data[3]]);
            prop_assert_eq!(raw, report.torque_cNm,
                "torque bytes in report must match torque_cNm field");
        }
    }

    /// Reserved bytes (after header fields) in the built report are all zero.
    #[test]
    fn prop_reserved_bytes_zero(seq in 0u16..=u16::MAX, torque in -20.0f32..=20.0f32) {
        let result = asetek::AsetekOutputReport::new(seq)
            .with_torque(torque)
            .build();
        prop_assert!(result.is_ok(), "build must succeed");
        if let Ok(data) = result {
            // Bytes 0-1: sequence, 2-3: torque, 4: led_mode, 5: led_value
            // Bytes 6..REPORT_SIZE_OUTPUT: reserved (zero-padded)
            for (i, &byte) in data.iter().enumerate().skip(6) {
                prop_assert_eq!(byte, 0,
                    "reserved byte at index {} must be 0, got {}", i, byte);
            }
        }
    }

    /// Sign symmetry: encoding +T and -T should produce cNm values that are
    /// negatives of each other.
    #[test]
    fn prop_sign_symmetry(torque in 0.01f32..=27.0f32) {
        let pos = asetek::AsetekOutputReport::new(0).with_torque(torque);
        let neg = asetek::AsetekOutputReport::new(0).with_torque(-torque);
        prop_assert_eq!(pos.torque_cNm, -neg.torque_cNm,
            "encoding +{} and -{} must be symmetric: {} vs {}",
            torque, torque, pos.torque_cNm, neg.torque_cNm);
    }
}
