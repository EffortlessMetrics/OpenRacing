//! Comprehensive tests for the Asetek HID protocol crate.
//!
//! Covers: input report parsing, output report construction, device identification,
//! encoding precision and safety, edge cases, property tests, and constant validation.

use hid_asetek_protocol::{
    self as asetek, AsetekInputReport, AsetekModel, AsetekOutputReport, ASETEK_FORTE_PEDALS_PID,
    ASETEK_FORTE_PID, ASETEK_INVICTA_PEDALS_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PEDALS_PID,
    ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID, ASETEK_VENDOR_ID, MAX_TORQUE_NM,
    REPORT_SIZE_INPUT, REPORT_SIZE_OUTPUT,
};
use proptest::prelude::*;

// ── Constant validation ──────────────────────────────────────────────────────

#[test]
fn vendor_id_constant() {
    assert_eq!(ASETEK_VENDOR_ID, 0x2433);
}

#[test]
fn lib_vendor_id_constant() {
    assert_eq!(asetek::VENDOR_ID, 0x2433);
}

#[test]
fn report_size_input_is_32() {
    assert_eq!(REPORT_SIZE_INPUT, 32);
}

#[test]
fn report_size_output_is_32() {
    assert_eq!(REPORT_SIZE_OUTPUT, 32);
}

#[test]
fn max_torque_nm_is_27() {
    assert!((MAX_TORQUE_NM - 27.0).abs() < f32::EPSILON);
}

#[test]
fn all_wheelbase_pids_distinct() {
    let pids = [
        ASETEK_FORTE_PID,
        ASETEK_INVICTA_PID,
        ASETEK_LAPRIMA_PID,
        ASETEK_TONY_KANAAN_PID,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(
                pids[i], pids[j],
                "PID index {i} ({:#06X}) collides with index {j} ({:#06X})",
                pids[i], pids[j]
            );
        }
    }
}

#[test]
fn all_pedal_pids_distinct() {
    let pids = [
        ASETEK_INVICTA_PEDALS_PID,
        ASETEK_FORTE_PEDALS_PID,
        ASETEK_LAPRIMA_PEDALS_PID,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j]);
        }
    }
}

#[test]
fn wheelbase_and_pedal_pids_do_not_overlap() {
    let wheelbases = [
        ASETEK_FORTE_PID,
        ASETEK_INVICTA_PID,
        ASETEK_LAPRIMA_PID,
        ASETEK_TONY_KANAAN_PID,
    ];
    let pedals = [
        ASETEK_INVICTA_PEDALS_PID,
        ASETEK_FORTE_PEDALS_PID,
        ASETEK_LAPRIMA_PEDALS_PID,
    ];
    for w in &wheelbases {
        for p in &pedals {
            assert_ne!(w, p, "wheelbase PID {w:#06X} collides with pedal PID {p:#06X}");
        }
    }
}

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn is_asetek_device_correct_vid() {
    assert!(asetek::is_asetek_device(ASETEK_VENDOR_ID));
}

#[test]
fn is_asetek_device_wrong_vid() {
    assert!(!asetek::is_asetek_device(0x0000));
    assert!(!asetek::is_asetek_device(0xFFFF));
    assert!(!asetek::is_asetek_device(0x045B)); // FFBeast
    assert!(!asetek::is_asetek_device(0x1FC9)); // AccuForce
}

#[test]
fn model_from_product_id_all_wheelbases() {
    assert_eq!(AsetekModel::from_product_id(ASETEK_FORTE_PID), AsetekModel::Forte);
    assert_eq!(AsetekModel::from_product_id(ASETEK_INVICTA_PID), AsetekModel::Invicta);
    assert_eq!(AsetekModel::from_product_id(ASETEK_LAPRIMA_PID), AsetekModel::LaPrima);
    assert_eq!(AsetekModel::from_product_id(ASETEK_TONY_KANAAN_PID), AsetekModel::TonyKanaan);
}

#[test]
fn model_from_product_id_all_pedals() {
    assert_eq!(
        AsetekModel::from_product_id(ASETEK_INVICTA_PEDALS_PID),
        AsetekModel::InvictaPedals
    );
    assert_eq!(
        AsetekModel::from_product_id(ASETEK_FORTE_PEDALS_PID),
        AsetekModel::FortePedals
    );
    assert_eq!(
        AsetekModel::from_product_id(ASETEK_LAPRIMA_PEDALS_PID),
        AsetekModel::LaPrimaPedals
    );
}

#[test]
fn model_from_product_id_unknown() {
    assert_eq!(AsetekModel::from_product_id(0x0000), AsetekModel::Unknown);
    assert_eq!(AsetekModel::from_product_id(0xFFFF), AsetekModel::Unknown);
}

#[test]
fn asetek_model_from_info_correct_vid() {
    let model = asetek::asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Forte);
}

#[test]
fn asetek_model_from_info_wrong_vid_returns_unknown() {
    let model = asetek::asetek_model_from_info(0x0000, ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Unknown);
}

#[test]
fn display_name_all_models() {
    assert_eq!(AsetekModel::Forte.display_name(), "Asetek Forte");
    assert_eq!(AsetekModel::Invicta.display_name(), "Asetek Invicta");
    assert_eq!(AsetekModel::LaPrima.display_name(), "Asetek La Prima");
    assert_eq!(AsetekModel::TonyKanaan.display_name(), "Asetek Tony Kanaan Edition");
    assert_eq!(AsetekModel::InvictaPedals.display_name(), "Asetek Invicta Pedals");
    assert_eq!(AsetekModel::FortePedals.display_name(), "Asetek Forte Pedals");
    assert_eq!(AsetekModel::LaPrimaPedals.display_name(), "Asetek La Prima Pedals");
    assert_eq!(AsetekModel::Unknown.display_name(), "Unknown Asetek Device");
}

#[test]
fn max_torque_wheelbase_models() {
    assert_eq!(AsetekModel::Forte.max_torque_nm(), 18.0);
    assert_eq!(AsetekModel::Invicta.max_torque_nm(), 27.0);
    assert_eq!(AsetekModel::LaPrima.max_torque_nm(), 12.0);
    assert_eq!(AsetekModel::TonyKanaan.max_torque_nm(), 27.0);
}

#[test]
fn max_torque_pedal_models_zero() {
    assert_eq!(AsetekModel::InvictaPedals.max_torque_nm(), 0.0);
    assert_eq!(AsetekModel::FortePedals.max_torque_nm(), 0.0);
    assert_eq!(AsetekModel::LaPrimaPedals.max_torque_nm(), 0.0);
}

#[test]
fn max_torque_unknown_model() {
    assert_eq!(AsetekModel::Unknown.max_torque_nm(), 18.0);
}

// ── Input report parsing ─────────────────────────────────────────────────────

#[test]
fn parse_minimum_valid_report() -> Result<(), String> {
    let data = vec![0u8; 16];
    let report = AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.sequence, 0);
    assert_eq!(report.wheel_angle, 0);
    assert_eq!(report.wheel_speed, 0);
    assert_eq!(report.torque, 0);
    assert_eq!(report.temperature, 0);
    assert_eq!(report.status, 0);
    Ok(())
}

#[test]
fn parse_report_with_known_values() -> Result<(), String> {
    let mut data = vec![0u8; 16];
    // sequence = 0x0001 (LE)
    data[0] = 0x01;
    data[1] = 0x00;
    // wheel_angle = 90000 (LE i32) => 0x00015F90
    data[2] = 0x90;
    data[3] = 0x5F;
    data[4] = 0x01;
    data[5] = 0x00;
    // wheel_speed = 100 (LE i16)
    data[6] = 0x64;
    data[7] = 0x00;
    // torque = 1500 (LE i16)
    data[8] = 0xDC;
    data[9] = 0x05;
    // temperature = 42
    data[10] = 42;
    // status = 0x03
    data[11] = 0x03;

    let report = AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.sequence, 1);
    assert_eq!(report.wheel_angle, 90000);
    assert_eq!(report.wheel_speed, 100);
    assert_eq!(report.torque, 1500);
    assert_eq!(report.temperature, 42);
    assert_eq!(report.status, 0x03);
    Ok(())
}

#[test]
fn parse_report_too_short() {
    for len in 0..16 {
        let data = vec![0u8; len];
        assert!(
            AsetekInputReport::parse(&data).is_err(),
            "parse of {len}-byte slice must fail"
        );
    }
}

#[test]
fn parse_report_extra_bytes_ok() -> Result<(), String> {
    let data = vec![0u8; 64];
    let report = AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.sequence, 0);
    Ok(())
}

#[test]
fn parse_report_negative_values() -> Result<(), String> {
    let mut data = vec![0u8; 16];
    // wheel_angle = -90000 (LE i32)
    let angle_bytes = (-90000i32).to_le_bytes();
    data[2..6].copy_from_slice(&angle_bytes);
    // wheel_speed = -500 (LE i16)
    let speed_bytes = (-500i16).to_le_bytes();
    data[6..8].copy_from_slice(&speed_bytes);
    // torque = -2700 (LE i16)
    let torque_bytes = (-2700i16).to_le_bytes();
    data[8..10].copy_from_slice(&torque_bytes);

    let report = AsetekInputReport::parse(&data).map_err(|e| e.to_string())?;
    assert_eq!(report.wheel_angle, -90000);
    assert_eq!(report.wheel_speed, -500);
    assert_eq!(report.torque, -2700);
    Ok(())
}

#[test]
fn wheel_angle_degrees_conversion() {
    let report = AsetekInputReport {
        wheel_angle: 90000,
        ..Default::default()
    };
    assert!((report.wheel_angle_degrees() - 90.0).abs() < 0.01);

    let neg = AsetekInputReport {
        wheel_angle: -180000,
        ..Default::default()
    };
    assert!((neg.wheel_angle_degrees() - (-180.0)).abs() < 0.01);
}

#[test]
fn wheel_angle_degrees_zero() {
    let report = AsetekInputReport {
        wheel_angle: 0,
        ..Default::default()
    };
    assert!((report.wheel_angle_degrees()).abs() < f32::EPSILON);
}

#[test]
fn wheel_speed_rad_s_conversion() {
    let report = AsetekInputReport {
        wheel_speed: 1800,
        ..Default::default()
    };
    let expected = std::f32::consts::PI;
    assert!(
        (report.wheel_speed_rad_s() - expected).abs() < 0.001,
        "1800 raw should be ~PI rad/s, got {}",
        report.wheel_speed_rad_s()
    );
}

#[test]
fn applied_torque_nm_conversion() {
    let report = AsetekInputReport {
        torque: 1500,
        ..Default::default()
    };
    assert!((report.applied_torque_nm() - 15.0).abs() < 0.01);

    let neg = AsetekInputReport {
        torque: -2700,
        ..Default::default()
    };
    assert!((neg.applied_torque_nm() - (-27.0)).abs() < 0.01);
}

#[test]
fn status_flags() {
    let connected_enabled = AsetekInputReport {
        status: 0x03,
        ..Default::default()
    };
    assert!(connected_enabled.is_connected());
    assert!(connected_enabled.is_enabled());

    let connected_only = AsetekInputReport {
        status: 0x01,
        ..Default::default()
    };
    assert!(connected_only.is_connected());
    assert!(!connected_only.is_enabled());

    let enabled_only = AsetekInputReport {
        status: 0x02,
        ..Default::default()
    };
    assert!(!enabled_only.is_connected());
    assert!(enabled_only.is_enabled());

    let neither = AsetekInputReport {
        status: 0x00,
        ..Default::default()
    };
    assert!(!neither.is_connected());
    assert!(!neither.is_enabled());
}

#[test]
fn input_default_values() {
    let d = AsetekInputReport::default();
    assert_eq!(d.sequence, 0);
    assert_eq!(d.wheel_angle, 0);
    assert_eq!(d.wheel_speed, 0);
    assert_eq!(d.torque, 0);
    assert_eq!(d.temperature, 25);
    assert_eq!(d.status, 0x03);
}

// ── Output report construction ───────────────────────────────────────────────

#[test]
fn output_report_default() {
    let report = AsetekOutputReport::default();
    assert_eq!(report.sequence, 0);
    assert_eq!(report.torque_cNm, 0);
    assert_eq!(report.led_mode, 0);
    assert_eq!(report.led_value, 0);
}

#[test]
fn output_report_new_sequence() {
    let report = AsetekOutputReport::new(42);
    assert_eq!(report.sequence, 42);
    assert_eq!(report.torque_cNm, 0);
}

#[test]
fn output_report_with_torque_positive() {
    let report = AsetekOutputReport::new(1).with_torque(10.5);
    assert_eq!(report.torque_cNm, 1050);
}

#[test]
fn output_report_with_torque_negative() {
    let report = AsetekOutputReport::new(1).with_torque(-15.0);
    assert_eq!(report.torque_cNm, -1500);
}

#[test]
fn output_report_with_torque_zero() {
    let report = AsetekOutputReport::new(1).with_torque(0.0);
    assert_eq!(report.torque_cNm, 0);
}

#[test]
fn output_report_torque_clamps_above_max() {
    let report = AsetekOutputReport::new(1).with_torque(50.0);
    let at_max = AsetekOutputReport::new(1).with_torque(MAX_TORQUE_NM);
    assert_eq!(report.torque_cNm, at_max.torque_cNm);
}

#[test]
fn output_report_torque_clamps_below_min() {
    let report = AsetekOutputReport::new(1).with_torque(-50.0);
    let at_min = AsetekOutputReport::new(1).with_torque(-MAX_TORQUE_NM);
    assert_eq!(report.torque_cNm, at_min.torque_cNm);
}

#[test]
fn output_report_with_led() {
    let report = AsetekOutputReport::new(0).with_led(0x02, 0xFF);
    assert_eq!(report.led_mode, 0x02);
    assert_eq!(report.led_value, 0xFF);
}

#[test]
fn output_report_chaining() {
    let report = AsetekOutputReport::new(7)
        .with_torque(5.0)
        .with_led(1, 128);
    assert_eq!(report.sequence, 7);
    assert_eq!(report.torque_cNm, 500);
    assert_eq!(report.led_mode, 1);
    assert_eq!(report.led_value, 128);
}

#[test]
fn output_report_build_size() -> Result<(), String> {
    let report = AsetekOutputReport::new(1).with_torque(10.0);
    let data = report.build().map_err(|e| e.to_string())?;
    assert_eq!(
        data.len(),
        REPORT_SIZE_OUTPUT,
        "build must produce exactly {REPORT_SIZE_OUTPUT} bytes"
    );
    Ok(())
}

#[test]
fn output_report_build_wire_format() -> Result<(), String> {
    let report = AsetekOutputReport::new(0x0102).with_torque(15.0).with_led(3, 200);
    let data = report.build().map_err(|e| e.to_string())?;

    // sequence u16 LE
    assert_eq!(data[0], 0x02);
    assert_eq!(data[1], 0x01);
    // torque_cNm i16 LE = 1500 = 0x05DC
    assert_eq!(data[2], 0xDC);
    assert_eq!(data[3], 0x05);
    // led_mode
    assert_eq!(data[4], 3);
    // led_value
    assert_eq!(data[5], 200);
    // rest should be zero-padded
    for &b in &data[6..] {
        assert_eq!(b, 0, "trailing bytes must be zero");
    }
    Ok(())
}

#[test]
fn output_report_build_default_zero_padded() -> Result<(), String> {
    let data = AsetekOutputReport::default().build().map_err(|e| e.to_string())?;
    for (i, &b) in data.iter().enumerate() {
        assert_eq!(b, 0, "byte {i} should be 0 in default report");
    }
    Ok(())
}

// ── WheelCapabilities ────────────────────────────────────────────────────────

#[test]
fn wheel_capabilities_forte() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::Forte);
    assert_eq!(caps.max_torque_nm, 18.0);
    assert_eq!(caps.max_speed_rpm, 3000);
    assert!(caps.supports_quick_release);
}

#[test]
fn wheel_capabilities_invicta() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::Invicta);
    assert_eq!(caps.max_torque_nm, 27.0);
    assert_eq!(caps.max_speed_rpm, 2500);
    assert!(caps.supports_quick_release);
}

#[test]
fn wheel_capabilities_laprima() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::LaPrima);
    assert_eq!(caps.max_torque_nm, 12.0);
    assert_eq!(caps.max_speed_rpm, 2000);
    assert!(caps.supports_quick_release);
}

#[test]
fn wheel_capabilities_unknown_uses_defaults() {
    let caps = asetek::WheelCapabilities::for_model(asetek::WheelModel::Unknown);
    let defaults = asetek::WheelCapabilities::default();
    assert_eq!(caps.max_torque_nm, defaults.max_torque_nm);
    assert_eq!(caps.max_speed_rpm, defaults.max_speed_rpm);
    assert_eq!(caps.supports_quick_release, defaults.supports_quick_release);
}

#[test]
fn wheel_model_default_is_unknown() {
    let model: asetek::WheelModel = Default::default();
    assert_eq!(model, asetek::WheelModel::Unknown);
}

// ── Quirks ───────────────────────────────────────────────────────────────────

#[test]
#[allow(clippy::assertions_on_constants)]
fn requires_always_poll_linux_is_true() {
    assert!(asetek::quirks::REQUIRES_ALWAYS_POLL_LINUX);
}

// ── Property tests ───────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    #[test]
    fn prop_parse_too_short_fails(len in 0usize..16) {
        let data = vec![0u8; len];
        prop_assert!(AsetekInputReport::parse(&data).is_err());
    }

    #[test]
    fn prop_parse_sufficient_length_succeeds(extra in 0usize..=48) {
        let data = vec![0u8; 16 + extra];
        prop_assert!(AsetekInputReport::parse(&data).is_ok());
    }

    #[test]
    fn prop_wheel_angle_degrees_scaling(angle: i32) {
        let report = AsetekInputReport { wheel_angle: angle, ..Default::default() };
        let expected = angle as f32 / 1000.0;
        prop_assert_eq!(report.wheel_angle_degrees(), expected);
    }

    #[test]
    fn prop_applied_torque_nm_scaling(raw_torque: i16) {
        let report = AsetekInputReport { torque: raw_torque, ..Default::default() };
        let expected = raw_torque as f32 / 100.0;
        prop_assert_eq!(report.applied_torque_nm(), expected);
    }

    #[test]
    fn prop_output_build_size(seq: u16, torque in -100.0f32..=100.0f32) {
        let result = AsetekOutputReport::new(seq).with_torque(torque).build();
        prop_assert!(result.is_ok());
        if let Ok(data) = result {
            prop_assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
        }
    }

    #[test]
    fn prop_torque_cnm_clamped(torque in -100.0f32..=100.0f32) {
        let report = AsetekOutputReport::new(0).with_torque(torque);
        let max_cnm = (MAX_TORQUE_NM * 100.0) as i16;
        prop_assert!(report.torque_cNm >= -max_cnm);
        prop_assert!(report.torque_cNm <= max_cnm);
    }

    #[test]
    fn prop_is_asetek_device_only_correct_vid(vid: u16) {
        prop_assert_eq!(asetek::is_asetek_device(vid), vid == ASETEK_VENDOR_ID);
    }

    #[test]
    fn prop_model_from_info_wrong_vid_always_unknown(vid: u16, pid: u16) {
        if vid != ASETEK_VENDOR_ID {
            prop_assert_eq!(asetek::asetek_model_from_info(vid, pid), AsetekModel::Unknown);
        }
    }

    #[test]
    fn prop_status_connected_flag(status: u8) {
        let report = AsetekInputReport { status, ..Default::default() };
        prop_assert_eq!(report.is_connected(), (status & 0x01) != 0);
    }

    #[test]
    fn prop_status_enabled_flag(status: u8) {
        let report = AsetekInputReport { status, ..Default::default() };
        prop_assert_eq!(report.is_enabled(), (status & 0x02) != 0);
    }

    #[test]
    fn prop_display_name_non_empty(pid: u16) {
        let model = AsetekModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty());
    }

    #[test]
    fn prop_max_torque_non_negative(pid: u16) {
        let model = AsetekModel::from_product_id(pid);
        prop_assert!(model.max_torque_nm() >= 0.0);
    }
}
