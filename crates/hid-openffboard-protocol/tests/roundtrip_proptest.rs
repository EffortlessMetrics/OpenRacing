//! Roundtrip property-based tests for the OpenFFBoard HID protocol.
//!
//! Verifies encode→decode and decode→encode consistency for:
//! - Torque encoder: normalised float → bytes → raw i16 → float roundtrip
//! - Enable FFB report: bool → bytes → bool roundtrip
//! - Gain report: u8 → bytes → u8 roundtrip
//! - Variant identification roundtrip
#![allow(clippy::redundant_closure)]

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::{
    build_enable_ffb, build_set_gain, is_openffboard_product,
    output::{ENABLE_FFB_REPORT_ID, MAX_TORQUE_SCALE},
    OpenFFBoardTorqueEncoder, OpenFFBoardVariant, CONSTANT_FORCE_REPORT_ID,
    CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
};

// ── Torque encoder roundtrip ────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Normalised torque [-1.0, 1.0] must round-trip through the i16 encoding
    /// with at most 1 LSB of error on the ±10000 scale.
    #[test]
    fn prop_torque_roundtrip(torque_norm in -1.0_f32..=1.0_f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque_norm);
        prop_assert_eq!(report.len(), CONSTANT_FORCE_REPORT_LEN);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        let decoded = raw as f32 / MAX_TORQUE_SCALE as f32;
        let tolerance = 1.0 / MAX_TORQUE_SCALE as f32 + 1e-5;
        let error = (torque_norm - decoded).abs();
        prop_assert!(
            error <= tolerance,
            "torque {torque_norm} roundtrips as {decoded} (err {error} > tol {tolerance})"
        );
    }

    /// Report byte 0 must always be the constant force report ID.
    #[test]
    fn prop_torque_report_id(torque_norm in -2.0_f32..=2.0_f32) {
        let report = OpenFFBoardTorqueEncoder.encode(torque_norm);
        prop_assert_eq!(report[0], CONSTANT_FORCE_REPORT_ID,
            "byte 0 must be CONSTANT_FORCE_REPORT_ID");
    }

    /// Reserved bytes [3..5] must always be zero.
    #[test]
    fn prop_torque_reserved_zero(torque_norm in -2.0_f32..=2.0_f32) {
        let report = OpenFFBoardTorqueEncoder.encode(torque_norm);
        prop_assert_eq!(report[3], 0x00, "byte 3 must be zero");
        prop_assert_eq!(report[4], 0x00, "byte 4 must be zero");
    }

    /// Torque values outside [-1.0, 1.0] must be clamped, not overflow.
    #[test]
    fn prop_torque_clamped(torque in -100.0_f32..=100.0_f32) {
        let report = OpenFFBoardTorqueEncoder.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(
            (-MAX_TORQUE_SCALE..=MAX_TORQUE_SCALE).contains(&raw),
            "raw {raw} must be within ±{MAX_TORQUE_SCALE}"
        );
    }

    /// Sign of the input must be preserved in the encoded value.
    #[test]
    fn prop_torque_sign_preserved(torque_norm in -1.0_f32..=1.0_f32) {
        let report = OpenFFBoardTorqueEncoder.encode(torque_norm);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        if torque_norm > 0.001 {
            prop_assert!(raw > 0, "positive torque must produce positive raw");
        } else if torque_norm < -0.001 {
            prop_assert!(raw < 0, "negative torque must produce negative raw");
        }
    }
}

// ── Enable FFB report roundtrip ─────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// enable FFB bool must round-trip through byte [1].
    #[test]
    fn prop_enable_ffb_roundtrip(enabled: bool) {
        let report = build_enable_ffb(enabled);
        prop_assert_eq!(report.len(), 3);
        prop_assert_eq!(report[0], ENABLE_FFB_REPORT_ID);
        let decoded = report[1] != 0;
        prop_assert_eq!(decoded, enabled, "enabled must round-trip");
        prop_assert_eq!(report[2], 0x00, "byte 2 must be zero");
    }

    /// Gain u8 must round-trip through byte [1].
    #[test]
    fn prop_gain_roundtrip(gain: u8) {
        let report = build_set_gain(gain);
        prop_assert_eq!(report.len(), 3);
        prop_assert_eq!(report[0], GAIN_REPORT_ID);
        prop_assert_eq!(report[1], gain, "gain must round-trip");
        prop_assert_eq!(report[2], 0x00, "byte 2 must be zero");
    }

    /// Main variant product ID must be recognized; Alternate must not.
    #[test]
    fn prop_variant_product_id_recognized(
        variant in prop_oneof![
            Just(OpenFFBoardVariant::Main),
            Just(OpenFFBoardVariant::Alternate),
        ]
    ) {
        let pid = variant.product_id();
        match variant {
            OpenFFBoardVariant::Main => {
                prop_assert!(is_openffboard_product(pid),
                    "Main variant {:?} PID {:#06X} must be recognized", variant, pid);
            }
            OpenFFBoardVariant::Alternate => {
                prop_assert!(!is_openffboard_product(pid),
                    "Alternate variant {:?} PID {:#06X} must NOT be recognized", variant, pid);
            }
        }
    }

    /// Random product IDs not matching known PIDs must return false.
    #[test]
    fn prop_unknown_pid_not_recognized(pid: u16) {
        let known = OpenFFBoardVariant::ALL.iter().any(|v| v.product_id() == pid);
        if !known {
            prop_assert!(!is_openffboard_product(pid),
                "unknown PID {:#06X} must not be recognized", pid);
        }
    }
}
