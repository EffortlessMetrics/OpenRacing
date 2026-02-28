//! Property-based tests for the OpenFFBoard HID protocol encoding.
//!
//! Uses proptest with 500 cases to verify invariants on torque encoding,
//! clamping, reserved bytes, and feature report structure.

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::{
    build_enable_ffb, build_set_gain, is_openffboard_product, OpenFFBoardTorqueEncoder,
    CONSTANT_FORCE_REPORT_ID, CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
};

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// Encoded torque must always be in [-10000, 10000].
    #[test]
    fn prop_torque_always_in_range(torque in -10.0f32..10.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(
            (-10_000..=10_000).contains(&raw),
            "raw torque {raw} must be in [-10000, 10000]"
        );
    }

    /// Report ID (byte 0) must always equal CONSTANT_FORCE_REPORT_ID.
    #[test]
    fn prop_report_id_always_correct(torque in -2.0f32..2.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        prop_assert_eq!(
            report[0],
            CONSTANT_FORCE_REPORT_ID,
            "byte 0 must be CONSTANT_FORCE_REPORT_ID"
        );
    }

    /// Reserved bytes (3–4) must always be zero.
    #[test]
    fn prop_reserved_bytes_always_zero(torque in -2.0f32..2.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        prop_assert_eq!(report[3], 0, "byte 3 (reserved) must be zero");
        prop_assert_eq!(report[4], 0, "byte 4 (reserved) must be zero");
    }

    /// Positive torque must produce a non-negative raw value.
    #[test]
    fn prop_positive_torque_nonneg_raw(torque in 0.001f32..1.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(
            raw >= 0,
            "positive torque {torque} must give raw >= 0, got {raw}"
        );
    }

    /// Negative torque must produce a non-positive raw value.
    #[test]
    fn prop_negative_torque_nonpos_raw(torque in -1.0f32..-0.001f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert!(
            raw <= 0,
            "negative torque {torque} must give raw <= 0, got {raw}"
        );
    }

    /// Any torque > 1.0 must clamp to the same result as 1.0.
    #[test]
    fn prop_clamping_over_one(excess in 0.001f32..100.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let over = enc.encode(1.0 + excess);
        let at_max = enc.encode(1.0);
        prop_assert_eq!(over, at_max, "torque > 1.0 must clamp to encode(1.0)");
    }

    /// Any torque < -1.0 must clamp to the same result as -1.0.
    #[test]
    fn prop_clamping_under_neg_one(excess in 0.001f32..100.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let under = enc.encode(-1.0 - excess);
        let at_min = enc.encode(-1.0);
        prop_assert_eq!(under, at_min, "torque < -1.0 must clamp to encode(-1.0)");
    }

    /// Report length must always equal CONSTANT_FORCE_REPORT_LEN.
    #[test]
    fn prop_report_length_constant(torque in -2.0f32..2.0f32) {
        let enc = OpenFFBoardTorqueEncoder;
        let report = enc.encode(torque);
        prop_assert_eq!(
            report.len(),
            CONSTANT_FORCE_REPORT_LEN,
            "report length must always be CONSTANT_FORCE_REPORT_LEN"
        );
    }

    /// build_set_gain must preserve the gain byte and use the correct report ID.
    #[test]
    fn prop_set_gain_report_structure(gain: u8) {
        let report = build_set_gain(gain);
        prop_assert_eq!(report[0], GAIN_REPORT_ID, "gain report ID must be GAIN_REPORT_ID");
        prop_assert_eq!(report[1], gain, "gain byte must be passed through unchanged");
        prop_assert_eq!(report[2], 0, "gain reserved byte must be zero");
    }

    /// build_enable_ffb must encode the enable flag correctly.
    #[test]
    fn prop_enable_ffb_report_structure(enabled: bool) {
        let report = build_enable_ffb(enabled);
        prop_assert_eq!(report[1], if enabled { 0x01 } else { 0x00 });
        prop_assert_eq!(report[2], 0, "enable reserved byte must be zero");
    }

    /// is_openffboard_product must only recognise the two known PIDs.
    #[test]
    fn prop_product_id_recognition(pid: u16) {
        let known = pid == 0xFFB0 || pid == 0xFFB1;
        prop_assert_eq!(
            is_openffboard_product(pid),
            known,
            "product ID 0x{:04X} should be recognised={}",
            pid,
            known
        );
    }
}
