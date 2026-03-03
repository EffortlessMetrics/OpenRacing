//! Advanced tests for Simucube protocol crate.
//!
//! Covers PID recognition for all models, SimpleMotion V2 command roundtrip,
//! high-resolution torque encoding, ActivePedal protocol, wireless wheel
//! compatibility, firmware update sequence, and proptest command roundtrip.

use hid_simucube_protocol::{
    SimucubeModel, SimucubeError, SimucubeOutputReport, EffectType,
    SimucubeHidReport, SimucubeInputReport,
    WheelModel, WheelCapabilities, DeviceStatus,
    simucube_model_from_info, is_simucube_device,
    VENDOR_ID,
    SIMUCUBE_VENDOR_ID, SIMUCUBE_1_PID, SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_PRO_PID, SIMUCUBE_2_ULTIMATE_PID,
    SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_WIRELESS_WHEEL_PID,
    SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_1_BOOTLOADER_PID,
    MAX_TORQUE_NM, MAX_TORQUE_SPORT, MAX_TORQUE_PRO, MAX_TORQUE_ULTIMATE,
    REPORT_SIZE_OUTPUT, ANGLE_SENSOR_MAX, ANGLE_SENSOR_BITS,
    HID_JOYSTICK_REPORT_MIN_BYTES, HID_BUTTON_BYTES, HID_ADDITIONAL_AXES,
};

// ─── PID recognition ─────────────────────────────────────────────────────

#[test]
fn test_simucube1_pid_recognition() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_1_PID);
    assert_eq!(model, SimucubeModel::Simucube1);
    assert!((model.max_torque_nm() - 25.0).abs() < f32::EPSILON);
    assert_eq!(model.display_name(), "Simucube 1");
}

#[test]
fn test_simucube2_sport_pid_recognition() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_2_SPORT_PID);
    assert_eq!(model, SimucubeModel::Sport);
    assert!((model.max_torque_nm() - 17.0).abs() < f32::EPSILON);
    assert_eq!(model.display_name(), "Simucube 2 Sport");
}

#[test]
fn test_simucube2_pro_pid_recognition() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_2_PRO_PID);
    assert_eq!(model, SimucubeModel::Pro);
    assert!((model.max_torque_nm() - 25.0).abs() < f32::EPSILON);
    assert_eq!(model.display_name(), "Simucube 2 Pro");
}

#[test]
fn test_simucube2_ultimate_pid_recognition() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_2_ULTIMATE_PID);
    assert_eq!(model, SimucubeModel::Ultimate);
    assert!((model.max_torque_nm() - 32.0).abs() < f32::EPSILON);
    assert_eq!(model.display_name(), "Simucube 2 Ultimate");
}

#[test]
fn test_active_pedal_pid_recognition() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_ACTIVE_PEDAL_PID);
    assert_eq!(model, SimucubeModel::ActivePedal);
    assert!((model.max_torque_nm() - 0.0).abs() < f32::EPSILON);
    assert_eq!(model.display_name(), "Simucube ActivePedal");
}

#[test]
fn test_wireless_wheel_pid_recognition() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_WIRELESS_WHEEL_PID);
    assert_eq!(model, SimucubeModel::WirelessWheel);
    assert!((model.max_torque_nm() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn test_unknown_pid_recognition() {
    let model = SimucubeModel::from_product_id(0xFFFF);
    assert_eq!(model, SimucubeModel::Unknown);
    assert_eq!(model.display_name(), "Unknown Simucube Device");
}

#[test]
fn test_simucube_model_from_info_wrong_vid() {
    let model = simucube_model_from_info(0x0000, SIMUCUBE_2_PRO_PID);
    assert_eq!(model, SimucubeModel::Unknown);
}

#[test]
fn test_simucube_model_from_info_correct_vid() {
    let model = simucube_model_from_info(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    assert_eq!(model, SimucubeModel::Pro);
}

#[test]
fn test_is_simucube_device_vid_check() {
    assert!(is_simucube_device(SIMUCUBE_VENDOR_ID));
    assert!(is_simucube_device(VENDOR_ID));
    assert!(!is_simucube_device(0x0000));
    assert!(!is_simucube_device(0x044F)); // Thrustmaster, not Simucube
}

// ─── SimpleMotion V2 / output command roundtrip ──────────────────────────

#[test]
fn test_output_report_build_size() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(42).with_torque(10.0);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    assert_eq!(data[0], 0x01); // report ID
    Ok(())
}

#[test]
fn test_output_report_torque_encoding_precision() {
    let report = SimucubeOutputReport::new(1).with_torque(12.34);
    // 12.34 clamped to MAX_TORQUE_NM, then * 100 → 1234 cNm
    assert_eq!(report.torque_cNm, 1234);
}

#[test]
fn test_output_report_negative_torque() {
    let report = SimucubeOutputReport::new(0).with_torque(-15.0);
    assert_eq!(report.torque_cNm, -1500);
}

#[test]
fn test_output_report_torque_clamping_high() {
    let report = SimucubeOutputReport::new(0).with_torque(100.0);
    assert_eq!(report.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);
}

#[test]
fn test_output_report_torque_clamping_low() {
    let report = SimucubeOutputReport::new(0).with_torque(-100.0);
    assert_eq!(report.torque_cNm, (-MAX_TORQUE_NM * 100.0) as i16);
}

// ─── High-resolution torque encoding (16-bit) ───────────────────────────

#[test]
fn test_torque_cnm_resolution() {
    // Verify centi-Newton-meter resolution: 0.01 Nm steps
    let report = SimucubeOutputReport::new(0).with_torque(0.01);
    assert_eq!(report.torque_cNm, 1);
    let report2 = SimucubeOutputReport::new(0).with_torque(0.99);
    assert_eq!(report2.torque_cNm, 99);
}

#[test]
fn test_torque_validate_within_range() {
    let report = SimucubeOutputReport::new(0).with_torque(20.0);
    assert!(report.validate_torque().is_ok());
}

#[test]
fn test_torque_validate_exceeds_range() {
    // Manually set a torque_cNm beyond MAX_TORQUE_NM
    let report = SimucubeOutputReport {
        torque_cNm: (MAX_TORQUE_NM * 200.0) as i16,
        ..Default::default()
    };
    assert!(matches!(
        report.validate_torque(),
        Err(SimucubeError::InvalidTorque(_))
    ));
}

// ─── ActivePedal protocol ────────────────────────────────────────────────

#[test]
fn test_active_pedal_capabilities() {
    let caps = WheelCapabilities::for_model(WheelModel::SimucubeActivePedal);
    assert!((caps.max_torque_nm - 0.0).abs() < f32::EPSILON);
    assert_eq!(caps.encoder_resolution_bits, 16);
    assert!(!caps.supports_wireless);
    assert!(caps.supports_active_pedal);
    assert_eq!(caps.max_speed_rpm, 0);
}

#[test]
fn test_active_pedal_is_not_wheelbase() {
    let model = SimucubeModel::from_product_id(SIMUCUBE_ACTIVE_PEDAL_PID);
    assert_eq!(model, SimucubeModel::ActivePedal);
    // ActivePedal has zero torque (not a wheelbase)
    assert!((model.max_torque_nm()).abs() < f32::EPSILON);
}

// ─── Wireless wheel compatibility ────────────────────────────────────────

#[test]
fn test_wireless_wheel_hid_report_buttons() -> Result<(), SimucubeError> {
    let mut buttons = [0u8; HID_BUTTON_BYTES];
    buttons[0] = 0b1010_0101; // buttons 0,2,5,7
    let data = make_hid_report(0x8000, 0x8000, [0; HID_ADDITIONAL_AXES], buttons);
    let report = SimucubeHidReport::parse(&data)?;
    assert!(report.button_pressed(0));
    assert!(!report.button_pressed(1));
    assert!(report.button_pressed(2));
    assert!(!report.button_pressed(3));
    assert!(report.button_pressed(5));
    assert!(report.button_pressed(7));
    assert_eq!(report.pressed_count(), 4);
    Ok(())
}

#[test]
fn test_wireless_battery_and_buttons_in_extended_report() {
    let mut data = [0u8; 17];
    data[14] = 0xFF; // all 8 low buttons
    data[15] = 0x01; // button 8 pressed
    data[16] = 75; // 75% battery
    let result = SimucubeInputReport::parse(&data);
    assert!(result.is_ok());
    if let Ok(report) = result {
        assert!(report.has_wireless_wheel());
        assert_eq!(report.wireless_battery_pct, 75);
        assert_eq!(report.wireless_buttons, 0x01FF);
    }
}

#[test]
fn test_no_wireless_wheel_when_short_report() {
    let data = [0u8; 16];
    let result = SimucubeInputReport::parse(&data);
    assert!(result.is_ok());
    if let Ok(report) = result {
        assert!(!report.has_wireless_wheel());
        assert_eq!(report.wireless_battery_pct, 0);
    }
}

// ─── Firmware update / bootloader detection ──────────────────────────────

#[test]
fn test_bootloader_pids_not_normal_models() {
    let s2_bl = SimucubeModel::from_product_id(SIMUCUBE_2_BOOTLOADER_PID);
    assert_eq!(s2_bl, SimucubeModel::Unknown, "SC2 bootloader PID should be Unknown model");

    let s1_bl = SimucubeModel::from_product_id(SIMUCUBE_1_BOOTLOADER_PID);
    assert_eq!(s1_bl, SimucubeModel::Unknown, "SC1 bootloader PID should be Unknown model");
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn test_bootloader_pid_constants() {
    assert_eq!(SIMUCUBE_2_BOOTLOADER_PID, 0x0D5E);
    assert_eq!(SIMUCUBE_1_BOOTLOADER_PID, 0x0D5B);
    // Bootloader PIDs should be close to normal PIDs
    assert!(SIMUCUBE_2_BOOTLOADER_PID < SIMUCUBE_2_SPORT_PID);
    assert!(SIMUCUBE_1_BOOTLOADER_PID > SIMUCUBE_1_PID);
}

// ─── DeviceStatus from flags ─────────────────────────────────────────────

#[test]
fn test_device_status_all_transitions() {
    assert_eq!(DeviceStatus::from_flags(0x00), DeviceStatus::Disconnected);
    assert_eq!(DeviceStatus::from_flags(0x01), DeviceStatus::Ready);
    assert_eq!(DeviceStatus::from_flags(0x03), DeviceStatus::Enabled);
    assert_eq!(DeviceStatus::from_flags(0x05), DeviceStatus::Calibrating);
    // Connected + Enabled + Error = Error
    assert_eq!(DeviceStatus::from_flags(0x0B), DeviceStatus::Error);
}

// ─── WheelCapabilities ───────────────────────────────────────────────────

#[test]
fn test_wheel_capabilities_all_models() {
    let sport = WheelCapabilities::for_model(WheelModel::Simucube2Sport);
    assert!((sport.max_torque_nm - 17.0).abs() < f32::EPSILON);
    assert_eq!(sport.encoder_resolution_bits, 22);

    let pro = WheelCapabilities::for_model(WheelModel::Simucube2Pro);
    assert!((pro.max_torque_nm - 25.0).abs() < f32::EPSILON);

    let ultimate = WheelCapabilities::for_model(WheelModel::Simucube2Ultimate);
    assert!((ultimate.max_torque_nm - 32.0).abs() < f32::EPSILON);

    let unknown = WheelCapabilities::for_model(WheelModel::Unknown);
    assert!((unknown.max_torque_nm - 25.0).abs() < f32::EPSILON);
}

// ─── Effect types ────────────────────────────────────────────────────────

#[test]
fn test_effect_type_discriminants() {
    assert_eq!(EffectType::None as u8, 0);
    assert_eq!(EffectType::Constant as u8, 1);
    assert_eq!(EffectType::Ramp as u8, 2);
    assert_eq!(EffectType::Square as u8, 3);
    assert_eq!(EffectType::Sine as u8, 4);
    assert_eq!(EffectType::Triangle as u8, 5);
    assert_eq!(EffectType::SawtoothUp as u8, 6);
    assert_eq!(EffectType::SawtoothDown as u8, 7);
    assert_eq!(EffectType::Spring as u8, 8);
    assert_eq!(EffectType::Damper as u8, 9);
    assert_eq!(EffectType::Friction as u8, 10);
}

#[test]
fn test_output_report_with_all_effect_types() -> Result<(), SimucubeError> {
    let effects = [
        EffectType::None, EffectType::Constant, EffectType::Ramp,
        EffectType::Square, EffectType::Sine, EffectType::Triangle,
        EffectType::SawtoothUp, EffectType::SawtoothDown,
        EffectType::Spring, EffectType::Damper, EffectType::Friction,
    ];
    for effect in effects {
        let report = SimucubeOutputReport::new(0).with_effect(effect, 500);
        assert_eq!(report.effect_type, effect);
        assert_eq!(report.effect_parameter, 500);
        let data = report.build()?;
        assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    }
    Ok(())
}

// ─── HID report parsing edge cases ──────────────────────────────────────

#[test]
fn test_hid_report_parse_too_short_rejected() {
    let data = [0u8; 31];
    let result = SimucubeHidReport::parse(&data);
    assert!(matches!(
        result,
        Err(SimucubeError::InvalidReportSize { expected: 32, actual: 31 })
    ));
}

#[test]
fn test_hid_report_steering_normalized_endpoints() -> Result<(), SimucubeError> {
    let data_min = make_hid_report(0x0000, 0x8000, [0; HID_ADDITIONAL_AXES], [0; HID_BUTTON_BYTES]);
    let report_min = SimucubeHidReport::parse(&data_min)?;
    assert!((report_min.steering_normalized() - 0.0).abs() < 0.001);

    let data_max = make_hid_report(0xFFFF, 0x8000, [0; HID_ADDITIONAL_AXES], [0; HID_BUTTON_BYTES]);
    let report_max = SimucubeHidReport::parse(&data_max)?;
    assert!((report_max.steering_normalized() - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn test_input_report_angle_degrees_quarter() {
    let report = SimucubeInputReport {
        wheel_angle_raw: ANGLE_SENSOR_MAX / 4,
        ..Default::default()
    };
    let degrees = report.wheel_angle_degrees();
    assert!((degrees - 90.0).abs() < 0.1);
}

#[test]
fn test_input_report_speed_rad_s() {
    let report = SimucubeInputReport {
        wheel_speed_rpm: 60,
        ..Default::default()
    };
    let rad_s = report.wheel_speed_rad_s();
    assert!((rad_s - 2.0 * std::f32::consts::PI).abs() < 0.01);
}

// ─── Constants validation ────────────────────────────────────────────────

#[test]
fn test_angle_sensor_constants() {
    assert_eq!(ANGLE_SENSOR_BITS, 22);
    assert_eq!(ANGLE_SENSOR_MAX, 0x3F_FFFF);
    assert_eq!(ANGLE_SENSOR_MAX, (1 << 22) - 1);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn test_torque_constants_hierarchy() {
    assert!(MAX_TORQUE_SPORT < MAX_TORQUE_PRO);
    assert!(MAX_TORQUE_PRO <= MAX_TORQUE_NM);
    assert!(MAX_TORQUE_PRO < MAX_TORQUE_ULTIMATE);
}

// ─── RGB LED output ──────────────────────────────────────────────────────

#[test]
fn test_output_report_rgb_encoding() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(0).with_rgb(255, 128, 0);
    assert_eq!(report.led_r, 255);
    assert_eq!(report.led_g, 128);
    assert_eq!(report.led_b, 0);
    let data = report.build()?;
    // Verify RGB appears in the built report (after seq + torque)
    assert_eq!(data[5], 255);
    assert_eq!(data[6], 128);
    assert_eq!(data[7], 0);
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────

fn make_hid_report(steering: u16, y: u16, axes: [u16; 6], buttons: [u8; 16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HID_JOYSTICK_REPORT_MIN_BYTES);
    buf.extend_from_slice(&steering.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    for ax in &axes {
        buf.extend_from_slice(&ax.to_le_bytes());
    }
    buf.extend_from_slice(&buttons);
    buf
}

// ─── Proptest: command roundtrip ─────────────────────────────────────────

mod proptest_advanced {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(300))]

        #[test]
        fn prop_output_report_torque_roundtrip(torque_nm in -25.0_f32..=25.0_f32) {
            let report = SimucubeOutputReport::new(0).with_torque(torque_nm);
            let expected_cnm = (torque_nm.clamp(-MAX_TORQUE_NM, MAX_TORQUE_NM) * 100.0) as i16;
            prop_assert_eq!(report.torque_cNm, expected_cnm);
        }

        #[test]
        fn prop_output_report_build_always_correct_size(seq in 0u16..=65535u16) {
            let report = SimucubeOutputReport::new(seq);
            let data = report.build();
            prop_assert!(data.is_ok());
            if let Ok(bytes) = data {
                prop_assert_eq!(bytes.len(), REPORT_SIZE_OUTPUT);
            }
        }

        #[test]
        fn prop_output_report_rgb_preserved(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
            let report = SimucubeOutputReport::new(0).with_rgb(r, g, b);
            prop_assert_eq!(report.led_r, r);
            prop_assert_eq!(report.led_g, g);
            prop_assert_eq!(report.led_b, b);
        }

        #[test]
        fn prop_hid_report_steering_roundtrip(steering in 0u16..=65535u16) {
            let data = make_hid_report(steering, 0x8000, [0; 6], [0; 16]);
            let result = SimucubeHidReport::parse(&data);
            prop_assert!(result.is_ok());
            if let Ok(report) = result {
                prop_assert_eq!(report.steering, steering);
            }
        }

        #[test]
        fn prop_model_from_pid_deterministic(pid in 0u16..=65535u16) {
            let model_a = SimucubeModel::from_product_id(pid);
            let model_b = SimucubeModel::from_product_id(pid);
            prop_assert_eq!(model_a, model_b);
        }

        #[test]
        fn prop_device_status_from_flags_never_panics(flags in 0u8..=255u8) {
            let _status = DeviceStatus::from_flags(flags);
        }

        #[test]
        fn prop_effect_parameter_preserved(param in 0u16..=65535u16) {
            let report = SimucubeOutputReport::new(0).with_effect(EffectType::Sine, param);
            prop_assert_eq!(report.effect_parameter, param);
        }
    }
}
