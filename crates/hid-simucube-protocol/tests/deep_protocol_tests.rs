//! Deep protocol tests for Simucube HID protocol.
//!
//! Tests cover device identification (SC1, SC2 Sport/Pro/Ultimate),
//! torque encoding, input report parsing, wireless wheel support, and
//! effect types.

use hid_simucube_protocol::{
    is_simucube_device, simucube_model_from_info, DeviceStatus, EffectType, SimucubeError,
    SimucubeHidReport, SimucubeInputReport, SimucubeModel, SimucubeOutputReport, WheelCapabilities,
    WheelModel, ANGLE_SENSOR_BITS, ANGLE_SENSOR_MAX, HID_ADDITIONAL_AXES, HID_BUTTON_BYTES,
    HID_BUTTON_COUNT, HID_JOYSTICK_REPORT_MIN_BYTES, MAX_TORQUE_NM, MAX_TORQUE_PRO,
    MAX_TORQUE_SPORT, MAX_TORQUE_ULTIMATE, PRODUCT_ID_PRO, PRODUCT_ID_SPORT, PRODUCT_ID_ULTIMATE,
    REPORT_SIZE_INPUT, REPORT_SIZE_OUTPUT, SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_1_PID,
    SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID,
    SIMUCUBE_WIRELESS_WHEEL_PID, VENDOR_ID,
};

// ─── Device identification ───────────────────────────────────────────────────

#[test]
fn vendor_id_is_granite_devices() {
    assert_eq!(SIMUCUBE_VENDOR_ID, 0x16D0);
    assert_eq!(VENDOR_ID, 0x16D0);
}

#[test]
fn is_simucube_device_recognises_vendor() {
    assert!(is_simucube_device(0x16D0));
    assert!(!is_simucube_device(0x0000));
}

#[test]
fn sc1_pid() {
    assert_eq!(SIMUCUBE_1_PID, 0x0D5A);
}

#[test]
fn sc2_sport_pid() {
    assert_eq!(SIMUCUBE_2_SPORT_PID, 0x0D61);
    assert_eq!(PRODUCT_ID_SPORT, 0x0D61);
}

#[test]
fn sc2_pro_pid() {
    assert_eq!(SIMUCUBE_2_PRO_PID, 0x0D60);
    assert_eq!(PRODUCT_ID_PRO, 0x0D60);
}

#[test]
fn sc2_ultimate_pid() {
    assert_eq!(SIMUCUBE_2_ULTIMATE_PID, 0x0D5F);
    assert_eq!(PRODUCT_ID_ULTIMATE, 0x0D5F);
}

#[test]
fn model_from_product_id_all_variants() {
    assert_eq!(SimucubeModel::from_product_id(SIMUCUBE_1_PID), SimucubeModel::Simucube1);
    assert_eq!(SimucubeModel::from_product_id(SIMUCUBE_2_SPORT_PID), SimucubeModel::Sport);
    assert_eq!(SimucubeModel::from_product_id(SIMUCUBE_2_PRO_PID), SimucubeModel::Pro);
    assert_eq!(SimucubeModel::from_product_id(SIMUCUBE_2_ULTIMATE_PID), SimucubeModel::Ultimate);
    assert_eq!(SimucubeModel::from_product_id(SIMUCUBE_ACTIVE_PEDAL_PID), SimucubeModel::ActivePedal);
    assert_eq!(SimucubeModel::from_product_id(SIMUCUBE_WIRELESS_WHEEL_PID), SimucubeModel::WirelessWheel);
    assert_eq!(SimucubeModel::from_product_id(0xFFFF), SimucubeModel::Unknown);
}

#[test]
fn model_from_info_rejects_wrong_vendor() {
    assert_eq!(simucube_model_from_info(0x1234, SIMUCUBE_2_PRO_PID), SimucubeModel::Unknown);
}

#[test]
fn bootloader_pids_distinct_from_runtime() {
    assert_ne!(SIMUCUBE_1_BOOTLOADER_PID, SIMUCUBE_1_PID);
    assert_ne!(SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_SPORT_PID);
    assert_ne!(SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_PRO_PID);
    assert_ne!(SIMUCUBE_2_BOOTLOADER_PID, SIMUCUBE_2_ULTIMATE_PID);
}

// ─── Model torque & capabilities ─────────────────────────────────────────────

#[test]
fn max_torque_per_model() {
    assert_eq!(SimucubeModel::Sport.max_torque_nm(), 17.0);
    assert_eq!(SimucubeModel::Pro.max_torque_nm(), 25.0);
    assert_eq!(SimucubeModel::Ultimate.max_torque_nm(), 32.0);
    assert_eq!(SimucubeModel::Simucube1.max_torque_nm(), 25.0);
}

#[test]
fn max_torque_constants_match_models() {
    assert_eq!(MAX_TORQUE_SPORT, SimucubeModel::Sport.max_torque_nm());
    assert_eq!(MAX_TORQUE_PRO, SimucubeModel::Pro.max_torque_nm());
    assert_eq!(MAX_TORQUE_ULTIMATE, SimucubeModel::Ultimate.max_torque_nm());
}

#[test]
fn wheel_capabilities_sport() {
    let caps = WheelCapabilities::for_model(WheelModel::Simucube2Sport);
    assert_eq!(caps.max_torque_nm, 17.0);
    assert!(caps.supports_wireless);
}

#[test]
fn wheel_capabilities_active_pedal() {
    let caps = WheelCapabilities::for_model(WheelModel::SimucubeActivePedal);
    assert_eq!(caps.max_torque_nm, 0.0);
    assert!(!caps.supports_wireless);
    assert!(caps.supports_active_pedal);
}

// ─── HID joystick report ────────────────────────────────────────────────────

fn make_hid_report(steering: u16, y: u16, axes: [u16; 6], buttons: [u8; 16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&steering.to_le_bytes());
    buf.extend_from_slice(&y.to_le_bytes());
    for ax in &axes {
        buf.extend_from_slice(&ax.to_le_bytes());
    }
    buf.extend_from_slice(&buttons);
    buf
}

#[test]
fn hid_report_parse_center() -> Result<(), SimucubeError> {
    let data = make_hid_report(0x8000, 0x8000, [0; 6], [0; 16]);
    let report = SimucubeHidReport::parse(&data)?;
    assert_eq!(report.steering, 0x8000);
    let signed = report.steering_signed();
    assert!(signed.abs() < 0.001);
    Ok(())
}

#[test]
fn hid_report_rejects_short_data() {
    let data = [0u8; 31];
    let result = SimucubeHidReport::parse(&data);
    assert!(matches!(result, Err(SimucubeError::InvalidReportSize { expected: 32, actual: 31 })));
}

#[test]
fn hid_report_button_pressed() -> Result<(), SimucubeError> {
    let mut buttons = [0u8; 16];
    buttons[0] = 0b0000_0001; // button 0
    buttons[15] = 0b1000_0000; // button 127
    let data = make_hid_report(0x8000, 0x8000, [0; 6], buttons);
    let report = SimucubeHidReport::parse(&data)?;
    assert!(report.button_pressed(0));
    assert!(report.button_pressed(127));
    assert!(!report.button_pressed(1));
    assert_eq!(report.pressed_count(), 2);
    Ok(())
}

#[test]
fn hid_report_out_of_range_button_returns_false() {
    let report = SimucubeHidReport::default();
    assert!(!report.button_pressed(128));
    assert!(!report.button_pressed(999));
}

#[test]
fn hid_report_steering_normalized() -> Result<(), SimucubeError> {
    let data = make_hid_report(u16::MAX, 0, [0; 6], [0; 16]);
    let report = SimucubeHidReport::parse(&data)?;
    assert!((report.steering_normalized() - 1.0).abs() < 0.001);
    Ok(())
}

// ─── Wireless wheel support ─────────────────────────────────────────────────

#[test]
fn wireless_wheel_detected_from_battery() {
    let report = SimucubeInputReport {
        wireless_battery_pct: 50,
        wireless_buttons: 0,
        ..Default::default()
    };
    assert!(report.has_wireless_wheel());
}

#[test]
fn wireless_wheel_detected_from_buttons() {
    let report = SimucubeInputReport {
        wireless_battery_pct: 0,
        wireless_buttons: 0x0001,
        ..Default::default()
    };
    assert!(report.has_wireless_wheel());
}

#[test]
fn no_wireless_wheel_when_zero() {
    let report = SimucubeInputReport {
        wireless_battery_pct: 0,
        wireless_buttons: 0,
        ..Default::default()
    };
    assert!(!report.has_wireless_wheel());
}

// ─── Output report ──────────────────────────────────────────────────────────

#[test]
fn output_report_torque_encoding() {
    let report = SimucubeOutputReport::new(1).with_torque(10.5);
    assert_eq!(report.torque_cNm, 1050);
}

#[test]
fn output_report_torque_clamps_to_max() {
    let report = SimucubeOutputReport::new(0).with_torque(100.0);
    assert_eq!(report.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);
}

#[test]
fn output_report_build_size() -> Result<(), SimucubeError> {
    let report = SimucubeOutputReport::new(0).with_torque(5.0);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn output_report_effect_type() {
    let report = SimucubeOutputReport::new(0).with_effect(EffectType::Damper, 1000);
    assert_eq!(report.effect_type, EffectType::Damper);
    assert_eq!(report.effect_parameter, 1000);
}

#[test]
fn effect_type_discriminants() {
    assert_eq!(EffectType::None as u8, 0);
    assert_eq!(EffectType::Constant as u8, 1);
    assert_eq!(EffectType::Sine as u8, 4);
    assert_eq!(EffectType::Spring as u8, 8);
    assert_eq!(EffectType::Friction as u8, 10);
}

// ─── Device status ──────────────────────────────────────────────────────────

#[test]
fn device_status_from_flags() {
    assert_eq!(DeviceStatus::from_flags(0x00), DeviceStatus::Disconnected);
    assert_eq!(DeviceStatus::from_flags(0x01), DeviceStatus::Ready);
    assert_eq!(DeviceStatus::from_flags(0x03), DeviceStatus::Enabled);
    assert_eq!(DeviceStatus::from_flags(0x05), DeviceStatus::Calibrating);
}

// ─── Constants ──────────────────────────────────────────────────────────────

#[test]
fn angle_sensor_constants() {
    assert_eq!(ANGLE_SENSOR_BITS, 22);
    assert_eq!(ANGLE_SENSOR_MAX, 0x3FFFFF);
    assert_eq!(ANGLE_SENSOR_MAX, (1 << 22) - 1);
}

#[test]
fn hid_report_size_constants() {
    assert_eq!(HID_ADDITIONAL_AXES, 6);
    assert_eq!(HID_BUTTON_COUNT, 128);
    assert_eq!(HID_BUTTON_BYTES, 16);
    assert_eq!(HID_JOYSTICK_REPORT_MIN_BYTES, 32);
    assert_eq!(REPORT_SIZE_INPUT, 64);
    assert_eq!(REPORT_SIZE_OUTPUT, 64);
}
