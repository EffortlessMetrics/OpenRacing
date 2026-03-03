//! Property-based tests for Simucube torque encoding and output report generation.
//!
//! Uses proptest with 500 cases to verify correctness properties independent of
//! specific numeric values.

use hid_simucube_protocol::{MAX_TORQUE_NM, SimucubeOutputReport};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign of the input torque must be preserved in the encoded cNm value.
    #[test]
    fn prop_sign_preserved(torque in -MAX_TORQUE_NM..=MAX_TORQUE_NM) {
        let report = SimucubeOutputReport::new(0).with_torque(torque);
        if torque > 0.01 {
            prop_assert!(report.torque_cNm > 0,
                "positive torque {torque} encoded as non-positive {}", report.torque_cNm);
        } else if torque < -0.01 {
            prop_assert!(report.torque_cNm < 0,
                "negative torque {torque} encoded as non-negative {}", report.torque_cNm);
        }
    }

    /// Encoded value must never exceed the i16 safe range for the maximum torque.
    #[test]
    fn prop_no_overflow(torque in -1000.0f32..=1000.0f32) {
        // with_torque clamps, so build() should always succeed
        let report = SimucubeOutputReport::new(0).with_torque(torque);
        let result = report.build();
        prop_assert!(result.is_ok());
    }

    /// Torque within ±MAX_TORQUE_NM must round-trip through the cNm encoding
    /// with at most 0.01 Nm of error.
    #[test]
    fn prop_round_trip(torque in -MAX_TORQUE_NM..=MAX_TORQUE_NM) {
        let report = SimucubeOutputReport::new(0).with_torque(torque);
        let decoded_nm = report.torque_cNm as f32 / 100.0;
        let error = (torque - decoded_nm).abs();
        prop_assert!(error < 0.01,
            "torque {torque} round-trips as {decoded_nm} (error {error})");
    }

    /// For any sequence number, the built report must have exactly REPORT_SIZE_OUTPUT bytes
    /// and the first byte must be 0x01 (report ID).
    #[test]
    fn prop_report_structure(seq in 0u16..=u16::MAX) {
        use hid_simucube_protocol::REPORT_SIZE_OUTPUT;
        let report = SimucubeOutputReport::new(seq);
        let data = report.build().map_err(|e| {
            TestCaseError::fail(format!("build should succeed: {e:?}"))
        })?;
        prop_assert_eq!(data.len(), REPORT_SIZE_OUTPUT,
            "data.len()={} != REPORT_SIZE_OUTPUT={}", data.len(), REPORT_SIZE_OUTPUT);
        prop_assert_eq!(data[0], 0x01, "first byte must be report ID 0x01");
    }

    /// Larger absolute torque values must produce larger absolute cNm values
    /// (monotonicity), within the clamped range.
    #[test]
    fn prop_monotone_positive(
        t1 in 0.0f32..MAX_TORQUE_NM,
        t2 in 0.0f32..MAX_TORQUE_NM,
    ) {
        let r1 = SimucubeOutputReport::new(0).with_torque(t1);
        let r2 = SimucubeOutputReport::new(0).with_torque(t2);
        if t1 < t2 - 0.01 {
            prop_assert!(r1.torque_cNm <= r2.torque_cNm,
                "t1={t1} -> {} should be <= t2={t2} -> {}", r1.torque_cNm, r2.torque_cNm);
        }
    }

    /// Effect type and parameter must round-trip through build: the bytes
    /// at positions 8 (effect_type) and 9–10 (parameter LE) must match.
    #[test]
    fn prop_effect_build_round_trip(
        effect_idx in 0u8..=10u8,
        parameter: u16,
        seq: u16,
    ) {
        let effect_type = match effect_idx {
            0 => hid_simucube_protocol::EffectType::None,
            1 => hid_simucube_protocol::EffectType::Constant,
            2 => hid_simucube_protocol::EffectType::Ramp,
            3 => hid_simucube_protocol::EffectType::Square,
            4 => hid_simucube_protocol::EffectType::Sine,
            5 => hid_simucube_protocol::EffectType::Triangle,
            6 => hid_simucube_protocol::EffectType::SawtoothUp,
            7 => hid_simucube_protocol::EffectType::SawtoothDown,
            8 => hid_simucube_protocol::EffectType::Spring,
            9 => hid_simucube_protocol::EffectType::Damper,
            _ => hid_simucube_protocol::EffectType::Friction,
        };
        let report = SimucubeOutputReport::new(seq)
            .with_effect(effect_type, parameter);
        let data = report.build().map_err(|e| {
            TestCaseError::fail(format!("build must succeed: {e:?}"))
        })?;
        // Layout: [0]=report_id, [1-2]=seq, [3-4]=torque, [5]=r, [6]=g, [7]=b,
        //         [8]=effect_type, [9-10]=parameter
        prop_assert_eq!(data[8], effect_idx.min(10),
            "effect_type byte must match");
        let recovered_param = u16::from_le_bytes([data[9], data[10]]);
        prop_assert_eq!(recovered_param, parameter,
            "effect parameter must round-trip via LE bytes");
        let recovered_seq = u16::from_le_bytes([data[1], data[2]]);
        prop_assert_eq!(recovered_seq, seq,
            "sequence must round-trip via LE bytes");
    }
}
