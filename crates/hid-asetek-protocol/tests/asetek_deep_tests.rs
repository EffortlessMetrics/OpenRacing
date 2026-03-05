//! Deep integration tests for the Asetek HID protocol crate.
//!
//! Covers: all device variants (Forte/Invicta/La Prima/Tony Kanaan + pedals),
//! input report parsing, force feedback command encoding (PIDFF effects),
//! output report torque encoding, VID/PID validation, proptest fuzzing,
//! quirks, and comprehensive error handling.

use hid_asetek_protocol::{
    ASETEK_FORTE_PEDALS_PID, ASETEK_FORTE_PID, ASETEK_INVICTA_PEDALS_PID, ASETEK_INVICTA_PID,
    ASETEK_LAPRIMA_PEDALS_PID, ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID, ASETEK_VENDOR_ID,
    AsetekError, AsetekInputReport, AsetekModel, AsetekOutputReport, AsetekResult, MAX_TORQUE_NM,
    REPORT_SIZE_INPUT, REPORT_SIZE_OUTPUT, WheelCapabilities, WheelModel, asetek_model_from_info,
    is_asetek_device,
};

// PIDFF re-exports through the asetek effects module
use hid_asetek_protocol::effects::device_control;
use hid_asetek_protocol::effects::report_ids;
use hid_asetek_protocol::{
    DURATION_INFINITE, EffectOp, EffectType, encode_block_free, encode_device_control,
    encode_device_gain, encode_effect_operation, encode_set_condition, encode_set_constant_force,
    encode_set_effect, encode_set_envelope, encode_set_periodic, encode_set_ramp_force,
};

use proptest::prelude::*;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn build_input_report(
    sequence: u16,
    wheel_angle: i32,
    wheel_speed: i16,
    torque: i16,
    temperature: u8,
    status: u8,
) -> Vec<u8> {
    let mut buf = vec![0u8; 32];
    buf[0..2].copy_from_slice(&sequence.to_le_bytes());
    buf[2..6].copy_from_slice(&wheel_angle.to_le_bytes());
    buf[6..8].copy_from_slice(&wheel_speed.to_le_bytes());
    buf[8..10].copy_from_slice(&torque.to_le_bytes());
    buf[10] = temperature;
    buf[11] = status;
    buf
}

// ═══════════════════════════════════════════════════════════════════════════════
// Device variant identification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn forte_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_FORTE_PID);
    assert_eq!(model, AsetekModel::Forte);
    assert_eq!(model.display_name(), "Asetek Forte");
    assert!((model.max_torque_nm() - 18.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn invicta_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_INVICTA_PID);
    assert_eq!(model, AsetekModel::Invicta);
    assert_eq!(model.display_name(), "Asetek Invicta");
    assert!((model.max_torque_nm() - 27.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn la_prima_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_LAPRIMA_PID);
    assert_eq!(model, AsetekModel::LaPrima);
    assert_eq!(model.display_name(), "Asetek La Prima");
    assert!((model.max_torque_nm() - 12.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn tony_kanaan_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_TONY_KANAAN_PID);
    assert_eq!(model, AsetekModel::TonyKanaan);
    assert_eq!(model.display_name(), "Asetek Tony Kanaan Edition");
    assert!((model.max_torque_nm() - 27.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn invicta_pedals_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_INVICTA_PEDALS_PID);
    assert_eq!(model, AsetekModel::InvictaPedals);
    assert_eq!(model.display_name(), "Asetek Invicta Pedals");
    assert!((model.max_torque_nm()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn forte_pedals_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_FORTE_PEDALS_PID);
    assert_eq!(model, AsetekModel::FortePedals);
    assert_eq!(model.display_name(), "Asetek Forte Pedals");
    assert!((model.max_torque_nm()).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn la_prima_pedals_identified_by_pid() -> Result<(), Box<dyn std::error::Error>> {
    let model = AsetekModel::from_product_id(ASETEK_LAPRIMA_PEDALS_PID);
    assert_eq!(model, AsetekModel::LaPrimaPedals);
    assert_eq!(model.display_name(), "Asetek La Prima Pedals");
    assert!((model.max_torque_nm()).abs() < f32::EPSILON);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// VID/PID validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn vendor_id_is_0x2433() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ASETEK_VENDOR_ID, 0x2433);
    assert!(is_asetek_device(ASETEK_VENDOR_ID));
    Ok(())
}

#[test]
fn foreign_vendor_ids_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let foreign = [0x0000, 0xFFFF, 0x30B7, 0x04D8, 0x16D0];
    for vid in foreign {
        assert!(
            !is_asetek_device(vid),
            "VID 0x{vid:04X} should NOT be recognised as Asetek"
        );
    }
    Ok(())
}

#[test]
fn model_from_info_requires_correct_vendor_id() -> Result<(), Box<dyn std::error::Error>> {
    // Correct VID
    assert_eq!(
        asetek_model_from_info(ASETEK_VENDOR_ID, ASETEK_FORTE_PID),
        AsetekModel::Forte
    );
    // Wrong VID with valid PID
    assert_eq!(
        asetek_model_from_info(0x0000, ASETEK_FORTE_PID),
        AsetekModel::Unknown
    );
    Ok(())
}

#[test]
fn all_pid_constants_are_nonzero_and_unique() -> Result<(), Box<dyn std::error::Error>> {
    let pids = [
        ASETEK_INVICTA_PID,
        ASETEK_FORTE_PID,
        ASETEK_LAPRIMA_PID,
        ASETEK_TONY_KANAAN_PID,
        ASETEK_INVICTA_PEDALS_PID,
        ASETEK_FORTE_PEDALS_PID,
        ASETEK_LAPRIMA_PEDALS_PID,
    ];
    for &pid in &pids {
        assert_ne!(pid, 0, "PID must be nonzero");
    }
    let mut sorted = pids;
    sorted.sort();
    for window in sorted.windows(2) {
        assert_ne!(window[0], window[1], "PIDs must be unique");
    }
    Ok(())
}

#[test]
fn wheelbase_pids_in_f3xx_range() -> Result<(), Box<dyn std::error::Error>> {
    let wb_pids = [
        ASETEK_INVICTA_PID,
        ASETEK_FORTE_PID,
        ASETEK_LAPRIMA_PID,
        ASETEK_TONY_KANAAN_PID,
    ];
    for pid in wb_pids {
        assert!(
            (0xF300..=0xF3FF).contains(&pid),
            "Wheelbase PID 0x{pid:04X} should be in 0xF3xx range"
        );
    }
    Ok(())
}

#[test]
fn pedal_pids_in_f1xx_range() -> Result<(), Box<dyn std::error::Error>> {
    let pedal_pids = [
        ASETEK_INVICTA_PEDALS_PID,
        ASETEK_FORTE_PEDALS_PID,
        ASETEK_LAPRIMA_PEDALS_PID,
    ];
    for pid in pedal_pids {
        assert!(
            (0xF100..=0xF1FF).contains(&pid),
            "Pedal PID 0x{pid:04X} should be in 0xF1xx range"
        );
    }
    Ok(())
}

#[test]
fn model_from_info_all_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<(u16, AsetekModel)> = vec![
        (ASETEK_FORTE_PID, AsetekModel::Forte),
        (ASETEK_INVICTA_PID, AsetekModel::Invicta),
        (ASETEK_LAPRIMA_PID, AsetekModel::LaPrima),
        (ASETEK_TONY_KANAAN_PID, AsetekModel::TonyKanaan),
    ];
    for (pid, expected) in cases {
        let model = asetek_model_from_info(ASETEK_VENDOR_ID, pid);
        assert_eq!(model, expected, "PID 0x{pid:04X} mismatch");
    }
    Ok(())
}

#[test]
fn model_from_info_all_pedals() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<(u16, AsetekModel)> = vec![
        (ASETEK_INVICTA_PEDALS_PID, AsetekModel::InvictaPedals),
        (ASETEK_FORTE_PEDALS_PID, AsetekModel::FortePedals),
        (ASETEK_LAPRIMA_PEDALS_PID, AsetekModel::LaPrimaPedals),
    ];
    for (pid, expected) in cases {
        let model = asetek_model_from_info(ASETEK_VENDOR_ID, pid);
        assert_eq!(model, expected, "PID 0x{pid:04X} mismatch");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Input report parsing — adversarial & boundary inputs
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_input_report_known_values() -> AsetekResult<()> {
    let raw = build_input_report(42, 90_000, 500, 1500, 40, 0x03);
    let report = AsetekInputReport::parse(&raw)?;
    assert_eq!(report.sequence, 42);
    assert_eq!(report.wheel_angle, 90_000);
    assert_eq!(report.wheel_speed, 500);
    assert_eq!(report.torque, 1500);
    assert_eq!(report.temperature, 40);
    assert_eq!(report.status, 0x03);
    Ok(())
}

#[test]
fn parse_input_report_negative_angle() -> AsetekResult<()> {
    let raw = build_input_report(1, -450_000, 0, 0, 25, 0x03);
    let report = AsetekInputReport::parse(&raw)?;
    assert_eq!(report.wheel_angle, -450_000);
    let degrees = report.wheel_angle_degrees();
    assert!((degrees - (-450.0)).abs() < 0.1);
    Ok(())
}

#[test]
fn parse_input_report_negative_torque() -> AsetekResult<()> {
    let raw = build_input_report(1, 0, 0, -2700, 25, 0x03);
    let report = AsetekInputReport::parse(&raw)?;
    let torque = report.applied_torque_nm();
    assert!((torque - (-27.0)).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_input_report_empty_fails() {
    let result = AsetekInputReport::parse(&[]);
    assert!(result.is_err());
    if let Err(AsetekError::InvalidReportSize { expected, actual }) = result {
        assert_eq!(expected, 16);
        assert_eq!(actual, 0);
    } else {
        panic!("expected InvalidReportSize");
    }
}

#[test]
fn parse_input_report_15_bytes_fails() {
    let result = AsetekInputReport::parse(&[0u8; 15]);
    assert!(result.is_err());
}

#[test]
fn parse_input_report_16_bytes_succeeds() -> AsetekResult<()> {
    let data = vec![0u8; 16];
    let _report = AsetekInputReport::parse(&data)?;
    Ok(())
}

#[test]
fn parse_input_report_with_trailing_bytes() -> AsetekResult<()> {
    let mut raw = build_input_report(10, 0, 0, 0, 25, 0x03);
    raw.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    let report = AsetekInputReport::parse(&raw)?;
    assert_eq!(report.sequence, 10);
    Ok(())
}

#[test]
fn parse_input_report_all_ff() -> AsetekResult<()> {
    let raw = vec![0xFF; 32];
    let report = AsetekInputReport::parse(&raw)?;
    assert_eq!(report.sequence, 0xFFFF);
    assert_eq!(report.wheel_angle, -1);
    assert_eq!(report.wheel_speed, -1);
    assert_eq!(report.torque, -1);
    assert_eq!(report.temperature, 0xFF);
    assert_eq!(report.status, 0xFF);
    Ok(())
}

#[test]
fn parse_input_report_all_zeros() -> AsetekResult<()> {
    let raw = vec![0x00; 32];
    let report = AsetekInputReport::parse(&raw)?;
    assert_eq!(report.sequence, 0);
    assert_eq!(report.wheel_angle, 0);
    assert_eq!(report.wheel_speed, 0);
    assert_eq!(report.torque, 0);
    assert_eq!(report.temperature, 0);
    assert_eq!(report.status, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Wheel angle / speed / torque conversions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn wheel_angle_degrees_at_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        wheel_angle: 0,
        ..Default::default()
    };
    assert!(report.wheel_angle_degrees().abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wheel_angle_degrees_at_full_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        wheel_angle: 360_000,
        ..Default::default()
    };
    assert!((report.wheel_angle_degrees() - 360.0).abs() < 0.1);
    Ok(())
}

#[test]
fn wheel_angle_degrees_negative_900() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        wheel_angle: -900_000,
        ..Default::default()
    };
    assert!((report.wheel_angle_degrees() - (-900.0)).abs() < 0.1);
    Ok(())
}

#[test]
fn wheel_speed_rad_s_at_zero() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        wheel_speed: 0,
        ..Default::default()
    };
    assert!(report.wheel_speed_rad_s().abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn wheel_speed_rad_s_positive() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        wheel_speed: 1800,
        ..Default::default()
    };
    // 1800 * π / 1800 = π
    let expected = std::f32::consts::PI;
    assert!((report.wheel_speed_rad_s() - expected).abs() < 0.001);
    Ok(())
}

#[test]
fn applied_torque_nm_scaling() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        torque: 2700,
        ..Default::default()
    };
    assert!((report.applied_torque_nm() - 27.0).abs() < 0.01);
    Ok(())
}

#[test]
fn applied_torque_nm_negative() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        torque: -1800,
        ..Default::default()
    };
    assert!((report.applied_torque_nm() - (-18.0)).abs() < 0.01);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Status flags
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn status_connected_and_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        status: 0x03,
        ..Default::default()
    };
    assert!(report.is_connected());
    assert!(report.is_enabled());
    Ok(())
}

#[test]
fn status_disconnected() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        status: 0x00,
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(!report.is_enabled());
    Ok(())
}

#[test]
fn status_connected_but_not_enabled() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        status: 0x01,
        ..Default::default()
    };
    assert!(report.is_connected());
    assert!(!report.is_enabled());
    Ok(())
}

#[test]
fn status_enabled_but_not_connected() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport {
        status: 0x02,
        ..Default::default()
    };
    assert!(!report.is_connected());
    assert!(report.is_enabled());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Force feedback command encoding (output report)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn output_report_default_is_zero_torque() -> AsetekResult<()> {
    let report = AsetekOutputReport::default();
    assert_eq!(report.sequence, 0);
    assert_eq!(report.torque_cNm, 0);
    assert_eq!(report.led_mode, 0);
    assert_eq!(report.led_value, 0);
    let data = report.build()?;
    assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    Ok(())
}

#[test]
fn output_torque_encoding_positive() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(1).with_torque(18.0);
    assert_eq!(report.torque_cNm, 1800);
    let data = report.build()?;
    let encoded_torque = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(encoded_torque, 1800);
    Ok(())
}

#[test]
fn output_torque_encoding_negative() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(1).with_torque(-12.0);
    assert_eq!(report.torque_cNm, -1200);
    let data = report.build()?;
    let encoded_torque = i16::from_le_bytes([data[2], data[3]]);
    assert_eq!(encoded_torque, -1200);
    Ok(())
}

#[test]
fn output_torque_clamped_above_max() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::new(1).with_torque(50.0);
    assert_eq!(report.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);
    Ok(())
}

#[test]
fn output_torque_clamped_below_negative_max() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekOutputReport::new(1).with_torque(-50.0);
    assert_eq!(report.torque_cNm, (-MAX_TORQUE_NM * 100.0) as i16);
    Ok(())
}

#[test]
fn output_torque_zero() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(0).with_torque(0.0);
    assert_eq!(report.torque_cNm, 0);
    let data = report.build()?;
    assert_eq!(i16::from_le_bytes([data[2], data[3]]), 0);
    Ok(())
}

#[test]
fn output_report_sequence_round_trip() -> AsetekResult<()> {
    for seq in [0_u16, 1, 0x7FFF, 0xFFFF] {
        let report = AsetekOutputReport::new(seq);
        let data = report.build()?;
        let encoded_seq = u16::from_le_bytes([data[0], data[1]]);
        assert_eq!(encoded_seq, seq, "sequence 0x{seq:04X} mismatch");
    }
    Ok(())
}

#[test]
fn output_report_led_encoded() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(0).with_led(0x03, 0xAB);
    let data = report.build()?;
    assert_eq!(data[4], 0x03);
    assert_eq!(data[5], 0xAB);
    Ok(())
}

#[test]
fn output_report_padded_to_32_bytes() -> AsetekResult<()> {
    let report = AsetekOutputReport::new(1).with_torque(5.0);
    let data = report.build()?;
    assert_eq!(data.len(), 32);
    // Bytes beyond the header should be zero-padded
    for &b in &data[6..] {
        assert_eq!(b, 0, "padding bytes should be zero");
    }
    Ok(())
}

#[test]
fn output_report_size_constant_is_32() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_SIZE_OUTPUT, 32);
    Ok(())
}

#[test]
fn input_report_size_constant_is_32() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(REPORT_SIZE_INPUT, 32);
    Ok(())
}

#[test]
fn max_torque_nm_is_27() -> Result<(), Box<dyn std::error::Error>> {
    assert!((MAX_TORQUE_NM - 27.0).abs() < f32::EPSILON);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// PIDFF effect encoding (via Asetek re-exports)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn pidff_constant_force_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_constant_force(1, 5000);
    assert_eq!(buf[0], report_ids::SET_CONSTANT_FORCE);
    assert_eq!(buf[1], 1);
    assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), 5000);
    Ok(())
}

#[test]
fn pidff_constant_force_negative() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_constant_force(1, -10000);
    assert_eq!(i16::from_le_bytes([buf[2], buf[3]]), -10000);
    Ok(())
}

#[test]
fn pidff_set_effect_report_id() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_effect(1, EffectType::Constant, 1000, 255, 0);
    assert_eq!(buf[0], report_ids::SET_EFFECT);
    assert_eq!(buf[1], 1); // block index
    Ok(())
}

#[test]
fn pidff_set_effect_duration_infinite() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_effect(0, EffectType::Sine, DURATION_INFINITE, 128, 0);
    assert_eq!(buf[0], report_ids::SET_EFFECT);
    let duration = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(duration, 0xFFFF);
    Ok(())
}

#[test]
fn pidff_set_envelope() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_envelope(1, 1000, 500, 200, 300);
    assert_eq!(buf[0], report_ids::SET_ENVELOPE);
    assert_eq!(buf[1], 1);
    Ok(())
}

#[test]
fn pidff_set_condition_spring() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_condition(1, 0, 0, 5000, -5000, 10000, 10000, 0);
    assert_eq!(buf[0], report_ids::SET_CONDITION);
    assert_eq!(buf[1], 1);
    Ok(())
}

#[test]
fn pidff_set_periodic_sine() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_periodic(1, 8000, 0, 0, 50);
    assert_eq!(buf[0], report_ids::SET_PERIODIC);
    assert_eq!(buf[1], 1);
    Ok(())
}

#[test]
fn pidff_set_ramp_force() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_set_ramp_force(1, 0, 10000);
    assert_eq!(buf[0], report_ids::SET_RAMP_FORCE);
    assert_eq!(buf[1], 1);
    let start = i16::from_le_bytes([buf[2], buf[3]]);
    let end = i16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(start, 0);
    assert_eq!(end, 10000);
    Ok(())
}

#[test]
fn pidff_effect_operation_start() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_effect_operation(1, EffectOp::Start, 1);
    assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
    assert_eq!(buf[1], 1);
    Ok(())
}

#[test]
fn pidff_effect_operation_stop() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_effect_operation(1, EffectOp::Stop, 0);
    assert_eq!(buf[0], report_ids::EFFECT_OPERATION);
    Ok(())
}

#[test]
fn pidff_block_free() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_block_free(5);
    assert_eq!(buf[0], report_ids::BLOCK_FREE);
    assert_eq!(buf[1], 5);
    Ok(())
}

#[test]
fn pidff_device_control_enable() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_device_control(device_control::ENABLE_ACTUATORS);
    assert_eq!(buf[0], report_ids::DEVICE_CONTROL);
    assert_eq!(buf[1], device_control::ENABLE_ACTUATORS);
    Ok(())
}

#[test]
fn pidff_device_control_disable() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_device_control(device_control::DISABLE_ACTUATORS);
    assert_eq!(buf[1], device_control::DISABLE_ACTUATORS);
    Ok(())
}

#[test]
fn pidff_device_control_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_device_control(device_control::STOP_ALL_EFFECTS);
    assert_eq!(buf[1], device_control::STOP_ALL_EFFECTS);
    Ok(())
}

#[test]
fn pidff_device_control_reset() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_device_control(device_control::DEVICE_RESET);
    assert_eq!(buf[1], device_control::DEVICE_RESET);
    Ok(())
}

#[test]
fn pidff_device_gain_normal() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_device_gain(5000);
    assert_eq!(buf[0], report_ids::DEVICE_GAIN);
    let gain = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(gain, 5000);
    Ok(())
}

#[test]
fn pidff_device_gain_clamped_to_10000() -> Result<(), Box<dyn std::error::Error>> {
    let buf = encode_device_gain(20000);
    let gain = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(gain, 10000);
    Ok(())
}

#[test]
fn pidff_effect_types_have_correct_discriminants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(EffectType::Constant as u8, 1);
    assert_eq!(EffectType::Ramp as u8, 2);
    assert_eq!(EffectType::Square as u8, 3);
    assert_eq!(EffectType::Sine as u8, 4);
    assert_eq!(EffectType::Triangle as u8, 5);
    assert_eq!(EffectType::SawtoothUp as u8, 6);
    assert_eq!(EffectType::SawtoothDown as u8, 7);
    assert_eq!(EffectType::Spring as u8, 8);
    assert_eq!(EffectType::Damper as u8, 9);
    assert_eq!(EffectType::Inertia as u8, 10);
    assert_eq!(EffectType::Friction as u8, 11);
    Ok(())
}

#[test]
fn pidff_report_ids_match_spec() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::SET_EFFECT, 0x01);
    assert_eq!(report_ids::SET_ENVELOPE, 0x02);
    assert_eq!(report_ids::SET_CONDITION, 0x03);
    assert_eq!(report_ids::SET_PERIODIC, 0x04);
    assert_eq!(report_ids::SET_CONSTANT_FORCE, 0x05);
    assert_eq!(report_ids::SET_RAMP_FORCE, 0x06);
    assert_eq!(report_ids::EFFECT_OPERATION, 0x0A);
    assert_eq!(report_ids::BLOCK_FREE, 0x0B);
    assert_eq!(report_ids::DEVICE_CONTROL, 0x0C);
    assert_eq!(report_ids::DEVICE_GAIN, 0x0D);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// WheelCapabilities per model
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn capabilities_forte() -> Result<(), Box<dyn std::error::Error>> {
    let caps = WheelCapabilities::for_model(WheelModel::Forte);
    assert!((caps.max_torque_nm - 18.0).abs() < f32::EPSILON);
    assert_eq!(caps.max_speed_rpm, 3000);
    assert!(caps.supports_quick_release);
    Ok(())
}

#[test]
fn capabilities_invicta() -> Result<(), Box<dyn std::error::Error>> {
    let caps = WheelCapabilities::for_model(WheelModel::Invicta);
    assert!((caps.max_torque_nm - 27.0).abs() < f32::EPSILON);
    assert_eq!(caps.max_speed_rpm, 2500);
    assert!(caps.supports_quick_release);
    Ok(())
}

#[test]
fn capabilities_la_prima() -> Result<(), Box<dyn std::error::Error>> {
    let caps = WheelCapabilities::for_model(WheelModel::LaPrima);
    assert!((caps.max_torque_nm - 12.0).abs() < f32::EPSILON);
    assert_eq!(caps.max_speed_rpm, 2000);
    assert!(caps.supports_quick_release);
    Ok(())
}

#[test]
fn capabilities_unknown_defaults_to_forte() -> Result<(), Box<dyn std::error::Error>> {
    let caps = WheelCapabilities::for_model(WheelModel::Unknown);
    let default = WheelCapabilities::default();
    assert!((caps.max_torque_nm - default.max_torque_nm).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn torque_ordering_la_prima_lt_forte_lt_invicta() -> Result<(), Box<dyn std::error::Error>> {
    let lp = WheelCapabilities::for_model(WheelModel::LaPrima);
    let f = WheelCapabilities::for_model(WheelModel::Forte);
    let i = WheelCapabilities::for_model(WheelModel::Invicta);
    assert!(lp.max_torque_nm < f.max_torque_nm);
    assert!(f.max_torque_nm < i.max_torque_nm);
    Ok(())
}

#[test]
fn all_models_support_quick_release() -> Result<(), Box<dyn std::error::Error>> {
    for model in [WheelModel::Forte, WheelModel::Invicta, WheelModel::LaPrima] {
        let caps = WheelCapabilities::for_model(model);
        assert!(
            caps.supports_quick_release,
            "{model:?} should support quick release"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Model ↔ AsetekModel torque cross-check
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn asetek_model_torque_matches_wheel_caps() -> Result<(), Box<dyn std::error::Error>> {
    let pairs: Vec<(AsetekModel, WheelModel)> = vec![
        (AsetekModel::Forte, WheelModel::Forte),
        (AsetekModel::Invicta, WheelModel::Invicta),
        (AsetekModel::LaPrima, WheelModel::LaPrima),
    ];
    for (asetek_m, wheel_m) in pairs {
        let caps = WheelCapabilities::for_model(wheel_m);
        assert!(
            (asetek_m.max_torque_nm() - caps.max_torque_nm).abs() < f32::EPSILON,
            "{asetek_m:?} torque mismatch with {wheel_m:?}"
        );
    }
    Ok(())
}

#[test]
fn tony_kanaan_has_same_torque_as_invicta() -> Result<(), Box<dyn std::error::Error>> {
    assert!(
        (AsetekModel::TonyKanaan.max_torque_nm() - AsetekModel::Invicta.max_torque_nm()).abs()
            < f32::EPSILON
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Quirks
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn always_poll_quirk_is_true() -> Result<(), Box<dyn std::error::Error>> {
    const { assert!(hid_asetek_protocol::quirks::REQUIRES_ALWAYS_POLL_LINUX) };
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Error handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn error_display_invalid_report_size() -> Result<(), Box<dyn std::error::Error>> {
    let err = AsetekError::InvalidReportSize {
        expected: 32,
        actual: 10,
    };
    let msg = format!("{err}");
    assert!(msg.contains("32"));
    assert!(msg.contains("10"));
    Ok(())
}

#[test]
fn error_display_invalid_torque() -> Result<(), Box<dyn std::error::Error>> {
    let err = AsetekError::InvalidTorque(99.9);
    let msg = format!("{err}");
    assert!(msg.contains("99.9"));
    Ok(())
}

#[test]
fn error_display_device_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let err = AsetekError::DeviceNotFound("Asetek Forte".to_string());
    let msg = format!("{err}");
    assert!(msg.contains("Asetek Forte"));
    Ok(())
}

#[test]
fn error_is_std_error() -> Result<(), Box<dyn std::error::Error>> {
    let err: Box<dyn std::error::Error> = Box::new(AsetekError::InvalidTorque(1.0));
    let _msg = format!("{err}");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Default report integrity
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn default_input_report_values() -> Result<(), Box<dyn std::error::Error>> {
    let report = AsetekInputReport::default();
    assert_eq!(report.sequence, 0);
    assert_eq!(report.wheel_angle, 0);
    assert_eq!(report.wheel_speed, 0);
    assert_eq!(report.torque, 0);
    assert_eq!(report.temperature, 25);
    assert_eq!(report.status, 0x03);
    assert!(report.is_connected());
    assert!(report.is_enabled());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rapid sequential parsing (stress)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_1000_sequential_input_reports() -> AsetekResult<()> {
    for i in 0..1000_u16 {
        let raw = build_input_report(i, i as i32 * 100, i as i16, i as i16, 25, 0x03);
        let report = AsetekInputReport::parse(&raw)?;
        assert_eq!(report.sequence, i);
    }
    Ok(())
}

#[test]
fn build_1000_sequential_output_reports() -> AsetekResult<()> {
    for i in 0..1000_u16 {
        let torque = (i as f32 / 1000.0) * MAX_TORQUE_NM;
        let report = AsetekOutputReport::new(i).with_torque(torque);
        let data = report.build()?;
        assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Proptest fuzzing
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn fuzz_parse_arbitrary_32_bytes(data in proptest::collection::vec(any::<u8>(), 32..=32)) {
        let result = AsetekInputReport::parse(&data);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn fuzz_parse_16_to_64_bytes_succeeds(data in proptest::collection::vec(any::<u8>(), 16..=64)) {
        let result = AsetekInputReport::parse(&data);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn fuzz_parse_short_buffer_always_fails(len in 0..16_usize) {
        let data = vec![0u8; len];
        let result = AsetekInputReport::parse(&data);
        prop_assert!(result.is_err());
    }

    #[test]
    fn fuzz_input_parse_round_trip(
        seq: u16,
        angle: i32,
        speed: i16,
        torque: i16,
        temp: u8,
        status: u8,
    ) {
        let raw = build_input_report(seq, angle, speed, torque, temp, status);
        let report = AsetekInputReport::parse(&raw).map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(report.sequence, seq);
        prop_assert_eq!(report.wheel_angle, angle);
        prop_assert_eq!(report.wheel_speed, speed);
        prop_assert_eq!(report.torque, torque);
        prop_assert_eq!(report.temperature, temp);
        prop_assert_eq!(report.status, status);
    }

    #[test]
    fn fuzz_output_torque_clamped_within_bounds(torque_nm in -100.0_f32..100.0) {
        let report = AsetekOutputReport::new(0).with_torque(torque_nm);
        let c_nm = report.torque_cNm;
        let max_c_nm = (MAX_TORQUE_NM * 100.0) as i16;
        prop_assert!(c_nm >= -max_c_nm);
        prop_assert!(c_nm <= max_c_nm);
    }

    #[test]
    fn fuzz_output_build_always_32_bytes(seq: u16, torque_nm in -27.0_f32..27.0, led_mode: u8, led_val: u8) {
        let report = AsetekOutputReport::new(seq)
            .with_torque(torque_nm)
            .with_led(led_mode, led_val);
        let data = report.build().map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
    }

    #[test]
    fn fuzz_arbitrary_pid_never_panics(pid: u16) {
        let model = AsetekModel::from_product_id(pid);
        let _name = model.display_name();
        let _torque = model.max_torque_nm();
    }

    #[test]
    fn fuzz_arbitrary_vid_pid_never_panics(vid: u16, pid: u16) {
        let model = asetek_model_from_info(vid, pid);
        let _name = model.display_name();
    }

    #[test]
    fn fuzz_wheel_angle_degrees_finite(angle: i32) {
        let report = AsetekInputReport { wheel_angle: angle, ..Default::default() };
        let degrees = report.wheel_angle_degrees();
        prop_assert!(degrees.is_finite());
    }

    #[test]
    fn fuzz_wheel_speed_rad_s_finite(speed: i16) {
        let report = AsetekInputReport { wheel_speed: speed, ..Default::default() };
        let rad = report.wheel_speed_rad_s();
        prop_assert!(rad.is_finite());
    }

    #[test]
    fn fuzz_applied_torque_nm_finite(torque: i16) {
        let report = AsetekInputReport { torque, ..Default::default() };
        let nm = report.applied_torque_nm();
        prop_assert!(nm.is_finite());
    }

    #[test]
    fn fuzz_status_bits_consistent(status: u8) {
        let report = AsetekInputReport { status, ..Default::default() };
        prop_assert_eq!(report.is_connected(), (status & 0x01) != 0);
        prop_assert_eq!(report.is_enabled(), (status & 0x02) != 0);
    }
}
