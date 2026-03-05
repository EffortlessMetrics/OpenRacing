//! Comprehensive VRS DirectForce Pro protocol hardening tests.
//!
//! Covers VID/PID validation, input report parsing roundtrips, FFB encoder
//! byte-level verification, device identification, and proptest fuzzing.

use racing_wheel_hid_vrs_protocol::*;

// ─── VID / PID golden values ────────────────────────────────────────────

#[test]
fn vid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        VRS_VENDOR_ID, 0x0483,
        "VRS VID must be STMicroelectronics 0x0483"
    );
    Ok(())
}

#[test]
fn primary_pid_golden_value() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VRS_PRODUCT_ID, 0xA355, "VRS DFP PID must be 0xA355");
    Ok(())
}

#[test]
fn product_id_constants_golden() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_ids::DIRECTFORCE_PRO, 0xA355);
    assert_eq!(product_ids::DIRECTFORCE_PRO_V2, 0xA356);
    assert_eq!(product_ids::R295, 0xA44C);
    assert_eq!(product_ids::PEDALS, 0xA3BE);
    assert_eq!(product_ids::PEDALS_V2, 0xA358);
    assert_eq!(product_ids::HANDBRAKE, 0xA359);
    assert_eq!(product_ids::SHIFTER, 0xA35A);
    Ok(())
}

#[test]
fn all_product_ids_nonzero_and_distinct() -> Result<(), Box<dyn std::error::Error>> {
    #[allow(deprecated)]
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
    for &pid in &pids {
        assert_ne!(pid, 0, "PID must be nonzero");
    }
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j], "PIDs at [{i}] and [{j}] must be distinct");
        }
    }
    Ok(())
}

// ─── Input report parsing ───────────────────────────────────────────────

fn make_vrs_input(
    steering: i16,
    throttle: u16,
    brake: u16,
    clutch: u16,
    buttons: u16,
    hat: u8,
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
    data
}

#[test]
fn parse_input_center_position() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_vrs_input(0, 0, 0, 0, 0, 0x0F);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.steering.abs() < 0.001);
    assert!(state.throttle.abs() < 0.001);
    assert!(state.brake.abs() < 0.001);
    assert!(state.clutch.abs() < 0.001);
    assert_eq!(state.buttons, 0);
    assert_eq!(state.hat, 0x0F);
    Ok(())
}

#[test]
fn parse_input_extremes() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_vrs_input(-32768, 0xFFFF, 0xFFFF, 0xFFFF, 0xFFFF, 0x00);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering + 1.0).abs() < 0.001, "full left = -1.0");
    assert!((state.throttle - 1.0).abs() < 0.001, "full throttle = 1.0");
    assert!((state.brake - 1.0).abs() < 0.001, "full brake = 1.0");
    assert!((state.clutch - 1.0).abs() < 0.001, "full clutch = 1.0");
    assert_eq!(state.buttons, 0xFFFF);
    assert_eq!(state.hat, 0x00);
    Ok(())
}

#[test]
fn parse_input_full_right() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_vrs_input(32767, 0, 0, 0, 0, 0x0F);
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering - 1.0).abs() < 0.001);
    Ok(())
}

#[test]
fn parse_input_report_too_short() -> Result<(), Box<dyn std::error::Error>> {
    assert!(parse_input_report(&[0u8; 0]).is_none());
    assert!(parse_input_report(&[0u8; 16]).is_none());
    assert!(parse_input_report(&[0u8; 17]).is_some()); // exactly min length
    Ok(())
}

#[test]
fn parse_input_disconnected_marker() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0xFFu8; 64];
    data[12] = 0x0F;
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert!(
        !state.connected,
        "0xFFFF steering bytes indicate disconnection"
    );
    Ok(())
}

#[test]
fn parse_input_encoder_values() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[13] = 127; // encoder1 = 127
    data[15] = 0x80; // encoder2 = -128 as i8
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.encoder1, 127);
    assert_eq!(state.encoder2, -128);
    Ok(())
}

#[test]
fn parse_input_hat_masked_to_4_bits() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[12] = 0xFF; // high nibble should be masked
    let state = parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x0F, "hat must be masked to 4 bits");
    Ok(())
}

#[test]
fn pedal_axes_raw_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let state = VrsInputState {
        throttle: 1.0,
        brake: 0.0,
        clutch: 0.5,
        ..Default::default()
    };
    let raw = state.pedal_axes_raw();
    assert_eq!(raw.throttle, 65535);
    assert_eq!(raw.brake, 0);
    // clutch: 0.5 * 65535 ≈ 32767
    assert!((raw.clutch as i32 - 32767).abs() <= 1);
    Ok(())
}

// ─── Device identification ──────────────────────────────────────────────

#[test]
fn identify_dfp_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::DIRECTFORCE_PRO);
    assert_eq!(id.name, "VRS DirectForce Pro");
    assert!(id.supports_ffb);
    let torque = id.max_torque_nm.ok_or("DFP must have torque")?;
    assert!((torque - 20.0).abs() < 0.001);
    Ok(())
}

#[test]
fn identify_r295_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::R295);
    assert_eq!(id.name, "VRS R295");
    assert!(id.supports_ffb);
    assert!(id.max_torque_nm.is_some());
    Ok(())
}

#[test]
fn identify_pedals_no_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::PEDALS);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    Ok(())
}

#[test]
fn identify_handbrake_no_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::HANDBRAKE);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    Ok(())
}

#[test]
fn identify_shifter_no_ffb() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(product_ids::SHIFTER);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    Ok(())
}

#[test]
fn identify_unknown_pid() -> Result<(), Box<dyn std::error::Error>> {
    let id = identify_device(0xFFFF);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    assert!(id.name.contains("Unknown"));
    Ok(())
}

#[test]
fn is_wheelbase_product_correct() -> Result<(), Box<dyn std::error::Error>> {
    assert!(is_wheelbase_product(product_ids::DIRECTFORCE_PRO));
    assert!(is_wheelbase_product(product_ids::DIRECTFORCE_PRO_V2));
    assert!(is_wheelbase_product(product_ids::R295));
    assert!(!is_wheelbase_product(product_ids::PEDALS));
    assert!(!is_wheelbase_product(product_ids::HANDBRAKE));
    assert!(!is_wheelbase_product(product_ids::SHIFTER));
    assert!(!is_wheelbase_product(0xFFFF));
    Ok(())
}

// ─── FFB effect type report IDs ─────────────────────────────────────────

#[test]
fn ffb_effect_type_report_ids_comprehensive() -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

// ─── Constant force encoder byte-level ──────────────────────────────────

#[test]
fn constant_force_encoder_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    // 10 Nm out of 20 Nm max = 50% = magnitude 5000
    enc.encode(10.0, &mut out);
    assert_eq!(out[0], 0x11, "report ID");
    assert_eq!(out[1], 1, "block index");
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000);
    // Bytes 5-7 reserved zeros
    assert_eq!(out[5], 0);
    assert_eq!(out[6], 0);
    assert_eq!(out[7], 0);
    Ok(())
}

#[test]
fn constant_force_encoder_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    enc.encode(100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000, "positive clamp");

    enc.encode(-100.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -10000, "negative clamp");
    Ok(())
}

#[test]
fn constant_force_encoder_min_torque_floor() -> Result<(), Box<dyn std::error::Error>> {
    // max_torque_nm < 0.01 should be floored to 0.01
    let enc = VrsConstantForceEncoder::new(0.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.005, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000, "0.005 / 0.01 = 0.5 -> 5000");
    Ok(())
}

// ─── Spring encoder byte-level ──────────────────────────────────────────

#[test]
fn spring_encoder_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsSpringEncoder::new(20.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    enc.encode(7500, 1234, -567, 100, &mut out);

    assert_eq!(out[0], 0x19, "spring report ID");
    assert_eq!(out[1], 1, "block index");
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 7500, "coefficient");
    assert_eq!(i16::from_le_bytes([out[4], out[5]]), 1234, "steering");
    assert_eq!(i16::from_le_bytes([out[6], out[7]]), -567, "center");
    assert_eq!(u16::from_le_bytes([out[8], out[9]]), 100, "deadzone");
    Ok(())
}

// ─── Damper encoder byte-level ──────────────────────────────────────────

#[test]
fn damper_encoder_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsDamperEncoder::new(20.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    enc.encode(4000, 2000, &mut out);

    assert_eq!(out[0], 0x1A, "damper report ID");
    assert_eq!(out[1], 1);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 4000, "coefficient");
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 2000, "velocity");
    assert_eq!(out[6], 0, "reserved");
    assert_eq!(out[7], 0, "reserved");
    Ok(())
}

// ─── Friction encoder byte-level ────────────────────────────────────────

#[test]
fn friction_encoder_known_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let enc = VrsFrictionEncoder::new(20.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    enc.encode(6000, 3000, &mut out);

    assert_eq!(out[0], 0x1B, "friction report ID");
    assert_eq!(out[1], 1);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 6000, "coefficient");
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 3000, "velocity");
    Ok(())
}

// ─── Build helpers ──────────────────────────────────────────────────────

#[test]
fn build_rotation_range_known_values() -> Result<(), Box<dyn std::error::Error>> {
    let tests: &[(u16, u8, u8)] = &[
        (900, 0x84, 0x03),
        (1080, 0x38, 0x04),
        (1440, 0xA0, 0x05),
        (360, 0x68, 0x01),
    ];
    for &(degrees, lsb, msb) in tests {
        let r = build_rotation_range(degrees);
        assert_eq!(r[0], 0x0C, "report ID for rotation range");
        assert_eq!(r[2], lsb, "LSB for {degrees}°");
        assert_eq!(r[3], msb, "MSB for {degrees}°");
    }
    Ok(())
}

#[test]
fn build_device_gain_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    let zero = build_device_gain(0);
    assert_eq!(zero[0], 0x0C);
    assert_eq!(zero[2], 0);

    let full = build_device_gain(0xFF);
    assert_eq!(full[2], 0xFF);
    Ok(())
}

#[test]
fn build_ffb_enable_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let on = build_ffb_enable(true);
    assert_eq!(on[0], 0x0B);
    assert_eq!(on[1], 0x01);

    let off = build_ffb_enable(false);
    assert_eq!(off[0], 0x0B);
    assert_eq!(off[1], 0x00);
    Ok(())
}

// ─── Report length constants ────────────────────────────────────────────

#[test]
fn report_length_constants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 8);
    assert_eq!(SPRING_REPORT_LEN, 10);
    assert_eq!(DAMPER_REPORT_LEN, 8);
    assert_eq!(FRICTION_REPORT_LEN, 10);
    Ok(())
}

// ─── Proptest fuzzing ───────────────────────────────────────────────────

mod proptest_vrs {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_parse_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_input_report(&data);
        }

        #[test]
        fn prop_parse_input_valid_length_always_some(data in proptest::collection::vec(any::<u8>(), 17..128)) {
            let result = parse_input_report(&data);
            prop_assert!(result.is_some(), "must parse any data >= 17 bytes");
        }

        #[test]
        fn prop_steering_always_in_range(steering in -32768i16..=32767) {
            let data = make_vrs_input(steering, 0, 0, 0, 0, 0);
            if let Some(state) = parse_input_report(&data) {
                prop_assert!(state.steering >= -1.0 && state.steering <= 1.0,
                    "steering {:.4} out of [-1, 1]", state.steering);
            }
        }

        #[test]
        fn prop_pedals_always_in_range(
            throttle in 0u16..=65535,
            brake in 0u16..=65535,
            clutch in 0u16..=65535,
        ) {
            let data = make_vrs_input(0, throttle, brake, clutch, 0, 0);
            if let Some(state) = parse_input_report(&data) {
                prop_assert!((0.0..=1.0).contains(&state.throttle));
                prop_assert!((0.0..=1.0).contains(&state.brake));
                prop_assert!((0.0..=1.0).contains(&state.clutch));
            }
        }

        #[test]
        fn prop_constant_force_report_id_stable(torque in -50.0f32..=50.0) {
            let enc = VrsConstantForceEncoder::new(20.0);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque, &mut out);
            prop_assert_eq!(out[0], 0x11);
            let mag = i16::from_le_bytes([out[3], out[4]]);
            prop_assert!((-10000..=10000).contains(&mag), "magnitude {mag} out of range");
        }

        #[test]
        fn prop_spring_coefficient_roundtrips(
            coeff in 0u16..=10000,
            steering in -32768i16..=32767,
            center in -32768i16..=32767,
            deadzone in 0u16..=10000,
        ) {
            let enc = VrsSpringEncoder::new(20.0);
            let mut out = [0u8; SPRING_REPORT_LEN];
            enc.encode(coeff, steering, center, deadzone, &mut out);
            prop_assert_eq!(u16::from_le_bytes([out[2], out[3]]), coeff);
            prop_assert_eq!(i16::from_le_bytes([out[4], out[5]]), steering);
            prop_assert_eq!(i16::from_le_bytes([out[6], out[7]]), center);
            prop_assert_eq!(u16::from_le_bytes([out[8], out[9]]), deadzone);
        }

        #[test]
        fn prop_damper_coefficient_roundtrips(coeff in 0u16..=10000, vel in 0u16..=10000) {
            let enc = VrsDamperEncoder::new(20.0);
            let mut out = [0u8; DAMPER_REPORT_LEN];
            enc.encode(coeff, vel, &mut out);
            prop_assert_eq!(u16::from_le_bytes([out[2], out[3]]), coeff);
            prop_assert_eq!(u16::from_le_bytes([out[4], out[5]]), vel);
        }

        #[test]
        fn prop_friction_coefficient_roundtrips(coeff in 0u16..=10000, vel in 0u16..=10000) {
            let enc = VrsFrictionEncoder::new(20.0);
            let mut out = [0u8; FRICTION_REPORT_LEN];
            enc.encode(coeff, vel, &mut out);
            prop_assert_eq!(u16::from_le_bytes([out[2], out[3]]), coeff);
            prop_assert_eq!(u16::from_le_bytes([out[4], out[5]]), vel);
        }

        #[test]
        fn prop_rotation_range_roundtrips(degrees in 0u16..=65535) {
            let report = build_rotation_range(degrees);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, degrees);
        }

        #[test]
        fn prop_identify_device_never_panics(pid in 0u16..=0xFFFF) {
            let id = identify_device(pid);
            prop_assert!(!id.name.is_empty());
        }

        #[test]
        fn prop_wheelbase_has_ffb(pid in 0u16..=0xFFFF) {
            let id = identify_device(pid);
            if is_wheelbase_product(pid) {
                prop_assert!(id.supports_ffb, "wheelbase PID 0x{pid:04X} must support FFB");
                prop_assert!(id.max_torque_nm.is_some());
            }
        }
    }

    fn make_vrs_input(
        steering: i16,
        throttle: u16,
        brake: u16,
        clutch: u16,
        buttons: u16,
        hat: u8,
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
        data
    }
}
