//! Property tests for the Cammus HID protocol.
//!
//! Verifies invariants across a wide range of inputs using `proptest`.

use racing_wheel_hid_cammus_protocol as cammus;
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// is_cammus returns true for the official VID + known PIDs.
    #[test]
    fn prop_is_cammus_correct(pid in prop_oneof![
        Just(cammus::PRODUCT_C5),
        Just(cammus::PRODUCT_C12),
    ]) {
        prop_assert!(cammus::is_cammus(cammus::VENDOR_ID, pid));
    }

    /// is_cammus returns false for any non-Cammus VID.
    #[test]
    fn prop_is_cammus_wrong_vid(vid in 0u16..=u16::MAX, pid in 0u16..=u16::MAX) {
        if vid != cammus::VENDOR_ID {
            prop_assert!(
                !cammus::is_cammus(vid, pid),
                "non-Cammus VID {vid:#06X} must not be recognised"
            );
        }
    }

    /// is_cammus returns false for unknown PIDs even with the correct VID.
    #[test]
    fn prop_is_cammus_unknown_pid(pid in 0u16..=u16::MAX) {
        if pid != cammus::PRODUCT_C5 && pid != cammus::PRODUCT_C12 {
            prop_assert!(!cammus::is_cammus(cammus::VENDOR_ID, pid));
        }
    }

    /// parse fails for any slice shorter than 12 bytes.
    #[test]
    fn prop_parse_too_short(len in 0usize..12) {
        let data = vec![0u8; len];
        prop_assert!(
            cammus::parse(&data).is_err(),
            "parse of {len}-byte slice must fail"
        );
    }

    /// parse succeeds for any slice >= 12 bytes.
    #[test]
    fn prop_parse_sufficient_length(extra in 0usize..=52) {
        let data = vec![0u8; 12 + extra];
        prop_assert!(
            cammus::parse(&data).is_ok(),
            "parse of {}-byte slice must succeed",
            data.len()
        );
    }

    /// Steering is always in [−1.0, +1.0] for any raw i16 steering input.
    #[test]
    fn prop_steering_in_bounds(raw in i16::MIN..=i16::MAX) {
        let mut data = [0u8; 64];
        let bytes = raw.to_le_bytes();
        data[0] = bytes[0];
        data[1] = bytes[1];
        let report = cammus::parse(&data).expect("parse must succeed for 64-byte slice");
        prop_assert!(
            report.steering >= -1.0 && report.steering <= 1.0,
            "steering {} out of [-1, 1] for raw {raw}", report.steering
        );
    }

    /// Throttle is always in [0.0, 1.0] for any raw u16 throttle input.
    #[test]
    fn prop_throttle_in_bounds(raw in u16::MIN..=u16::MAX) {
        let mut data = [0u8; 64];
        let bytes = raw.to_le_bytes();
        data[2] = bytes[0];
        data[3] = bytes[1];
        let report = cammus::parse(&data).expect("parse must succeed for 64-byte slice");
        prop_assert!(
            report.throttle >= 0.0 && report.throttle <= 1.0,
            "throttle {} out of [0, 1] for raw {raw}", report.throttle
        );
    }

    /// Brake is always in [0.0, 1.0] for any raw u16 brake input.
    #[test]
    fn prop_brake_in_bounds(raw in u16::MIN..=u16::MAX) {
        let mut data = [0u8; 64];
        let bytes = raw.to_le_bytes();
        data[4] = bytes[0];
        data[5] = bytes[1];
        let report = cammus::parse(&data).expect("parse must succeed for 64-byte slice");
        prop_assert!(
            report.brake >= 0.0 && report.brake <= 1.0,
            "brake {} out of [0, 1] for raw {raw}", report.brake
        );
    }

    /// Clutch is always in [0.0, 1.0] for any raw u16 clutch input.
    #[test]
    fn prop_clutch_in_bounds(raw in u16::MIN..=u16::MAX) {
        let mut data = [0u8; 64];
        let bytes = raw.to_le_bytes();
        data[8] = bytes[0];
        data[9] = bytes[1];
        let report = cammus::parse(&data).expect("parse must succeed for 64-byte slice");
        prop_assert!(
            report.clutch >= 0.0 && report.clutch <= 1.0,
            "clutch {} out of [0, 1] for raw {raw}", report.clutch
        );
    }

    /// Handbrake is always in [0.0, 1.0] for any raw u16 handbrake input.
    #[test]
    fn prop_handbrake_in_bounds(raw in u16::MIN..=u16::MAX) {
        let mut data = [0u8; 64];
        let bytes = raw.to_le_bytes();
        data[10] = bytes[0];
        data[11] = bytes[1];
        let report = cammus::parse(&data).expect("parse must succeed for 64-byte slice");
        prop_assert!(
            report.handbrake >= 0.0 && report.handbrake <= 1.0,
            "handbrake {} out of [0, 1] for raw {raw}", report.handbrake
        );
    }

    /// Button state is round-tripped exactly from bytes 6–7.
    #[test]
    fn prop_buttons_round_trip(buttons in 0u16..=u16::MAX) {
        let mut data = [0u8; 64];
        data[6] = (buttons & 0xFF) as u8;
        data[7] = (buttons >> 8) as u8;
        let report = cammus::parse(&data).expect("parse must succeed for 64-byte slice");
        prop_assert_eq!(report.buttons, buttons);
    }

    /// CammusModel::from_pid returns Some for known PIDs.
    #[test]
    fn prop_model_from_known_pid(pid in prop_oneof![
        Just(cammus::PRODUCT_C5),
        Just(cammus::PRODUCT_C12),
    ]) {
        prop_assert!(cammus::CammusModel::from_pid(pid).is_some());
    }

    /// CammusModel::from_pid returns None for unknown PIDs.
    #[test]
    fn prop_model_from_unknown_pid(pid in 0u16..=u16::MAX) {
        if pid != cammus::PRODUCT_C5 && pid != cammus::PRODUCT_C12 {
            prop_assert!(cammus::CammusModel::from_pid(pid).is_none());
        }
    }

    /// max_torque_nm is strictly positive for all known models.
    #[test]
    fn prop_model_max_torque_positive(pid in prop_oneof![
        Just(cammus::PRODUCT_C5),
        Just(cammus::PRODUCT_C12),
    ]) {
        let model = cammus::CammusModel::from_pid(pid).expect("known PID must yield a model");
        prop_assert!(model.max_torque_nm() > 0.0);
    }

    /// name() is non-empty for all known models.
    #[test]
    fn prop_model_name_non_empty(pid in prop_oneof![
        Just(cammus::PRODUCT_C5),
        Just(cammus::PRODUCT_C12),
    ]) {
        let model = cammus::CammusModel::from_pid(pid).expect("known PID must yield a model");
        prop_assert!(!model.name().is_empty());
    }

    /// encode_torque output length is always FFB_REPORT_LEN.
    #[test]
    fn prop_encode_torque_report_len(torque in -100.0f32..=100.0f32) {
        prop_assert_eq!(cammus::encode_torque(torque).len(), cammus::FFB_REPORT_LEN);
    }

    /// encode_torque first byte is always FFB_REPORT_ID.
    #[test]
    fn prop_encode_torque_report_id(torque in -100.0f32..=100.0f32) {
        let report = cammus::encode_torque(torque);
        prop_assert_eq!(report[0], cammus::FFB_REPORT_ID);
    }

    /// encode_torque saturates at ±i16::MAX for out-of-range inputs.
    #[test]
    fn prop_encode_torque_saturation(torque in 1.0f32..=100.0f32) {
        let report = cammus::encode_torque(torque);
        let raw = i16::from_le_bytes([report[1], report[2]]);
        prop_assert_eq!(raw, i16::MAX, "torque >= 1.0 must saturate to i16::MAX");
    }

    /// encode_torque byte 3 (mode) is always MODE_GAME.
    #[test]
    fn prop_encode_torque_mode_byte(torque in -100.0f32..=100.0f32) {
        let report = cammus::encode_torque(torque);
        prop_assert_eq!(report[3], cammus::MODE_GAME);
    }

    /// Reserved bytes [4..8] are always zero.
    #[test]
    fn prop_encode_torque_reserved_zeros(torque in -100.0f32..=100.0f32) {
        let report = cammus::encode_torque(torque);
        prop_assert_eq!(&report[4..], &[0x00u8, 0x00, 0x00, 0x00]);
    }
}
