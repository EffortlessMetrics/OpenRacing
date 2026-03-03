//! Deep protocol tests for VRS DirectForce Pro HID protocol crate.

use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VRS_PRODUCT_ID, VRS_VENDOR_ID, VrsConstantForceEncoder, VrsDamperEncoder, VrsFfbEffectType,
    VrsFrictionEncoder, VrsPedalAxesRaw, VrsSpringEncoder, build_device_gain, build_ffb_enable,
    build_rotation_range, identify_device, is_wheelbase_product, parse_input_report, product_ids,
};

// ── Device identification ────────────────────────────────────────────────────

#[test]
fn vendor_id_is_stm_shared_vid() {
    assert_eq!(VRS_VENDOR_ID, 0x0483);
}

#[test]
fn primary_product_id_matches_kernel_constant() {
    assert_eq!(VRS_PRODUCT_ID, 0xA355);
    assert_eq!(product_ids::DIRECTFORCE_PRO, VRS_PRODUCT_ID);
}

#[test]
fn all_product_ids_are_nonzero_and_unique() {
    let pids = [
        product_ids::DIRECTFORCE_PRO,
        product_ids::DIRECTFORCE_PRO_V2,
        product_ids::R295,
        product_ids::PEDALS,
        product_ids::PEDALS_V1,
        product_ids::PEDALS_V2,
        product_ids::HANDBRAKE,
        product_ids::SHIFTER,
    ];
    for pid in pids {
        assert_ne!(pid, 0, "PID must not be zero");
    }
    // Check uniqueness among key PIDs (excluding backward-compat aliases)
    let key_pids = [
        product_ids::DIRECTFORCE_PRO,
        product_ids::DIRECTFORCE_PRO_V2,
        product_ids::R295,
        product_ids::PEDALS,
        product_ids::PEDALS_V2,
        product_ids::HANDBRAKE,
        product_ids::SHIFTER,
    ];
    for (i, &a) in key_pids.iter().enumerate() {
        for (j, &b) in key_pids.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "PIDs at index {i} and {j} must be unique");
            }
        }
    }
}

#[test]
fn identify_device_wheelbases_have_ffb() {
    let wb_pids = [
        product_ids::DIRECTFORCE_PRO,
        product_ids::DIRECTFORCE_PRO_V2,
        product_ids::R295,
    ];
    for pid in wb_pids {
        let id = identify_device(pid);
        assert!(id.supports_ffb, "PID 0x{pid:04X} must support FFB");
        assert!(
            id.max_torque_nm.is_some(),
            "PID 0x{pid:04X} must have torque"
        );
        assert!(is_wheelbase_product(pid));
    }
}

#[test]
fn identify_device_peripherals_lack_ffb() {
    let periph_pids = [
        product_ids::PEDALS,
        product_ids::PEDALS_V1,
        product_ids::PEDALS_V2,
        product_ids::HANDBRAKE,
        product_ids::SHIFTER,
    ];
    for pid in periph_pids {
        let id = identify_device(pid);
        assert!(!id.supports_ffb, "PID 0x{pid:04X} must not support FFB");
        assert!(!is_wheelbase_product(pid));
    }
}

#[test]
fn identify_unknown_device() {
    let id = identify_device(0xFFFF);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    assert!(!is_wheelbase_product(0xFFFF));
}

#[test]
fn dfp_torque_is_20nm() {
    let id = identify_device(product_ids::DIRECTFORCE_PRO);
    assert!((id.max_torque_nm.unwrap_or(0.0) - 20.0).abs() < 0.01);
}

// ── Torque command encoding ──────────────────────────────────────────────────

#[test]
fn constant_force_report_id_and_length() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(5.0, &mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], 0x11);
    assert_eq!(out[1], 1); // effect block index
    Ok(())
}

#[test]
fn constant_force_half_torque_encodes_5000() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(10.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000);
    Ok(())
}

#[test]
fn constant_force_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-20.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -10000);
    Ok(())
}

#[test]
fn constant_force_saturates_above_max() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(999.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000);
    Ok(())
}

#[test]
fn constant_force_zero_report() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x11);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 0);
    Ok(())
}

// ── Spring / damper / friction encoding ──────────────────────────────────────

#[test]
fn spring_report_id_and_structure() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let len = enc.encode(5000, 100, 0, 200, &mut out);
    assert_eq!(len, SPRING_REPORT_LEN);
    assert_eq!(out[0], 0x19);
    assert_eq!(out[1], 1);
    let coeff = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(coeff, 5000);
    Ok(())
}

#[test]
fn damper_report_id_and_structure() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    let len = enc.encode(8000, 4000, &mut out);
    assert_eq!(len, DAMPER_REPORT_LEN);
    assert_eq!(out[0], 0x1A);
    let coeff = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(coeff, 8000);
    let vel = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(vel, 4000);
    Ok(())
}

#[test]
fn friction_report_id_and_structure() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    let len = enc.encode(6000, 3000, &mut out);
    assert_eq!(len, FRICTION_REPORT_LEN);
    assert_eq!(out[0], 0x1B);
    let coeff = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(coeff, 6000);
    Ok(())
}

// ── Input report parsing ─────────────────────────────────────────────────────

#[test]
fn parse_input_report_too_short_returns_none() {
    assert!(parse_input_report(&[0u8; 16]).is_none());
    assert!(parse_input_report(&[0u8; 0]).is_none());
}

#[test]
fn parse_input_report_center_position() -> Result<(), String> {
    let data = vec![0u8; 64];
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.steering.abs() < 0.001);
    assert!(state.throttle.abs() < 0.001);
    assert!(state.brake.abs() < 0.001);
    assert!(state.clutch.abs() < 0.001);
    assert_eq!(state.buttons, 0);
    Ok(())
}

#[test]
fn parse_input_report_full_left_steering() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0x00;
    data[1] = 0x80; // i16 = -32768
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering + 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_input_report_full_pedals() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    // throttle = 0xFFFF
    data[2] = 0xFF;
    data[3] = 0xFF;
    // brake = 0xFFFF
    data[4] = 0xFF;
    data[5] = 0xFF;
    // clutch = 0xFFFF
    data[6] = 0xFF;
    data[7] = 0xFF;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.throttle - 1.0).abs() < 0.001);
    assert!((state.brake - 1.0).abs() < 0.001);
    assert!((state.clutch - 1.0).abs() < 0.001);
    Ok(())
}

// ── Pedal data parsing ───────────────────────────────────────────────────────

#[test]
fn pedal_axes_normalize_zero() {
    let raw = VrsPedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: 0,
    };
    let norm = raw.normalize();
    assert!(norm.throttle.abs() < 0.001);
    assert!(norm.brake.abs() < 0.001);
    assert!(norm.clutch.abs() < 0.001);
}

#[test]
fn pedal_axes_normalize_max() {
    let raw = VrsPedalAxesRaw {
        throttle: u16::MAX,
        brake: u16::MAX,
        clutch: u16::MAX,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 1.0).abs() < 0.001);
    assert!((norm.brake - 1.0).abs() < 0.001);
    assert!((norm.clutch - 1.0).abs() < 0.001);
}

#[test]
fn pedal_axes_normalize_midpoint() {
    let raw = VrsPedalAxesRaw {
        throttle: 32768,
        brake: 32768,
        clutch: 32768,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 0.5).abs() < 0.01);
    assert!((norm.brake - 0.5).abs() < 0.01);
    assert!((norm.clutch - 0.5).abs() < 0.01);
}

// ── FFB effect type report IDs ───────────────────────────────────────────────

#[test]
fn ffb_effect_type_report_ids_match_pidff_standard() {
    assert_eq!(VrsFfbEffectType::Constant.report_id(), 0x11);
    assert_eq!(VrsFfbEffectType::Ramp.report_id(), 0x13);
    assert_eq!(VrsFfbEffectType::Square.report_id(), 0x14);
    assert_eq!(VrsFfbEffectType::Sine.report_id(), 0x15);
    assert_eq!(VrsFfbEffectType::Triangle.report_id(), 0x16);
    assert_eq!(VrsFfbEffectType::SawtoothUp.report_id(), 0x17);
    assert_eq!(VrsFfbEffectType::SawtoothDown.report_id(), 0x18);
    assert_eq!(VrsFfbEffectType::Spring.report_id(), 0x19);
    assert_eq!(VrsFfbEffectType::Damper.report_id(), 0x1A);
    assert_eq!(VrsFfbEffectType::Friction.report_id(), 0x1B);
    assert_eq!(VrsFfbEffectType::Custom.report_id(), 0x1C);
}

// ── Control report builders ──────────────────────────────────────────────────

#[test]
fn build_rotation_range_encodes_degrees() {
    let r = build_rotation_range(900);
    assert_eq!(r[0], 0x0C); // SET_REPORT id
    let deg = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(deg, 900);
}

#[test]
fn build_device_gain_encodes_value() {
    let r = build_device_gain(0xFF);
    assert_eq!(r[0], 0x0C);
    assert_eq!(r[1], 0x01);
    assert_eq!(r[2], 0xFF);
}

#[test]
fn build_ffb_enable_toggle() {
    let on = build_ffb_enable(true);
    assert_eq!(on[0], 0x0B); // DEVICE_CONTROL id
    assert_eq!(on[1], 0x01);

    let off = build_ffb_enable(false);
    assert_eq!(off[0], 0x0B);
    assert_eq!(off[1], 0x00);
}
