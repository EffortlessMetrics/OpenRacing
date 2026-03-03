//! Advanced tests for Fanatec protocol crate — device PID recognition, tuning menu,
//! LED/display/rumble commands, pedal reports, QR rim detection, and proptest roundtrips.

use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, FanatecModel, FanatecPedalModel,
    FanatecRimId, MAX_ROTATION_DEGREES, MIN_ROTATION_DEGREES, build_display_report,
    build_kernel_range_sequence, build_led_report, build_mode_switch_report,
    build_rotation_range_report, build_rumble_report, build_set_gain_report, build_stop_all_report,
    fix_report_values, is_pedal_product, is_wheelbase_product, led_commands, parse_extended_report,
    parse_pedal_report, parse_standard_report, product_ids, rim_ids,
};

use proptest::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Device PID Recognition — all wheelbases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_csl_dd_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CSL_DD);
    assert_eq!(model, FanatecModel::CslDd);
    assert!((model.max_torque_nm() - 8.0).abs() < 0.01);
    assert!(model.supports_1000hz());
    assert!(model.is_highres());
    assert_eq!(model.max_rotation_degrees(), 2520);
    Ok(())
}

#[test]
fn test_dd1_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::DD1);
    assert_eq!(model, FanatecModel::Dd1);
    assert!((model.max_torque_nm() - 20.0).abs() < 0.01);
    assert!(model.supports_1000hz());
    assert_eq!(model.encoder_cpr(), 16_384);
    Ok(())
}

#[test]
fn test_dd2_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::DD2);
    assert_eq!(model, FanatecModel::Dd2);
    assert!((model.max_torque_nm() - 25.0).abs() < 0.01);
    assert!(model.supports_1000hz());
    assert!(model.is_highres());
    Ok(())
}

#[test]
fn test_gt_dd_pro_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::GT_DD_PRO);
    assert_eq!(model, FanatecModel::GtDdPro);
    assert!((model.max_torque_nm() - 8.0).abs() < 0.01);
    assert!(model.supports_1000hz());
    Ok(())
}

#[test]
fn test_clubsport_v2_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CLUBSPORT_V2);
    assert_eq!(model, FanatecModel::ClubSportV2);
    assert!((model.max_torque_nm() - 8.0).abs() < 0.01);
    assert!(!model.supports_1000hz());
    assert!(!model.is_highres());
    assert_eq!(model.max_rotation_degrees(), 900);
    Ok(())
}

#[test]
fn test_clubsport_v25_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CLUBSPORT_V2_5);
    assert_eq!(model, FanatecModel::ClubSportV25);
    assert_eq!(model.max_rotation_degrees(), 900);
    Ok(())
}

#[test]
fn test_csl_elite_ps4_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CSL_ELITE_PS4);
    assert_eq!(model, FanatecModel::CslElite);
    assert!((model.max_torque_nm() - 6.0).abs() < 0.01);
    assert_eq!(model.max_rotation_degrees(), 1080);
    Ok(())
}

#[test]
fn test_csl_elite_pc_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CSL_ELITE);
    assert_eq!(model, FanatecModel::CslElite);
    Ok(())
}

#[test]
fn test_csr_elite_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CSR_ELITE);
    assert_eq!(model, FanatecModel::CsrElite);
    assert!((model.max_torque_nm() - 5.0).abs() < 0.01);
    assert!(!model.needs_sign_fix());
    Ok(())
}

#[test]
fn test_clubsport_dd_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = FanatecModel::from_product_id(product_ids::CLUBSPORT_DD);
    assert_eq!(model, FanatecModel::ClubSportDd);
    assert!((model.max_torque_nm() - 12.0).abs() < 0.01);
    assert!(model.supports_1000hz());
    Ok(())
}

#[test]
fn test_all_wheelbase_pids_recognized() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbase_pids = [
        product_ids::CLUBSPORT_V2,
        product_ids::CLUBSPORT_V2_5,
        product_ids::CSL_ELITE_PS4,
        product_ids::DD1,
        product_ids::DD2,
        product_ids::CSR_ELITE,
        product_ids::CSL_DD,
        product_ids::GT_DD_PRO,
        product_ids::CLUBSPORT_DD,
        product_ids::CSL_ELITE,
    ];
    for pid in wheelbase_pids {
        assert!(
            is_wheelbase_product(pid),
            "PID 0x{pid:04X} must be a wheelbase"
        );
        let model = FanatecModel::from_product_id(pid);
        assert_ne!(
            model,
            FanatecModel::Unknown,
            "PID 0x{pid:04X} must map to a known model"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Pedal model classification
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_all_pedal_pids_recognized() -> Result<(), Box<dyn std::error::Error>> {
    let pedal_pids = [
        (
            product_ids::CLUBSPORT_PEDALS_V1_V2,
            FanatecPedalModel::ClubSportV1V2,
            2,
        ),
        (
            product_ids::CLUBSPORT_PEDALS_V3,
            FanatecPedalModel::ClubSportV3,
            3,
        ),
        (
            product_ids::CSL_ELITE_PEDALS,
            FanatecPedalModel::CslElitePedals,
            2,
        ),
        (
            product_ids::CSL_PEDALS_LC,
            FanatecPedalModel::CslPedalsLc,
            3,
        ),
        (
            product_ids::CSL_PEDALS_V2,
            FanatecPedalModel::CslPedalsV2,
            3,
        ),
    ];
    for (pid, expected_model, expected_axes) in pedal_pids {
        assert!(is_pedal_product(pid), "PID 0x{pid:04X} must be a pedal");
        let model = FanatecPedalModel::from_product_id(pid);
        assert_eq!(model, expected_model, "PID 0x{pid:04X} model mismatch");
        assert_eq!(
            model.axis_count(),
            expected_axes,
            "PID 0x{pid:04X} axis count"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Rim ID / QR system detection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_all_rim_ids_decode_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let rims = [
        (rim_ids::BMW_GT2, FanatecRimId::BmwGt2),
        (rim_ids::FORMULA_V2, FanatecRimId::FormulaV2),
        (rim_ids::FORMULA_V2_5, FanatecRimId::FormulaV25),
        (rim_ids::MCLAREN_GT3_V2, FanatecRimId::McLarenGt3V2),
        (rim_ids::PORSCHE_911_GT3_R, FanatecRimId::Porsche911Gt3R),
        (rim_ids::PORSCHE_918_RSR, FanatecRimId::Porsche918Rsr),
        (rim_ids::CLUBSPORT_RS, FanatecRimId::ClubSportRs),
        (rim_ids::WRC, FanatecRimId::Wrc),
        (rim_ids::CSL_ELITE_P1, FanatecRimId::CslEliteP1),
        (rim_ids::PODIUM_HUB, FanatecRimId::PodiumHub),
    ];
    for (byte, expected) in rims {
        let actual = FanatecRimId::from_byte(byte);
        assert_eq!(actual, expected, "rim byte 0x{byte:02X} mismatch");
        assert_ne!(actual, FanatecRimId::Unknown);
    }
    Ok(())
}

#[test]
fn test_rim_capabilities_mclaren_gt3_v2() -> Result<(), Box<dyn std::error::Error>> {
    let rim = FanatecRimId::McLarenGt3V2;
    assert!(rim.has_funky_switch());
    assert!(rim.has_dual_clutch());
    assert!(rim.has_rotary_encoders());
    Ok(())
}

#[test]
fn test_rim_capabilities_formula_v2() -> Result<(), Box<dyn std::error::Error>> {
    let rim = FanatecRimId::FormulaV2;
    assert!(!rim.has_funky_switch());
    assert!(rim.has_dual_clutch());
    assert!(!rim.has_rotary_encoders());
    Ok(())
}

#[test]
fn test_rim_capabilities_formula_v25() -> Result<(), Box<dyn std::error::Error>> {
    let rim = FanatecRimId::FormulaV25;
    assert!(!rim.has_funky_switch());
    assert!(rim.has_dual_clutch());
    assert!(rim.has_rotary_encoders());
    Ok(())
}

#[test]
fn test_rim_id_unknown_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(FanatecRimId::from_byte(0x00), FanatecRimId::Unknown);
    assert_eq!(FanatecRimId::from_byte(0xFF), FanatecRimId::Unknown);
    assert_eq!(FanatecRimId::from_byte(0x7F), FanatecRimId::Unknown);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. LED strip control commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_led_report_single_led() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_led_report(0x0001, 255);
    assert_eq!(report[0], 0x08);
    assert_eq!(report[1], led_commands::REV_LIGHTS);
    assert_eq!(report[2], 0x01);
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 255);
    Ok(())
}

#[test]
fn test_led_report_full_bitmask() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_led_report(0xFFFF, 128);
    assert_eq!(report[2], 0xFF);
    assert_eq!(report[3], 0xFF);
    assert_eq!(report[4], 128);
    Ok(())
}

#[test]
fn test_display_report_digits() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_display_report(0, [b'A', b'B', b'C'], 200);
    assert_eq!(report[0], 0x08);
    assert_eq!(report[1], led_commands::DISPLAY);
    assert_eq!(report[2], 0);
    assert_eq!(report[3], b'A');
    assert_eq!(report[4], b'B');
    assert_eq!(report[5], b'C');
    assert_eq!(report[6], 200);
    assert_eq!(report[7], 0);
    Ok(())
}

#[test]
fn test_display_report_auto_mode() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_display_report(1, [0, 0, 0], 0);
    assert_eq!(report[2], 1, "mode byte");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Rumble motor commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_rumble_full_intensity() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rumble_report(255, 255, 255);
    assert_eq!(report[0], 0x08);
    assert_eq!(report[1], led_commands::RUMBLE);
    assert_eq!(report[2], 255);
    assert_eq!(report[3], 255);
    assert_eq!(report[4], 255);
    Ok(())
}

#[test]
fn test_rumble_asymmetric() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_rumble_report(200, 50, 100);
    assert_eq!(report[2], 200, "left motor");
    assert_eq!(report[3], 50, "right motor");
    assert_eq!(report[4], 100, "duration");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Constant force encoder
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_constant_force_per_model() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (FanatecModel::Dd1, 20.0_f32),
        (FanatecModel::Dd2, 25.0),
        (FanatecModel::CslDd, 8.0),
        (FanatecModel::CslElite, 6.0),
        (FanatecModel::CsrElite, 5.0),
        (FanatecModel::ClubSportDd, 12.0),
    ];
    for (model, max_nm) in models {
        let enc = FanatecConstantForceEncoder::new(max_nm);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(max_nm, 0, &mut out);
        let raw = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!(
            raw,
            i16::MAX,
            "{model:?} at max {max_nm} Nm must produce i16::MAX, got {raw}"
        );
    }
    Ok(())
}

#[test]
fn test_constant_force_half_dd2() -> Result<(), Box<dyn std::error::Error>> {
    let enc = FanatecConstantForceEncoder::new(25.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(12.5, 0, &mut out);
    let raw = i16::from_le_bytes([out[2], out[3]]);
    // 12.5 / 25.0 = 0.5 → ~i16::MAX / 2
    assert!(raw > 16_000 && raw < 16_500, "expected ~16384, got {raw}");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Rotation range reports
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_rotation_range_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let below_min = build_rotation_range_report(10);
    let range = u16::from_le_bytes([below_min[2], below_min[3]]);
    assert_eq!(range, MIN_ROTATION_DEGREES);

    let above_max = build_rotation_range_report(10000);
    let range2 = u16::from_le_bytes([above_max[2], above_max[3]]);
    assert_eq!(range2, MAX_ROTATION_DEGREES);
    Ok(())
}

#[test]
fn test_kernel_range_sequence_structure() -> Result<(), Box<dyn std::error::Error>> {
    let seq = build_kernel_range_sequence(540);
    assert_eq!(seq[0][0], 0xF5, "step 1 must start with 0xF5");
    assert_eq!(seq[1][0], 0xF8, "step 2 must start with 0xF8");
    assert_eq!(seq[2][0], 0xF8, "step 3 must start with 0xF8");
    assert_eq!(seq[2][1], 0x81, "step 3 cmd must be 0x81");
    let range = u16::from_le_bytes([seq[2][2], seq[2][3]]);
    assert_eq!(range, 540);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Sign fix (CSR Elite exclusion)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_sign_fix_needed_for_dd_bases() -> Result<(), Box<dyn std::error::Error>> {
    assert!(FanatecModel::Dd1.needs_sign_fix());
    assert!(FanatecModel::Dd2.needs_sign_fix());
    assert!(FanatecModel::CslDd.needs_sign_fix());
    assert!(FanatecModel::CslElite.needs_sign_fix());
    assert!(!FanatecModel::CsrElite.needs_sign_fix());
    assert!(!FanatecModel::Unknown.needs_sign_fix());
    Ok(())
}

#[test]
fn test_fix_report_values_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let mut vals: [i16; 7] = [0x7F, 0x80, 0x81, 0xFE, 0xFF, 0x00, 0x01];
    fix_report_values(&mut vals);
    assert_eq!(vals[0], 0x7F, "0x7F untouched");
    assert_eq!(vals[1], -128, "0x80 → -128");
    assert_eq!(vals[2], -127, "0x81 → -127");
    assert_eq!(vals[3], -2, "0xFE → -2");
    assert_eq!(vals[4], -1, "0xFF → -1");
    assert_eq!(vals[5], 0, "0x00 untouched");
    assert_eq!(vals[6], 1, "0x01 untouched");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Extended telemetry report
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_extended_report_temperature_extraction() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    data[5] = 85; // motor temp
    data[6] = 55; // board temp
    data[7] = 30; // current 3.0A
    data[10] = 0x03; // over-temp + over-current
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.motor_temp_c, 85);
    assert_eq!(state.board_temp_c, 55);
    assert_eq!(state.current_raw, 30);
    assert_eq!(state.fault_flags & 0x03, 0x03);
    Ok(())
}

#[test]
fn test_extended_report_steering_velocity() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x02;
    // steering_raw = -500 (0xFE0C LE)
    let steer_bytes = (-500i16).to_le_bytes();
    data[1] = steer_bytes[0];
    data[2] = steer_bytes[1];
    // velocity = 1234
    let vel_bytes = 1234i16.to_le_bytes();
    data[3] = vel_bytes[0];
    data[4] = vel_bytes[1];
    let state = parse_extended_report(&data).ok_or("parse failed")?;
    assert_eq!(state.steering_raw, -500);
    assert_eq!(state.steering_velocity, 1234);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Mode switch and stop reports
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_stop_all_report_byte_layout() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_stop_all_report();
    assert_eq!(report[0], 0x01);
    assert_eq!(report[1], 0x0F);
    assert_eq!(&report[2..], &[0u8; 6]);
    Ok(())
}

#[test]
fn test_mode_switch_report_byte_layout() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_mode_switch_report();
    assert_eq!(report[0], 0x01);
    assert_eq!(report[1], 0x01);
    assert_eq!(report[2], 0x03);
    Ok(())
}

#[test]
fn test_set_gain_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let g0 = build_set_gain_report(0);
    assert_eq!(g0[2], 0);
    let g100 = build_set_gain_report(100);
    assert_eq!(g100[2], 100);
    let g_over = build_set_gain_report(255);
    assert_eq!(g_over[2], 100, "must clamp to 100");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Proptest roundtrips
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    #[test]
    fn prop_constant_force_roundtrip(
        max_torque in 0.1_f32..=25.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max_torque * frac;
        let enc = FanatecConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque_nm, 0, &mut out);

        let raw = i16::from_le_bytes([out[2], out[3]]);
        prop_assert_eq!(out[0], 0x01, "report ID");
        prop_assert_eq!(out[1], 0x01, "constant force cmd");

        // Monotonicity: positive torque → positive raw
        if torque_nm > 0.01 {
            prop_assert!(raw > 0, "positive torque {torque_nm} must produce positive raw {raw}");
        } else if torque_nm < -0.01 {
            prop_assert!(raw < 0, "negative torque {torque_nm} must produce negative raw {raw}");
        }
    }

    #[test]
    fn prop_led_bitmask_preserved(bitmask in proptest::num::u16::ANY, brightness in proptest::num::u8::ANY) {
        let report = build_led_report(bitmask, brightness);
        let recovered = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(recovered, bitmask);
        prop_assert_eq!(report[4], brightness);
    }

    #[test]
    fn prop_rotation_range_clamped(degrees in proptest::num::u16::ANY) {
        let report = build_rotation_range_report(degrees);
        let range = u16::from_le_bytes([report[2], report[3]]);
        prop_assert!(range >= MIN_ROTATION_DEGREES);
        prop_assert!(range <= MAX_ROTATION_DEGREES);
    }

    #[test]
    fn prop_rumble_report_preserves_values(
        left in proptest::num::u8::ANY,
        right in proptest::num::u8::ANY,
        dur in proptest::num::u8::ANY,
    ) {
        let report = build_rumble_report(left, right, dur);
        prop_assert_eq!(report[2], left);
        prop_assert_eq!(report[3], right);
        prop_assert_eq!(report[4], dur);
    }

    #[test]
    fn prop_standard_report_parse_never_panics(
        data in proptest::collection::vec(proptest::num::u8::ANY, 0..=64)
    ) {
        let _ = parse_standard_report(&data);
    }

    #[test]
    fn prop_extended_report_parse_never_panics(
        data in proptest::collection::vec(proptest::num::u8::ANY, 0..=64)
    ) {
        let _ = parse_extended_report(&data);
    }

    #[test]
    fn prop_pedal_report_parse_never_panics(
        data in proptest::collection::vec(proptest::num::u8::ANY, 0..=16)
    ) {
        let _ = parse_pedal_report(&data);
    }
}
