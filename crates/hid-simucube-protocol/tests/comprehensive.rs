//! Comprehensive tests for the Simucube HID protocol crate.
//!
//! Covers input report parsing, output report construction, device identification,
//! torque encoding precision, protocol feature negotiation, edge cases, property
//! tests for round-trips, and constant validation.

use hid_simucube_protocol::{
    DeviceStatus, EffectType, SimucubeError, SimucubeHidReport, SimucubeInputReport,
    SimucubeModel, SimucubeOutputReport, WheelCapabilities, WheelModel,
    ANGLE_SENSOR_BITS, ANGLE_SENSOR_MAX, HID_ADDITIONAL_AXES, HID_BUTTON_BYTES,
    HID_BUTTON_COUNT, HID_JOYSTICK_REPORT_MIN_BYTES, MAX_TORQUE_NM, MAX_TORQUE_PRO,
    MAX_TORQUE_SPORT, MAX_TORQUE_ULTIMATE, PRODUCT_ID_PRO, PRODUCT_ID_SPORT,
    PRODUCT_ID_ULTIMATE, REPORT_SIZE_INPUT, REPORT_SIZE_OUTPUT, SIMUCUBE_1_BOOTLOADER_PID,
    SIMUCUBE_1_PID, SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID,
    SIMUCUBE_WIRELESS_WHEEL_PID, VENDOR_ID, is_simucube_device, simucube_model_from_info,
};
use proptest::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Build a valid HID joystick report byte buffer from components.
fn build_hid_bytes(steering: u16, y: u16, axes: [u16; 6], buttons: [u8; 16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HID_JOYSTICK_REPORT_MIN_BYTES);
    buf.extend_from_slice(&steering.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    for ax in &axes {
        buf.extend_from_slice(&ax.to_le_bytes());
    }
    buf.extend_from_slice(&buttons);
    buf
}

/// Build a speculative extended input report byte buffer (16 bytes).
///
/// Layout: seq(2) + angle(4) + speed(2) + torque(2) + temp(1) + fault(1) +
///         reserved(1) + status(1) + padding(2) = 16 bytes.
fn build_extended_bytes(
    sequence: u16,
    angle: u32,
    speed: i16,
    torque: i16,
    temp: u8,
    fault: u8,
    status: u8,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(&sequence.to_le_bytes()); // 2
    buf.extend_from_slice(&angle.to_le_bytes());    // 4
    buf.extend_from_slice(&speed.to_le_bytes());    // 2
    buf.extend_from_slice(&torque.to_le_bytes());   // 2
    buf.push(temp);                                  // 1
    buf.push(fault);                                 // 1
    buf.push(0x00); // reserved                      // 1
    buf.push(status);                                // 1
    // Pad to 16 bytes if needed
    while buf.len() < 16 {
        buf.push(0x00);
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Input report parsing (wheel angle, buttons, axes)
// ═══════════════════════════════════════════════════════════════════════════════

mod input_report_parsing {
    use super::*;

    #[test]
    fn hid_report_center_position() -> Result<(), SimucubeError> {
        let data = build_hid_bytes(0x8000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0x8000);
        assert_eq!(report.y_axis, 0x8000);
        assert!(report.steering_signed().abs() < 0.001);
        Ok(())
    }

    #[test]
    fn hid_report_full_left() -> Result<(), SimucubeError> {
        let data = build_hid_bytes(0x0000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0);
        assert!((report.steering_signed() - (-1.0)).abs() < 0.001);
        assert!(report.steering_normalized().abs() < 0.001);
        Ok(())
    }

    #[test]
    fn hid_report_full_right() -> Result<(), SimucubeError> {
        let data = build_hid_bytes(0xFFFF, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0xFFFF);
        assert!((report.steering_normalized() - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn hid_report_quarter_turn_positions() -> Result<(), SimucubeError> {
        // 25% position
        let data = build_hid_bytes(0x4000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        let norm = report.steering_normalized();
        assert!((norm - 0.25).abs() < 0.01);

        // 75% position
        let data = build_hid_bytes(0xC000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        let norm = report.steering_normalized();
        assert!((norm - 0.75).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn hid_report_all_axes_distinct() -> Result<(), SimucubeError> {
        let axes = [100, 200, 300, 400, 500, 600];
        let data = build_hid_bytes(0x8000, 0x4000, axes, [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.axes, axes);
        assert_eq!(report.y_axis, 0x4000);
        for (i, &expected) in axes.iter().enumerate() {
            let norm = report.axis_normalized(i);
            let expected_norm = expected as f32 / u16::MAX as f32;
            assert!(
                (norm - expected_norm).abs() < 0.001,
                "axis {i}: expected {expected_norm}, got {norm}"
            );
        }
        Ok(())
    }

    #[test]
    fn hid_report_axes_at_extremes() -> Result<(), SimucubeError> {
        let axes = [0, u16::MAX, 0, u16::MAX, 0, u16::MAX];
        let data = build_hid_bytes(0x8000, 0x8000, axes, [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert!(report.axis_normalized(0).abs() < 0.001);
        assert!((report.axis_normalized(1) - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn hid_report_axis_out_of_range_returns_zero() {
        let report = SimucubeHidReport::default();
        assert_eq!(report.axis_normalized(6), 0.0);
        assert_eq!(report.axis_normalized(100), 0.0);
        assert_eq!(report.axis_normalized(usize::MAX), 0.0);
    }

    #[test]
    fn hid_report_individual_buttons() -> Result<(), SimucubeError> {
        let mut buttons = [0u8; 16];
        buttons[0] = 0b0000_0001; // button 0
        buttons[1] = 0b0000_0010; // button 9
        buttons[15] = 0b1000_0000; // button 127
        let data = build_hid_bytes(0x8000, 0x8000, [0; 6], buttons);
        let report = SimucubeHidReport::parse(&data)?;
        assert!(report.button_pressed(0));
        assert!(!report.button_pressed(1));
        assert!(report.button_pressed(9));
        assert!(!report.button_pressed(8));
        assert!(report.button_pressed(127));
        assert_eq!(report.pressed_count(), 3);
        Ok(())
    }

    #[test]
    fn hid_report_all_128_buttons_pressed() -> Result<(), SimucubeError> {
        let buttons = [0xFF; 16];
        let data = build_hid_bytes(0x8000, 0x8000, [0; 6], buttons);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.pressed_count(), 128);
        for i in 0..128 {
            assert!(report.button_pressed(i), "button {i} should be pressed");
        }
        Ok(())
    }

    #[test]
    fn hid_report_no_buttons_pressed() -> Result<(), SimucubeError> {
        let data = build_hid_bytes(0x8000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.pressed_count(), 0);
        for i in 0..128 {
            assert!(!report.button_pressed(i), "button {i} should not be pressed");
        }
        Ok(())
    }

    #[test]
    fn hid_report_button_out_of_range_is_false() {
        let report = SimucubeHidReport::default();
        assert!(!report.button_pressed(128));
        assert!(!report.button_pressed(255));
        assert!(!report.button_pressed(1000));
        assert!(!report.button_pressed(usize::MAX));
    }

    #[test]
    fn extended_report_parse_basic() -> Result<(), SimucubeError> {
        let data = build_extended_bytes(42, 1_000_000, 120, 500, 45, 0, 0x03);
        let report = SimucubeInputReport::parse(&data)?;
        assert_eq!(report.sequence, 42);
        assert_eq!(report.wheel_angle_raw, 1_000_000);
        assert_eq!(report.wheel_speed_rpm, 120);
        assert_eq!(report.torque_nm, 500);
        assert_eq!(report.temperature_c, 45);
        assert_eq!(report.fault_flags, 0);
        assert_eq!(report.status_flags, 0x03);
        assert!(report.is_connected());
        assert!(report.is_enabled());
        assert!(!report.has_fault());
        Ok(())
    }

    #[test]
    fn extended_report_angle_degrees_at_quarter() {
        let report = SimucubeInputReport {
            wheel_angle_raw: ANGLE_SENSOR_MAX / 4,
            ..Default::default()
        };
        let degrees = report.wheel_angle_degrees();
        assert!((degrees - 90.0).abs() < 0.1);
    }

    #[test]
    fn extended_report_angle_degrees_at_zero() {
        let report = SimucubeInputReport {
            wheel_angle_raw: 0,
            ..Default::default()
        };
        assert!(report.wheel_angle_degrees().abs() < 0.001);
    }

    #[test]
    fn extended_report_angle_degrees_at_max() {
        let report = SimucubeInputReport {
            wheel_angle_raw: ANGLE_SENSOR_MAX,
            ..Default::default()
        };
        assert!((report.wheel_angle_degrees() - 360.0).abs() < 0.01);
    }

    #[test]
    fn extended_report_angle_radians_at_half() {
        let report = SimucubeInputReport {
            wheel_angle_raw: ANGLE_SENSOR_MAX / 2,
            ..Default::default()
        };
        let radians = report.wheel_angle_radians();
        assert!((radians - std::f32::consts::PI).abs() < 0.01);
    }

    #[test]
    fn extended_report_speed_conversion() {
        let report = SimucubeInputReport {
            wheel_speed_rpm: 60,
            ..Default::default()
        };
        // 60 RPM = 2π rad/s
        let rad_s = report.wheel_speed_rad_s();
        assert!((rad_s - 2.0 * std::f32::consts::PI).abs() < 0.01);
    }

    #[test]
    fn extended_report_negative_speed() {
        let report = SimucubeInputReport {
            wheel_speed_rpm: -60,
            ..Default::default()
        };
        let rad_s = report.wheel_speed_rad_s();
        assert!((rad_s - (-2.0 * std::f32::consts::PI)).abs() < 0.01);
    }

    #[test]
    fn extended_report_applied_torque() {
        let report = SimucubeInputReport {
            torque_nm: 1500,
            ..Default::default()
        };
        assert!((report.applied_torque_nm() - 15.0).abs() < 0.01);
    }

    #[test]
    fn extended_report_negative_torque() {
        let report = SimucubeInputReport {
            torque_nm: -2000,
            ..Default::default()
        };
        assert!((report.applied_torque_nm() - (-20.0)).abs() < 0.01);
    }

    #[test]
    fn extended_report_status_flags_combinations() {
        let cases: &[(u8, bool, bool)] = &[
            (0x00, false, false),
            (0x01, true, false),
            (0x02, false, true),
            (0x03, true, true),
        ];
        for &(flags, connected, enabled) in cases {
            let report = SimucubeInputReport {
                status_flags: flags,
                ..Default::default()
            };
            assert_eq!(
                report.is_connected(),
                connected,
                "flags={flags:#04x}: is_connected"
            );
            assert_eq!(
                report.is_enabled(),
                enabled,
                "flags={flags:#04x}: is_enabled"
            );
        }
    }

    #[test]
    fn extended_report_fault_detection() {
        let no_fault = SimucubeInputReport {
            fault_flags: 0,
            ..Default::default()
        };
        assert!(!no_fault.has_fault());

        for bit in 0..8u8 {
            let report = SimucubeInputReport {
                fault_flags: 1 << bit,
                ..Default::default()
            };
            assert!(report.has_fault(), "fault_flags={:#04x} should indicate fault", 1u8 << bit);
        }
    }

    #[test]
    fn extended_report_wireless_wheel_present() {
        let mut data = vec![0u8; 17];
        data[14] = 0b0000_0101;
        data[15] = 0x00;
        data[16] = 85;
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.wireless_buttons, 0b0000_0101);
            assert_eq!(report.wireless_battery_pct, 85);
            assert!(report.has_wireless_wheel());
        }
    }

    #[test]
    fn extended_report_wireless_wheel_absent() {
        let data = vec![0u8; 16];
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert!(!report.has_wireless_wheel());
            assert_eq!(report.wireless_buttons, 0);
            assert_eq!(report.wireless_battery_pct, 0);
        }
    }

    #[test]
    fn extended_report_wireless_battery_only() {
        // Battery > 0 but no buttons pressed should still count as present
        let mut data = vec![0u8; 17];
        data[16] = 50; // 50% battery
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert!(report.has_wireless_wheel());
        }
    }

    #[test]
    fn extended_report_wireless_buttons_only() {
        // Buttons pressed but battery == 0 should still count as present
        let mut data = vec![0u8; 17];
        data[14] = 0x01; // button 0 pressed
        data[16] = 0;
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert!(report.has_wireless_wheel());
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Output report construction (torque/FFB commands)
// ═══════════════════════════════════════════════════════════════════════════════

mod output_report_construction {
    use super::*;

    #[test]
    fn default_output_report() {
        let report = SimucubeOutputReport::default();
        assert_eq!(report.sequence, 0);
        assert_eq!(report.torque_cNm, 0);
        assert_eq!(report.led_r, 0);
        assert_eq!(report.led_g, 0);
        assert_eq!(report.led_b, 0);
        assert_eq!(report.effect_type, EffectType::None);
        assert_eq!(report.effect_parameter, 0);
    }

    #[test]
    fn output_report_with_torque() {
        let report = SimucubeOutputReport::new(1).with_torque(10.5);
        assert_eq!(report.sequence, 1);
        assert_eq!(report.torque_cNm, 1050);
    }

    #[test]
    fn output_report_zero_torque() {
        let report = SimucubeOutputReport::new(0).with_torque(0.0);
        assert_eq!(report.torque_cNm, 0);
    }

    #[test]
    fn output_report_negative_torque() {
        let report = SimucubeOutputReport::new(0).with_torque(-15.0);
        assert_eq!(report.torque_cNm, -1500);
    }

    #[test]
    fn output_report_clamps_above_max() {
        let report = SimucubeOutputReport::new(0).with_torque(100.0);
        assert_eq!(report.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);
    }

    #[test]
    fn output_report_clamps_below_neg_max() {
        let report = SimucubeOutputReport::new(0).with_torque(-100.0);
        assert_eq!(report.torque_cNm, (-MAX_TORQUE_NM * 100.0) as i16);
    }

    #[test]
    fn output_report_rgb_values() {
        let report = SimucubeOutputReport::new(0).with_rgb(255, 128, 0);
        assert_eq!(report.led_r, 255);
        assert_eq!(report.led_g, 128);
        assert_eq!(report.led_b, 0);
    }

    #[test]
    fn output_report_builder_chaining() {
        let report = SimucubeOutputReport::new(99)
            .with_torque(5.0)
            .with_rgb(10, 20, 30)
            .with_effect(EffectType::Sine, 1000);
        assert_eq!(report.sequence, 99);
        assert_eq!(report.torque_cNm, 500);
        assert_eq!(report.led_r, 10);
        assert_eq!(report.effect_type, EffectType::Sine);
        assert_eq!(report.effect_parameter, 1000);
    }

    #[test]
    fn output_report_build_size() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0).with_torque(5.0);
        let data = report.build()?;
        assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
        Ok(())
    }

    #[test]
    fn output_report_build_report_id() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0);
        let data = report.build()?;
        assert_eq!(data[0], 0x01, "first byte must be report ID 0x01");
        Ok(())
    }

    #[test]
    fn output_report_build_sequence_encoding() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0x1234);
        let data = report.build()?;
        let seq = u16::from_le_bytes([data[1], data[2]]);
        assert_eq!(seq, 0x1234);
        Ok(())
    }

    #[test]
    fn output_report_build_torque_encoding() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0).with_torque(12.34);
        let data = report.build()?;
        let torque = i16::from_le_bytes([data[3], data[4]]);
        assert_eq!(torque, 1234);
        Ok(())
    }

    #[test]
    fn output_report_build_rgb_encoding() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0).with_rgb(0xAA, 0xBB, 0xCC);
        let data = report.build()?;
        assert_eq!(data[5], 0xAA);
        assert_eq!(data[6], 0xBB);
        assert_eq!(data[7], 0xCC);
        Ok(())
    }

    #[test]
    fn output_report_build_effect_encoding() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0)
            .with_effect(EffectType::Damper, 0xABCD);
        let data = report.build()?;
        assert_eq!(data[8], EffectType::Damper as u8);
        let param = u16::from_le_bytes([data[9], data[10]]);
        assert_eq!(param, 0xABCD);
        Ok(())
    }

    #[test]
    fn output_report_build_zero_padded() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0);
        let data = report.build()?;
        // Bytes beyond the structured fields should be zero-padded
        for (i, &byte) in data.iter().enumerate().skip(11) {
            assert_eq!(byte, 0, "byte {i} should be zero-padded");
        }
        Ok(())
    }

    #[test]
    fn output_report_all_effect_types() {
        let effects = [
            (EffectType::None, 0u8),
            (EffectType::Constant, 1),
            (EffectType::Ramp, 2),
            (EffectType::Square, 3),
            (EffectType::Sine, 4),
            (EffectType::Triangle, 5),
            (EffectType::SawtoothUp, 6),
            (EffectType::SawtoothDown, 7),
            (EffectType::Spring, 8),
            (EffectType::Damper, 9),
            (EffectType::Friction, 10),
        ];
        for (effect, expected_val) in effects {
            assert_eq!(effect as u8, expected_val, "effect {effect:?} value mismatch");
        }
    }

    #[test]
    fn output_report_validate_torque_valid() {
        let report = SimucubeOutputReport::new(0).with_torque(10.0);
        assert!(report.validate_torque().is_ok());
    }

    #[test]
    fn output_report_validate_torque_at_max() {
        let report = SimucubeOutputReport::new(0).with_torque(MAX_TORQUE_NM);
        assert!(report.validate_torque().is_ok());
    }

    #[test]
    fn output_report_validate_torque_over_max_raw() {
        // Manually set cNm beyond the clamped range
        let report = SimucubeOutputReport {
            torque_cNm: i16::MAX,
            ..Default::default()
        };
        assert!(matches!(
            report.validate_torque(),
            Err(SimucubeError::InvalidTorque(_))
        ));
    }

    #[test]
    fn output_report_max_sequence() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(u16::MAX);
        let data = report.build()?;
        let seq = u16::from_le_bytes([data[1], data[2]]);
        assert_eq!(seq, u16::MAX);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device identification (SimuCube 1/2/2 Pro/2 Ultimate/2 Sport)
// ═══════════════════════════════════════════════════════════════════════════════

mod device_identification {
    use super::*;

    #[test]
    fn model_from_pid_simucube1() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_1_PID),
            SimucubeModel::Simucube1
        );
    }

    #[test]
    fn model_from_pid_sport() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_SPORT_PID),
            SimucubeModel::Sport
        );
    }

    #[test]
    fn model_from_pid_pro() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_PRO_PID),
            SimucubeModel::Pro
        );
    }

    #[test]
    fn model_from_pid_ultimate() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_ULTIMATE_PID),
            SimucubeModel::Ultimate
        );
    }

    #[test]
    fn model_from_pid_active_pedal() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_ACTIVE_PEDAL_PID),
            SimucubeModel::ActivePedal
        );
    }

    #[test]
    fn model_from_pid_wireless_wheel() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_WIRELESS_WHEEL_PID),
            SimucubeModel::WirelessWheel
        );
    }

    #[test]
    fn model_from_pid_unknown() {
        assert_eq!(
            SimucubeModel::from_product_id(0xFFFF),
            SimucubeModel::Unknown
        );
        assert_eq!(
            SimucubeModel::from_product_id(0x0000),
            SimucubeModel::Unknown
        );
    }

    #[test]
    fn model_from_info_correct_vid() {
        let model = simucube_model_from_info(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
        assert_eq!(model, SimucubeModel::Pro);
    }

    #[test]
    fn model_from_info_wrong_vid() {
        let model = simucube_model_from_info(0x1234, SIMUCUBE_2_PRO_PID);
        assert_eq!(model, SimucubeModel::Unknown);
    }

    #[test]
    fn model_from_info_zero_vid() {
        let model = simucube_model_from_info(0x0000, SIMUCUBE_2_SPORT_PID);
        assert_eq!(model, SimucubeModel::Unknown);
    }

    #[test]
    fn is_simucube_device_valid() {
        assert!(is_simucube_device(SIMUCUBE_VENDOR_ID));
    }

    #[test]
    fn is_simucube_device_invalid() {
        assert!(!is_simucube_device(0x0000));
        assert!(!is_simucube_device(0xFFFF));
        assert!(!is_simucube_device(0x1234));
    }

    #[test]
    fn display_names_all_models() {
        let cases: &[(SimucubeModel, &str)] = &[
            (SimucubeModel::Simucube1, "Simucube 1"),
            (SimucubeModel::Sport, "Simucube 2 Sport"),
            (SimucubeModel::Pro, "Simucube 2 Pro"),
            (SimucubeModel::Ultimate, "Simucube 2 Ultimate"),
            (SimucubeModel::ActivePedal, "Simucube ActivePedal"),
            (SimucubeModel::WirelessWheel, "SimuCube Wireless Wheel"),
            (SimucubeModel::Unknown, "Unknown Simucube Device"),
        ];
        for &(model, expected) in cases {
            assert_eq!(model.display_name(), expected, "model={model:?}");
        }
    }

    #[test]
    fn display_names_contain_brand() {
        let models = [
            SimucubeModel::Simucube1,
            SimucubeModel::Sport,
            SimucubeModel::Pro,
            SimucubeModel::Ultimate,
            SimucubeModel::ActivePedal,
            SimucubeModel::WirelessWheel,
            SimucubeModel::Unknown,
        ];
        for model in models {
            let name = model.display_name();
            assert!(
                name.contains("Simucube") || name.contains("SimuCube"),
                "{model:?} display_name '{name}' must contain Simucube brand"
            );
        }
    }

    #[test]
    fn bootloader_pids_resolve_to_unknown() {
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_1_BOOTLOADER_PID),
            SimucubeModel::Unknown
        );
        assert_eq!(
            SimucubeModel::from_product_id(SIMUCUBE_2_BOOTLOADER_PID),
            SimucubeModel::Unknown
        );
    }

    #[test]
    fn max_torque_per_model() {
        assert_eq!(SimucubeModel::Simucube1.max_torque_nm(), 25.0);
        assert_eq!(SimucubeModel::Sport.max_torque_nm(), 17.0);
        assert_eq!(SimucubeModel::Pro.max_torque_nm(), 25.0);
        assert_eq!(SimucubeModel::Ultimate.max_torque_nm(), 32.0);
        assert_eq!(SimucubeModel::ActivePedal.max_torque_nm(), 0.0);
        assert_eq!(SimucubeModel::WirelessWheel.max_torque_nm(), 0.0);
        assert_eq!(SimucubeModel::Unknown.max_torque_nm(), 25.0);
    }

    #[test]
    fn wheel_capabilities_per_model() {
        let sport = WheelCapabilities::for_model(WheelModel::Simucube2Sport);
        assert_eq!(sport.max_torque_nm, 17.0);
        assert_eq!(sport.encoder_resolution_bits, 22);
        assert!(sport.supports_wireless);

        let pro = WheelCapabilities::for_model(WheelModel::Simucube2Pro);
        assert_eq!(pro.max_torque_nm, 25.0);

        let ult = WheelCapabilities::for_model(WheelModel::Simucube2Ultimate);
        assert_eq!(ult.max_torque_nm, 32.0);

        let pedal = WheelCapabilities::for_model(WheelModel::SimucubeActivePedal);
        assert_eq!(pedal.max_torque_nm, 0.0);
        assert!(!pedal.supports_wireless);
        assert_eq!(pedal.max_speed_rpm, 0);

        let unknown = WheelCapabilities::for_model(WheelModel::Unknown);
        assert_eq!(unknown.max_torque_nm, 25.0);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Torque encoding precision per model tier
// ═══════════════════════════════════════════════════════════════════════════════

mod torque_encoding_precision {
    use super::*;

    #[test]
    fn sport_max_torque_encodes_correctly() {
        let report = SimucubeOutputReport::new(0).with_torque(MAX_TORQUE_SPORT);
        assert_eq!(report.torque_cNm, (MAX_TORQUE_SPORT * 100.0) as i16);
    }

    #[test]
    fn pro_max_torque_encodes_correctly() {
        let report = SimucubeOutputReport::new(0).with_torque(MAX_TORQUE_PRO);
        assert_eq!(report.torque_cNm, (MAX_TORQUE_PRO * 100.0) as i16);
    }

    #[test]
    fn ultimate_max_torque_encodes_correctly() {
        // Ultimate is 32 Nm, but MAX_TORQUE_NM clamp is 25 Nm, so it clamps
        let report = SimucubeOutputReport::new(0).with_torque(MAX_TORQUE_ULTIMATE);
        let clamped = MAX_TORQUE_ULTIMATE.min(MAX_TORQUE_NM);
        assert_eq!(report.torque_cNm, (clamped * 100.0) as i16);
    }

    #[test]
    fn centinewton_meter_resolution() {
        // 0.01 Nm resolution: 1 cNm step
        let r1 = SimucubeOutputReport::new(0).with_torque(0.01);
        assert_eq!(r1.torque_cNm, 1);

        let r2 = SimucubeOutputReport::new(0).with_torque(0.02);
        assert_eq!(r2.torque_cNm, 2);

        let r3 = SimucubeOutputReport::new(0).with_torque(0.99);
        assert_eq!(r3.torque_cNm, 99);
    }

    #[test]
    fn torque_round_trip_precision() {
        let test_values = [0.0f32, 0.01, 0.5, 1.0, 5.0, 10.0, 17.0, 25.0, -1.0, -10.0, -25.0];
        for torque in test_values {
            let report = SimucubeOutputReport::new(0).with_torque(torque);
            let decoded = report.torque_cNm as f32 / 100.0;
            let clamped = torque.clamp(-MAX_TORQUE_NM, MAX_TORQUE_NM);
            let error = (decoded - clamped).abs();
            assert!(
                error < 0.01,
                "torque {torque}: encoded={}, decoded={decoded}, error={error}",
                report.torque_cNm
            );
        }
    }

    #[test]
    fn torque_sign_preservation() {
        let positive = SimucubeOutputReport::new(0).with_torque(10.0);
        assert!(positive.torque_cNm > 0);

        let negative = SimucubeOutputReport::new(0).with_torque(-10.0);
        assert!(negative.torque_cNm < 0);

        let zero = SimucubeOutputReport::new(0).with_torque(0.0);
        assert_eq!(zero.torque_cNm, 0);
    }

    #[test]
    fn sport_torque_fits_in_i16() {
        let max_cnm = (MAX_TORQUE_SPORT * 100.0) as i32;
        assert!(max_cnm <= i16::MAX as i32);
        assert!(-max_cnm >= i16::MIN as i32);
    }

    #[test]
    fn pro_torque_fits_in_i16() {
        let max_cnm = (MAX_TORQUE_PRO * 100.0) as i32;
        assert!(max_cnm <= i16::MAX as i32);
        assert!(-max_cnm >= i16::MIN as i32);
    }

    #[test]
    fn ultimate_torque_fits_in_i16() {
        let max_cnm = (MAX_TORQUE_ULTIMATE * 100.0) as i32;
        assert!(max_cnm <= i16::MAX as i32);
        assert!(-max_cnm >= i16::MIN as i32);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn torque_ordering_by_tier() {
        assert!(MAX_TORQUE_SPORT < MAX_TORQUE_PRO);
        assert!(MAX_TORQUE_PRO < MAX_TORQUE_ULTIMATE);
    }

    #[test]
    fn small_torque_increments_are_distinct() {
        // Verify integer cNm values produce exact encodings
        for cnm in 0i16..100 {
            let torque = cnm as f32 / 100.0;
            let report = SimucubeOutputReport::new(0).with_torque(torque);
            // Allow ±1 cNm due to floating-point truncation in the multiply
            let diff = (report.torque_cNm - cnm).abs();
            assert!(
                diff <= 1,
                "torque {torque} (target {cnm} cNm): got {} cNm, diff {diff}",
                report.torque_cNm
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Protocol feature negotiation
// ═══════════════════════════════════════════════════════════════════════════════

mod protocol_feature_negotiation {
    use super::*;

    #[test]
    fn device_status_from_flags_disconnected() {
        assert_eq!(DeviceStatus::from_flags(0x00), DeviceStatus::Disconnected);
    }

    #[test]
    fn device_status_from_flags_ready() {
        assert_eq!(DeviceStatus::from_flags(0x01), DeviceStatus::Ready);
    }

    #[test]
    fn device_status_from_flags_enabled() {
        assert_eq!(DeviceStatus::from_flags(0x03), DeviceStatus::Enabled);
    }

    #[test]
    fn device_status_from_flags_calibrating() {
        // Calibrating takes priority if bit 2 is set and connected
        assert_eq!(DeviceStatus::from_flags(0x05), DeviceStatus::Calibrating);
        assert_eq!(DeviceStatus::from_flags(0x07), DeviceStatus::Calibrating);
    }

    #[test]
    fn device_status_from_flags_error() {
        // Error: connected + enabled + error bit
        assert_eq!(DeviceStatus::from_flags(0x0B), DeviceStatus::Error);
    }

    #[test]
    fn device_status_disconnected_overrides_everything() {
        // If bit 0 (connected) is not set, always Disconnected
        assert_eq!(DeviceStatus::from_flags(0x00), DeviceStatus::Disconnected);
        assert_eq!(DeviceStatus::from_flags(0x02), DeviceStatus::Disconnected);
        assert_eq!(DeviceStatus::from_flags(0x04), DeviceStatus::Disconnected);
        assert_eq!(DeviceStatus::from_flags(0x08), DeviceStatus::Disconnected);
        assert_eq!(DeviceStatus::from_flags(0x0E), DeviceStatus::Disconnected);
        assert_eq!(DeviceStatus::from_flags(0xFE), DeviceStatus::Disconnected);
    }

    #[test]
    fn device_status_default_is_disconnected() {
        assert_eq!(DeviceStatus::default(), DeviceStatus::Disconnected);
    }

    #[test]
    fn wheel_capabilities_wireless_support() {
        // Wheelbases support wireless, pedals don't
        assert!(WheelCapabilities::for_model(WheelModel::Simucube2Sport).supports_wireless);
        assert!(WheelCapabilities::for_model(WheelModel::Simucube2Pro).supports_wireless);
        assert!(WheelCapabilities::for_model(WheelModel::Simucube2Ultimate).supports_wireless);
        assert!(!WheelCapabilities::for_model(WheelModel::SimucubeActivePedal).supports_wireless);
    }

    #[test]
    fn wheel_capabilities_active_pedal_support() {
        // All models list active_pedal support in current impl
        assert!(WheelCapabilities::for_model(WheelModel::Simucube2Sport).supports_active_pedal);
        assert!(
            WheelCapabilities::for_model(WheelModel::SimucubeActivePedal).supports_active_pedal
        );
    }

    #[test]
    fn wheel_capabilities_encoder_resolution() {
        // Wheelbases have 22-bit encoder, pedal has 16-bit
        let wb = WheelCapabilities::for_model(WheelModel::Simucube2Pro);
        assert_eq!(wb.encoder_resolution_bits, 22);

        let pedal = WheelCapabilities::for_model(WheelModel::SimucubeActivePedal);
        assert_eq!(pedal.encoder_resolution_bits, 16);
    }

    #[test]
    fn wheel_capabilities_max_speed_rpm() {
        let wb = WheelCapabilities::for_model(WheelModel::Simucube2Pro);
        assert_eq!(wb.max_speed_rpm, 3000);

        let pedal = WheelCapabilities::for_model(WheelModel::SimucubeActivePedal);
        assert_eq!(pedal.max_speed_rpm, 0);
    }

    #[test]
    fn wheel_model_default_is_unknown() {
        assert!(matches!(WheelModel::default(), WheelModel::Unknown));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Edge cases: boundary values, short reports, model-specific quirks
// ═══════════════════════════════════════════════════════════════════════════════

mod edge_cases {
    use super::*;

    // -- Short / empty reports --

    #[test]
    fn hid_report_empty_input() {
        let result = SimucubeHidReport::parse(&[]);
        assert!(matches!(
            result,
            Err(SimucubeError::InvalidReportSize {
                expected: 32,
                actual: 0
            })
        ));
    }

    #[test]
    fn hid_report_one_byte_short() {
        let data = vec![0u8; HID_JOYSTICK_REPORT_MIN_BYTES - 1];
        let result = SimucubeHidReport::parse(&data);
        assert!(matches!(
            result,
            Err(SimucubeError::InvalidReportSize { .. })
        ));
    }

    #[test]
    fn hid_report_exactly_minimum_size() -> Result<(), SimucubeError> {
        let data = vec![0u8; HID_JOYSTICK_REPORT_MIN_BYTES];
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0);
        Ok(())
    }

    #[test]
    fn hid_report_extra_bytes_ignored() -> Result<(), SimucubeError> {
        let mut data = vec![0xAA; 64];
        // Set steering to known value
        data[0] = 0x00;
        data[1] = 0x80; // 0x8000 LE
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0x8000);
        Ok(())
    }

    #[test]
    fn extended_report_too_short() {
        for len in 0..16 {
            let data = vec![0u8; len];
            let result = SimucubeInputReport::parse(&data);
            assert!(
                result.is_err(),
                "extended report of {len} bytes should fail"
            );
        }
    }

    #[test]
    fn extended_report_exactly_16_bytes() {
        let data = vec![0u8; 16];
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
    }

    // -- Boundary values --

    #[test]
    fn hid_report_steering_boundary_values() -> Result<(), SimucubeError> {
        for &steering in &[0u16, 1, 0x7FFF, 0x8000, 0x8001, 0xFFFE, 0xFFFF] {
            let data = build_hid_bytes(steering, 0x8000, [0; 6], [0; 16]);
            let report = SimucubeHidReport::parse(&data)?;
            assert_eq!(report.steering, steering);
        }
        Ok(())
    }

    #[test]
    fn extended_report_max_angle_sensor() {
        let report = SimucubeInputReport {
            wheel_angle_raw: ANGLE_SENSOR_MAX,
            ..Default::default()
        };
        assert!(report.wheel_angle_degrees().is_finite());
        assert!(report.wheel_angle_radians().is_finite());
    }

    #[test]
    fn extended_report_zero_angle() {
        let report = SimucubeInputReport {
            wheel_angle_raw: 0,
            ..Default::default()
        };
        assert!(report.wheel_angle_degrees().abs() < f32::EPSILON);
    }

    #[test]
    fn extended_report_max_speed() {
        let report = SimucubeInputReport {
            wheel_speed_rpm: i16::MAX,
            ..Default::default()
        };
        assert!(report.wheel_speed_rad_s().is_finite());
        assert!(report.wheel_speed_rad_s() > 0.0);
    }

    #[test]
    fn extended_report_min_speed() {
        let report = SimucubeInputReport {
            wheel_speed_rpm: i16::MIN,
            ..Default::default()
        };
        assert!(report.wheel_speed_rad_s().is_finite());
        assert!(report.wheel_speed_rad_s() < 0.0);
    }

    #[test]
    fn extended_report_max_torque_feedback() {
        let report = SimucubeInputReport {
            torque_nm: i16::MAX,
            ..Default::default()
        };
        assert!(report.applied_torque_nm().is_finite());
    }

    #[test]
    fn extended_report_min_torque_feedback() {
        let report = SimucubeInputReport {
            torque_nm: i16::MIN,
            ..Default::default()
        };
        assert!(report.applied_torque_nm().is_finite());
    }

    #[test]
    fn extended_report_max_temperature() {
        let data = build_extended_bytes(0, 0, 0, 0, 255, 0, 0x01);
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.temperature_c, 255);
        }
    }

    #[test]
    fn extended_report_all_faults_active() {
        let report = SimucubeInputReport {
            fault_flags: 0xFF,
            ..Default::default()
        };
        assert!(report.has_fault());
    }

    #[test]
    fn output_report_max_effect_parameter() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0)
            .with_effect(EffectType::Spring, u16::MAX);
        let data = report.build()?;
        let param = u16::from_le_bytes([data[9], data[10]]);
        assert_eq!(param, u16::MAX);
        Ok(())
    }

    #[test]
    fn output_report_zero_effect_parameter() -> Result<(), SimucubeError> {
        let report = SimucubeOutputReport::new(0)
            .with_effect(EffectType::Constant, 0);
        let data = report.build()?;
        let param = u16::from_le_bytes([data[9], data[10]]);
        assert_eq!(param, 0);
        Ok(())
    }

    // -- Model-specific quirks --

    #[test]
    fn active_pedal_has_zero_torque() {
        assert_eq!(SimucubeModel::ActivePedal.max_torque_nm(), 0.0);
    }

    #[test]
    fn wireless_wheel_has_zero_torque() {
        assert_eq!(SimucubeModel::WirelessWheel.max_torque_nm(), 0.0);
    }

    #[test]
    fn simucube1_and_unknown_share_default_torque() {
        assert_eq!(
            SimucubeModel::Simucube1.max_torque_nm(),
            SimucubeModel::Unknown.max_torque_nm()
        );
    }

    #[test]
    fn hid_report_default_centered() {
        let report = SimucubeHidReport::default();
        assert_eq!(report.steering, 0x8000);
        assert_eq!(report.y_axis, 0x8000);
        assert_eq!(report.axes, [0u16; HID_ADDITIONAL_AXES]);
        assert_eq!(report.buttons, [0u8; HID_BUTTON_BYTES]);
    }

    #[test]
    fn extended_report_default_connected_enabled() {
        let report = SimucubeInputReport::default();
        assert_eq!(report.status_flags, 0x03);
        assert!(report.is_connected());
        assert!(report.is_enabled());
        assert!(!report.has_fault());
        assert_eq!(report.temperature_c, 25);
    }

    #[test]
    fn device_status_all_eight_bit_patterns() {
        // Exhaustively test all 256 possible flag byte values
        for flags in 0u16..=255 {
            let status = DeviceStatus::from_flags(flags as u8);
            // Must always return a valid variant
            let _ = format!("{status:?}"); // must not panic
        }
    }

    #[test]
    fn hid_report_alternating_buttons() -> Result<(), SimucubeError> {
        // Alternating bit pattern: 0xAA = 0b10101010
        let buttons = [0xAA; 16];
        let data = build_hid_bytes(0x8000, 0x8000, [0; 6], buttons);
        let report = SimucubeHidReport::parse(&data)?;
        // 0xAA has 4 bits set per byte, 16 bytes = 64 buttons pressed
        assert_eq!(report.pressed_count(), 64);
        // Button 0 = bit 0 of byte 0 = 0 (not pressed in 0xAA)
        assert!(!report.button_pressed(0));
        // Button 1 = bit 1 of byte 0 = 1 (pressed in 0xAA)
        assert!(report.button_pressed(1));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Property tests for encoding/decoding round-trips
// ═══════════════════════════════════════════════════════════════════════════════

mod property_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// HID report round-trip: parse(build(fields)) == fields.
        #[test]
        fn prop_hid_report_field_roundtrip(
            steering: u16,
            y_axis: u16,
            ax0: u16, ax1: u16, ax2: u16, ax3: u16, ax4: u16, ax5: u16,
            btn in proptest::collection::vec(any::<u8>(), 16..=16),
        ) {
            let mut buf = Vec::with_capacity(32);
            buf.extend_from_slice(&steering.to_le_bytes());
            buf.extend_from_slice(&y_axis.to_le_bytes());
            for ax in [ax0, ax1, ax2, ax3, ax4, ax5] {
                buf.extend_from_slice(&ax.to_le_bytes());
            }
            buf.extend_from_slice(&btn);
            let report = SimucubeHidReport::parse(&buf).map_err(|e| {
                TestCaseError::fail(format!("{e:?}"))
            })?;
            prop_assert_eq!(report.steering, steering);
            prop_assert_eq!(report.y_axis, y_axis);
            prop_assert_eq!(report.axes, [ax0, ax1, ax2, ax3, ax4, ax5]);
            prop_assert_eq!(&report.buttons[..], &btn[..]);
        }

        /// Steering normalized is always in [0.0, 1.0].
        #[test]
        fn prop_steering_normalized_range(steering: u16) {
            let report = SimucubeHidReport { steering, ..Default::default() };
            let n = report.steering_normalized();
            prop_assert!((0.0..=1.0).contains(&n), "normalized={n}");
        }

        /// Steering signed is always in [-1.0, ~1.0].
        #[test]
        fn prop_steering_signed_range(steering: u16) {
            let report = SimucubeHidReport { steering, ..Default::default() };
            let s = report.steering_signed();
            prop_assert!((-1.001..=1.001).contains(&s), "signed={s}");
        }

        /// Output report torque clamped to ±MAX_TORQUE_NM.
        #[test]
        fn prop_torque_always_clamped(torque in -500.0f32..500.0f32) {
            let report = SimucubeOutputReport::new(0).with_torque(torque);
            let decoded = report.torque_cNm as f32 / 100.0;
            prop_assert!(decoded >= -MAX_TORQUE_NM - 0.01);
            prop_assert!(decoded <= MAX_TORQUE_NM + 0.01);
        }

        /// Output report torque round-trip within clamped range.
        #[test]
        fn prop_torque_round_trip(torque in -MAX_TORQUE_NM..=MAX_TORQUE_NM) {
            let report = SimucubeOutputReport::new(0).with_torque(torque);
            let decoded = report.torque_cNm as f32 / 100.0;
            let error = (torque - decoded).abs();
            prop_assert!(error < 0.01, "torque={torque}, decoded={decoded}, error={error}");
        }

        /// Output report build always succeeds and produces correct size.
        #[test]
        fn prop_build_always_succeeds_correct_size(torque in -200.0f32..200.0f32, seq: u16) {
            let report = SimucubeOutputReport::new(seq).with_torque(torque);
            let result = report.build();
            prop_assert!(result.is_ok());
            if let Ok(data) = result {
                prop_assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
                prop_assert_eq!(data[0], 0x01);
            }
        }

        /// Output report sequence round-trips through build bytes.
        #[test]
        fn prop_sequence_round_trip(seq: u16) {
            let report = SimucubeOutputReport::new(seq);
            let data = report.build().map_err(|e| {
                TestCaseError::fail(format!("{e:?}"))
            })?;
            let recovered = u16::from_le_bytes([data[1], data[2]]);
            prop_assert_eq!(recovered, seq);
        }

        /// Output report effect type and parameter round-trip through build bytes.
        #[test]
        fn prop_effect_round_trip(effect_idx in 0u8..=10u8, parameter: u16) {
            let effect = match effect_idx {
                0 => EffectType::None,
                1 => EffectType::Constant,
                2 => EffectType::Ramp,
                3 => EffectType::Square,
                4 => EffectType::Sine,
                5 => EffectType::Triangle,
                6 => EffectType::SawtoothUp,
                7 => EffectType::SawtoothDown,
                8 => EffectType::Spring,
                9 => EffectType::Damper,
                _ => EffectType::Friction,
            };
            let report = SimucubeOutputReport::new(0).with_effect(effect, parameter);
            let data = report.build().map_err(|e| {
                TestCaseError::fail(format!("{e:?}"))
            })?;
            prop_assert_eq!(data[8], effect_idx.min(10));
            let recovered = u16::from_le_bytes([data[9], data[10]]);
            prop_assert_eq!(recovered, parameter);
        }

        /// RGB values round-trip through build bytes.
        #[test]
        fn prop_rgb_round_trip(r: u8, g: u8, b: u8) {
            let report = SimucubeOutputReport::new(0).with_rgb(r, g, b);
            let data = report.build().map_err(|e| {
                TestCaseError::fail(format!("{e:?}"))
            })?;
            prop_assert_eq!(data[5], r);
            prop_assert_eq!(data[6], g);
            prop_assert_eq!(data[7], b);
        }

        /// SimucubeModel::from_product_id is deterministic.
        #[test]
        fn prop_model_detection_deterministic(pid: u16) {
            let a = SimucubeModel::from_product_id(pid);
            let b = SimucubeModel::from_product_id(pid);
            prop_assert_eq!(a, b);
        }

        /// Wrong VID always yields Unknown.
        #[test]
        fn prop_wrong_vid_yields_unknown(vid: u16, pid: u16) {
            prop_assume!(vid != SIMUCUBE_VENDOR_ID);
            let model = simucube_model_from_info(vid, pid);
            prop_assert_eq!(model, SimucubeModel::Unknown);
        }

        /// max_torque_nm is always non-negative and finite for any PID.
        #[test]
        fn prop_max_torque_non_negative_finite(pid: u16) {
            let model = SimucubeModel::from_product_id(pid);
            let torque = model.max_torque_nm();
            prop_assert!(torque >= 0.0);
            prop_assert!(torque.is_finite());
        }

        /// display_name is never empty for any PID.
        #[test]
        fn prop_display_name_non_empty(pid: u16) {
            let model = SimucubeModel::from_product_id(pid);
            prop_assert!(!model.display_name().is_empty());
        }

        /// HID report parse fails for any buffer shorter than minimum.
        #[test]
        fn prop_short_buffer_rejected(len in 0usize..HID_JOYSTICK_REPORT_MIN_BYTES) {
            let data = vec![0u8; len];
            let result = SimucubeHidReport::parse(&data);
            prop_assert!(result.is_err());
        }

        /// HID report parse succeeds for any buffer at or above minimum size.
        #[test]
        fn prop_valid_size_accepted(data in proptest::collection::vec(any::<u8>(), HID_JOYSTICK_REPORT_MIN_BYTES..=128)) {
            let result = SimucubeHidReport::parse(&data);
            prop_assert!(result.is_ok());
        }

        /// Extended input report parse fails for buffers shorter than 16 bytes.
        #[test]
        fn prop_extended_short_buffer_rejected(len in 0usize..16) {
            let data = vec![0u8; len];
            let result = SimucubeInputReport::parse(&data);
            prop_assert!(result.is_err());
        }

        /// Torque sign preservation: non-negative input -> non-negative cNm.
        #[test]
        fn prop_nonneg_torque_nonneg_cnm(torque in 0.0f32..200.0f32) {
            let report = SimucubeOutputReport::new(0).with_torque(torque);
            prop_assert!(report.torque_cNm >= 0);
        }

        /// Torque sign preservation: non-positive input -> non-positive cNm.
        #[test]
        fn prop_nonpos_torque_nonpos_cnm(torque in -200.0f32..=0.0f32) {
            let report = SimucubeOutputReport::new(0).with_torque(torque);
            prop_assert!(report.torque_cNm <= 0);
        }

        /// Torque monotonicity within the valid range.
        #[test]
        fn prop_torque_monotone(
            t1 in 0.0f32..MAX_TORQUE_NM,
            t2 in 0.0f32..MAX_TORQUE_NM,
        ) {
            let r1 = SimucubeOutputReport::new(0).with_torque(t1);
            let r2 = SimucubeOutputReport::new(0).with_torque(t2);
            if t1 < t2 - 0.01 {
                prop_assert!(r1.torque_cNm <= r2.torque_cNm);
            }
        }

        /// button_pressed returns false for all out-of-range indices.
        #[test]
        fn prop_button_out_of_range(idx in 128usize..1024) {
            let report = SimucubeHidReport::default();
            prop_assert!(!report.button_pressed(idx));
        }

        /// pressed_count matches manual popcount of button bytes.
        #[test]
        fn prop_pressed_count_matches(data in proptest::collection::vec(any::<u8>(), 32..=64)) {
            let report = SimucubeHidReport::parse(&data).map_err(|e| {
                TestCaseError::fail(format!("{e:?}"))
            })?;
            let expected: u32 = report.buttons.iter().map(|b| b.count_ones()).sum();
            prop_assert_eq!(report.pressed_count(), expected);
        }

        /// DeviceStatus::from_flags never panics for any u8 value.
        #[test]
        fn prop_device_status_never_panics(flags: u8) {
            let _ = DeviceStatus::from_flags(flags);
        }

        /// Disconnected (bit 0 == 0) always yields Disconnected status.
        #[test]
        fn prop_disconnected_when_bit0_unset(flags: u8) {
            prop_assume!(flags & 0x01 == 0);
            prop_assert_eq!(DeviceStatus::from_flags(flags), DeviceStatus::Disconnected);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Constant validation (VID/PIDs, torque ranges, encoder resolution)
// ═══════════════════════════════════════════════════════════════════════════════

mod constant_validation {
    use super::*;

    #[test]
    fn vendor_id_is_0x16d0() {
        assert_eq!(VENDOR_ID, 0x16D0);
        assert_eq!(SIMUCUBE_VENDOR_ID, 0x16D0);
        // Both constants must agree
        assert_eq!(VENDOR_ID, SIMUCUBE_VENDOR_ID);
    }

    #[test]
    fn product_ids_match_ids_module() {
        assert_eq!(PRODUCT_ID_SPORT, SIMUCUBE_2_SPORT_PID);
        assert_eq!(PRODUCT_ID_PRO, SIMUCUBE_2_PRO_PID);
        assert_eq!(PRODUCT_ID_ULTIMATE, SIMUCUBE_2_ULTIMATE_PID);
    }

    #[test]
    fn known_pids_golden_values() {
        assert_eq!(SIMUCUBE_1_PID, 0x0D5A);
        assert_eq!(SIMUCUBE_2_SPORT_PID, 0x0D61);
        assert_eq!(SIMUCUBE_2_PRO_PID, 0x0D60);
        assert_eq!(SIMUCUBE_2_ULTIMATE_PID, 0x0D5F);
        assert_eq!(SIMUCUBE_ACTIVE_PEDAL_PID, 0x0D66);
        assert_eq!(SIMUCUBE_WIRELESS_WHEEL_PID, 0x0D63);
    }

    #[test]
    fn bootloader_pids_golden_values() {
        assert_eq!(SIMUCUBE_1_BOOTLOADER_PID, 0x0D5B);
        assert_eq!(SIMUCUBE_2_BOOTLOADER_PID, 0x0D5E);
    }

    #[test]
    fn all_pids_unique() {
        let pids = [
            SIMUCUBE_1_PID,
            SIMUCUBE_2_SPORT_PID,
            SIMUCUBE_2_PRO_PID,
            SIMUCUBE_2_ULTIMATE_PID,
            SIMUCUBE_ACTIVE_PEDAL_PID,
            SIMUCUBE_WIRELESS_WHEEL_PID,
            SIMUCUBE_1_BOOTLOADER_PID,
            SIMUCUBE_2_BOOTLOADER_PID,
        ];
        for (i, a) in pids.iter().enumerate() {
            for (j, b) in pids.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "PID at {i} ({a:#06X}) collides with {j} ({b:#06X})");
                }
            }
        }
    }

    #[test]
    fn all_pids_nonzero() {
        let pids = [
            SIMUCUBE_1_PID,
            SIMUCUBE_2_SPORT_PID,
            SIMUCUBE_2_PRO_PID,
            SIMUCUBE_2_ULTIMATE_PID,
            SIMUCUBE_ACTIVE_PEDAL_PID,
            SIMUCUBE_WIRELESS_WHEEL_PID,
            SIMUCUBE_1_BOOTLOADER_PID,
            SIMUCUBE_2_BOOTLOADER_PID,
        ];
        for pid in pids {
            assert_ne!(pid, 0, "PID {pid:#06X} must not be zero");
        }
    }

    #[test]
    fn torque_range_constants() {
        assert_eq!(MAX_TORQUE_SPORT, 17.0);
        assert_eq!(MAX_TORQUE_PRO, 25.0);
        assert_eq!(MAX_TORQUE_ULTIMATE, 32.0);
        assert_eq!(MAX_TORQUE_NM, 25.0);
    }

    #[test]
    fn torque_constants_positive_finite() {
        for &t in &[MAX_TORQUE_NM, MAX_TORQUE_SPORT, MAX_TORQUE_PRO, MAX_TORQUE_ULTIMATE] {
            assert!(t > 0.0, "torque constant {t} must be positive");
            assert!(t.is_finite(), "torque constant {t} must be finite");
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn torque_ordering() {
        assert!(MAX_TORQUE_SPORT < MAX_TORQUE_PRO);
        assert!(MAX_TORQUE_PRO < MAX_TORQUE_ULTIMATE);
    }

    #[test]
    fn max_torque_constants_match_model_methods() {
        assert_eq!(MAX_TORQUE_SPORT, SimucubeModel::Sport.max_torque_nm());
        assert_eq!(MAX_TORQUE_PRO, SimucubeModel::Pro.max_torque_nm());
        assert_eq!(MAX_TORQUE_ULTIMATE, SimucubeModel::Ultimate.max_torque_nm());
    }

    #[test]
    fn encoder_resolution() {
        assert_eq!(ANGLE_SENSOR_BITS, 22);
        assert_eq!(ANGLE_SENSOR_MAX, (1 << 22) - 1);
        assert_eq!(ANGLE_SENSOR_MAX, 0x3F_FFFF);
    }

    #[test]
    fn hid_layout_constants() {
        assert_eq!(HID_ADDITIONAL_AXES, 6);
        assert_eq!(HID_BUTTON_COUNT, 128);
        assert_eq!(HID_BUTTON_BYTES, 16);
        assert_eq!(HID_BUTTON_BYTES, HID_BUTTON_COUNT / 8);
        // 8 axes * 2 bytes + 16 button bytes = 32
        assert_eq!(HID_JOYSTICK_REPORT_MIN_BYTES, 32);
        assert_eq!(HID_JOYSTICK_REPORT_MIN_BYTES, 8 * 2 + HID_BUTTON_BYTES);
    }

    #[test]
    fn report_sizes() {
        assert_eq!(REPORT_SIZE_INPUT, 64);
        assert_eq!(REPORT_SIZE_OUTPUT, 64);
    }

    #[test]
    fn all_torque_values_fit_i16() {
        // Ensure no model's max torque * 100 overflows i16
        let models = [
            MAX_TORQUE_SPORT,
            MAX_TORQUE_PRO,
            MAX_TORQUE_ULTIMATE,
        ];
        for &t in &models {
            let cnm = (t * 100.0) as i32;
            assert!(
                cnm <= i16::MAX as i32,
                "torque {t} Nm = {cnm} cNm overflows i16::MAX"
            );
            assert!(
                -cnm >= i16::MIN as i32,
                "torque -{t} Nm = {cnm} cNm overflows i16::MIN"
            );
        }
    }

    #[test]
    fn simucube_error_display() {
        let err = SimucubeError::InvalidReportSize {
            expected: 32,
            actual: 16,
        };
        let msg = format!("{err}");
        assert!(msg.contains("32"));
        assert!(msg.contains("16"));

        let err = SimucubeError::InvalidTorque(99.9);
        let msg = format!("{err}");
        assert!(msg.contains("99.9"));

        let err = SimucubeError::DeviceNotFound("test".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("test"));

        let err = SimucubeError::Communication("fail".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("fail"));
    }
}
