//! Deep protocol tests for Fanatec HID protocol.
//!
//! Covers constant-force encoding, LED/display/rumble wire format, device
//! identification, rim capabilities, pedal parsing, extended telemetry,
//! and property-based round-trip guarantees.

use racing_wheel_hid_fanatec_protocol::ids::{
    FANATEC_VENDOR_ID, ffb_commands, led_commands, product_ids, report_ids, rim_ids,
};
use racing_wheel_hid_fanatec_protocol::output::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder,
    MAX_ROTATION_DEGREES, MIN_ROTATION_DEGREES, build_display_report,
    build_kernel_range_sequence, build_led_report, build_mode_switch_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report,
    build_stop_all_report, fix_report_values,
};
use racing_wheel_hid_fanatec_protocol::input::{
    parse_extended_report, parse_pedal_report, parse_standard_report,
};
use racing_wheel_hid_fanatec_protocol::types::{
    FanatecModel, FanatecPedalModel, FanatecRimId, is_pedal_product, is_wheelbase_product,
};

// ─── Vendor ID ───────────────────────────────────────────────────────────────

#[test]
fn vendor_id_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FANATEC_VENDOR_ID, 0x0EB7);
    Ok(())
}

// ─── Constant force encoding: sign preservation ─────────────────────────────

#[test]
fn cf_encode_sign_preservation_small_values() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    enc.encode(0.01, 0, &mut out);
    let raw_pos = i16::from_le_bytes([out[2], out[3]]);
    assert!(raw_pos >= 0, "small positive torque must encode non-negative: {raw_pos}");

    enc.encode(-0.01, 0, &mut out);
    let raw_neg = i16::from_le_bytes([out[2], out[3]]);
    assert!(raw_neg <= 0, "small negative torque must encode non-positive: {raw_neg}");
    Ok(())
}

#[test]
fn cf_encode_negative_max_torque_clamps_to_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(-10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(5.0, 0, &mut out);
    let raw = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(raw, 0, "negative max_torque → 0 max → raw must be 0");
    Ok(())
}

#[test]
fn cf_encode_returns_report_len() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(4.0, 0, &mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    Ok(())
}

#[test]
fn cf_encode_zero_returns_report_len() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(8.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode_zero(&mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    Ok(())
}

#[test]
fn cf_encode_reserved_bytes_always_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(25.0);
    let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(12.5, 0, &mut out);
    assert_eq!(out[4], 0, "byte 4 must be zero");
    assert_eq!(out[5], 0, "byte 5 must be zero");
    assert_eq!(out[6], 0, "byte 6 must be zero");
    assert_eq!(out[7], 0, "byte 7 must be zero");
    Ok(())
}

// ─── Constant force: torque → raw round-trip per-model ──────────────────────

#[test]
fn cf_round_trip_all_models() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (FanatecModel::Dd1, 20.0_f32),
        (FanatecModel::Dd2, 25.0),
        (FanatecModel::CslElite, 6.0),
        (FanatecModel::CslDd, 8.0),
        (FanatecModel::ClubSportV2, 8.0),
        (FanatecModel::CsrElite, 5.0),
        (FanatecModel::ClubSportDd, 12.0),
    ];
    for (model, max_torque) in models {
        let enc = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let half = max_torque * 0.5;
        enc.encode(half, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        let decoded = (raw as f32 / i16::MAX as f32) * max_torque;
        assert!(
            (decoded - half).abs() < 0.05,
            "{model:?}: round-trip error: encoded {half} → raw {raw} → decoded {decoded}"
        );
    }
    Ok(())
}

// ─── Report IDs golden values ────────────────────────────────────────────────

#[test]
fn report_ids_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::STANDARD_INPUT, 0x01);
    assert_eq!(report_ids::EXTENDED_INPUT, 0x02);
    assert_eq!(report_ids::MODE_SWITCH, 0x01);
    assert_eq!(report_ids::FFB_OUTPUT, 0x01);
    assert_eq!(report_ids::LED_DISPLAY, 0x08);
    Ok(())
}

// ─── FFB command golden values ───────────────────────────────────────────────

#[test]
fn ffb_commands_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(ffb_commands::CONSTANT_FORCE, 0x01);
    assert_eq!(ffb_commands::SET_ROTATION_RANGE, 0x12);
    assert_eq!(ffb_commands::SET_GAIN, 0x10);
    assert_eq!(ffb_commands::STOP_ALL, 0x0F);
    Ok(())
}

// ─── LED command golden values ───────────────────────────────────────────────

#[test]
fn led_commands_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(led_commands::REV_LIGHTS, 0x80);
    assert_eq!(led_commands::DISPLAY, 0x81);
    assert_eq!(led_commands::RUMBLE, 0x82);
    Ok(())
}

// ─── Device identification: all PIDs ─────────────────────────────────────────

#[test]
fn model_from_all_wheelbase_pids() -> Result<(), Box<dyn std::error::Error>> {
    let expected = [
        (product_ids::CLUBSPORT_V2, FanatecModel::ClubSportV2),
        (product_ids::CLUBSPORT_V2_5, FanatecModel::ClubSportV25),
        (product_ids::CSL_ELITE_PS4, FanatecModel::CslElite),
        (product_ids::CSL_ELITE, FanatecModel::CslElite),
        (product_ids::DD1, FanatecModel::Dd1),
        (product_ids::DD2, FanatecModel::Dd2),
        (product_ids::CSL_DD, FanatecModel::CslDd),
        (product_ids::GT_DD_PRO, FanatecModel::GtDdPro),
        (product_ids::CLUBSPORT_DD, FanatecModel::ClubSportDd),
        (product_ids::CSR_ELITE, FanatecModel::CsrElite),
    ];
    for (pid, expected_model) in expected {
        let model = FanatecModel::from_product_id(pid);
        assert_eq!(model, expected_model, "PID 0x{pid:04X}");
    }
    Ok(())
}

// ─── Model torque ordering ──────────────────────────────────────────────────

#[test]
fn model_torque_ordering() -> Result<(), Box<dyn std::error::Error>> {
    assert!(FanatecModel::CsrElite.max_torque_nm() < FanatecModel::CslElite.max_torque_nm());
    assert!(FanatecModel::CslElite.max_torque_nm() < FanatecModel::CslDd.max_torque_nm());
    assert!(FanatecModel::CslDd.max_torque_nm() < FanatecModel::ClubSportDd.max_torque_nm());
    assert!(FanatecModel::ClubSportDd.max_torque_nm() < FanatecModel::Dd1.max_torque_nm());
    assert!(FanatecModel::Dd1.max_torque_nm() < FanatecModel::Dd2.max_torque_nm());
    Ok(())
}

// ─── Model max_torque known values ──────────────────────────────────────────

#[test]
fn model_torque_known_values() -> Result<(), Box<dyn std::error::Error>> {
    assert!((FanatecModel::Dd1.max_torque_nm() - 20.0).abs() < 0.01);
    assert!((FanatecModel::Dd2.max_torque_nm() - 25.0).abs() < 0.01);
    assert!((FanatecModel::CslElite.max_torque_nm() - 6.0).abs() < 0.01);
    assert!((FanatecModel::CslDd.max_torque_nm() - 8.0).abs() < 0.01);
    assert!((FanatecModel::GtDdPro.max_torque_nm() - 8.0).abs() < 0.01);
    assert!((FanatecModel::ClubSportDd.max_torque_nm() - 12.0).abs() < 0.01);
    assert!((FanatecModel::ClubSportV2.max_torque_nm() - 8.0).abs() < 0.01);
    assert!((FanatecModel::CsrElite.max_torque_nm() - 5.0).abs() < 0.01);
    assert!((FanatecModel::Unknown.max_torque_nm() - 5.0).abs() < 0.01);
    Ok(())
}

// ─── Model encoder CPR ──────────────────────────────────────────────────────

#[test]
fn model_encoder_cpr_values() -> Result<(), Box<dyn std::error::Error>> {
    // DD bases: 16384 CPR
    assert_eq!(FanatecModel::Dd1.encoder_cpr(), 16_384);
    assert_eq!(FanatecModel::Dd2.encoder_cpr(), 16_384);
    assert_eq!(FanatecModel::CslDd.encoder_cpr(), 16_384);
    assert_eq!(FanatecModel::GtDdPro.encoder_cpr(), 16_384);
    assert_eq!(FanatecModel::ClubSportDd.encoder_cpr(), 16_384);
    // Belt-drive: 4096 CPR
    assert_eq!(FanatecModel::CslElite.encoder_cpr(), 4_096);
    assert_eq!(FanatecModel::ClubSportV2.encoder_cpr(), 4_096);
    assert_eq!(FanatecModel::CsrElite.encoder_cpr(), 4_096);
    Ok(())
}

// ─── Model max rotation degrees ──────────────────────────────────────────────

#[test]
fn model_max_rotation_per_model() -> Result<(), Box<dyn std::error::Error>> {
    // DD bases: 2520°
    assert_eq!(FanatecModel::Dd1.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::Dd2.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::CslDd.max_rotation_degrees(), 2520);
    assert_eq!(FanatecModel::ClubSportDd.max_rotation_degrees(), 2520);
    // CSL Elite: 1080°
    assert_eq!(FanatecModel::CslElite.max_rotation_degrees(), 1080);
    // Belt-drive: 900°
    assert_eq!(FanatecModel::ClubSportV2.max_rotation_degrees(), 900);
    assert_eq!(FanatecModel::CsrElite.max_rotation_degrees(), 900);
    assert_eq!(FanatecModel::Unknown.max_rotation_degrees(), 900);
    Ok(())
}

// ─── Model is_highres ────────────────────────────────────────────────────────

#[test]
fn model_highres_dd_models_only() -> Result<(), Box<dyn std::error::Error>> {
    // Highres
    assert!(FanatecModel::Dd1.is_highres());
    assert!(FanatecModel::Dd2.is_highres());
    assert!(FanatecModel::CslDd.is_highres());
    assert!(FanatecModel::GtDdPro.is_highres());
    assert!(FanatecModel::ClubSportDd.is_highres());
    // Not highres
    assert!(!FanatecModel::CslElite.is_highres());
    assert!(!FanatecModel::ClubSportV2.is_highres());
    assert!(!FanatecModel::CsrElite.is_highres());
    assert!(!FanatecModel::Unknown.is_highres());
    Ok(())
}

// ─── Model needs_sign_fix ────────────────────────────────────────────────────

#[test]
fn model_needs_sign_fix_not_csr_elite() -> Result<(), Box<dyn std::error::Error>> {
    // CSR Elite and Unknown skip sign fix
    assert!(!FanatecModel::CsrElite.needs_sign_fix());
    assert!(!FanatecModel::Unknown.needs_sign_fix());
    // All others need it
    assert!(FanatecModel::Dd1.needs_sign_fix());
    assert!(FanatecModel::Dd2.needs_sign_fix());
    assert!(FanatecModel::CslDd.needs_sign_fix());
    assert!(FanatecModel::CslElite.needs_sign_fix());
    assert!(FanatecModel::ClubSportV2.needs_sign_fix());
    assert!(FanatecModel::ClubSportV25.needs_sign_fix());
    assert!(FanatecModel::GtDdPro.needs_sign_fix());
    assert!(FanatecModel::ClubSportDd.needs_sign_fix());
    Ok(())
}

// ─── CSL Elite PS4 aliases to CslElite ───────────────────────────────────────

#[test]
fn csl_elite_ps4_and_pc_map_same_model() -> Result<(), Box<dyn std::error::Error>> {
    let ps4 = FanatecModel::from_product_id(product_ids::CSL_ELITE_PS4);
    let pc = FanatecModel::from_product_id(product_ids::CSL_ELITE);
    assert_eq!(ps4, FanatecModel::CslElite);
    assert_eq!(pc, FanatecModel::CslElite);
    assert_eq!(ps4, pc);
    Ok(())
}

// ─── Wheelbase/pedal product classification ──────────────────────────────────

#[test]
fn accessory_pids_not_wheelbase_or_pedal() -> Result<(), Box<dyn std::error::Error>> {
    assert!(!is_wheelbase_product(product_ids::CLUBSPORT_SHIFTER));
    assert!(!is_pedal_product(product_ids::CLUBSPORT_SHIFTER));
    assert!(!is_wheelbase_product(product_ids::CLUBSPORT_HANDBRAKE));
    assert!(!is_pedal_product(product_ids::CLUBSPORT_HANDBRAKE));
    Ok(())
}

#[test]
fn pedal_pids_not_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let pedal_pids = [
        product_ids::CLUBSPORT_PEDALS_V1_V2,
        product_ids::CLUBSPORT_PEDALS_V3,
        product_ids::CSL_ELITE_PEDALS,
        product_ids::CSL_PEDALS_LC,
        product_ids::CSL_PEDALS_V2,
    ];
    for pid in pedal_pids {
        assert!(is_pedal_product(pid), "PID 0x{pid:04X} must be pedal");
        assert!(!is_wheelbase_product(pid), "PID 0x{pid:04X} must not be wheelbase");
    }
    Ok(())
}

// ─── Pedal model axis count ──────────────────────────────────────────────────

#[test]
fn pedal_model_axis_count_all() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FanatecPedalModel::ClubSportV3.axis_count(), 3);
    assert_eq!(FanatecPedalModel::CslPedalsLc.axis_count(), 3);
    assert_eq!(FanatecPedalModel::CslPedalsV2.axis_count(), 3);
    assert_eq!(FanatecPedalModel::ClubSportV1V2.axis_count(), 2);
    assert_eq!(FanatecPedalModel::CslElitePedals.axis_count(), 2);
    assert_eq!(FanatecPedalModel::Unknown.axis_count(), 2);
    Ok(())
}

// ─── Rim ID capabilities matrix ──────────────────────────────────────────────

#[test]
fn rim_capabilities_matrix() -> Result<(), Box<dyn std::error::Error>> {
    // McLaren GT3 V2 has everything
    let mclaren = FanatecRimId::McLarenGt3V2;
    assert!(mclaren.has_funky_switch());
    assert!(mclaren.has_dual_clutch());
    assert!(mclaren.has_rotary_encoders());

    // Formula V2 has dual clutch but no funky or rotary
    let formula = FanatecRimId::FormulaV2;
    assert!(!formula.has_funky_switch());
    assert!(formula.has_dual_clutch());
    assert!(!formula.has_rotary_encoders());

    // Formula V2.5 has dual clutch and rotary
    let formula_25 = FanatecRimId::FormulaV25;
    assert!(!formula_25.has_funky_switch());
    assert!(formula_25.has_dual_clutch());
    assert!(formula_25.has_rotary_encoders());

    // BMW GT2 has nothing
    let bmw = FanatecRimId::BmwGt2;
    assert!(!bmw.has_funky_switch());
    assert!(!bmw.has_dual_clutch());
    assert!(!bmw.has_rotary_encoders());

    // CSL Elite P1 has nothing
    let p1 = FanatecRimId::CslEliteP1;
    assert!(!p1.has_funky_switch());
    assert!(!p1.has_dual_clutch());
    assert!(!p1.has_rotary_encoders());

    // WRC has nothing
    let wrc = FanatecRimId::Wrc;
    assert!(!wrc.has_funky_switch());
    assert!(!wrc.has_dual_clutch());
    assert!(!wrc.has_rotary_encoders());

    // PodiumHub has nothing
    let hub = FanatecRimId::PodiumHub;
    assert!(!hub.has_funky_switch());
    assert!(!hub.has_dual_clutch());
    assert!(!hub.has_rotary_encoders());
    Ok(())
}

// ─── Rim IDs are unique ──────────────────────────────────────────────────────

#[test]
fn rim_id_bytes_all_unique() -> Result<(), Box<dyn std::error::Error>> {
    let all = [
        rim_ids::BMW_GT2,
        rim_ids::FORMULA_V2,
        rim_ids::FORMULA_V2_5,
        rim_ids::CSL_ELITE_P1,
        rim_ids::MCLAREN_GT3_V2,
        rim_ids::PORSCHE_911_GT3_R,
        rim_ids::PORSCHE_918_RSR,
        rim_ids::CLUBSPORT_RS,
        rim_ids::WRC,
        rim_ids::PODIUM_HUB,
    ];
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            assert_ne!(all[i], all[j], "rim IDs at {i} and {j} must differ");
        }
    }
    Ok(())
}

// ─── fix_report_values: edge cases ──────────────────────────────────────────

#[test]
fn fix_report_values_boundary_0x80() -> Result<(), Box<dyn std::error::Error>> {
    let mut values: [i16; 7] = [0x80, 0x7F, 0x81, 0xFF, 0x00, 0x01, 0xFE];
    fix_report_values(&mut values);
    assert_eq!(values[0], -128, "0x80 → -128");
    assert_eq!(values[1], 0x7F, "0x7F unchanged");
    assert_eq!(values[2], -127, "0x81 → -127");
    assert_eq!(values[3], -1, "0xFF → -1");
    assert_eq!(values[4], 0, "0x00 unchanged");
    assert_eq!(values[5], 1, "0x01 unchanged");
    assert_eq!(values[6], -2, "0xFE → -2");
    Ok(())
}

#[test]
fn fix_report_values_all_zero_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let mut values = [0i16; 7];
    fix_report_values(&mut values);
    assert_eq!(values, [0i16; 7]);
    Ok(())
}

#[test]
fn fix_report_values_all_max_below_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let mut values = [0x7Fi16; 7];
    let original = values;
    fix_report_values(&mut values);
    assert_eq!(values, original);
    Ok(())
}

// ─── Extended report fault flags ─────────────────────────────────────────────

#[test]
fn extended_report_fault_flags_individual_bits() -> Result<(), Box<dyn std::error::Error>> {
    let fault_bits: &[(u8, &str)] = &[
        (0x01, "over-temp"),
        (0x02, "over-current"),
        (0x04, "comm-error"),
        (0x08, "motor-fault"),
    ];
    for &(flag, label) in fault_bits {
        let mut data = [0u8; 64];
        data[0] = report_ids::EXTENDED_INPUT;
        data[10] = flag;
        let state = parse_extended_report(&data).ok_or("parse failed")?;
        assert_eq!(state.fault_flags, flag, "fault bit {label} must be preserved");
    }
    Ok(())
}

#[test]
fn extended_report_all_fault_flags_combined() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::EXTENDED_INPUT;
    data[10] = 0x0F; // all 4 flags set
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.fault_flags, 0x0F);
    Ok(())
}

#[test]
fn extended_report_signed_steering_angle() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::EXTENDED_INPUT;
    // steering_raw = -1000 = 0xFC18
    let raw_bytes = (-1000i16).to_le_bytes();
    data[1] = raw_bytes[0];
    data[2] = raw_bytes[1];
    // velocity = 500
    let vel_bytes = 500i16.to_le_bytes();
    data[3] = vel_bytes[0];
    data[4] = vel_bytes[1];
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.steering_raw, -1000);
    assert_eq!(state.steering_velocity, 500);
    Ok(())
}

#[test]
fn extended_report_temperature_fields() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::EXTENDED_INPUT;
    data[5] = 85; // motor temp
    data[6] = 55; // board temp
    data[7] = 42; // current draw (0.1A units)
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.motor_temp_c, 85);
    assert_eq!(state.board_temp_c, 55);
    assert_eq!(state.current_raw, 42);
    Ok(())
}

// ─── Standard input: inverted pedal axis range ──────────────────────────────

#[test]
fn standard_input_inverted_pedal_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::STANDARD_INPUT;
    data[1] = 0x00; data[2] = 0x80; // center steering

    // All pedals fully pressed (inverted: 0x00 = 1.0)
    data[3] = 0x00;
    data[4] = 0x00;
    data[5] = 0x00;
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert!((state.throttle - 1.0).abs() < 0.01, "fully pressed throttle");
    assert!((state.brake - 1.0).abs() < 0.01, "fully pressed brake");
    assert!((state.clutch - 1.0).abs() < 0.01, "fully pressed clutch");

    // All pedals released (inverted: 0xFF = 0.0)
    data[3] = 0xFF;
    data[4] = 0xFF;
    data[5] = 0xFF;
    let state2 = parse_standard_report(&data).ok_or("parse failed")?;
    assert!(state2.throttle.abs() < 0.01, "released throttle");
    assert!(state2.brake.abs() < 0.01, "released brake");
    assert!(state2.clutch.abs() < 0.01, "released clutch");
    Ok(())
}

// ─── Standard input: hat switch ──────────────────────────────────────────────

#[test]
fn standard_input_hat_all_directions() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::STANDARD_INPUT;
    for hat_val in 0u8..=0x0F {
        data[9] = hat_val;
        let state = parse_standard_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, hat_val, "hat={hat_val}");
    }
    // Upper nibble should be masked away
    data[9] = 0xF3;
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x03);
    Ok(())
}

// ─── Standard input: buttons ─────────────────────────────────────────────────

#[test]
fn standard_input_all_button_bits() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = report_ids::STANDARD_INPUT;
    data[7] = 0xFF;
    data[8] = 0xFF;
    let state = parse_standard_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons, 0xFFFF, "all 16 buttons must be set");
    Ok(())
}

// ─── Pedal report: 12-bit masking ────────────────────────────────────────────

#[test]
fn pedal_report_12bit_upper_bits_masked() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 7];
    data[0] = 0x01;
    // throttle = 0xF123 → masked to 0x0123
    data[1] = 0x23;
    data[2] = 0xF1;
    // brake = 0xFABC → masked to 0x0ABC
    data[3] = 0xBC;
    data[4] = 0xFA;
    // clutch = 0xFDEF → masked to 0x0DEF
    data[5] = 0xEF;
    data[6] = 0xFD;
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(state.throttle_raw, 0x0123);
    assert_eq!(state.brake_raw, 0x0ABC);
    assert_eq!(state.clutch_raw, 0x0DEF);
    Ok(())
}

#[test]
fn pedal_report_full_press_is_0x0fff() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 7];
    data[0] = 0x01;
    data[1] = 0xFF; data[2] = 0x0F; // 0x0FFF
    data[3] = 0xFF; data[4] = 0x0F; // 0x0FFF
    data[5] = 0xFF; data[6] = 0x0F; // 0x0FFF
    let state = parse_pedal_report(&data).ok_or("parse failed")?;
    assert_eq!(state.throttle_raw, 0x0FFF);
    assert_eq!(state.brake_raw, 0x0FFF);
    assert_eq!(state.clutch_raw, 0x0FFF);
    assert_eq!(state.axis_count, 3);
    Ok(())
}

// ─── Display report: mode + digits ───────────────────────────────────────────

#[test]
fn display_report_auto_mode() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_display_report(0x01, [0x00, 0x00, 0x00], 0);
    assert_eq!(report[2], 0x01, "mode byte");
    Ok(())
}

#[test]
fn display_report_all_digits_ff() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_display_report(0x00, [0xFF, 0xFF, 0xFF], 0xFF);
    assert_eq!(report[3], 0xFF);
    assert_eq!(report[4], 0xFF);
    assert_eq!(report[5], 0xFF);
    assert_eq!(report[6], 0xFF, "brightness");
    Ok(())
}

// ─── LED report: single LED ─────────────────────────────────────────────────

#[test]
fn led_report_single_led() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_led_report(0x0001, 128);
    assert_eq!(report[2], 0x01, "low byte");
    assert_eq!(report[3], 0x00, "high byte");
    assert_eq!(report[4], 128, "brightness");
    Ok(())
}

#[test]
fn led_report_full_bitmask() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_led_report(0xFFFF, 255);
    assert_eq!(report[2], 0xFF);
    assert_eq!(report[3], 0xFF);
    assert_eq!(report[4], 255);
    Ok(())
}

// ─── Rumble report: max values ───────────────────────────────────────────────

#[test]
fn rumble_report_max_intensity() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rumble_report(255, 255, 255);
    assert_eq!(report[2], 255, "left motor");
    assert_eq!(report[3], 255, "right motor");
    assert_eq!(report[4], 255, "duration = ~2.55 seconds");
    Ok(())
}

// ─── Rotation range: constants ──────────────────────────────────────────────

#[test]
fn rotation_range_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(MIN_ROTATION_DEGREES, 90);
    assert_eq!(MAX_ROTATION_DEGREES, 2520);
    Ok(())
}

// ─── Rotation range: specific common values ──────────────────────────────────

#[test]
fn rotation_range_report_1080() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rotation_range_report(1080);
    let range = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(range, 1080);
    Ok(())
}

#[test]
fn rotation_range_report_2520() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rotation_range_report(2520);
    let range = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(range, 2520);
    Ok(())
}

// ─── Kernel range sequence: step 1 and 2 fixed ──────────────────────────────

#[test]
fn kernel_range_sequence_fixed_steps() -> Result<(), Box<dyn std::error::Error>> {
    let seq1 = build_kernel_range_sequence(500);
    let seq2 = build_kernel_range_sequence(1000);
    // Steps 1 and 2 must be identical regardless of degrees
    assert_eq!(seq1[0], seq2[0], "step 1 must be constant");
    assert_eq!(seq1[1], seq2[1], "step 2 must be constant");
    // Step 3 must differ
    assert_ne!(seq1[2], seq2[2], "step 3 must differ for different degrees");
    Ok(())
}

#[test]
fn kernel_range_sequence_clamps_above_max() -> Result<(), Box<dyn std::error::Error>> {
    let at_max = build_kernel_range_sequence(MAX_ROTATION_DEGREES);
    let above = build_kernel_range_sequence(MAX_ROTATION_DEGREES + 100);
    assert_eq!(at_max[2], above[2], "above MAX must clamp to MAX");
    Ok(())
}

// ─── Stop all report ─────────────────────────────────────────────────────────

#[test]
fn stop_all_report_wire_format() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_stop_all_report();
    assert_eq!(report[0], report_ids::FFB_OUTPUT);
    assert_eq!(report[1], ffb_commands::STOP_ALL);
    assert_eq!(&report[2..], &[0u8; 6]);
    Ok(())
}

// ─── Gain report clamp ──────────────────────────────────────────────────────

#[test]
fn gain_report_clamp_to_100() -> Result<(), Box<dyn std::error::Error>> {
    for gain in [101u8, 150, 200, 255] {
        let report = build_set_gain_report(gain);
        assert_eq!(report[2], 100, "gain {gain} must clamp to 100");
    }
    Ok(())
}

#[test]
fn gain_report_passthrough_valid() -> Result<(), Box<dyn std::error::Error>> {
    for gain in [0u8, 50, 100] {
        let report = build_set_gain_report(gain);
        assert_eq!(report[2], gain, "gain {gain} must pass through");
    }
    Ok(())
}

// ─── Mode switch report ─────────────────────────────────────────────────────

#[test]
fn mode_switch_report_full_wire() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_mode_switch_report();
    assert_eq!(report, [0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00]);
    Ok(())
}

// ─── Proptest ────────────────────────────────────────────────────────────────

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any valid torque in [-max, max] round-trips with bounded error.
        #[test]
        fn prop_cf_round_trip(
            max_torque in 1.0_f32..=25.0_f32,
            fraction in -1.0_f32..=1.0_f32,
        ) {
            let torque_nm = max_torque * fraction;
            let enc = FanatecConstantForceEncoder::new(max_torque);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque_nm, 0, &mut out);
            let raw = i16::from_le_bytes([out[2], out[3]]);

            let decoded = if raw >= 0 {
                (raw as f32 / i16::MAX as f32) * max_torque
            } else {
                (raw as f32 / (-(i16::MIN as f32))) * max_torque
            };

            let lsb = max_torque / i16::MAX as f32;
            let error = (decoded - torque_nm).abs();
            prop_assert!(
                error < lsb + 0.01,
                "round-trip error {error} exceeds 1 LSB ({lsb}) for torque={torque_nm}"
            );
        }

        /// Reserved bytes (4-7) are always zero.
        #[test]
        fn prop_cf_reserved_bytes_zero(
            max_torque in 0.1_f32..=25.0_f32,
            torque in -50.0_f32..=50.0_f32,
        ) {
            let enc = FanatecConstantForceEncoder::new(max_torque);
            let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque, 0, &mut out);
            prop_assert_eq!(out[4], 0);
            prop_assert_eq!(out[5], 0);
            prop_assert_eq!(out[6], 0);
            prop_assert_eq!(out[7], 0);
        }

        /// Report ID and command byte are always correct.
        #[test]
        fn prop_cf_header_bytes(
            max_torque in 0.1_f32..=25.0_f32,
            torque in -50.0_f32..=50.0_f32,
        ) {
            let enc = FanatecConstantForceEncoder::new(max_torque);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque, 0, &mut out);
            prop_assert_eq!(out[0], report_ids::FFB_OUTPUT);
            prop_assert_eq!(out[1], ffb_commands::CONSTANT_FORCE);
        }

        /// Rotation range report always clamps to [90, 2520].
        #[test]
        fn prop_rotation_range_clamped(degrees: u16) {
            let report = build_rotation_range_report(degrees);
            let range = u16::from_le_bytes([report[2], report[3]]);
            prop_assert!(range >= MIN_ROTATION_DEGREES, "range {range} < MIN {MIN_ROTATION_DEGREES}");
            prop_assert!(range <= MAX_ROTATION_DEGREES, "range {range} > MAX {MAX_ROTATION_DEGREES}");
        }

        /// LED report always has correct header.
        #[test]
        fn prop_led_report_header(bitmask: u16, brightness: u8) {
            let report = build_led_report(bitmask, brightness);
            prop_assert_eq!(report[0], report_ids::LED_DISPLAY);
            prop_assert_eq!(report[1], led_commands::REV_LIGHTS);
            let decoded_mask = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded_mask, bitmask);
            prop_assert_eq!(report[4], brightness);
        }

        /// Gain report clamps to [0, 100].
        #[test]
        fn prop_gain_clamped(gain: u8) {
            let report = build_set_gain_report(gain);
            prop_assert!(report[2] <= 100);
            if gain <= 100 {
                prop_assert_eq!(report[2], gain);
            }
        }

        /// fix_report_values is idempotent (applied twice = same as once).
        #[test]
        fn prop_fix_report_values_idempotent(
            v0 in 0i16..=255i16,
            v1 in 0i16..=255i16,
            v2 in 0i16..=255i16,
            v3 in 0i16..=255i16,
            v4 in 0i16..=255i16,
            v5 in 0i16..=255i16,
            v6 in 0i16..=255i16,
        ) {
            let mut values1 = [v0, v1, v2, v3, v4, v5, v6];
            fix_report_values(&mut values1);
            let after_first = values1;
            // Values after first fix are all in [-128, 127], so second fix is no-op
            // (only if all values < 0x80 after first pass)
            let mut values2 = after_first;
            fix_report_values(&mut values2);
            // After first fix, all values ≥ 0x80 become negative. Since the
            // function checks *v >= 0x80 using i16, negative values won't trigger.
            prop_assert_eq!(values2, after_first, "fix_report_values must be idempotent on byte range");
        }
    }
}
