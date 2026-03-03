//! Deep tests for VRS DirectForce Pro: torque roundtrip, effect commands,
//! firmware/device identity, settings, telemetry parsing, and proptest boundaries.

use racing_wheel_hid_vrs_protocol::{
    ids::report_ids, parse_input_report, product_ids, CONSTANT_FORCE_REPORT_LEN,
    DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN, VrsConstantForceEncoder,
    VrsDamperEncoder, VrsDeviceIdentity, VrsFfbEffectType, VrsFrictionEncoder, VrsPedalAxesRaw,
    VrsSpringEncoder, build_device_gain, build_ffb_enable, build_rotation_range, identify_device,
    is_wheelbase_product,
};

// ── Torque encoding/decoding roundtrip ───────────────────────────────────────

#[test]
fn torque_roundtrip_positive_half() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(10.0, &mut buf);
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    // 10 / 20 * 10000 = 5000
    assert_eq!(mag, 5000);
    // Decode back to normalised fraction
    let decoded = mag as f32 / 10_000.0;
    assert!((decoded - 0.5).abs() < 0.001);
    Ok(())
}

#[test]
fn torque_roundtrip_negative_quarter() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-5.0, &mut buf);
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, -2500);
    let decoded = mag as f32 / 10_000.0;
    assert!((decoded - (-0.25)).abs() < 0.001);
    Ok(())
}

#[test]
fn torque_roundtrip_full_scale_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(15.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(15.0, &mut buf);
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, 10_000);
    Ok(())
}

#[test]
fn torque_roundtrip_full_scale_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(15.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-15.0, &mut buf);
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, -10_000);
    Ok(())
}

#[test]
fn torque_roundtrip_zero() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.0, &mut buf);
    let mag = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag, 0);
    Ok(())
}

#[test]
fn torque_encode_zero_helper_matches_encode_zero_nm() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut buf_a = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let mut buf_b = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.0, &mut buf_a);
    enc.encode_zero(&mut buf_b);
    assert_eq!(buf_a, buf_b);
    Ok(())
}

#[test]
fn torque_saturation_clamps_beyond_max() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(10.0);
    let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(500.0, &mut buf);
    let mag_pos = i16::from_le_bytes([buf[3], buf[4]]);
    enc.encode(-500.0, &mut buf);
    let mag_neg = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(mag_pos, 10_000);
    assert_eq!(mag_neg, -10_000);
    Ok(())
}

// ── Effect commands: spring ──────────────────────────────────────────────────

#[test]
fn spring_encodes_coefficient_le() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut buf = [0u8; SPRING_REPORT_LEN];
    enc.encode(7500, 0, 0, 0, &mut buf);
    let coeff = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(coeff, 7500);
    Ok(())
}

#[test]
fn spring_encodes_steering_position_signed() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut buf = [0u8; SPRING_REPORT_LEN];
    enc.encode(5000, -12345, 0, 0, &mut buf);
    let pos = i16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(pos, -12345);
    Ok(())
}

#[test]
fn spring_encodes_center_offset_signed() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut buf = [0u8; SPRING_REPORT_LEN];
    enc.encode(1000, 0, -500, 0, &mut buf);
    let center = i16::from_le_bytes([buf[6], buf[7]]);
    assert_eq!(center, -500);
    Ok(())
}

#[test]
fn spring_encodes_deadzone() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut buf = [0u8; SPRING_REPORT_LEN];
    enc.encode(0, 0, 0, 3000, &mut buf);
    let dz = u16::from_le_bytes([buf[8], buf[9]]);
    assert_eq!(dz, 3000);
    Ok(())
}

#[test]
fn spring_zero_clears_all_params() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut buf = [0xFFu8; SPRING_REPORT_LEN];
    enc.encode_zero(&mut buf);
    assert_eq!(buf[0], report_ids::SPRING_EFFECT);
    // coefficient, steering, center, deadzone should all be zero
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0);
    assert_eq!(i16::from_le_bytes([buf[4], buf[5]]), 0);
    assert_eq!(i16::from_le_bytes([buf[6], buf[7]]), 0);
    assert_eq!(u16::from_le_bytes([buf[8], buf[9]]), 0);
    Ok(())
}

// ── Effect commands: damper ──────────────────────────────────────────────────

#[test]
fn damper_encodes_coefficient_and_velocity() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsDamperEncoder::new(20.0);
    let mut buf = [0u8; DAMPER_REPORT_LEN];
    enc.encode(9000, 6000, &mut buf);
    assert_eq!(buf[0], report_ids::DAMPER_EFFECT);
    let coeff = u16::from_le_bytes([buf[2], buf[3]]);
    let vel = u16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(coeff, 9000);
    assert_eq!(vel, 6000);
    Ok(())
}

#[test]
fn damper_zero_clears_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsDamperEncoder::new(20.0);
    let mut buf = [0xFFu8; DAMPER_REPORT_LEN];
    enc.encode_zero(&mut buf);
    assert_eq!(buf[0], report_ids::DAMPER_EFFECT);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0);
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 0);
    Ok(())
}

// ── Effect commands: friction ────────────────────────────────────────────────

#[test]
fn friction_encodes_coefficient_and_velocity() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut buf = [0u8; FRICTION_REPORT_LEN];
    enc.encode(4500, 2200, &mut buf);
    assert_eq!(buf[0], report_ids::FRICTION_EFFECT);
    let coeff = u16::from_le_bytes([buf[2], buf[3]]);
    let vel = u16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(coeff, 4500);
    assert_eq!(vel, 2200);
    Ok(())
}

#[test]
fn friction_zero_clears_coefficients() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut buf = [0xFFu8; FRICTION_REPORT_LEN];
    enc.encode_zero(&mut buf);
    assert_eq!(buf[0], report_ids::FRICTION_EFFECT);
    assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0);
    assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 0);
    Ok(())
}

// ── Firmware / device identity ───────────────────────────────────────────────

#[test]
fn dfp_identity_has_correct_torque_and_name() {
    let id: VrsDeviceIdentity = identify_device(product_ids::DIRECTFORCE_PRO);
    assert_eq!(id.product_id, 0xA355);
    assert!(id.name.contains("DirectForce Pro"));
    assert!(id.supports_ffb);
    let torque = id.max_torque_nm.unwrap_or(0.0);
    assert!((torque - 20.0).abs() < 0.01);
}

#[test]
fn r295_identity_is_wheelbase_with_ffb() {
    let id = identify_device(product_ids::R295);
    assert!(id.supports_ffb);
    assert!(is_wheelbase_product(product_ids::R295));
    assert!(id.max_torque_nm.is_some());
}

#[test]
fn pedals_identity_has_no_ffb() {
    let id = identify_device(product_ids::PEDALS);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    assert!(!is_wheelbase_product(product_ids::PEDALS));
}

#[test]
fn report_ids_follow_pidff_standard_ordering() {
    // PIDFF standard: constant < ramp < periodic < condition
    const _: () = assert!(report_ids::CONSTANT_FORCE < report_ids::RAMP_FORCE);
    const _: () = assert!(report_ids::RAMP_FORCE < report_ids::SQUARE_EFFECT);
    const _: () = assert!(report_ids::SQUARE_EFFECT < report_ids::SINE_EFFECT);
    const _: () = assert!(report_ids::SPRING_EFFECT < report_ids::DAMPER_EFFECT);
    const _: () = assert!(report_ids::DAMPER_EFFECT < report_ids::FRICTION_EFFECT);
}

#[test]
fn all_ffb_effect_types_have_distinct_report_ids() {
    let effects = [
        VrsFfbEffectType::Constant,
        VrsFfbEffectType::Ramp,
        VrsFfbEffectType::Square,
        VrsFfbEffectType::Sine,
        VrsFfbEffectType::Triangle,
        VrsFfbEffectType::SawtoothUp,
        VrsFfbEffectType::SawtoothDown,
        VrsFfbEffectType::Spring,
        VrsFfbEffectType::Damper,
        VrsFfbEffectType::Friction,
        VrsFfbEffectType::Custom,
    ];
    for (i, a) in effects.iter().enumerate() {
        for (j, b) in effects.iter().enumerate() {
            if i != j {
                assert_ne!(
                    a.report_id(),
                    b.report_id(),
                    "Effect types at {i} and {j} must have distinct report IDs"
                );
            }
        }
    }
}

// ── Settings commands: rotation range, force limit ───────────────────────────

#[test]
fn rotation_range_roundtrip_common_values() {
    for degrees in [180u16, 270, 360, 540, 720, 900, 1080, 1440, 2520] {
        let r = build_rotation_range(degrees);
        assert_eq!(r[0], report_ids::SET_REPORT);
        let decoded = u16::from_le_bytes([r[2], r[3]]);
        assert_eq!(decoded, degrees, "Rotation range roundtrip failed for {degrees}°");
    }
}

#[test]
fn rotation_range_boundary_zero() {
    let r = build_rotation_range(0);
    let decoded = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(decoded, 0);
}

#[test]
fn rotation_range_boundary_max_u16() {
    let r = build_rotation_range(u16::MAX);
    let decoded = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(decoded, u16::MAX);
}

#[test]
fn device_gain_boundary_values() {
    for gain in [0u8, 1, 127, 128, 254, 255] {
        let r = build_device_gain(gain);
        assert_eq!(r[0], report_ids::SET_REPORT);
        assert_eq!(r[1], 0x01); // gain sub-command
        assert_eq!(r[2], gain, "Gain roundtrip failed for {gain}");
    }
}

#[test]
fn ffb_enable_disable_report_structure() {
    let on = build_ffb_enable(true);
    assert_eq!(on[0], report_ids::DEVICE_CONTROL);
    assert_eq!(on[1], 0x01);
    assert_eq!(on.len(), 8);

    let off = build_ffb_enable(false);
    assert_eq!(off[0], report_ids::DEVICE_CONTROL);
    assert_eq!(off[1], 0x00);
    // Reserved bytes must be zero
    for &b in &off[2..] {
        assert_eq!(b, 0x00);
    }
}

// ── Telemetry report parsing ─────────────────────────────────────────────────

#[test]
fn telemetry_parse_full_right_steering() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    // i16 max = 32767 = 0x7FFF (LE: 0xFF, 0x7F)
    data[0] = 0xFF;
    data[1] = 0x7F;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn telemetry_parse_hat_switch_directions() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    // Up = 0x00
    data[12] = 0x00;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x00);

    // Neutral = 0x0F
    data[12] = 0x0F;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x0F);

    // High nibble should be masked off
    data[12] = 0xF3;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x03);
    Ok(())
}

#[test]
fn telemetry_parse_encoder_signed_values() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[13] = 0x80; // -128 as i8
    data[15] = 0x7F; // 127 as i8
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.encoder1, -128);
    assert_eq!(state.encoder2, 127);
    Ok(())
}

#[test]
fn telemetry_parse_multiple_buttons() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[8] = 0xFF; // buttons 0-7 all pressed
    data[9] = 0x00; // buttons 8-15 none pressed
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons & 0x00FF, 0xFF);
    assert_eq!(state.buttons & 0xFF00, 0x00);

    data[8] = 0x00;
    data[9] = 0xFF;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons & 0x00FF, 0x00);
    assert_eq!(state.buttons & 0xFF00, 0xFF00);
    Ok(())
}

#[test]
fn telemetry_parse_connected_flag_normal() -> Result<(), String> {
    let data = vec![0u8; 64];
    let state = parse_input_report(&data).ok_or("parse failed")?;
    // Bytes 0-1 are 0x0000, which is != 0xFFFF, so connected
    assert!(state.connected);
    Ok(())
}

#[test]
fn telemetry_parse_disconnected_flag() -> Result<(), String> {
    let mut data = vec![0u8; 64];
    data[0] = 0xFF;
    data[1] = 0xFF;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state.connected);
    Ok(())
}

#[test]
fn telemetry_parse_minimum_valid_length() -> Result<(), String> {
    let data = vec![0u8; 17]; // exactly minimum
    let state = parse_input_report(&data).ok_or("17 bytes should parse")?;
    assert!(state.steering.abs() < 0.001);
    Ok(())
}

#[test]
fn telemetry_parse_rejects_16_bytes() {
    assert!(parse_input_report(&[0u8; 16]).is_none());
}

#[test]
fn pedal_axes_raw_roundtrip_normalize() {
    let raw = VrsPedalAxesRaw {
        throttle: 32768,
        brake: 16384,
        clutch: 49152,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 0.5).abs() < 0.01);
    assert!((norm.brake - 0.25).abs() < 0.01);
    assert!((norm.clutch - 0.75).abs() < 0.01);
}

// ── Proptest boundary value testing ──────────────────────────────────────────

mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(300))]

        #[test]
        fn prop_torque_magnitude_bounded(torque in -100.0f32..100.0f32) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque, &mut buf);
            let mag = i16::from_le_bytes([buf[3], buf[4]]);
            prop_assert!(mag >= -10_000);
            prop_assert!(mag <= 10_000);
        }

        #[test]
        fn prop_torque_encode_decode_roundtrip(nm in -20.0f32..20.0f32) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(nm, &mut buf);
            let mag = i16::from_le_bytes([buf[3], buf[4]]);
            // Decode back to Nm
            let decoded_nm = (mag as f32 / 10_000.0) * 20.0;
            // Allow ±0.01 Nm tolerance from integer truncation
            prop_assert!((decoded_nm - nm).abs() < 0.01,
                "roundtrip error: encoded {nm} Nm, got back {decoded_nm} Nm");
        }

        #[test]
        fn prop_spring_parameters_roundtrip(
            coeff in 0u16..=10000u16,
            steering in -32768i16..=32767i16,
            center in -32768i16..=32767i16,
            deadzone in 0u16..=10000u16,
        ) {
            let enc = VrsSpringEncoder::new(20.0);
            let mut buf = [0u8; SPRING_REPORT_LEN];
            enc.encode(coeff, steering, center, deadzone, &mut buf);
            // Decode and verify roundtrip
            let dec_coeff = u16::from_le_bytes([buf[2], buf[3]]);
            let dec_steer = i16::from_le_bytes([buf[4], buf[5]]);
            let dec_center = i16::from_le_bytes([buf[6], buf[7]]);
            let dec_dz = u16::from_le_bytes([buf[8], buf[9]]);
            prop_assert_eq!(dec_coeff, coeff);
            prop_assert_eq!(dec_steer, steering);
            prop_assert_eq!(dec_center, center);
            prop_assert_eq!(dec_dz, deadzone);
        }

        #[test]
        fn prop_damper_parameters_roundtrip(
            coeff in 0u16..=10000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = VrsDamperEncoder::new(20.0);
            let mut buf = [0u8; DAMPER_REPORT_LEN];
            enc.encode(coeff, velocity, &mut buf);
            let dec_coeff = u16::from_le_bytes([buf[2], buf[3]]);
            let dec_vel = u16::from_le_bytes([buf[4], buf[5]]);
            prop_assert_eq!(dec_coeff, coeff);
            prop_assert_eq!(dec_vel, velocity);
        }

        #[test]
        fn prop_friction_parameters_roundtrip(
            coeff in 0u16..=10000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = VrsFrictionEncoder::new(20.0);
            let mut buf = [0u8; FRICTION_REPORT_LEN];
            enc.encode(coeff, velocity, &mut buf);
            let dec_coeff = u16::from_le_bytes([buf[2], buf[3]]);
            let dec_vel = u16::from_le_bytes([buf[4], buf[5]]);
            prop_assert_eq!(dec_coeff, coeff);
            prop_assert_eq!(dec_vel, velocity);
        }

        #[test]
        fn prop_rotation_range_roundtrip(degrees in 0u16..=65535u16) {
            let r = build_rotation_range(degrees);
            prop_assert_eq!(r[0], report_ids::SET_REPORT);
            let decoded = u16::from_le_bytes([r[2], r[3]]);
            prop_assert_eq!(decoded, degrees);
        }

        #[test]
        fn prop_device_gain_roundtrip(gain in 0u8..=255u8) {
            let r = build_device_gain(gain);
            prop_assert_eq!(r[0], report_ids::SET_REPORT);
            prop_assert_eq!(r[2], gain);
        }

        #[test]
        fn prop_input_report_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_input_report(&data);
        }

        #[test]
        fn prop_pedal_normalization_bounded(
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
        ) {
            let raw = VrsPedalAxesRaw { throttle, brake, clutch };
            let norm = raw.normalize();
            prop_assert!(norm.throttle >= 0.0 && norm.throttle <= 1.0);
            prop_assert!(norm.brake >= 0.0 && norm.brake <= 1.0);
            prop_assert!(norm.clutch >= 0.0 && norm.clutch <= 1.0);
        }

        #[test]
        fn prop_encoder_new_clamps_tiny_max_torque(max_torque in -100.0f32..0.005f32) {
            let enc = VrsConstantForceEncoder::new(max_torque);
            let mut buf = [0u8; CONSTANT_FORCE_REPORT_LEN];
            // Should not panic or divide-by-zero
            enc.encode(1.0, &mut buf);
            let mag = i16::from_le_bytes([buf[3], buf[4]]);
            prop_assert!(mag >= -10_000);
            prop_assert!(mag <= 10_000);
        }
    }
}
