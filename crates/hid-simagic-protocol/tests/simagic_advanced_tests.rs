//! Advanced tests for Simagic protocol crate.
//!
//! Covers PID recognition for all models, torque encoding at various rated
//! torques, FFB effect commands, LED control, and proptest verification.

use racing_wheel_hid_simagic_protocol::types::{QuickReleaseStatus, SimagicGear};
use racing_wheel_hid_simagic_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SIMAGIC_VENDOR_ID,
    SPRING_REPORT_LEN, SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicDeviceCategory,
    SimagicFfbEffectType, SimagicFrictionEncoder, SimagicInputState, SimagicModel,
    SimagicSpringEncoder, build_device_gain, build_led_report, build_rotation_range,
    build_sine_effect, build_square_effect, build_triangle_effect, identify_device,
    is_wheelbase_product, product_ids,
};

// ─── PID recognition ─────────────────────────────────────────────────────

#[test]
fn test_evo_sport_recognition() {
    let identity = identify_device(product_ids::EVO_SPORT);
    assert_eq!(identity.name, "Simagic EVO Sport");
    assert_eq!(identity.category, SimagicDeviceCategory::Wheelbase);
    assert!(identity.supports_ffb);
    assert!((identity.max_torque_nm.unwrap_or(0.0) - 9.0).abs() < f32::EPSILON);
    assert!(is_wheelbase_product(product_ids::EVO_SPORT));
}

#[test]
fn test_evo_recognition() {
    let identity = identify_device(product_ids::EVO);
    assert_eq!(identity.name, "Simagic EVO");
    assert_eq!(identity.category, SimagicDeviceCategory::Wheelbase);
    assert!(identity.supports_ffb);
    assert!((identity.max_torque_nm.unwrap_or(0.0) - 12.0).abs() < f32::EPSILON);
}

#[test]
fn test_evo_pro_recognition() {
    let identity = identify_device(product_ids::EVO_PRO);
    assert_eq!(identity.name, "Simagic EVO Pro");
    assert!(identity.supports_ffb);
    assert!((identity.max_torque_nm.unwrap_or(0.0) - 18.0).abs() < f32::EPSILON);
}

#[test]
fn test_alpha_evo_recognition() {
    let identity = identify_device(product_ids::ALPHA_EVO);
    assert_eq!(identity.name, "Simagic Alpha EVO");
    assert!(identity.supports_ffb);
    assert!((identity.max_torque_nm.unwrap_or(0.0) - 15.0).abs() < f32::EPSILON);
}

#[test]
fn test_neo_recognition() {
    let identity = identify_device(product_ids::NEO);
    assert_eq!(identity.name, "Simagic Neo");
    assert!(identity.supports_ffb);
    assert!((identity.max_torque_nm.unwrap_or(0.0) - 10.0).abs() < f32::EPSILON);
}

#[test]
fn test_neo_mini_recognition() {
    let identity = identify_device(product_ids::NEO_MINI);
    assert_eq!(identity.name, "Simagic Neo Mini");
    assert!(identity.supports_ffb);
    assert!((identity.max_torque_nm.unwrap_or(0.0) - 7.0).abs() < f32::EPSILON);
}

#[test]
fn test_model_from_pid_all_wheelbases() {
    assert_eq!(
        SimagicModel::from_pid(product_ids::EVO_SPORT),
        SimagicModel::EvoSport
    );
    assert_eq!(SimagicModel::from_pid(product_ids::EVO), SimagicModel::Evo);
    assert_eq!(
        SimagicModel::from_pid(product_ids::EVO_PRO),
        SimagicModel::EvoPro
    );
    assert_eq!(
        SimagicModel::from_pid(product_ids::ALPHA_EVO),
        SimagicModel::AlphaEvo
    );
    assert_eq!(SimagicModel::from_pid(product_ids::NEO), SimagicModel::Neo);
    assert_eq!(
        SimagicModel::from_pid(product_ids::NEO_MINI),
        SimagicModel::NeoMini
    );
}

#[test]
fn test_pedal_recognition() {
    let p1000 = identify_device(product_ids::P1000_PEDALS);
    assert_eq!(p1000.category, SimagicDeviceCategory::Pedals);
    assert!(!p1000.supports_ffb);
    assert!(p1000.max_torque_nm.is_none());
    assert!(!is_wheelbase_product(product_ids::P1000_PEDALS));

    let p2000 = identify_device(product_ids::P2000_PEDALS);
    assert_eq!(p2000.category, SimagicDeviceCategory::Pedals);
}

#[test]
fn test_shifter_recognition() {
    let h_shifter = identify_device(product_ids::SHIFTER_H);
    assert_eq!(h_shifter.category, SimagicDeviceCategory::Shifter);
    assert!(!h_shifter.supports_ffb);

    let seq_shifter = identify_device(product_ids::SHIFTER_SEQ);
    assert_eq!(seq_shifter.category, SimagicDeviceCategory::Shifter);
}

#[test]
fn test_handbrake_recognition() {
    let hb = identify_device(product_ids::HANDBRAKE);
    assert_eq!(hb.category, SimagicDeviceCategory::Handbrake);
    assert!(!hb.supports_ffb);
}

#[test]
fn test_rim_recognition() {
    for &pid in &[
        product_ids::RIM_WR1,
        product_ids::RIM_GT1,
        product_ids::RIM_GT_NEO,
        product_ids::RIM_FORMULA,
    ] {
        let identity = identify_device(pid);
        assert_eq!(identity.category, SimagicDeviceCategory::Rim);
        assert!(!identity.supports_ffb);
    }
}

#[test]
fn test_unknown_pid_defaults() {
    let unknown = identify_device(0xFFFF);
    assert_eq!(unknown.category, SimagicDeviceCategory::Unknown);
    assert!(!unknown.supports_ffb);
    assert!(unknown.max_torque_nm.is_none());
    assert_eq!(SimagicModel::from_pid(0xFFFF), SimagicModel::Unknown);
}

#[test]
fn test_vendor_id_constant() {
    assert_eq!(SIMAGIC_VENDOR_ID, 0x3670);
}

// ─── Torque encoding at various rated torques ────────────────────────────

#[test]
fn test_constant_force_at_evo_sport_torque() {
    // EVO Sport: 9 Nm
    let enc = SimagicConstantForceEncoder::new(9.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(4.5, &mut out); // half torque
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000);
}

#[test]
fn test_constant_force_at_evo_pro_torque() {
    // EVO Pro: 18 Nm
    let enc = SimagicConstantForceEncoder::new(18.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(18.0, &mut out); // full torque
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000);
}

#[test]
fn test_constant_force_saturation() {
    let enc = SimagicConstantForceEncoder::new(12.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000);
    enc.encode(-100.0, &mut out);
    let mag_neg = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag_neg, -10000);
}

#[test]
fn test_constant_force_zero_output() {
    let enc = SimagicConstantForceEncoder::new(12.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x11);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 0);
}

// ─── FFB effect commands ─────────────────────────────────────────────────

#[test]
fn test_spring_encoder_encodes_all_params() {
    let enc = SimagicSpringEncoder::new(12.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    enc.encode(800, 500, -200, 50, &mut out);
    assert_eq!(out[0], 0x12);
    assert_eq!(out[1], 1);
    let strength = u16::from_le_bytes([out[2], out[3]]);
    let steering = i16::from_le_bytes([out[4], out[5]]);
    let center = i16::from_le_bytes([out[6], out[7]]);
    let deadzone = u16::from_le_bytes([out[8], out[9]]);
    assert_eq!(strength, 800);
    assert_eq!(steering, 500);
    assert_eq!(center, -200);
    assert_eq!(deadzone, 50);
}

#[test]
fn test_damper_encoder_strength_velocity() {
    let enc = SimagicDamperEncoder::new(12.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    enc.encode(600, 3000, &mut out);
    assert_eq!(out[0], 0x13);
    let strength = u16::from_le_bytes([out[2], out[3]]);
    let velocity = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(strength, 600);
    assert_eq!(velocity, 3000);
}

#[test]
fn test_friction_encoder_coefficient_velocity() {
    let enc = SimagicFrictionEncoder::new(12.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    enc.encode(450, 7500, &mut out);
    assert_eq!(out[0], 0x14);
    let coefficient = u16::from_le_bytes([out[2], out[3]]);
    let velocity = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(coefficient, 450);
    assert_eq!(velocity, 7500);
}

#[test]
fn test_sine_effect_encoding() {
    let report = build_sine_effect(500, 2.0, 90);
    assert_eq!(report[0], 0x15);
    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 500);
    let freq_encoded = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq_encoded, 200); // 2.0 * 100
    let phase = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(phase, 90);
}

#[test]
fn test_square_effect_encoding() {
    let report = build_square_effect(750, 5.0, 50);
    assert_eq!(report[0], 0x16);
    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 750);
    let duty = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(duty, 50);
}

#[test]
fn test_triangle_effect_encoding() {
    let report = build_triangle_effect(300, 1.5);
    assert_eq!(report[0], 0x17);
    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 300);
    let freq_encoded = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq_encoded, 150); // 1.5 * 100
}

#[test]
fn test_square_effect_duty_cycle_clamped() {
    let report = build_square_effect(100, 1.0, 200); // 200 > 100
    let duty = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(duty, 100); // clamped to max 100
}

#[test]
fn test_sine_frequency_clamped() {
    let low = build_sine_effect(100, 0.01, 0); // below 0.1
    let high = build_sine_effect(100, 100.0, 0); // above 20
    let freq_low = u16::from_le_bytes([low[4], low[5]]);
    let freq_high = u16::from_le_bytes([high[4], high[5]]);
    assert_eq!(freq_low, 10); // 0.1 * 100
    assert_eq!(freq_high, 2000); // 20.0 * 100
}

// ─── LED control ─────────────────────────────────────────────────────────

#[test]
fn test_led_report_format() {
    let report = build_led_report(0xAB);
    assert_eq!(report[0], 0x30);
    assert_eq!(report[1], 0xAB);
    // Rest should be zeros
    for &b in &report[2..] {
        assert_eq!(b, 0);
    }
}

#[test]
fn test_led_report_zero_pattern() {
    let report = build_led_report(0x00);
    assert_eq!(report[0], 0x30);
    assert_eq!(report[1], 0x00);
}

// ─── Rotation range and device gain ──────────────────────────────────────

#[test]
fn test_rotation_range_encoding() {
    let report = build_rotation_range(900);
    assert_eq!(report[0], 0x20);
    let decoded = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(decoded, 900);
}

#[test]
fn test_device_gain_encoding() {
    let report = build_device_gain(0xFF);
    assert_eq!(report[0], 0x21);
    assert_eq!(report[1], 0xFF);
}

// ─── FFB effect type report IDs ──────────────────────────────────────────

#[test]
fn test_ffb_effect_type_report_ids() {
    assert_eq!(SimagicFfbEffectType::Constant.report_id(), 0x11);
    assert_eq!(SimagicFfbEffectType::Spring.report_id(), 0x12);
    assert_eq!(SimagicFfbEffectType::Damper.report_id(), 0x13);
    assert_eq!(SimagicFfbEffectType::Friction.report_id(), 0x14);
    assert_eq!(SimagicFfbEffectType::Sine.report_id(), 0x15);
    assert_eq!(SimagicFfbEffectType::Square.report_id(), 0x16);
    assert_eq!(SimagicFfbEffectType::Triangle.report_id(), 0x17);
}

// ─── Model torque values ─────────────────────────────────────────────────

#[test]
fn test_model_max_torque_all_wheelbases() {
    assert!((SimagicModel::EvoSport.max_torque_nm() - 9.0).abs() < f32::EPSILON);
    assert!((SimagicModel::Evo.max_torque_nm() - 12.0).abs() < f32::EPSILON);
    assert!((SimagicModel::EvoPro.max_torque_nm() - 18.0).abs() < f32::EPSILON);
    assert!((SimagicModel::AlphaEvo.max_torque_nm() - 12.0).abs() < f32::EPSILON);
    assert!((SimagicModel::Neo.max_torque_nm() - 10.0).abs() < f32::EPSILON);
    assert!((SimagicModel::NeoMini.max_torque_nm() - 7.0).abs() < f32::EPSILON);
}

#[test]
fn test_model_max_torque_non_wheelbases() {
    assert!((SimagicModel::P1000.max_torque_nm() - 0.0).abs() < f32::EPSILON);
    assert!((SimagicModel::P2000.max_torque_nm() - 0.0).abs() < f32::EPSILON);
    assert!((SimagicModel::ShifterH.max_torque_nm() - 0.0).abs() < f32::EPSILON);
    assert!((SimagicModel::Handbrake.max_torque_nm() - 0.0).abs() < f32::EPSILON);
}

// ─── Gear and quick release types ────────────────────────────────────────

#[test]
fn test_gear_from_raw_all_values() {
    assert_eq!(SimagicGear::from_raw(0), SimagicGear::Neutral);
    assert_eq!(SimagicGear::from_raw(1), SimagicGear::First);
    assert_eq!(SimagicGear::from_raw(2), SimagicGear::Second);
    assert_eq!(SimagicGear::from_raw(3), SimagicGear::Third);
    assert_eq!(SimagicGear::from_raw(4), SimagicGear::Fourth);
    assert_eq!(SimagicGear::from_raw(5), SimagicGear::Fifth);
    assert_eq!(SimagicGear::from_raw(6), SimagicGear::Sixth);
    assert_eq!(SimagicGear::from_raw(7), SimagicGear::Seventh);
    assert_eq!(SimagicGear::from_raw(8), SimagicGear::Eighth);
    assert_eq!(SimagicGear::from_raw(255), SimagicGear::Unknown);
}

#[test]
fn test_quick_release_from_raw() {
    assert_eq!(
        QuickReleaseStatus::from_raw(0),
        QuickReleaseStatus::Attached
    );
    assert_eq!(
        QuickReleaseStatus::from_raw(1),
        QuickReleaseStatus::Detached
    );
    assert_eq!(QuickReleaseStatus::from_raw(2), QuickReleaseStatus::Unknown);
    assert_eq!(
        QuickReleaseStatus::from_raw(255),
        QuickReleaseStatus::Unknown
    );
}

// ─── Input state helpers ─────────────────────────────────────────────────

#[test]
fn test_input_state_empty() {
    let state = SimagicInputState::empty();
    assert!((state.steering - 0.0).abs() < f32::EPSILON);
    assert!((state.throttle - 0.0).abs() < f32::EPSILON);
    assert!((state.brake - 0.0).abs() < f32::EPSILON);
}

// ─── Encoder zero methods ────────────────────────────────────────────────

#[test]
fn test_all_encoders_zero_clears_magnitude() {
    let cf = SimagicConstantForceEncoder::new(10.0);
    let mut cf_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    cf.encode_zero(&mut cf_out);
    assert_eq!(cf_out[0], 0x11);
    assert_eq!(i16::from_le_bytes([cf_out[3], cf_out[4]]), 0);

    let sp = SimagicSpringEncoder::new(10.0);
    let mut sp_out = [0u8; SPRING_REPORT_LEN];
    sp.encode_zero(&mut sp_out);
    assert_eq!(sp_out[0], 0x12);
    assert_eq!(u16::from_le_bytes([sp_out[2], sp_out[3]]), 0);

    let dm = SimagicDamperEncoder::new(10.0);
    let mut dm_out = [0u8; DAMPER_REPORT_LEN];
    dm.encode_zero(&mut dm_out);
    assert_eq!(dm_out[0], 0x13);
    assert_eq!(u16::from_le_bytes([dm_out[2], dm_out[3]]), 0);

    let fr = SimagicFrictionEncoder::new(10.0);
    let mut fr_out = [0u8; FRICTION_REPORT_LEN];
    fr.encode_zero(&mut fr_out);
    assert_eq!(fr_out[0], 0x14);
    assert_eq!(u16::from_le_bytes([fr_out[2], fr_out[3]]), 0);
}

// ─── Proptest ────────────────────────────────────────────────────────────

mod proptest_advanced {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(300))]

        #[test]
        fn prop_constant_force_sign_preserved(
            max_torque in 0.1_f32..=25.0_f32,
            fraction in -1.0_f32..=1.0_f32,
        ) {
            let torque = max_torque * fraction;
            let enc = SimagicConstantForceEncoder::new(max_torque);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque, &mut out);
            let mag = i16::from_le_bytes([out[3], out[4]]);
            if torque > 0.01 {
                prop_assert!(mag >= 0, "positive torque {torque} yielded negative mag {mag}");
            } else if torque < -0.01 {
                prop_assert!(mag <= 0, "negative torque {torque} yielded positive mag {mag}");
            }
        }

        #[test]
        fn prop_rotation_range_roundtrip(degrees in 0u16..=10000u16) {
            let report = build_rotation_range(degrees);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, degrees);
        }

        #[test]
        fn prop_device_gain_roundtrip(gain in 0u8..=255u8) {
            let report = build_device_gain(gain);
            prop_assert_eq!(report[0], 0x21);
            prop_assert_eq!(report[1], gain);
        }

        #[test]
        fn prop_led_pattern_roundtrip(pattern in 0u8..=255u8) {
            let report = build_led_report(pattern);
            prop_assert_eq!(report[0], 0x30);
            prop_assert_eq!(report[1], pattern);
        }

        #[test]
        fn prop_spring_strength_roundtrip(
            strength in 0u16..=10000u16,
            steering in i16::MIN..=i16::MAX,
        ) {
            let enc = SimagicSpringEncoder::new(12.0);
            let mut out = [0u8; SPRING_REPORT_LEN];
            enc.encode(strength, steering, 0, 0, &mut out);
            let decoded_strength = u16::from_le_bytes([out[2], out[3]]);
            let decoded_steering = i16::from_le_bytes([out[4], out[5]]);
            prop_assert_eq!(decoded_strength, strength);
            prop_assert_eq!(decoded_steering, steering);
        }

        #[test]
        fn prop_damper_roundtrip(
            strength in 0u16..=10000u16,
            velocity in 0u16..=10000u16,
        ) {
            let enc = SimagicDamperEncoder::new(12.0);
            let mut out = [0u8; DAMPER_REPORT_LEN];
            enc.encode(strength, velocity, &mut out);
            let decoded_strength = u16::from_le_bytes([out[2], out[3]]);
            let decoded_velocity = u16::from_le_bytes([out[4], out[5]]);
            prop_assert_eq!(decoded_strength, strength);
            prop_assert_eq!(decoded_velocity, velocity);
        }

        #[test]
        fn prop_gear_from_raw_never_panics(value in 0u8..=255u8) {
            let _gear = SimagicGear::from_raw(value);
        }

        #[test]
        fn prop_quick_release_from_raw_never_panics(value in 0u8..=255u8) {
            let _qr = QuickReleaseStatus::from_raw(value);
        }

        #[test]
        fn prop_model_from_pid_deterministic(pid in 0u16..=65535u16) {
            let a = SimagicModel::from_pid(pid);
            let b = SimagicModel::from_pid(pid);
            prop_assert_eq!(a, b);
        }
    }
}
