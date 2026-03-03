//! Comprehensive tests for the Logitech HID protocol crate.
//!
//! Covers:
//! 1. Input report parsing for G29/G920/G923/PRO/TRUEFORCE device scenarios
//! 2. Output report construction (FFB constant/spring/damper/friction encoding)
//! 3. Device identification via PID for all supported models
//! 4. Mode switching (compatibility → native) for every known mode ID
//! 5. Force feedback effect encoding precision per device model
//! 6. Edge cases: short reports, incompatible modes, legacy devices
//! 7. Property tests for axis resolution and encoding
//! 8. Known constant validation (PIDs, report sizes, FFB ranges)

use proptest::prelude::*;
use racing_wheel_hid_logitech_protocol as lg;
use racing_wheel_hid_logitech_protocol::ids::{commands, product_ids, report_ids};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Input report parsing — per-device scenarios
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper to build a 12-byte input report with specified fields.
fn build_input(
    steering_raw: u16,
    throttle: u8,
    brake: u8,
    clutch: u8,
    buttons: u16,
    hat: u8,
    paddles: u8,
) -> [u8; 12] {
    let [s_lo, s_hi] = steering_raw.to_le_bytes();
    let [b_lo, b_hi] = buttons.to_le_bytes();
    [
        0x01, s_lo, s_hi, throttle, brake, clutch, b_lo, b_hi, hat, paddles, 0x00, 0x00,
    ]
}

/// G29: full left lock, full throttle + brake (left-foot braking scenario).
#[test]
fn input_g29_full_left_lock_full_pedals() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input(0x0000, 0xFF, 0xFF, 0x00, 0x0000, 0x08, 0x00);
    let state = lg::parse_input_report(&data).ok_or("G29 report should parse")?;
    assert!((state.steering + 1.0).abs() < 0.001, "full left = -1.0");
    assert!((state.throttle - 1.0).abs() < 0.001, "throttle fully pressed");
    assert!((state.brake - 1.0).abs() < 0.001, "brake fully pressed");
    assert!(state.clutch.abs() < 0.001, "clutch released");
    assert_eq!(state.hat, 0x08, "hat neutral");
    Ok(())
}

/// G920: center steering, half pedals, upshift paddle engaged.
#[test]
fn input_g920_center_half_pedals_upshift() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input(0x8000, 0x80, 0x40, 0x00, 0x0000, 0x08, 0x01);
    let state = lg::parse_input_report(&data).ok_or("G920 report should parse")?;
    assert!(state.steering.abs() < 0.001, "center = 0.0");
    assert!((state.throttle - 128.0 / 255.0).abs() < 0.01, "~50% throttle");
    assert!((state.brake - 64.0 / 255.0).abs() < 0.01, "~25% brake");
    assert_eq!(state.paddles, 0x01, "right/upshift paddle only");
    Ok(())
}

/// G923: full right lock, all buttons pressed (0xFFFF), both paddles.
#[test]
fn input_g923_full_right_all_buttons() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input(0xFFFF, 0x00, 0x00, 0xFF, 0xFFFF, 0x00, 0x03);
    let state = lg::parse_input_report(&data).ok_or("G923 report should parse")?;
    assert!((state.steering - 1.0).abs() < 0.001, "full right ≈ +1.0");
    assert!(state.throttle.abs() < 0.001, "throttle released");
    assert!(state.brake.abs() < 0.001, "brake released");
    assert!((state.clutch - 1.0).abs() < 0.001, "clutch fully pressed");
    assert_eq!(state.buttons, 0xFFFF, "all 16 button bits set");
    assert_eq!(state.paddles, 0x03, "both paddles engaged");
    assert_eq!(state.hat, 0x00, "hat = up (0x00)");
    Ok(())
}

/// G PRO: steering at 25% right, hat = right (0x02), no paddles.
#[test]
fn input_gpro_quarter_right_hat_right() -> Result<(), Box<dyn std::error::Error>> {
    // 25% right = 0x8000 + 0x2000 = 0xA000
    let data = build_input(0xA000, 0x00, 0x00, 0x00, 0x0000, 0x02, 0x00);
    let state = lg::parse_input_report(&data).ok_or("G PRO report should parse")?;
    let expected_steering = (0xA000u16 as f32 - 32768.0) / 32768.0;
    assert!(
        (state.steering - expected_steering).abs() < 0.001,
        "steering should be ~+0.25"
    );
    assert_eq!(state.hat, 0x02, "hat = right");
    assert_eq!(state.paddles, 0x00, "no paddles");
    Ok(())
}

/// G923 TrueForce model: verify model classification for both PS and Xbox PIDs.
#[test]
fn input_g923_trueforce_model_variants() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (product_ids::G923, "G923 native"),
        (product_ids::G923_PS, "G923 PS compat"),
        (product_ids::G923_XBOX, "G923 Xbox"),
        (product_ids::G923_XBOX_ALT, "G923 Xbox alt"),
    ];
    for (pid, name) in models {
        let model = lg::LogitechModel::from_product_id(pid);
        assert_eq!(
            model,
            lg::LogitechModel::G923,
            "{name} (0x{pid:04X}) must classify as G923"
        );
        assert!(
            model.supports_trueforce(),
            "{name} must support TrueForce"
        );
    }
    Ok(())
}

/// Verify D-pad hat positions parse correctly for all 9 standard positions.
#[test]
fn input_hat_switch_all_positions() -> Result<(), Box<dyn std::error::Error>> {
    let positions = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    for &pos in &positions {
        let data = build_input(0x8000, 0, 0, 0, 0, pos, 0);
        let state = lg::parse_input_report(&data).ok_or("hat report should parse")?;
        assert_eq!(state.hat, pos, "hat position 0x{pos:02X} must round-trip");
    }
    Ok(())
}

/// Steering resolution: verify specific raw values decode correctly.
#[test]
fn input_steering_resolution_specific_values() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u16, f32)] = &[
        (0x0000, -1.0),
        (0x4000, -0.5),
        (0x8000, 0.0),
        (0xC000, 0.5),
        (0xFFFF, 0xFFFFu16 as f32 / 32768.0 - 1.0), // ~+0.99997
    ];
    for &(raw, expected) in cases {
        let data = build_input(raw, 0, 0, 0, 0, 0x08, 0);
        let state = lg::parse_input_report(&data).ok_or("steering report should parse")?;
        assert!(
            (state.steering - expected).abs() < 0.001,
            "raw 0x{raw:04X}: expected {expected}, got {}",
            state.steering
        );
    }
    Ok(())
}

/// Pedal axis boundaries: 0x00 = 0.0, 0x80 ≈ 0.502, 0xFF = 1.0.
#[test]
fn input_pedal_axis_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let cases: &[(u8, f32)] = &[
        (0x00, 0.0),
        (0x01, 1.0 / 255.0),
        (0x80, 128.0 / 255.0),
        (0xFE, 254.0 / 255.0),
        (0xFF, 1.0),
    ];
    for &(raw, expected) in cases {
        let data = build_input(0x8000, raw, 0, 0, 0, 0x08, 0);
        let state = lg::parse_input_report(&data).ok_or("pedal report should parse")?;
        assert!(
            (state.throttle - expected).abs() < 1e-5,
            "throttle raw 0x{raw:02X}: expected {expected}, got {}",
            state.throttle
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Output report construction — FFB constant/spring/damper/friction
// ═══════════════════════════════════════════════════════════════════════════════

/// Constant force encoder: specific torque values for G29 (2.2 Nm).
#[test]
fn output_constant_force_g29_specific_torques() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

    // 0.22 Nm ≈ 10% of max → magnitude ≈ 1000 (±1 for f32 precision)
    enc.encode(0.22, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert!(
        (mag - 1000).abs() <= 1,
        "0.22 Nm / 2.2 Nm ≈ 0.1 × 10000 ≈ 1000, got {mag}"
    );

    // -1.1 Nm = -50% of max → magnitude -5000
    enc.encode(-1.1, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -5000, "-1.1 Nm / 2.2 Nm = -0.5 × 10000 = -5000");

    Ok(())
}

/// Constant force encoder: specific torque values for G PRO (11 Nm).
#[test]
fn output_constant_force_gpro_specific_torques() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(11.0);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

    // 5.5 Nm = 50% → magnitude 5000
    enc.encode(5.5, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 5000, "5.5 Nm / 11.0 Nm = 0.5 × 10000 = 5000");

    // 11.0 Nm = 100% → magnitude 10000
    enc.encode(11.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000, "full torque = 10000");

    Ok(())
}

/// Constant force encoder: G25 (2.5 Nm) and WingMan (0.5 Nm).
#[test]
fn output_constant_force_various_models() -> Result<(), Box<dyn std::error::Error>> {
    // G25: 2.5 Nm max
    let enc_g25 = lg::LogitechConstantForceEncoder::new(2.5);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc_g25.encode(1.25, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 5000, "G25: 1.25 / 2.5 = 0.5 → 5000");

    // WingMan: 0.5 Nm max
    let enc_wm = lg::LogitechConstantForceEncoder::new(0.5);
    enc_wm.encode(0.5, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000, "WingMan: 0.5 / 0.5 = 1.0 → 10000");

    Ok(())
}

/// Constant force encoder: very small max_torque should clamp to 0.01 minimum.
#[test]
fn output_constant_force_tiny_max_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(0.001); // should clamp to 0.01
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.005, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    // 0.005 / 0.01 = 0.5 → 5000
    assert_eq!(mag, 5000, "tiny max_torque should clamp to 0.01");
    Ok(())
}

/// Constant force encoder: negative max_torque should clamp to 0.01.
#[test]
fn output_constant_force_negative_max_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(-5.0); // should clamp to 0.01
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.01, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000, "negative max_torque → clamp to 0.01, 0.01/0.01 = 1.0 → 10000");
    Ok(())
}

/// Verify the constant force report structure matches HID PID spec.
#[test]
fn output_constant_force_report_structure() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(0.0, &mut out);
    assert_eq!(len, lg::CONSTANT_FORCE_REPORT_LEN, "encode returns report length");
    assert_eq!(out[0], report_ids::CONSTANT_FORCE, "report ID = 0x12");
    assert_eq!(out[1], 1, "effect block index = 1 (1-based)");
    Ok(())
}

/// encode_zero report structure.
#[test]
fn output_encode_zero_structure() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(11.0);
    let mut out = [0xFFu8; lg::CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode_zero(&mut out);
    assert_eq!(len, lg::CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
    assert_eq!(out[1], 1);
    assert_eq!(out[2], 0, "magnitude low byte zeroed");
    assert_eq!(out[3], 0, "magnitude high byte zeroed");
    Ok(())
}

/// Gain report: boundary values.
#[test]
fn output_gain_report_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let r0 = lg::build_gain_report(0x00);
    assert_eq!(r0, [report_ids::DEVICE_GAIN, 0x00], "zero gain");

    let r50 = lg::build_gain_report(0x80);
    assert_eq!(r50, [report_ids::DEVICE_GAIN, 0x80], "50% gain");

    let r100 = lg::build_gain_report(0xFF);
    assert_eq!(r100, [report_ids::DEVICE_GAIN, 0xFF], "100% gain");

    Ok(())
}

/// Autocenter report: full strength, maximum rate.
#[test]
fn output_autocenter_full_params() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_set_autocenter_report(0xFF, 0xFF);
    assert_eq!(r[0], report_ids::VENDOR);
    assert_eq!(r[1], commands::SET_AUTOCENTER);
    assert_eq!(r[2], 0xFF, "strength = max");
    assert_eq!(r[3], 0xFF, "rate = max");
    assert_eq!(&r[4..], &[0u8; 3]);
    Ok(())
}

/// LED report: individual LED patterns.
#[test]
fn output_led_individual_patterns() -> Result<(), Box<dyn std::error::Error>> {
    for bit in 0..5u8 {
        let mask = 1u8 << bit;
        let r = lg::build_set_leds_report(mask);
        assert_eq!(r[2], mask, "LED bit {bit} should be set");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Device identification — all supported models
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify every PID maps to the correct LogitechModel variant.
#[test]
fn device_id_all_pid_to_model_mapping() -> Result<(), Box<dyn std::error::Error>> {
    let expected: &[(u16, lg::LogitechModel)] = &[
        (product_ids::MOMO, lg::LogitechModel::MOMO),
        (product_ids::MOMO_2, lg::LogitechModel::MOMO),
        (product_ids::WINGMAN_FORMULA_FORCE_GP, lg::LogitechModel::WingManFormulaForce),
        (product_ids::WINGMAN_FORMULA_FORCE, lg::LogitechModel::WingManFormulaForce),
        (product_ids::VIBRATION_WHEEL, lg::LogitechModel::VibrationWheel),
        (product_ids::DRIVING_FORCE_EX, lg::LogitechModel::DrivingForceEX),
        (product_ids::DRIVING_FORCE_PRO, lg::LogitechModel::DrivingForcePro),
        (product_ids::DRIVING_FORCE_GT, lg::LogitechModel::DrivingForceGT),
        (product_ids::SPEED_FORCE_WIRELESS, lg::LogitechModel::SpeedForceWireless),
        (product_ids::G25, lg::LogitechModel::G25),
        (product_ids::G27, lg::LogitechModel::G27),
        (product_ids::G29_PS, lg::LogitechModel::G29),
        (product_ids::G920, lg::LogitechModel::G920),
        (product_ids::G923, lg::LogitechModel::G923),
        (product_ids::G923_PS, lg::LogitechModel::G923),
        (product_ids::G923_XBOX, lg::LogitechModel::G923),
        (product_ids::G923_XBOX_ALT, lg::LogitechModel::G923),
        (product_ids::G_PRO, lg::LogitechModel::GPro),
        (product_ids::G_PRO_XBOX, lg::LogitechModel::GPro),
    ];
    for &(pid, ref expected_model) in expected {
        let model = lg::LogitechModel::from_product_id(pid);
        assert_eq!(
            &model, expected_model,
            "PID 0x{pid:04X} should map to {expected_model:?}, got {model:?}"
        );
    }
    Ok(())
}

/// Verify torque specs for every model match hardware datasheets.
#[test]
fn device_id_model_torque_specs() -> Result<(), Box<dyn std::error::Error>> {
    let specs: &[(lg::LogitechModel, f32)] = &[
        (lg::LogitechModel::WingManFormulaForce, 0.5),
        (lg::LogitechModel::MOMO, 2.0),
        (lg::LogitechModel::DrivingForceEX, 2.0),
        (lg::LogitechModel::DrivingForcePro, 2.0),
        (lg::LogitechModel::DrivingForceGT, 2.0),
        (lg::LogitechModel::SpeedForceWireless, 2.0),
        (lg::LogitechModel::VibrationWheel, 0.5),
        (lg::LogitechModel::G25, 2.5),
        (lg::LogitechModel::G27, 2.5),
        (lg::LogitechModel::G29, 2.2),
        (lg::LogitechModel::G920, 2.2),
        (lg::LogitechModel::G923, 2.2),
        (lg::LogitechModel::GPro, 11.0),
        (lg::LogitechModel::Unknown, 2.0),
    ];
    for &(model, expected_nm) in specs {
        assert!(
            (model.max_torque_nm() - expected_nm).abs() < 0.01,
            "{model:?}: expected {expected_nm} Nm, got {}",
            model.max_torque_nm()
        );
    }
    Ok(())
}

/// Verify rotation specs for every model.
#[test]
fn device_id_model_rotation_specs() -> Result<(), Box<dyn std::error::Error>> {
    let specs: &[(lg::LogitechModel, u16)] = &[
        (lg::LogitechModel::WingManFormulaForce, 180),
        (lg::LogitechModel::MOMO, 270),
        (lg::LogitechModel::DrivingForceEX, 270),
        (lg::LogitechModel::SpeedForceWireless, 270),
        (lg::LogitechModel::VibrationWheel, 270),
        (lg::LogitechModel::DrivingForcePro, 900),
        (lg::LogitechModel::DrivingForceGT, 900),
        (lg::LogitechModel::G25, 900),
        (lg::LogitechModel::G27, 900),
        (lg::LogitechModel::G29, 900),
        (lg::LogitechModel::G920, 900),
        (lg::LogitechModel::G923, 900),
        (lg::LogitechModel::GPro, 1080),
        (lg::LogitechModel::Unknown, 900),
    ];
    for &(model, expected_deg) in specs {
        assert_eq!(
            model.max_rotation_deg(),
            expected_deg,
            "{model:?}: expected {expected_deg}°, got {}°",
            model.max_rotation_deg()
        );
    }
    Ok(())
}

/// G923 has four PIDs; all must be recognised as wheels.
#[test]
fn device_id_g923_all_four_pids_are_wheels() -> Result<(), Box<dyn std::error::Error>> {
    let g923_pids = [
        product_ids::G923,
        product_ids::G923_PS,
        product_ids::G923_XBOX,
        product_ids::G923_XBOX_ALT,
    ];
    for pid in g923_pids {
        assert!(
            lg::is_wheel_product(pid),
            "G923 PID 0x{pid:04X} must be a recognised wheel"
        );
    }
    Ok(())
}

/// G PRO has PS and Xbox PIDs; both must map to GPro model.
#[test]
fn device_id_gpro_both_variants() -> Result<(), Box<dyn std::error::Error>> {
    let ps = lg::LogitechModel::from_product_id(product_ids::G_PRO);
    let xbox = lg::LogitechModel::from_product_id(product_ids::G_PRO_XBOX);
    assert_eq!(ps, lg::LogitechModel::GPro);
    assert_eq!(xbox, lg::LogitechModel::GPro);
    assert!(!ps.supports_trueforce(), "G PRO does not support TrueForce");
    assert_eq!(ps.max_rotation_deg(), 1080, "G PRO = 1080°");
    Ok(())
}

/// MOMO and MOMO_2 must both map to the same model.
#[test]
fn device_id_momo_dual_pid() -> Result<(), Box<dyn std::error::Error>> {
    let m1 = lg::LogitechModel::from_product_id(product_ids::MOMO);
    let m2 = lg::LogitechModel::from_product_id(product_ids::MOMO_2);
    assert_eq!(m1, m2, "both MOMO PIDs must classify identically");
    assert_eq!(m1, lg::LogitechModel::MOMO);
    Ok(())
}

/// WingMan FFG and FF must both map to WingManFormulaForce.
#[test]
fn device_id_wingman_dual_pid() -> Result<(), Box<dyn std::error::Error>> {
    let ffg = lg::LogitechModel::from_product_id(product_ids::WINGMAN_FORMULA_FORCE_GP);
    let ff = lg::LogitechModel::from_product_id(product_ids::WINGMAN_FORMULA_FORCE);
    assert_eq!(ffg, ff, "both WingMan PIDs must classify identically");
    assert_eq!(ffg, lg::LogitechModel::WingManFormulaForce);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Mode switching — all known mode IDs
// ═══════════════════════════════════════════════════════════════════════════════

/// All documented mode-switch IDs from the kernel `lg4ff_mode_switch_ext09_*`.
#[test]
fn mode_switch_all_known_mode_ids() -> Result<(), Box<dyn std::error::Error>> {
    let modes: &[(u8, &str, bool)] = &[
        (0x00, "DF-EX (compatibility)", true),
        (0x01, "DFP", true),
        (0x02, "G25", true),
        (0x03, "DFGT", true),
        (0x04, "G27", true),
        (0x05, "G29", true),
        (0x07, "G923 PS", true),
    ];
    for &(mode_id, name, detach) in modes {
        let r = lg::build_mode_switch_report(mode_id, detach);
        assert_eq!(r[0], report_ids::VENDOR, "{name}: byte 0 = 0xF8");
        assert_eq!(r[1], commands::MODE_SWITCH, "{name}: byte 1 = 0x09");
        assert_eq!(r[2], mode_id, "{name}: byte 2 = mode_id 0x{mode_id:02X}");
        assert_eq!(r[3], 0x01, "{name}: byte 3 = 0x01");
        let expected_detach = if detach { 0x01 } else { 0x00 };
        assert_eq!(r[4], expected_detach, "{name}: byte 4 = detach flag");
        assert_eq!(&r[5..], &[0x00, 0x00], "{name}: bytes 5-6 = zero padding");
    }
    Ok(())
}

/// Mode switch without detach flag.
#[test]
fn mode_switch_no_detach() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_mode_switch_report(0x05, false);
    assert_eq!(r[4], 0x00, "detach=false → byte 4 = 0x00");
    Ok(())
}

/// Native mode report (revert-on-reset) structure.
#[test]
fn mode_switch_native_mode_report_structure() -> Result<(), Box<dyn std::error::Error>> {
    let r = lg::build_native_mode_report();
    assert_eq!(r.len(), lg::VENDOR_REPORT_LEN, "must be 7 bytes");
    assert_eq!(r[0], report_ids::VENDOR, "byte 0 = 0xF8");
    assert_eq!(r[1], commands::NATIVE_MODE, "byte 1 = 0x0A");
    assert_eq!(&r[2..], &[0u8; 5], "bytes 2-6 all zero");
    Ok(())
}

/// Two-step mode switch sequence: native_mode then mode_switch.
#[test]
fn mode_switch_two_step_sequence_g29() -> Result<(), Box<dyn std::error::Error>> {
    let step1 = lg::build_native_mode_report();
    let step2 = lg::build_mode_switch_report(0x05, true);

    // Step 1: revert-on-reset
    assert_eq!(step1, [0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Step 2: switch to G29 native mode with detach
    assert_eq!(step2, [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]);

    Ok(())
}

/// Two-step mode switch sequence for G923 PS (kernel-verified).
#[test]
fn mode_switch_two_step_sequence_g923_ps() -> Result<(), Box<dyn std::error::Error>> {
    let step1 = lg::build_native_mode_report();
    let step2 = lg::build_mode_switch_report(0x07, true);

    assert_eq!(step1[1], 0x0A, "step 1 cmd = NATIVE_MODE");
    assert_eq!(step2, [0xF8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00],
        "G923 PS mode switch must match new-lg4ff lg4ff_mode_switch_ext09_g923");

    Ok(())
}

/// Two-step sequence for G27 (kernel-verified).
#[test]
fn mode_switch_two_step_sequence_g27() -> Result<(), Box<dyn std::error::Error>> {
    let step1 = lg::build_native_mode_report();
    let step2 = lg::build_mode_switch_report(0x04, true);

    assert_eq!(step1, [0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(step2, [0xF8, 0x09, 0x04, 0x01, 0x01, 0x00, 0x00]);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. FFB encoding precision — per-device model
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify encoding precision: 1 LSB = max_torque / 10000.
#[test]
fn ffb_encoding_lsb_precision() -> Result<(), Box<dyn std::error::Error>> {
    let models = [
        (lg::LogitechModel::G29, 2.2f32),
        (lg::LogitechModel::G920, 2.2),
        (lg::LogitechModel::G923, 2.2),
        (lg::LogitechModel::GPro, 11.0),
        (lg::LogitechModel::G25, 2.5),
        (lg::LogitechModel::G27, 2.5),
        (lg::LogitechModel::WingManFormulaForce, 0.5),
    ];
    for (model, max_torque) in models {
        let lsb_nm = max_torque / 10000.0;
        let enc = lg::LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

        // Encode exactly 1 LSB worth of torque
        enc.encode(lsb_nm, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        assert!(
            mag == 0 || mag == 1,
            "{model:?}: 1 LSB torque ({lsb_nm} Nm) should encode to 0 or 1, got {mag}"
        );
    }
    Ok(())
}

/// Verify that FFB magnitude range is exactly [-10000, +10000].
#[test]
fn ffb_encoding_magnitude_range() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

    enc.encode(2.2, &mut out);
    assert_eq!(i16::from_le_bytes([out[2], out[3]]), 10000, "+max = +10000");

    enc.encode(-2.2, &mut out);
    assert_eq!(i16::from_le_bytes([out[2], out[3]]), -10000, "-max = -10000");

    enc.encode(0.0, &mut out);
    assert_eq!(i16::from_le_bytes([out[2], out[3]]), 0, "zero = 0");

    Ok(())
}

/// DFP range encoding: verify boundary between short (≤200°) and long (>200°) modes.
#[test]
fn ffb_dfp_range_short_long_boundary() -> Result<(), Box<dyn std::error::Error>> {
    // 200° = short mode boundary
    let [coarse_200, _] = lg::build_set_range_dfp_reports(200);
    assert_eq!(coarse_200[1], 0x02, "200° uses short mode (0x02)");

    // 201° = long mode
    let [coarse_201, _] = lg::build_set_range_dfp_reports(201);
    assert_eq!(coarse_201[1], 0x03, "201° uses long mode (0x03)");

    // 199° = short mode
    let [coarse_199, _] = lg::build_set_range_dfp_reports(199);
    assert_eq!(coarse_199[1], 0x02, "199° uses short mode (0x02)");

    Ok(())
}

/// DFP range: clamping at boundaries (0 → 40, 1500 → 900).
#[test]
fn ffb_dfp_range_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let reports_0 = lg::build_set_range_dfp_reports(0);
    let reports_40 = lg::build_set_range_dfp_reports(40);
    assert_eq!(reports_0, reports_40, "0° should clamp to 40°");

    let reports_1500 = lg::build_set_range_dfp_reports(1500);
    let reports_900 = lg::build_set_range_dfp_reports(900);
    assert_eq!(reports_1500, reports_900, "1500° should clamp to 900°");

    Ok(())
}

/// Set-range report for various wheel rotation values.
#[test]
fn ffb_set_range_various_values() -> Result<(), Box<dyn std::error::Error>> {
    let test_values: &[(u16, u8, u8)] = &[
        (40, 0x28, 0x00),   // 40 = 0x0028
        (270, 0x0E, 0x01),  // 270 = 0x010E
        (540, 0x1C, 0x02),  // 540 = 0x021C
        (900, 0x84, 0x03),  // 900 = 0x0384
        (1080, 0x38, 0x04), // 1080 = 0x0438
    ];
    for &(degrees, expected_lsb, expected_msb) in test_values {
        let r = lg::build_set_range_report(degrees);
        assert_eq!(r[2], expected_lsb, "{degrees}° LSB should be 0x{expected_lsb:02X}");
        assert_eq!(r[3], expected_msb, "{degrees}° MSB should be 0x{expected_msb:02X}");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Edge cases
// ═══════════════════════════════════════════════════════════════════════════════

/// Empty input data must return None.
#[test]
fn edge_empty_report() -> Result<(), Box<dyn std::error::Error>> {
    assert!(lg::parse_input_report(&[]).is_none(), "empty data → None");
    Ok(())
}

/// Single-byte input with correct report ID but insufficient length.
#[test]
fn edge_single_byte_correct_id() -> Result<(), Box<dyn std::error::Error>> {
    assert!(lg::parse_input_report(&[0x01]).is_none(), "1 byte → None");
    Ok(())
}

/// Report ID 0x00 must be rejected.
#[test]
fn edge_report_id_zero() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0x00u8; 12];
    assert!(lg::parse_input_report(&data).is_none(), "report ID 0x00 → None");
    Ok(())
}

/// All-0xFF report with wrong report ID must fail.
#[test]
fn edge_all_ff_report() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xFFu8; 12];
    assert!(lg::parse_input_report(&data).is_none(), "report ID 0xFF → None");
    Ok(())
}

/// Very long report (64 bytes) with correct ID should parse successfully.
#[test]
fn edge_oversized_report() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80; // center steering
    let state = lg::parse_input_report(&data).ok_or("64-byte report should parse")?;
    assert!(state.steering.abs() < 0.001, "center steering");
    Ok(())
}

/// Exactly 9 bytes (one too few) must be rejected.
#[test]
fn edge_nine_bytes_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 9];
    data[0] = 0x01;
    assert!(lg::parse_input_report(&data).is_none(), "9 bytes → None");
    Ok(())
}

/// Exactly 10 bytes (minimum valid) should parse with paddles from byte 9.
#[test]
fn edge_ten_bytes_minimum_valid() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 10];
    data[0] = 0x01;
    data[1] = 0x00;
    data[2] = 0x80;
    data[9] = 0x02; // left/downshift paddle
    let state = lg::parse_input_report(&data).ok_or("10-byte report should parse")?;
    assert_eq!(state.paddles, 0x02, "paddle from byte 9");
    Ok(())
}

/// Exactly 11 bytes (between 10 and 12) should parse.
#[test]
fn edge_eleven_bytes_valid() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 11];
    data[0] = 0x01;
    data[1] = 0xFF;
    data[2] = 0x7F; // 0x7FFF = just below center
    let state = lg::parse_input_report(&data).ok_or("11-byte report should parse")?;
    assert!(state.steering < 0.0, "0x7FFF should be slightly negative");
    Ok(())
}

/// Unknown PID must map to Unknown model with sensible defaults.
#[test]
fn edge_unknown_pid_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let model = lg::LogitechModel::from_product_id(0x0000);
    assert_eq!(model, lg::LogitechModel::Unknown);
    assert!(!lg::is_wheel_product(0x0000));
    assert!((model.max_torque_nm() - 2.0).abs() < 0.01, "Unknown defaults to 2.0 Nm");
    assert_eq!(model.max_rotation_deg(), 900, "Unknown defaults to 900°");
    assert!(!model.supports_trueforce());
    assert!(!model.supports_hardware_friction());
    assert!(!model.supports_range_command());
    Ok(())
}

/// Legacy wheels: DrivingForceEX compatibility mode (G25/G27/DFGT/G29 emulating DF-EX).
#[test]
fn edge_legacy_dfex_compatibility_mode() -> Result<(), Box<dyn std::error::Error>> {
    let model = lg::LogitechModel::from_product_id(product_ids::DRIVING_FORCE_EX);
    assert_eq!(model, lg::LogitechModel::DrivingForceEX);
    assert!(!model.supports_range_command(), "DF-EX has no range command");
    assert!(!model.supports_hardware_friction(), "DF-EX has no hardware friction");
    assert_eq!(model.max_rotation_deg(), 270, "DF-EX limited to 270°");
    Ok(())
}

/// Vibration Wheel: basic rumble-only, no FFB capabilities.
#[test]
fn edge_vibration_wheel_limited_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    let model = lg::LogitechModel::from_product_id(product_ids::VIBRATION_WHEEL);
    assert_eq!(model, lg::LogitechModel::VibrationWheel);
    assert!(!model.supports_range_command());
    assert!(!model.supports_hardware_friction());
    assert!(!model.supports_trueforce());
    assert_eq!(model.max_rotation_deg(), 270);
    Ok(())
}

/// HID++ models (G920, G923 Xbox) use a different protocol but should still
/// be identified correctly.
#[test]
fn edge_hidpp_models_identified() -> Result<(), Box<dyn std::error::Error>> {
    let g920 = lg::LogitechModel::from_product_id(product_ids::G920);
    assert_eq!(g920, lg::LogitechModel::G920);
    assert!(g920.supports_range_command());

    let g923_xbox = lg::LogitechModel::from_product_id(product_ids::G923_XBOX);
    assert_eq!(g923_xbox, lg::LogitechModel::G923);
    assert!(g923_xbox.supports_trueforce());

    Ok(())
}

/// Hardware friction support matrix: only DFP, G25, DFGT, G27 have it.
#[test]
fn edge_friction_support_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let supported = [
        lg::LogitechModel::DrivingForcePro,
        lg::LogitechModel::G25,
        lg::LogitechModel::DrivingForceGT,
        lg::LogitechModel::G27,
    ];
    let unsupported = [
        lg::LogitechModel::WingManFormulaForce,
        lg::LogitechModel::MOMO,
        lg::LogitechModel::DrivingForceEX,
        lg::LogitechModel::SpeedForceWireless,
        lg::LogitechModel::VibrationWheel,
        lg::LogitechModel::G29,
        lg::LogitechModel::G920,
        lg::LogitechModel::G923,
        lg::LogitechModel::GPro,
        lg::LogitechModel::Unknown,
    ];
    for model in supported {
        assert!(
            model.supports_hardware_friction(),
            "{model:?} must support hardware friction"
        );
    }
    for model in unsupported {
        assert!(
            !model.supports_hardware_friction(),
            "{model:?} must NOT support hardware friction"
        );
    }
    Ok(())
}

/// Range command support matrix.
#[test]
fn edge_range_command_support_matrix() -> Result<(), Box<dyn std::error::Error>> {
    let supported = [
        lg::LogitechModel::DrivingForcePro,
        lg::LogitechModel::G25,
        lg::LogitechModel::DrivingForceGT,
        lg::LogitechModel::G27,
        lg::LogitechModel::G29,
        lg::LogitechModel::G920,
        lg::LogitechModel::G923,
        lg::LogitechModel::GPro,
    ];
    let unsupported = [
        lg::LogitechModel::WingManFormulaForce,
        lg::LogitechModel::MOMO,
        lg::LogitechModel::DrivingForceEX,
        lg::LogitechModel::SpeedForceWireless,
        lg::LogitechModel::VibrationWheel,
        lg::LogitechModel::Unknown,
    ];
    for model in supported {
        assert!(
            model.supports_range_command(),
            "{model:?} must support range command"
        );
    }
    for model in unsupported {
        assert!(
            !model.supports_range_command(),
            "{model:?} must NOT support range command"
        );
    }
    Ok(())
}

/// Constant force encoder: NaN and infinity should be clamped, not crash.
#[test]
fn edge_constant_force_nan_inf() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(2.2);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

    enc.encode(f32::NAN, &mut out);
    let mag_nan = i16::from_le_bytes([out[2], out[3]]);
    assert!(
        (-10_000..=10_000).contains(&mag_nan),
        "NaN torque must produce in-range magnitude, got {mag_nan}"
    );

    enc.encode(f32::INFINITY, &mut out);
    let mag_inf = i16::from_le_bytes([out[2], out[3]]);
    assert!(
        (-10_000..=10_000).contains(&mag_inf),
        "infinity torque must produce in-range magnitude, got {mag_inf}"
    );

    enc.encode(f32::NEG_INFINITY, &mut out);
    let mag_ninf = i16::from_le_bytes([out[2], out[3]]);
    assert!(
        (-10_000..=10_000).contains(&mag_ninf),
        "-infinity torque must produce in-range magnitude, got {mag_ninf}"
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Property tests — axis resolution and encoding
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Steering 16-bit resolution: raw value round-trips through encoding.
    #[test]
    fn prop_steering_16bit_roundtrip(raw: u16) {
        let expected = (raw as f32 - 32768.0) / 32768.0;
        let data = build_input(raw, 0, 0, 0, 0, 0x08, 0);
        let state = lg::parse_input_report(&data);
        prop_assert!(state.is_some(), "valid report must parse");
        if let Some(s) = state {
            prop_assert!(
                (s.steering - expected).abs() < 1e-5,
                "raw 0x{:04X}: expected {}, got {}",
                raw,
                expected,
                s.steering
            );
        }
    }

    /// Pedal 8-bit resolution: raw byte round-trips through encoding.
    #[test]
    fn prop_pedal_8bit_roundtrip(throttle: u8, brake: u8, clutch: u8) {
        let data = build_input(0x8000, throttle, brake, clutch, 0, 0x08, 0);
        let state = lg::parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            let expected_t = throttle as f32 / 255.0;
            let expected_b = brake as f32 / 255.0;
            let expected_c = clutch as f32 / 255.0;
            prop_assert!((s.throttle - expected_t).abs() < 1e-5);
            prop_assert!((s.brake - expected_b).abs() < 1e-5);
            prop_assert!((s.clutch - expected_c).abs() < 1e-5);
        }
    }

    /// Buttons 16-bit: exact round-trip for all button combinations.
    #[test]
    fn prop_buttons_16bit_roundtrip(buttons: u16) {
        let data = build_input(0x8000, 0, 0, 0, buttons, 0x08, 0);
        let state = lg::parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.buttons, buttons, "buttons must round-trip exactly");
        }
    }

    /// Hat switch lower nibble masking: upper nibble bits are stripped.
    #[test]
    fn prop_hat_nibble_masking(hat_raw: u8) {
        let data = build_input(0x8000, 0, 0, 0, 0, hat_raw, 0);
        let state = lg::parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.hat, hat_raw & 0x0F, "hat must be masked to lower nibble");
        }
    }

    /// Paddles 2-bit field: upper 6 bits are stripped.
    #[test]
    fn prop_paddles_two_bit_masking(paddle_raw: u8) {
        let data = build_input(0x8000, 0, 0, 0, 0, 0x08, paddle_raw);
        let state = lg::parse_input_report(&data);
        prop_assert!(state.is_some());
        if let Some(s) = state {
            prop_assert_eq!(s.paddles, paddle_raw & 0x03, "paddles must be masked to 2 bits");
        }
    }

    /// FFB encoder: torque encoding for each model must stay within ±10000.
    #[test]
    fn prop_ffb_per_model_bounds(
        pid_idx in 0usize..19usize,
        frac in -2.0f32..2.0f32,
    ) {
        let pids = [
            product_ids::MOMO, product_ids::MOMO_2,
            product_ids::WINGMAN_FORMULA_FORCE_GP, product_ids::WINGMAN_FORMULA_FORCE,
            product_ids::VIBRATION_WHEEL, product_ids::DRIVING_FORCE_EX,
            product_ids::DRIVING_FORCE_PRO, product_ids::DRIVING_FORCE_GT,
            product_ids::SPEED_FORCE_WIRELESS, product_ids::G25,
            product_ids::G27, product_ids::G29_PS,
            product_ids::G920, product_ids::G923,
            product_ids::G923_XBOX, product_ids::G923_XBOX_ALT,
            product_ids::G923_PS, product_ids::G_PRO,
            product_ids::G_PRO_XBOX,
        ];
        let pid = pids[pid_idx];
        let model = lg::LogitechModel::from_product_id(pid);
        let max_torque = model.max_torque_nm();
        let torque = frac * max_torque;

        let enc = lg::LogitechConstantForceEncoder::new(max_torque);
        let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        let mag = i16::from_le_bytes([out[2], out[3]]);
        prop_assert!(
            (-10_000..=10_000).contains(&mag),
            "PID 0x{:04X} ({:?}): torque {torque} Nm → mag {mag} out of range",
            pid,
            model
        );
    }

    /// DFP range: coarse command byte is deterministic based on 200° threshold.
    #[test]
    fn prop_dfp_coarse_command_threshold(degrees in 0u16..=2000u16) {
        let [coarse, _] = lg::build_set_range_dfp_reports(degrees);
        let clamped = degrees.clamp(40, 900);
        if clamped > 200 {
            prop_assert_eq!(coarse[1], 0x03, "clamped {}° > 200 → 0x03", clamped);
        } else {
            prop_assert_eq!(coarse[1], 0x02, "clamped {}° ≤ 200 → 0x02", clamped);
        }
    }

    /// Set-range report: degrees round-trip via LE bytes.
    #[test]
    fn prop_set_range_roundtrip(degrees: u16) {
        let r = lg::build_set_range_report(degrees);
        let recovered = u16::from_le_bytes([r[2], r[3]]);
        prop_assert_eq!(recovered, degrees, "degrees must round-trip");
    }

    /// Gain report: gain value preserved verbatim.
    #[test]
    fn prop_gain_roundtrip(gain: u8) {
        let r = lg::build_gain_report(gain);
        prop_assert_eq!(r[1], gain);
    }

    /// LED mask: high bits stripped to 5-bit field.
    #[test]
    fn prop_led_mask_5bit(mask: u8) {
        let r = lg::build_set_leds_report(mask);
        prop_assert_eq!(r[2], mask & 0x1F);
    }

    /// Autocenter: strength and rate preserved verbatim.
    #[test]
    fn prop_autocenter_roundtrip(strength: u8, rate: u8) {
        let r = lg::build_set_autocenter_report(strength, rate);
        prop_assert_eq!(r[2], strength);
        prop_assert_eq!(r[3], rate);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Known constant validation
// ═══════════════════════════════════════════════════════════════════════════════

/// Logitech USB vendor ID.
#[test]
fn constants_vendor_id() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(lg::LOGITECH_VENDOR_ID, 0x046D);
    Ok(())
}

/// Report IDs match the Logitech HID protocol specification.
#[test]
fn constants_report_ids() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(report_ids::STANDARD_INPUT, 0x01, "standard input report");
    assert_eq!(report_ids::VENDOR, 0xF8, "vendor feature/output report");
    assert_eq!(report_ids::CONSTANT_FORCE, 0x12, "constant force output");
    assert_eq!(report_ids::DEVICE_GAIN, 0x16, "device gain output");
    Ok(())
}

/// Command bytes for vendor reports.
#[test]
fn constants_commands() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(commands::NATIVE_MODE, 0x0A, "revert mode upon USB reset");
    assert_eq!(commands::SET_RANGE, 0x81, "set rotation range");
    assert_eq!(commands::SET_AUTOCENTER, 0x14, "set autocenter spring");
    assert_eq!(commands::SET_LEDS, 0x12, "set rev-light LEDs");
    assert_eq!(commands::MODE_SWITCH, 0x09, "extended mode switch");
    Ok(())
}

/// Report size constants.
#[test]
fn constants_report_sizes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(lg::CONSTANT_FORCE_REPORT_LEN, 4, "constant force = 4 bytes");
    assert_eq!(lg::VENDOR_REPORT_LEN, 7, "vendor report = 7 bytes");
    Ok(())
}

/// FFB magnitude range: ±10000.
#[test]
fn constants_ffb_magnitude_range() -> Result<(), Box<dyn std::error::Error>> {
    let enc = lg::LogitechConstantForceEncoder::new(1.0);
    let mut out = [0u8; lg::CONSTANT_FORCE_REPORT_LEN];

    enc.encode(1.0, &mut out);
    assert_eq!(i16::from_le_bytes([out[2], out[3]]), 10_000, "+max = 10000");

    enc.encode(-1.0, &mut out);
    assert_eq!(i16::from_le_bytes([out[2], out[3]]), -10_000, "-max = -10000");

    Ok(())
}

/// All PIDs verified against Linux kernel hid-ids.h, new-lg4ff, and oversteer.
#[test]
fn constants_all_pids_verified() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::MOMO, 0xC295);
    assert_eq!(product_ids::MOMO_2, 0xCA03);
    assert_eq!(product_ids::WINGMAN_FORMULA_FORCE_GP, 0xC293);
    assert_eq!(product_ids::WINGMAN_FORMULA_FORCE, 0xC291);
    assert_eq!(product_ids::VIBRATION_WHEEL, 0xCA04);
    assert_eq!(product_ids::DRIVING_FORCE_EX, 0xC294);
    assert_eq!(product_ids::DRIVING_FORCE_PRO, 0xC298);
    assert_eq!(product_ids::DRIVING_FORCE_GT, 0xC29A);
    assert_eq!(product_ids::SPEED_FORCE_WIRELESS, 0xC29C);
    assert_eq!(product_ids::G25, 0xC299);
    assert_eq!(product_ids::G27, 0xC29B);
    assert_eq!(product_ids::G29_PS, 0xC24F);
    assert_eq!(product_ids::G920, 0xC262);
    assert_eq!(product_ids::G923, 0xC266);
    assert_eq!(product_ids::G923_PS, 0xC267);
    assert_eq!(product_ids::G923_XBOX, 0xC26E);
    assert_eq!(product_ids::G923_XBOX_ALT, 0xC26D);
    assert_eq!(product_ids::G_PRO, 0xC268);
    assert_eq!(product_ids::G_PRO_XBOX, 0xC272);
    Ok(())
}

/// All 19 known PIDs are recognised as wheel products and map to non-Unknown models.
#[test]
fn constants_all_pids_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let all_pids = [
        product_ids::MOMO,
        product_ids::MOMO_2,
        product_ids::WINGMAN_FORMULA_FORCE_GP,
        product_ids::WINGMAN_FORMULA_FORCE,
        product_ids::VIBRATION_WHEEL,
        product_ids::DRIVING_FORCE_EX,
        product_ids::DRIVING_FORCE_PRO,
        product_ids::DRIVING_FORCE_GT,
        product_ids::SPEED_FORCE_WIRELESS,
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
            lg::is_wheel_product(pid),
            "PID 0x{pid:04X} must be a wheel"
        );
        assert_ne!(
            lg::LogitechModel::from_product_id(pid),
            lg::LogitechModel::Unknown,
            "PID 0x{pid:04X} must not be Unknown"
        );
    }
    Ok(())
}
