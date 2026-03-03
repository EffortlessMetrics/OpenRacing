//! Advanced tests for Logitech protocol crate — device PID recognition, FFB encoding,
//! LED RPM indicators, range setting, legacy DFP protocol, and proptest boundary tests.

use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LOGITECH_VENDOR_ID, LogitechConstantForceEncoder, LogitechModel,
    build_gain_report, build_mode_switch_report, build_native_mode_report,
    build_set_autocenter_report, build_set_leds_report, build_set_range_dfp_reports,
    build_set_range_report, is_wheel_product, parse_input_report, product_ids,
};

use proptest::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Device PID recognition — all 19 product IDs
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_all_product_ids_are_wheel_products() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        product_ids::WINGMAN_FORMULA_FORCE,
        product_ids::WINGMAN_FORMULA_FORCE_GP,
        product_ids::MOMO,
        product_ids::DRIVING_FORCE_EX,
        product_ids::DRIVING_FORCE_PRO,
        product_ids::DRIVING_FORCE_GT,
        product_ids::SPEED_FORCE_WIRELESS,
        product_ids::MOMO_2,
        product_ids::VIBRATION_WHEEL,
        product_ids::G25,
        product_ids::G27,
        product_ids::G29_PS,
        product_ids::G920,
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
        product_ids::G923_XBOX_ALT,
        product_ids::G_PRO,
        product_ids::G_PRO_XBOX,
    ];
    for pid in all_pids {
        assert!(
            is_wheel_product(pid),
            "PID 0x{pid:04X} must be a wheel product"
        );
    }
    assert!(!is_wheel_product(0x0000), "0x0000 must not be a wheel");
    assert!(!is_wheel_product(0xFFFF), "0xFFFF must not be a wheel");
    Ok(())
}

#[test]
fn test_wingman_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::WINGMAN_FORMULA_FORCE);
    assert_eq!(model, LogitechModel::WingManFormulaForce);
    assert!((model.max_torque_nm() - 0.5).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 180);
    assert!(!model.supports_trueforce());
    assert!(!model.supports_range_command());
    Ok(())
}

#[test]
fn test_momo_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::MOMO);
    assert_eq!(model, LogitechModel::MOMO);
    assert!((model.max_torque_nm() - 2.0).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 270);
    Ok(())
}

#[test]
fn test_driving_force_pro_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::DRIVING_FORCE_PRO);
    assert_eq!(model, LogitechModel::DrivingForcePro);
    assert!((model.max_torque_nm() - 2.0).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 900);
    assert!(model.supports_hardware_friction());
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_driving_force_gt_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::DRIVING_FORCE_GT);
    assert_eq!(model, LogitechModel::DrivingForceGT);
    assert!(model.supports_hardware_friction());
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_g25_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G25);
    assert_eq!(model, LogitechModel::G25);
    assert!((model.max_torque_nm() - 2.5).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 900);
    assert!(model.supports_hardware_friction());
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_g27_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G27);
    assert_eq!(model, LogitechModel::G27);
    assert!((model.max_torque_nm() - 2.5).abs() < 0.01);
    assert!(model.supports_hardware_friction());
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_g29_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G29_PS);
    assert_eq!(model, LogitechModel::G29);
    assert!((model.max_torque_nm() - 2.2).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 900);
    assert!(!model.supports_trueforce());
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_g920_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G920);
    assert_eq!(model, LogitechModel::G920);
    assert!((model.max_torque_nm() - 2.2).abs() < 0.01);
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_g923_all_variants_same_model() -> Result<(), Box<dyn std::error::Error>> {
    let g923_pids = [
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
        product_ids::G923_XBOX_ALT,
    ];
    for pid in g923_pids {
        let model = LogitechModel::from_product_id(pid);
        assert_eq!(
            model,
            LogitechModel::G923,
            "PID 0x{pid:04X} must map to G923"
        );
    }
    let g923 = LogitechModel::G923;
    assert!((g923.max_torque_nm() - 2.2).abs() < 0.01);
    assert!(g923.supports_trueforce());
    assert!(g923.supports_range_command());
    Ok(())
}

#[test]
fn test_g_pro_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G_PRO);
    assert_eq!(model, LogitechModel::GPro);
    assert!((model.max_torque_nm() - 11.0).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 1080);
    assert!(model.supports_range_command());
    Ok(())
}

#[test]
fn test_g_pro_xbox_same_model() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(product_ids::G_PRO_XBOX);
    assert_eq!(model, LogitechModel::GPro);
    Ok(())
}

#[test]
fn test_unknown_pid_fallback() -> Result<(), Box<dyn std::error::Error>> {
    let model = LogitechModel::from_product_id(0xFFFF);
    assert_eq!(model, LogitechModel::Unknown);
    assert!((model.max_torque_nm() - 2.0).abs() < 0.01);
    assert_eq!(model.max_rotation_deg(), 900);
    assert!(!model.supports_trueforce());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Constant force encoding
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_constant_force_encoding_max() -> Result<(), Box<dyn std::error::Error>> {
    let enc = LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(2.2, &mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], 0x12, "report ID");
    assert_eq!(out[1], 1, "slot 1");
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10_000, "max torque must map to +10000");
    Ok(())
}

#[test]
fn test_constant_force_encoding_neg_max() -> Result<(), Box<dyn std::error::Error>> {
    let enc = LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-2.2, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -10_000, "neg max torque must map to -10000");
    Ok(())
}

#[test]
fn test_constant_force_encoding_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = LogitechConstantForceEncoder::new(11.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 0, "zero torque must produce zero magnitude");
    Ok(())
}

#[test]
fn test_constant_force_half_g_pro() -> Result<(), Box<dyn std::error::Error>> {
    let enc = LogitechConstantForceEncoder::new(11.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(5.5, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 5000, "50% of GPro must give magnitude 5000");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. LED RPM indicator commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_led_mask_single_leds() -> Result<(), Box<dyn std::error::Error>> {
    for bit in 0..5u8 {
        let report = build_set_leds_report(1 << bit);
        assert_eq!(report[0], 0xF8);
        assert_eq!(report[1], 0x12);
        assert_eq!(report[2], 1 << bit, "bit {bit} must be preserved");
        assert_eq!(&report[3..], &[0, 0, 0, 0]);
    }
    Ok(())
}

#[test]
fn test_led_mask_all_on() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_leds_report(0x1F);
    assert_eq!(report[2], 0x1F);
    Ok(())
}

#[test]
fn test_led_mask_truncates_high_bits() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_leds_report(0xFF);
    assert_eq!(report[2], 0x1F, "only lowest 5 bits allowed");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Range setting — standard and DFP legacy
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_set_range_standard_report() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_range_report(900);
    assert_eq!(report[0], 0xF8);
    assert_eq!(report[1], 0x81);
    let range = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(range, 900);
    assert_eq!(&report[4..], &[0, 0, 0]);
    Ok(())
}

#[test]
fn test_set_range_large_value() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_range_report(2520);
    let range = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(range, 2520);
    Ok(())
}

#[test]
fn test_dfp_range_reports_clamping() -> Result<(), Box<dyn std::error::Error>> {
    // Below min (10) clamps to 40 — which is ≤200, so coarse cmd = 0x02
    let below = build_set_range_dfp_reports(10);
    assert_eq!(below[0][0], 0xF8, "coarse report ID");
    assert_eq!(below[0][1], 0x02, "≤200° coarse cmd must be 0x02");

    // Above max (5000) clamps to 900 — which is a no-op fine limit (zeroed)
    let above = build_set_range_dfp_reports(5000);
    assert_eq!(above[0][1], 0x03, ">200° coarse cmd must be 0x03");
    assert_eq!(above[1][0], 0x81, "fine report byte 0");
    assert_eq!(above[1][1], 0x0b, "fine report byte 1");
    // 900° maps to no-op fine limit (all zeroed after header)
    assert_eq!(&above[1][2..7], &[0, 0, 0, 0, 0], "900° fine must be no-op");
    Ok(())
}

#[test]
fn test_dfp_range_reports_structure() -> Result<(), Box<dyn std::error::Error>> {
    let reports = build_set_range_dfp_reports(540);
    assert_eq!(reports.len(), 2, "DFP range needs 2 reports");
    assert_eq!(reports[0][0], 0xF8, "coarse report ID");
    assert_eq!(reports[0][1], 0x03, ">200° coarse cmd");
    assert_eq!(reports[1][0], 0x81, "fine report byte 0");
    assert_eq!(reports[1][1], 0x0b, "fine report byte 1");
    // For 540°, fine limit values must be non-trivial
    assert_ne!(&reports[1][2..7], &[0, 0, 0, 0, 0], "540° fine must not be no-op");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Autocenter and gain commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_autocenter_report_structure() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_set_autocenter_report(255, 127);
    assert_eq!(report[0], 0xF8);
    assert_eq!(report[1], 0x14);
    assert_eq!(report[2], 255);
    assert_eq!(report[3], 127);
    assert_eq!(&report[4..], &[0, 0, 0]);
    Ok(())
}

#[test]
fn test_gain_report_two_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_gain_report(50);
    assert_eq!(report.len(), 2);
    assert_eq!(report[0], 0x16);
    assert_eq!(report[1], 50);
    Ok(())
}

#[test]
fn test_gain_report_zero_and_max() -> Result<(), Box<dyn std::error::Error>> {
    let zero = build_gain_report(0);
    assert_eq!(zero[1], 0);
    let max = build_gain_report(255);
    assert_eq!(max[1], 255);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Native mode and mode switch
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_native_mode_report() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_native_mode_report();
    assert_eq!(report[0], 0xF8);
    assert_eq!(report[1], 0x0A);
    assert_eq!(&report[2..], &[0, 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn test_mode_switch_report() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_mode_switch_report(0x03, true);
    assert_eq!(report[0], 0xF8);
    assert_eq!(report[1], 0x09);
    assert_eq!(report[2], 0x03);
    assert_eq!(report[3], 0x01);
    assert_eq!(report[4], 0x01);
    Ok(())
}

#[test]
fn test_mode_switch_no_detach() -> Result<(), Box<dyn std::error::Error>> {
    let report = build_mode_switch_report(0x05, false);
    assert_eq!(report[2], 0x05);
    assert_eq!(report[4], 0x00, "detach=false → 0");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Model capabilities — TrueForce, hardware friction, range command
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_trueforce_only_g923() -> Result<(), Box<dyn std::error::Error>> {
    let models_without_tf = [
        LogitechModel::WingManFormulaForce,
        LogitechModel::MOMO,
        LogitechModel::DrivingForceEX,
        LogitechModel::DrivingForcePro,
        LogitechModel::DrivingForceGT,
        LogitechModel::SpeedForceWireless,
        LogitechModel::VibrationWheel,
        LogitechModel::G25,
        LogitechModel::G27,
        LogitechModel::G29,
        LogitechModel::G920,
        LogitechModel::GPro,
        LogitechModel::Unknown,
    ];
    for model in models_without_tf {
        assert!(!model.supports_trueforce(), "{model:?} must NOT support TrueForce");
    }
    assert!(LogitechModel::G923.supports_trueforce());
    Ok(())
}

#[test]
fn test_hardware_friction_support() -> Result<(), Box<dyn std::error::Error>> {
    let friction_models = [
        LogitechModel::DrivingForcePro,
        LogitechModel::G25,
        LogitechModel::DrivingForceGT,
        LogitechModel::G27,
    ];
    for model in friction_models {
        assert!(
            model.supports_hardware_friction(),
            "{model:?} must support hardware friction"
        );
    }
    assert!(!LogitechModel::G29.supports_hardware_friction());
    assert!(!LogitechModel::G920.supports_hardware_friction());
    assert!(!LogitechModel::G923.supports_hardware_friction());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Vendor ID
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_vendor_id_constant() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(LOGITECH_VENDOR_ID, 0x046D);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Proptest boundary tests
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    #[test]
    fn prop_constant_force_roundtrip(
        max_torque in 0.1_f32..=11.0_f32,
        frac in -1.0_f32..=1.0_f32,
    ) {
        let torque_nm = max_torque * frac;
        let enc = LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque_nm, &mut out);

        let raw = i16::from_le_bytes([out[2], out[3]]);

        // Report structure
        prop_assert_eq!(out[0], 0x12, "report ID must be 0x12");

        // Sign correctness
        if torque_nm > 0.01 {
            prop_assert!(raw > 0, "positive torque {torque_nm} → positive raw {raw}");
        } else if torque_nm < -0.01 {
            prop_assert!(raw < 0, "negative torque {torque_nm} → negative raw {raw}");
        }

        // Magnitude bounded
        prop_assert!(raw.abs() <= 10_000, "magnitude {raw} must be ≤ 10000");
    }

    #[test]
    fn prop_led_mask_truncation(mask in proptest::num::u8::ANY) {
        let report = build_set_leds_report(mask);
        prop_assert_eq!(report[2], mask & 0x1F);
    }

    #[test]
    fn prop_set_range_preserves_value(degrees in 0u16..=4000u16) {
        let report = build_set_range_report(degrees);
        let recovered = u16::from_le_bytes([report[2], report[3]]);
        prop_assert_eq!(recovered, degrees);
    }

    #[test]
    fn prop_autocenter_preserves_values(
        strength in proptest::num::u8::ANY,
        rate in proptest::num::u8::ANY,
    ) {
        let report = build_set_autocenter_report(strength, rate);
        prop_assert_eq!(report[2], strength);
        prop_assert_eq!(report[3], rate);
    }

    #[test]
    fn prop_gain_report_preserves_value(gain in proptest::num::u8::ANY) {
        let report = build_gain_report(gain);
        prop_assert_eq!(report[1], gain);
    }

    #[test]
    fn prop_input_report_parse_never_panics(
        data in proptest::collection::vec(proptest::num::u8::ANY, 0..=64)
    ) {
        let _ = parse_input_report(&data);
    }

    #[test]
    fn prop_is_wheel_product_never_panics(pid in proptest::num::u16::ANY) {
        let _ = is_wheel_product(pid);
    }
}
