//! Comprehensive tests for the VRS DirectForce Pro HID protocol crate.
//!
//! Covers: input report parsing round-trips, output report construction,
//! device identification via PID, torque encoding precision and safety limits,
//! edge cases (boundary values, short reports, invalid data), property tests
//! for encoding round-trips, and known constant validation.

use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VRS_PRODUCT_ID, VRS_VENDOR_ID, VrsConstantForceEncoder, VrsDamperEncoder, VrsFfbEffectType,
    VrsFrictionEncoder, VrsInputState, VrsPedalAxesRaw, VrsSpringEncoder, build_device_gain,
    build_ffb_enable, build_rotation_range, identify_device, is_wheelbase_product,
    parse_input_report, product_ids,
};

// ---------------------------------------------------------------------------
// 1. Known constant validation
// ---------------------------------------------------------------------------

#[test]
fn constants_vendor_id() {
    assert_eq!(
        VRS_VENDOR_ID, 0x0483,
        "VRS VID must match STMicroelectronics generic VID"
    );
}

#[test]
fn constants_primary_product_id() {
    assert_eq!(VRS_PRODUCT_ID, 0xA355);
    assert_eq!(product_ids::DIRECTFORCE_PRO, 0xA355);
}

#[test]
fn constants_product_ids_confirmed() {
    assert_eq!(product_ids::DIRECTFORCE_PRO, 0xA355);
    assert_eq!(product_ids::DIRECTFORCE_PRO_V2, 0xA356);
    assert_eq!(product_ids::R295, 0xA44C);
    assert_eq!(product_ids::PEDALS, 0xA3BE);
    assert_eq!(product_ids::PEDALS_V1, 0xA357);
    assert_eq!(product_ids::PEDALS_V2, 0xA358);
    assert_eq!(product_ids::HANDBRAKE, 0xA359);
    assert_eq!(product_ids::SHIFTER, 0xA35A);
}

#[test]
fn constants_report_sizes() {
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 8);
    assert_eq!(SPRING_REPORT_LEN, 10);
    assert_eq!(DAMPER_REPORT_LEN, 8);
    assert_eq!(FRICTION_REPORT_LEN, 10);
}

// ---------------------------------------------------------------------------
// 2. Device identification via PID
// ---------------------------------------------------------------------------

#[test]
fn identify_all_known_wheelbases() {
    let wheelbases = [
        product_ids::DIRECTFORCE_PRO,
        product_ids::DIRECTFORCE_PRO_V2,
        product_ids::R295,
    ];
    for &pid in &wheelbases {
        let id = identify_device(pid);
        assert!(
            id.supports_ffb,
            "Wheelbase PID 0x{pid:04X} should support FFB"
        );
        assert!(
            id.max_torque_nm.is_some(),
            "Wheelbase PID 0x{pid:04X} should have torque"
        );
        assert!(is_wheelbase_product(pid));
    }
}

#[test]
fn identify_dfp_torque() {
    let id = identify_device(product_ids::DIRECTFORCE_PRO);
    assert_eq!(id.name, "VRS DirectForce Pro");
    let torque = id.max_torque_nm;
    assert!(torque.is_some());
    assert!((torque.map_or(0.0, |t| t) - 20.0).abs() < f32::EPSILON);
}

#[test]
fn identify_dfp_v2_torque() {
    let id = identify_device(product_ids::DIRECTFORCE_PRO_V2);
    assert_eq!(id.name, "VRS DirectForce Pro V2");
    let torque = id.max_torque_nm;
    assert!(torque.is_some());
    assert!((torque.map_or(0.0, |t| t) - 25.0).abs() < f32::EPSILON);
}

#[test]
fn identify_non_wheelbase_peripherals() {
    let peripherals = [
        product_ids::PEDALS,
        product_ids::PEDALS_V1,
        product_ids::PEDALS_V2,
        product_ids::HANDBRAKE,
        product_ids::SHIFTER,
    ];
    for &pid in &peripherals {
        let id = identify_device(pid);
        assert!(
            !id.supports_ffb,
            "Peripheral PID 0x{pid:04X} should not support FFB"
        );
        assert!(!is_wheelbase_product(pid));
    }
}

#[test]
fn identify_unknown_pid() {
    let id = identify_device(0x0000);
    assert_eq!(id.name, "VRS Unknown");
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
}

// ---------------------------------------------------------------------------
// 3. Input report parsing
// ---------------------------------------------------------------------------

fn build_vrs_report(
    steering: i16,
    throttle: u16,
    brake: u16,
    clutch: u16,
    buttons: u16,
    hat: u8,
    enc1: u8,
    enc2: u8,
) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    let s = steering.to_le_bytes();
    data[0] = s[0];
    data[1] = s[1];
    let t = throttle.to_le_bytes();
    data[2] = t[0];
    data[3] = t[1];
    let b = brake.to_le_bytes();
    data[4] = b[0];
    data[5] = b[1];
    let c = clutch.to_le_bytes();
    data[6] = c[0];
    data[7] = c[1];
    let btn = buttons.to_le_bytes();
    data[8] = btn[0];
    data[9] = btn[1];
    data[12] = hat;
    data[13] = enc1;
    data[15] = enc2;
    data
}

#[test]
fn parse_input_too_short() {
    for len in 0..17 {
        let data = vec![0u8; len];
        assert!(
            parse_input_report(&data).is_none(),
            "length {len} should fail"
        );
    }
}

#[test]
fn parse_input_exact_minimum() -> Result<(), String> {
    let data = vec![0u8; 17];
    let state = parse_input_report(&data).ok_or("parse failed at minimum length")?;
    assert!(state.steering.abs() < 0.001);
    assert!(state.throttle.abs() < 0.001);
    assert!(state.brake.abs() < 0.001);
    assert!(state.clutch.abs() < 0.001);
    Ok(())
}

#[test]
fn parse_input_center_steering() -> Result<(), String> {
    let data = build_vrs_report(0, 0, 0, 0, 0, 0x0F, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.steering.abs() < 0.001);
    Ok(())
}

#[test]
fn parse_input_full_left() -> Result<(), String> {
    let data = build_vrs_report(i16::MIN, 0, 0, 0, 0, 0, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering + 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_input_full_right() -> Result<(), String> {
    let data = build_vrs_report(i16::MAX, 0, 0, 0, 0, 0, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering - 1.0).abs() < 0.01);
    Ok(())
}

#[test]
fn parse_input_pedals_full() -> Result<(), String> {
    let data = build_vrs_report(0, u16::MAX, u16::MAX, u16::MAX, 0, 0, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.throttle - 1.0).abs() < 0.001);
    assert!((state.brake - 1.0).abs() < 0.001);
    assert!((state.clutch - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_input_buttons() -> Result<(), String> {
    let data = build_vrs_report(0, 0, 0, 0, 0xABCD, 0, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons, 0xABCD);
    Ok(())
}

#[test]
fn parse_input_hat_neutral() -> Result<(), String> {
    let data = build_vrs_report(0, 0, 0, 0, 0, 0x0F, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x0F);
    Ok(())
}

#[test]
fn parse_input_hat_up() -> Result<(), String> {
    let data = build_vrs_report(0, 0, 0, 0, 0, 0x00, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x00);
    Ok(())
}

#[test]
fn parse_input_hat_masks_upper_nibble() -> Result<(), String> {
    let data = build_vrs_report(0, 0, 0, 0, 0, 0xF3, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x03, "upper nibble of hat byte should be masked");
    Ok(())
}

#[test]
fn parse_input_encoders() -> Result<(), String> {
    let data = build_vrs_report(0, 0, 0, 0, 0, 0, 42, 200);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.encoder1, 42);
    assert_eq!(state.encoder2, -56); // 200 as i8 = -56
    Ok(())
}

#[test]
fn parse_input_connection_status() -> Result<(), String> {
    // Normal data: connected = true
    let data = build_vrs_report(100, 0, 0, 0, 0, 0, 0, 0);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.connected);

    // All 0xFF in steering bytes: disconnected
    let data = build_vrs_report(-1, 0, 0, 0, 0, 0, 0, 0); // -1 as i16 = 0xFFFF
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state.connected);
    Ok(())
}

#[test]
fn vrs_input_state_empty() {
    let state = VrsInputState::empty();
    assert!(state.steering.abs() < f32::EPSILON);
    assert!(state.throttle.abs() < f32::EPSILON);
    assert!(state.brake.abs() < f32::EPSILON);
    assert!(state.clutch.abs() < f32::EPSILON);
    assert_eq!(state.buttons, 0);
    assert_eq!(state.hat, 0);
    assert_eq!(state.encoder1, 0);
    assert_eq!(state.encoder2, 0);
    assert!(!state.connected);
}

// ---------------------------------------------------------------------------
// 4. Pedal axes raw/normalize
// ---------------------------------------------------------------------------

#[test]
fn pedal_axes_raw_normalize_zero() {
    let raw = VrsPedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: 0,
    };
    let norm = raw.normalize();
    assert!(norm.throttle.abs() < f32::EPSILON);
    assert!(norm.brake.abs() < f32::EPSILON);
    assert!(norm.clutch.abs() < f32::EPSILON);
}

#[test]
fn pedal_axes_raw_normalize_full() {
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
fn pedal_axes_raw_normalize_half() {
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

#[test]
fn pedal_axes_raw_roundtrip_from_input_state() {
    let state = VrsInputState {
        throttle: 0.5,
        brake: 0.25,
        clutch: 0.75,
        ..Default::default()
    };
    let raw = state.pedal_axes_raw();
    let norm = raw.normalize();
    assert!((norm.throttle - 0.5).abs() < 0.01);
    assert!((norm.brake - 0.25).abs() < 0.01);
    assert!((norm.clutch - 0.75).abs() < 0.01);
}

// ---------------------------------------------------------------------------
// 5. Output report construction (FFB)
// ---------------------------------------------------------------------------

#[test]
fn constant_force_encode_positive() -> Result<(), String> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(10.0, &mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], 0x11);
    assert_eq!(out[1], 1); // effect block index
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000);
    Ok(())
}

#[test]
fn constant_force_encode_negative() {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-10.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -5000);
}

#[test]
fn constant_force_encode_zero() {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x11);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 0);
}

#[test]
fn constant_force_saturation() {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000);

    enc.encode(-100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -10000);
}

#[test]
fn constant_force_max_torque_floor() {
    // max_torque_nm of 0 should be floored to 0.01
    let enc = VrsConstantForceEncoder::new(0.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.005, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000); // 0.005 / 0.01 = 0.5 => 5000
}

#[test]
fn spring_encode_fields() {
    let enc = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let len = enc.encode(5000, 1000, -500, 200, &mut out);
    assert_eq!(len, SPRING_REPORT_LEN);
    assert_eq!(out[0], 0x19);
    assert_eq!(out[1], 1);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 5000);
    assert_eq!(i16::from_le_bytes([out[4], out[5]]), 1000);
    assert_eq!(i16::from_le_bytes([out[6], out[7]]), -500);
    assert_eq!(u16::from_le_bytes([out[8], out[9]]), 200);
}

#[test]
fn spring_encode_zero() {
    let enc = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x19);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 0);
    assert_eq!(i16::from_le_bytes([out[4], out[5]]), 0);
}

#[test]
fn damper_encode_fields() {
    let enc = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    let len = enc.encode(7500, 3000, &mut out);
    assert_eq!(len, DAMPER_REPORT_LEN);
    assert_eq!(out[0], 0x1A);
    assert_eq!(out[1], 1);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 7500);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 3000);
    assert_eq!(&out[6..8], &[0, 0]); // reserved
}

#[test]
fn damper_encode_zero() {
    let enc = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x1A);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 0);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 0);
}

#[test]
fn friction_encode_fields() {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    let len = enc.encode(4000, 2000, &mut out);
    assert_eq!(len, FRICTION_REPORT_LEN);
    assert_eq!(out[0], 0x1B);
    assert_eq!(out[1], 1);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 4000);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 2000);
    assert_eq!(&out[6..10], &[0, 0, 0, 0]); // reserved
}

#[test]
fn friction_encode_zero() {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], 0x1B);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 0);
}

// ---------------------------------------------------------------------------
// 6. Configuration reports
// ---------------------------------------------------------------------------

#[test]
fn rotation_range_900() {
    let r = build_rotation_range(900);
    assert_eq!(r[0], 0x0C);
    let degrees = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(degrees, 900);
}

#[test]
fn rotation_range_1080() {
    let r = build_rotation_range(1080);
    let degrees = u16::from_le_bytes([r[2], r[3]]);
    assert_eq!(degrees, 1080);
}

#[test]
fn rotation_range_boundary_values() {
    for &deg in &[0u16, 180, 360, 540, 720, 900, 1080, 1440, 2880, u16::MAX] {
        let r = build_rotation_range(deg);
        let decoded = u16::from_le_bytes([r[2], r[3]]);
        assert_eq!(decoded, deg);
    }
}

#[test]
fn device_gain_full_range() {
    let r_min = build_device_gain(0x00);
    assert_eq!(r_min[0], 0x0C);
    assert_eq!(r_min[1], 0x01);
    assert_eq!(r_min[2], 0x00);

    let r_max = build_device_gain(0xFF);
    assert_eq!(r_max[2], 0xFF);
}

#[test]
fn ffb_enable_disable() {
    let on = build_ffb_enable(true);
    assert_eq!(on[0], 0x0B);
    assert_eq!(on[1], 0x01);

    let off = build_ffb_enable(false);
    assert_eq!(off[0], 0x0B);
    assert_eq!(off[1], 0x00);
}

// ---------------------------------------------------------------------------
// 7. FFB effect type report IDs
// ---------------------------------------------------------------------------

#[test]
fn effect_type_report_ids() {
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

#[test]
fn effect_type_report_ids_unique() {
    let ids = [
        VrsFfbEffectType::Constant.report_id(),
        VrsFfbEffectType::Ramp.report_id(),
        VrsFfbEffectType::Square.report_id(),
        VrsFfbEffectType::Sine.report_id(),
        VrsFfbEffectType::Triangle.report_id(),
        VrsFfbEffectType::SawtoothUp.report_id(),
        VrsFfbEffectType::SawtoothDown.report_id(),
        VrsFfbEffectType::Spring.report_id(),
        VrsFfbEffectType::Damper.report_id(),
        VrsFfbEffectType::Friction.report_id(),
        VrsFfbEffectType::Custom.report_id(),
    ];
    let mut sorted = ids.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        ids.len(),
        sorted.len(),
        "all effect type report IDs must be unique"
    );
}

// ---------------------------------------------------------------------------
// 8. Edge cases
// ---------------------------------------------------------------------------

#[test]
fn parse_input_all_ones() -> Result<(), String> {
    let data = vec![0xFFu8; 64];
    let state = parse_input_report(&data).ok_or("parse failed")?;
    // steering 0xFFFF = -1 as i16, normalised ≈ -0.00003
    assert!(state.steering.abs() < 0.01);
    assert!((state.throttle - 1.0).abs() < 0.001);
    assert!((state.brake - 1.0).abs() < 0.001);
    assert!((state.clutch - 1.0).abs() < 0.001);
    assert_eq!(state.buttons, 0xFFFF);
    // hat = 0xFF & 0x0F = 0x0F
    assert_eq!(state.hat, 0x0F);
    Ok(())
}

#[test]
fn parse_input_empty_slice() {
    assert!(parse_input_report(&[]).is_none());
}

#[test]
fn constant_force_negative_max_torque_floored() {
    let enc = VrsConstantForceEncoder::new(-5.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.005, &mut out);
    // max_torque is floored to 0.01, so 0.005/0.01 = 0.5 => 5000
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000);
}

// ---------------------------------------------------------------------------
// 9. Property tests
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_parse_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_input_report(&data);
        }

        #[test]
        fn prop_parse_input_valid_when_long_enough(data in proptest::collection::vec(any::<u8>(), 17..128)) {
            let state = parse_input_report(&data);
            prop_assert!(state.is_some());
            let s = state.ok_or_else(|| proptest::test_runner::TestCaseError::Fail("parse returned None".into()))?;
            prop_assert!((-1.0..=1.0).contains(&s.steering));
            prop_assert!((0.0..=1.0).contains(&s.throttle));
            prop_assert!((0.0..=1.0).contains(&s.brake));
            prop_assert!((0.0..=1.0).contains(&s.clutch));
            prop_assert!(s.hat <= 0x0F);
        }

        #[test]
        fn prop_constant_force_magnitude_bounded(torque_nm in -50.0f32..=50.0f32) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque_nm, &mut out);
            let mag = i16::from_le_bytes([out[3], out[4]]);
            prop_assert!(mag >= -10000);
            prop_assert!(mag <= 10000);
        }

        #[test]
        fn prop_constant_force_sign_preserved(torque_nm in -20.0f32..=20.0f32) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque_nm, &mut out);
            let mag = i16::from_le_bytes([out[3], out[4]]);
            if torque_nm > 0.1 {
                prop_assert!(mag > 0);
            } else if torque_nm < -0.1 {
                prop_assert!(mag < 0);
            }
        }

        #[test]
        fn prop_constant_force_monotone(a in -20.0f32..=20.0f32, b in -20.0f32..=20.0f32) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut out_a = [0u8; CONSTANT_FORCE_REPORT_LEN];
            let mut out_b = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(a, &mut out_a);
            enc.encode(b, &mut out_b);
            let mag_a = i16::from_le_bytes([out_a[3], out_a[4]]);
            let mag_b = i16::from_le_bytes([out_b[3], out_b[4]]);
            if a > b {
                prop_assert!(mag_a >= mag_b);
            } else if a < b {
                prop_assert!(mag_a <= mag_b);
            }
        }

        #[test]
        fn prop_rotation_range_roundtrip(degrees in 0u16..=65535u16) {
            let report = build_rotation_range(degrees);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, degrees);
        }

        #[test]
        fn prop_device_gain_roundtrip(gain in 0u8..=255u8) {
            let report = build_device_gain(gain);
            prop_assert_eq!(report[2], gain);
        }

        #[test]
        fn prop_pedal_axes_normalize_bounded(
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
        ) {
            let raw = VrsPedalAxesRaw { throttle, brake, clutch };
            let norm = raw.normalize();
            prop_assert!((0.0..=1.0).contains(&norm.throttle));
            prop_assert!((0.0..=1.0).contains(&norm.brake));
            prop_assert!((0.0..=1.0).contains(&norm.clutch));
        }

        #[test]
        fn prop_input_roundtrip(
            steering in -32768i16..=32767i16,
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
            buttons in 0u16..=65535u16,
        ) {
            let data = build_vrs_report(steering, throttle, brake, clutch, buttons, 0, 0, 0);
            let state = parse_input_report(&data)
                .ok_or_else(|| proptest::test_runner::TestCaseError::Fail("parse returned None".into()))?;
            prop_assert!((-1.0..=1.0).contains(&state.steering));
            prop_assert!((0.0..=1.0).contains(&state.throttle));
            prop_assert!((0.0..=1.0).contains(&state.brake));
            prop_assert!((0.0..=1.0).contains(&state.clutch));
            prop_assert_eq!(state.buttons, buttons);
        }
    }
}
