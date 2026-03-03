//! Deep pedal-specific protocol tests for Asetek HID protocol.
//!
//! Covers Invicta/Forte/La Prima pedal identification, force feedback on
//! pedals, and haptic feedback modes.

use hid_asetek_protocol::{
    ASETEK_FORTE_PEDALS_PID, ASETEK_FORTE_PID, ASETEK_INVICTA_PEDALS_PID, ASETEK_INVICTA_PID,
    ASETEK_LAPRIMA_PEDALS_PID, ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID, ASETEK_VENDOR_ID,
    AsetekInputReport, AsetekModel, AsetekOutputReport, AsetekResult, MAX_TORQUE_NM,
    REPORT_SIZE_OUTPUT, WheelCapabilities, WheelModel, asetek_model_from_info, is_asetek_device,
};

// ─── Pedal identification ───────────────────────────────────────────────────

#[test]
fn invicta_pedals_identified_by_pid() {
    let model = AsetekModel::from_product_id(ASETEK_INVICTA_PEDALS_PID);
    assert_eq!(model, AsetekModel::InvictaPedals);
    assert_eq!(model.display_name(), "Asetek Invicta Pedals");
}

#[test]
fn forte_pedals_identified_by_pid() {
    let model = AsetekModel::from_product_id(ASETEK_FORTE_PEDALS_PID);
    assert_eq!(model, AsetekModel::FortePedals);
    assert_eq!(model.display_name(), "Asetek Forte Pedals");
}

#[test]
fn la_prima_pedals_identified_by_pid() {
    let model = AsetekModel::from_product_id(ASETEK_LAPRIMA_PEDALS_PID);
    assert_eq!(model, AsetekModel::LaPrimaPedals);
    assert_eq!(model.display_name(), "Asetek La Prima Pedals");
}

#[test]
fn pedal_pids_follow_f10x_pattern() {
    assert_eq!(ASETEK_INVICTA_PEDALS_PID, 0xF100);
    assert_eq!(ASETEK_FORTE_PEDALS_PID, 0xF101);
    assert_eq!(ASETEK_LAPRIMA_PEDALS_PID, 0xF102);
}

#[test]
fn pedal_models_report_zero_torque() {
    assert_eq!(AsetekModel::InvictaPedals.max_torque_nm(), 0.0);
    assert_eq!(AsetekModel::FortePedals.max_torque_nm(), 0.0);
    assert_eq!(AsetekModel::LaPrimaPedals.max_torque_nm(), 0.0);
}

#[test]
fn pedal_models_via_vid_pid_with_correct_vendor() {
    let model = asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_INVICTA_PEDALS_PID);
    assert_eq!(model, AsetekModel::InvictaPedals);

    let model = asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_FORTE_PEDALS_PID);
    assert_eq!(model, AsetekModel::FortePedals);

    let model = asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_LAPRIMA_PEDALS_PID);
    assert_eq!(model, AsetekModel::LaPrimaPedals);
}

#[test]
fn pedal_models_via_wrong_vendor_id_return_unknown() {
    let model = asetek_model_from_info(0x0000, ASETEK_INVICTA_PEDALS_PID);
    assert_eq!(model, AsetekModel::Unknown);
}

#[test]
fn wheelbase_pids_distinct_from_pedal_pids() {
    let wheelbase_pids = [
        ASETEK_FORTE_PID,
        ASETEK_INVICTA_PID,
        ASETEK_LAPRIMA_PID,
        ASETEK_TONY_KANAAN_PID,
    ];
    let pedal_pids = [
        ASETEK_INVICTA_PEDALS_PID,
        ASETEK_FORTE_PEDALS_PID,
        ASETEK_LAPRIMA_PEDALS_PID,
    ];
    for wp in &wheelbase_pids {
        for pp in &pedal_pids {
            assert_ne!(wp, pp, "wheelbase PID should not equal pedal PID");
        }
    }
}

// ─── Force feedback on pedals ───────────────────────────────────────────────

#[test]
fn output_report_torque_positive() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(1).with_torque(10.0);
    assert_eq!(report.torque_cNm, 1000);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn output_report_torque_negative() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(2).with_torque(-15.5);
    assert_eq!(report.torque_cNm, -1550);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn output_report_torque_clamped_to_max() {
    let report = AsetekOutputReport::new(1).with_torque(100.0);
    // MAX_TORQUE_NM is 27.0, so should clamp
    assert_eq!(report.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);
}

#[test]
fn output_report_torque_clamped_to_negative_max() {
    let report = AsetekOutputReport::new(1).with_torque(-100.0);
    assert_eq!(report.torque_cNm, (-MAX_TORQUE_NM * 100.0) as i16);
}

#[test]
fn output_report_zero_torque() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(0).with_torque(0.0);
    assert_eq!(report.torque_cNm, 0);
    let data = report.build()?;
    // First two bytes are sequence (LE), next two are torque (LE)
    assert_eq!(data[2], 0);
    assert_eq!(data[3], 0);
    Ok(())
}

#[test]
fn output_report_with_led_control() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(5).with_led(0x01, 0xFF);
    assert_eq!(report.led_mode, 0x01);
    assert_eq!(report.led_value, 0xFF);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn output_report_sequence_encoded_correctly() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(0x1234);
    let data = report.build()?;
    assert_eq!(data[0], 0x34); // low byte
    assert_eq!(data[1], 0x12); // high byte
    Ok(())
}

// ─── Haptic feedback modes ──────────────────────────────────────────────────

#[test]
fn haptic_torque_granularity_centinewton_metres() {
    // 0.01 Nm precision — verify smallest representable torque
    let report = AsetekOutputReport::new(0).with_torque(0.01);
    assert_eq!(report.torque_cNm, 1);
}

#[test]
fn haptic_combined_torque_and_led() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(10)
        .with_torque(5.0)
        .with_led(0x02, 0x80);
    assert_eq!(report.torque_cNm, 500);
    assert_eq!(report.led_mode, 0x02);
    assert_eq!(report.led_value, 0x80);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn input_report_connected_and_enabled_status() {
    let report = AsetekInputReport {
        status: 0x03,
        ..Default::default()
    };
    assert!(report.is_connected());
    assert!(report.is_enabled());
}

#[test]
fn input_report_disconnected_status() {
    let report = AsetekInputReport {
        status: 0x00,
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(!report.is_enabled());
}

#[test]
fn input_report_temperature_default() {
    let report = AsetekInputReport::default();
    assert_eq!(report.temperature, 25);
}

#[test]
fn wheel_capabilities_invicta_highest_torque() {
    let forte = WheelCapabilities::for_model(WheelModel::Forte);
    let invicta = WheelCapabilities::for_model(WheelModel::Invicta);
    let la_prima = WheelCapabilities::for_model(WheelModel::LaPrima);
    assert!(invicta.max_torque_nm > forte.max_torque_nm);
    assert!(forte.max_torque_nm > la_prima.max_torque_nm);
}

#[test]
fn all_wheel_models_support_quick_release() {
    for model in [WheelModel::Forte, WheelModel::Invicta, WheelModel::LaPrima] {
        let caps = WheelCapabilities::for_model(model);
        assert!(
            caps.supports_quick_release,
            "{model:?} should support quick release"
        );
    }
}

#[test]
fn asetek_vendor_id_recognised() {
    assert!(is_asetek_device(ASETEK_VENDOR_ID));
    assert!(!is_asetek_device(0x0000));
}
