//! Property tests for the Asetek HID protocol.
//!
//! Verifies invariants across a wide range of inputs using `proptest`.

use hid_asetek_protocol as asetek;
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// is_asetek_device returns true only for the official Asetek vendor ID.
    #[test]
    fn prop_is_asetek_device_correct_vid(_x in 0u8..=1u8) {
        prop_assert!(asetek::is_asetek_device(asetek::ASETEK_VENDOR_ID));
    }

    /// is_asetek_device returns false for any VID other than ASETEK_VENDOR_ID.
    #[test]
    fn prop_is_asetek_device_wrong_vid(vid in 0u16..=u16::MAX) {
        if vid != asetek::ASETEK_VENDOR_ID {
            prop_assert!(!asetek::is_asetek_device(vid));
        }
    }

    /// asetek_model_from_info returns Unknown for any non-Asetek VID.
    #[test]
    fn prop_model_from_info_wrong_vid(vid in 0u16..=u16::MAX, pid in 0u16..=u16::MAX) {
        if vid != asetek::ASETEK_VENDOR_ID {
            let model = asetek::asetek_model_from_info(vid, pid);
            prop_assert_eq!(
                model,
                asetek::AsetekModel::Unknown,
                "non-Asetek VID {:#06X} must always yield Unknown",
                vid
            );
        }
    }

    /// Known PIDs with the correct VID never map to Unknown.
    #[test]
    fn prop_known_pids_are_not_unknown(pid in prop_oneof![
        Just(asetek::ASETEK_FORTE_PID),
        Just(asetek::ASETEK_INVICTA_PID),
        Just(asetek::ASETEK_LAPRIMA_PID),
        Just(asetek::ASETEK_TONY_KANNAN_PID),
    ]) {
        let model = asetek::asetek_model_from_info(asetek::ASETEK_VENDOR_ID, pid);
        prop_assert_ne!(
            model,
            asetek::AsetekModel::Unknown,
            "known PID {:#06X} must not map to Unknown",
            pid
        );
    }

    /// Unknown PIDs with the correct VID yield Unknown.
    #[test]
    fn prop_unknown_pid_yields_unknown(pid in 0u16..=u16::MAX) {
        let known = [
            asetek::ASETEK_FORTE_PID,
            asetek::ASETEK_INVICTA_PID,
            asetek::ASETEK_LAPRIMA_PID,
            asetek::ASETEK_TONY_KANNAN_PID,
        ];
        if !known.contains(&pid) {
            let model = asetek::asetek_model_from_info(asetek::ASETEK_VENDOR_ID, pid);
            prop_assert_eq!(model, asetek::AsetekModel::Unknown);
        }
    }

    /// Output report build always returns at least REPORT_SIZE_OUTPUT bytes.
    #[test]
    fn prop_output_report_length(seq in 0u16..=u16::MAX, torque in -100.0f32..=100.0f32) {
        let data = asetek::AsetekOutputReport::new(seq)
            .with_torque(torque)
            .build()
            .expect("build must not fail");
        prop_assert!(
            data.len() >= asetek::REPORT_SIZE_OUTPUT,
            "output len {} < REPORT_SIZE_OUTPUT {}",
            data.len(),
            asetek::REPORT_SIZE_OUTPUT
        );
    }

    /// Torque above MAX_TORQUE_NM saturates to the same cNm as MAX_TORQUE_NM.
    #[test]
    fn prop_torque_clamped_above_max(torque in 20.0f32..=100.0f32) {
        let clamped = asetek::AsetekOutputReport::new(0).with_torque(torque);
        let at_max = asetek::AsetekOutputReport::new(0).with_torque(asetek::MAX_TORQUE_NM);
        prop_assert_eq!(
            clamped.torque_cNm,
            at_max.torque_cNm,
            "torque {} above MAX must clamp",
            torque
        );
    }

    /// Torque below -MAX_TORQUE_NM saturates to the same cNm as -MAX_TORQUE_NM.
    #[test]
    fn prop_torque_clamped_below_min(torque in -100.0f32..=-20.0f32) {
        let clamped = asetek::AsetekOutputReport::new(0).with_torque(torque);
        let at_min = asetek::AsetekOutputReport::new(0).with_torque(-asetek::MAX_TORQUE_NM);
        prop_assert_eq!(
            clamped.torque_cNm,
            at_min.torque_cNm,
            "torque {} below -MAX must clamp",
            torque
        );
    }

    /// In-range torques encode to cNm = round(torque * 100) within ±1 unit.
    #[test]
    fn prop_torque_encoding_in_range(torque in -20.0f32..=20.0f32) {
        let report = asetek::AsetekOutputReport::new(0).with_torque(torque);
        let expected = (torque * 100.0) as i16;
        prop_assert!(
            (report.torque_cNm - expected).abs() <= 1,
            "torque_cNm={} expected≈{} for torque={torque}",
            report.torque_cNm, expected
        );
    }

    /// Input parse fails for any slice shorter than 16 bytes.
    #[test]
    fn prop_input_parse_too_short(len in 0usize..16) {
        let data = vec![0u8; len];
        prop_assert!(
            asetek::AsetekInputReport::parse(&data).is_err(),
            "parse of {len}-byte slice must fail"
        );
    }

    /// Input parse succeeds for any slice of >= 16 bytes.
    #[test]
    fn prop_input_parse_sufficient_length(extra in 0usize..=48) {
        let data = vec![0u8; 16 + extra];
        prop_assert!(
            asetek::AsetekInputReport::parse(&data).is_ok(),
            "parse of {}-byte slice must succeed",
            data.len()
        );
    }

    /// wheel_angle_degrees matches the definition: raw / 1000.0.
    #[test]
    fn prop_wheel_angle_degrees_scaling(angle in i32::MIN..=i32::MAX) {
        let report = asetek::AsetekInputReport {
            wheel_angle: angle,
            ..Default::default()
        };
        let expected = angle as f32 / 1000.0;
        prop_assert_eq!(
            report.wheel_angle_degrees(),
            expected,
            "wheel_angle_degrees mismatch for angle={}",
            angle
        );
    }

    /// applied_torque_nm matches the definition: raw / 100.0.
    #[test]
    fn prop_applied_torque_nm_scaling(raw_torque in i16::MIN..=i16::MAX) {
        let report = asetek::AsetekInputReport {
            torque: raw_torque,
            ..Default::default()
        };
        let expected = raw_torque as f32 / 100.0;
        prop_assert_eq!(
            report.applied_torque_nm(),
            expected,
            "applied_torque_nm mismatch for raw={}",
            raw_torque
        );
    }

    /// All known-model max_torque_nm values are strictly positive.
    #[test]
    fn prop_model_max_torque_positive(pid in prop_oneof![
        Just(asetek::ASETEK_FORTE_PID),
        Just(asetek::ASETEK_INVICTA_PID),
        Just(asetek::ASETEK_LAPRIMA_PID),
        Just(asetek::ASETEK_TONY_KANNAN_PID),
    ]) {
        let model = asetek::AsetekModel::from_product_id(pid);
        prop_assert!(
            model.max_torque_nm() > 0.0,
            "{model:?} max_torque_nm must be positive"
        );
    }

    /// display_name for any model is non-empty.
    #[test]
    fn prop_display_name_non_empty(pid in prop_oneof![
        Just(asetek::ASETEK_FORTE_PID),
        Just(asetek::ASETEK_INVICTA_PID),
        Just(asetek::ASETEK_LAPRIMA_PID),
        Just(asetek::ASETEK_TONY_KANNAN_PID),
    ]) {
        let model = asetek::AsetekModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty());
    }
}
