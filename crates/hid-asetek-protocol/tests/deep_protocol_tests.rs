//! Deep protocol tests for Asetek HID protocol.
//!
//! Tests cover device identification, input report parsing, output report
//! encoding, model capabilities, and calibration.

use hid_asetek_protocol::{
    asetek_model_from_info, is_asetek_device, AsetekError, AsetekInputReport, AsetekModel,
    AsetekOutputReport, WheelCapabilities, WheelModel, ASETEK_FORTE_PEDALS_PID, ASETEK_FORTE_PID,
    ASETEK_INVICTA_PEDALS_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PEDALS_PID, ASETEK_LAPRIMA_PID,
    ASETEK_TONY_KANAAN_PID, ASETEK_VENDOR_ID, MAX_TORQUE_NM, REPORT_SIZE_INPUT,
    REPORT_SIZE_OUTPUT, VENDOR_ID,
};

// ─── Device identification ───────────────────────────────────────────────────

#[test]
fn vendor_id_is_asetek() {
    assert_eq!(ASETEK_VENDOR_ID, 0x2433);
    assert_eq!(VENDOR_ID, 0x2433);
}

#[test]
fn is_asetek_device_recognises_vendor() {
    assert!(is_asetek_device(0x2433));
    assert!(!is_asetek_device(0x0000));
    assert!(!is_asetek_device(0xFFFF));
}

#[test]
fn all_wheelbase_pids_map_to_correct_models() {
    assert_eq!(AsetekModel::from_product_id(ASETEK_FORTE_PID), AsetekModel::Forte);
    assert_eq!(AsetekModel::from_product_id(ASETEK_INVICTA_PID), AsetekModel::Invicta);
    assert_eq!(AsetekModel::from_product_id(ASETEK_LAPRIMA_PID), AsetekModel::LaPrima);
    assert_eq!(AsetekModel::from_product_id(ASETEK_TONY_KANAAN_PID), AsetekModel::TonyKanaan);
}

#[test]
fn all_pedal_pids_map_to_correct_models() {
    assert_eq!(AsetekModel::from_product_id(ASETEK_INVICTA_PEDALS_PID), AsetekModel::InvictaPedals);
    assert_eq!(AsetekModel::from_product_id(ASETEK_FORTE_PEDALS_PID), AsetekModel::FortePedals);
    assert_eq!(AsetekModel::from_product_id(ASETEK_LAPRIMA_PEDALS_PID), AsetekModel::LaPrimaPedals);
}

#[test]
fn unknown_pid_maps_to_unknown() {
    assert_eq!(AsetekModel::from_product_id(0xFFFF), AsetekModel::Unknown);
    assert_eq!(AsetekModel::from_product_id(0x0000), AsetekModel::Unknown);
}

#[test]
fn model_from_info_rejects_wrong_vendor() {
    let model = asetek_model_from_info(0x1234, ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Unknown);
}

#[test]
fn model_from_info_accepts_correct_vendor() {
    let model = asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_INVICTA_PID);
    assert_eq!(model, AsetekModel::Invicta);
}

// ─── Model properties ────────────────────────────────────────────────────────

#[test]
fn max_torque_per_model() {
    assert_eq!(AsetekModel::Forte.max_torque_nm(), 18.0);
    assert_eq!(AsetekModel::Invicta.max_torque_nm(), 27.0);
    assert_eq!(AsetekModel::LaPrima.max_torque_nm(), 12.0);
    assert_eq!(AsetekModel::TonyKanaan.max_torque_nm(), 27.0);
}

#[test]
fn pedal_models_have_zero_torque() {
    assert_eq!(AsetekModel::InvictaPedals.max_torque_nm(), 0.0);
    assert_eq!(AsetekModel::FortePedals.max_torque_nm(), 0.0);
    assert_eq!(AsetekModel::LaPrimaPedals.max_torque_nm(), 0.0);
}

#[test]
fn display_names_non_empty() {
    let models = [
        AsetekModel::Forte, AsetekModel::Invicta, AsetekModel::LaPrima,
        AsetekModel::TonyKanaan, AsetekModel::InvictaPedals,
        AsetekModel::FortePedals, AsetekModel::LaPrimaPedals, AsetekModel::Unknown,
    ];
    for m in &models {
        assert!(!m.display_name().is_empty());
    }
}

// ─── Input report parsing ────────────────────────────────────────────────────

#[test]
fn parse_input_report_minimum_valid() -> Result<(), AsetekError> {
    let data = [0u8; 16];
    let report = AsetekInputReport::parse(&data)?;
    assert_eq!(report.sequence, 0);
    assert_eq!(report.wheel_angle, 0);
    assert_eq!(report.wheel_speed, 0);
    assert_eq!(report.torque, 0);
    Ok(())
}

#[test]
fn parse_input_report_rejects_short_buffer() {
    let data = [0u8; 15];
    let result = AsetekInputReport::parse(&data);
    assert!(matches!(result, Err(AsetekError::InvalidReportSize { expected: 16, actual: 15 })));
}

#[test]
fn parse_input_report_known_values() -> Result<(), AsetekError> {
    // Construct: seq=1, angle=90000 (90°), speed=100, torque=1500 (15Nm), temp=42, status=0x03
    let mut data = [0u8; 16];
    data[0..2].copy_from_slice(&1u16.to_le_bytes());
    data[2..6].copy_from_slice(&90_000i32.to_le_bytes());
    data[6..8].copy_from_slice(&100i16.to_le_bytes());
    data[8..10].copy_from_slice(&1500i16.to_le_bytes());
    data[10] = 42; // temperature
    data[11] = 0x03; // status: connected + enabled

    let report = AsetekInputReport::parse(&data)?;
    assert_eq!(report.sequence, 1);
    assert_eq!(report.wheel_angle, 90_000);
    assert!((report.wheel_angle_degrees() - 90.0).abs() < 0.1);
    assert!((report.applied_torque_nm() - 15.0).abs() < 0.01);
    assert_eq!(report.temperature, 42);
    assert!(report.is_connected());
    assert!(report.is_enabled());
    Ok(())
}

#[test]
fn input_report_status_bits() {
    let report_00 = AsetekInputReport { status: 0x00, ..Default::default() };
    assert!(!report_00.is_connected());
    assert!(!report_00.is_enabled());

    let report_01 = AsetekInputReport { status: 0x01, ..Default::default() };
    assert!(report_01.is_connected());
    assert!(!report_01.is_enabled());

    let report_02 = AsetekInputReport { status: 0x02, ..Default::default() };
    assert!(!report_02.is_connected());
    assert!(report_02.is_enabled());

    let report_03 = AsetekInputReport { status: 0x03, ..Default::default() };
    assert!(report_03.is_connected());
    assert!(report_03.is_enabled());
}

#[test]
fn wheel_speed_rad_s_conversion() {
    let report = AsetekInputReport {
        wheel_speed: 1800,
        ..Default::default()
    };
    let rad_s = report.wheel_speed_rad_s();
    assert!((rad_s - std::f32::consts::PI).abs() < 0.01);
}

// ─── Output report encoding ─────────────────────────────────────────────────

#[test]
fn output_report_default_values() {
    let report = AsetekOutputReport::default();
    assert_eq!(report.sequence, 0);
    assert_eq!(report.torque_cNm, 0);
    assert_eq!(report.led_mode, 0);
    assert_eq!(report.led_value, 0);
}

#[test]
fn output_report_torque_conversion() {
    let report = AsetekOutputReport::new(1).with_torque(10.5);
    assert_eq!(report.torque_cNm, 1050);
}

#[test]
fn output_report_torque_clamping() {
    let over = AsetekOutputReport::new(0).with_torque(50.0);
    assert_eq!(over.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);

    let under = AsetekOutputReport::new(0).with_torque(-50.0);
    assert_eq!(under.torque_cNm, (-MAX_TORQUE_NM * 100.0) as i16);
}

#[test]
fn output_report_build_size() -> Result<(), AsetekError> {
    let report = AsetekOutputReport::new(42).with_torque(15.0);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn output_report_led_builder() {
    let report = AsetekOutputReport::new(0).with_led(0x01, 0xFF);
    assert_eq!(report.led_mode, 0x01);
    assert_eq!(report.led_value, 0xFF);
}

// ─── Wheel capabilities ─────────────────────────────────────────────────────

#[test]
fn wheel_capabilities_per_model() {
    let forte = WheelCapabilities::for_model(WheelModel::Forte);
    assert_eq!(forte.max_torque_nm, 18.0);
    assert!(forte.supports_quick_release);

    let invicta = WheelCapabilities::for_model(WheelModel::Invicta);
    assert_eq!(invicta.max_torque_nm, 27.0);

    let laprima = WheelCapabilities::for_model(WheelModel::LaPrima);
    assert_eq!(laprima.max_torque_nm, 12.0);
}

#[test]
fn report_sizes_are_consistent() {
    assert_eq!(REPORT_SIZE_INPUT, 32);
    assert_eq!(REPORT_SIZE_OUTPUT, 32);
}
