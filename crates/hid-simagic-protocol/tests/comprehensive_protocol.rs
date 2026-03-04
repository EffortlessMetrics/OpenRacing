//! Comprehensive tests for the Simagic HID protocol crate.
//!
//! Covers: input report parsing for multiple models, output report construction,
//! device identification via PID, torque encoding precision and safety limits,
//! motor enable/disable patterns, edge cases, property tests for round-trips,
//! and known constant validation.

use racing_wheel_hid_simagic_protocol::{
    self as simagic, CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN,
    SPRING_REPORT_LEN, SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicFrictionEncoder,
    SimagicSpringEncoder,
    ids::{
        SIMAGIC_LEGACY_PID, SIMAGIC_LEGACY_VENDOR_ID, SIMAGIC_VENDOR_ID, product_ids, report_ids,
    },
    types::{
        QuickReleaseStatus, SimagicDeviceCategory, SimagicFfbEffectType, SimagicGear, SimagicModel,
        SimagicPedalAxesRaw,
    },
};

// ═══════════════════════════════════════════════════════════════════════════════
// § 1  Input report parsing — model-specific test data
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper: build a 64-byte input report with specified axis values.
#[allow(clippy::too_many_arguments)]
fn build_input_report(
    steering_raw: u16,
    throttle_raw: u16,
    brake_raw: u16,
    clutch_raw: u16,
    handbrake_raw: u16,
    buttons: u16,
    hat: u8,
    rotary1: u8,
    rotary2: u8,
    gear: u8,
    flags: u8,
    qr_status: u8,
    fw: Option<(u8, u8, u8)>,
) -> Vec<u8> {
    let mut data = vec![0u8; 64];
    data[0..2].copy_from_slice(&steering_raw.to_le_bytes());
    data[2..4].copy_from_slice(&throttle_raw.to_le_bytes());
    data[4..6].copy_from_slice(&brake_raw.to_le_bytes());
    data[6..8].copy_from_slice(&clutch_raw.to_le_bytes());
    data[8..10].copy_from_slice(&handbrake_raw.to_le_bytes());
    data[10..12].copy_from_slice(&buttons.to_le_bytes());
    data[12] = hat;
    data[13] = rotary1;
    data[14] = rotary2;
    data[15] = gear;
    data[16] = flags;
    data[19] = qr_status;
    if let Some((major, minor, patch)) = fw {
        data[20] = major;
        data[21] = minor;
        data[22] = patch;
    }
    data
}

/// Simulate an Alpha wheelbase input report: full lock left, braking hard.
#[test]
fn input_alpha_full_left_heavy_brake() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input_report(
        0x0000, // full left
        0x0000, // throttle released
        0xC000, // ~75% brake
        0x0000, // clutch released
        0x0000, // handbrake released
        0x0000,
        0x08,
        0,
        0,
        0,
        0,
        0,
        Some((2, 1, 0)),
    );
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;

    assert!((state.steering + 1.0).abs() < 0.001, "should be full left");
    assert!(state.throttle < 0.001, "throttle released");
    assert!(
        state.brake > 0.74 && state.brake < 0.76,
        "~75% brake: {}",
        state.brake
    );
    assert_eq!(state.firmware_version, Some((2, 1, 0)));
    Ok(())
}

/// Simulate an M10 input report: center steering, half throttle, in 3rd gear.
#[test]
fn input_m10_center_half_throttle_third_gear() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input_report(
        0x8000, // center
        0x8000, // ~50% throttle
        0x0000,
        0x0000,
        0x0000,
        0x0000,
        0x08,
        0,
        0,
        3,    // third gear
        0x01, // clutch in range
        0,
        Some((1, 5, 3)),
    );
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;

    assert!(state.steering.abs() < 0.001, "center steering");
    assert!(
        (state.throttle - 0.5).abs() < 0.01,
        "~50% throttle: {}",
        state.throttle
    );
    assert_eq!(state.shifter.gear, SimagicGear::Third);
    assert!(state.shifter.clutch_in_range);
    assert!(!state.shifter.sequential_up_pressed);
    assert_eq!(state.firmware_version, Some((1, 5, 3)));
    Ok(())
}

/// Simulate a GTC/FX wheelbase: full right, full throttle, sequential shift up.
#[test]
fn input_fx_full_right_seq_shift_up() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input_report(
        0xFFFF, // full right
        0xFFFF, // full throttle
        0x0000, 0x0000, 0x0000, 0x0003, // buttons 0+1 pressed (paddle flags)
        0x02,   // hat right-up
        0xFF, 0x80, 0,    // neutral
        0x02, // sequential up
        0, None,
    );
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;

    assert!((state.steering - 1.0).abs() < 0.001, "full right");
    assert!((state.throttle - 1.0).abs() < 0.001, "full throttle");
    assert!(state.shifter.sequential_up_pressed);
    assert!(!state.shifter.sequential_down_pressed);
    assert_eq!(state.shifter.gear, SimagicGear::Neutral);
    assert_eq!(state.rotary1, 0xFF);
    assert_eq!(state.rotary2, 0x80);
    assert_eq!(state.buttons & 0x03, 0x03);
    // 64-byte report includes firmware bytes (20-22), all zero → Some((0,0,0))
    assert_eq!(state.firmware_version, Some((0, 0, 0)));
    Ok(())
}

/// All pedals fully pressed, 8th gear, all shifter flags active.
#[test]
fn input_all_pedals_max_eighth_gear_all_flags() -> Result<(), Box<dyn std::error::Error>> {
    let data = build_input_report(
        0x8000,
        0xFFFF,
        0xFFFF,
        0xFFFF,
        0xFFFF,
        0xFFFF,
        0x0F,
        0xFF,
        0xFF,
        8,    // eighth gear
        0x07, // all flags
        1,    // detached
        Some((0, 0, 0)),
    );
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;

    assert!((state.throttle - 1.0).abs() < 0.001);
    assert!((state.brake - 1.0).abs() < 0.001);
    assert!((state.clutch - 1.0).abs() < 0.001);
    assert!((state.handbrake - 1.0).abs() < 0.001);
    assert_eq!(state.shifter.gear, SimagicGear::Eighth);
    assert!(state.shifter.clutch_in_range);
    assert!(state.shifter.sequential_up_pressed);
    assert!(state.shifter.sequential_down_pressed);
    assert_eq!(state.buttons, 0xFFFF);
    assert_eq!(state.hat, 0x0F);
    assert_eq!(state.quick_release, QuickReleaseStatus::Detached);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 2  Output report construction — torque commands per model
// ═══════════════════════════════════════════════════════════════════════════════

/// EVO Sport (9 Nm max): encode half torque = 4.5 Nm → magnitude 5000.
#[test]
fn output_evo_sport_half_torque() -> Result<(), Box<dyn std::error::Error>> {
    let enc = SimagicConstantForceEncoder::new(9.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(4.5, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 5000, "4.5/9.0 = 0.5 → magnitude 5000");
    Ok(())
}

/// EVO Pro (18 Nm max): full positive torque → magnitude 10000.
#[test]
fn output_evo_pro_full_positive() -> Result<(), Box<dyn std::error::Error>> {
    let enc = SimagicConstantForceEncoder::new(18.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(18.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000);
    Ok(())
}

/// Neo Mini (7 Nm max): full negative → magnitude -10000.
#[test]
fn output_neo_mini_full_negative() -> Result<(), Box<dyn std::error::Error>> {
    let enc = SimagicConstantForceEncoder::new(7.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(-7.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -10000);
    Ok(())
}

/// Spring encoder: verify all fields are placed at correct offsets.
#[test]
fn output_spring_field_placement() -> Result<(), Box<dyn std::error::Error>> {
    let enc = SimagicSpringEncoder::new(12.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    enc.encode(800, -5000, 200, 100, &mut out);

    assert_eq!(out[0], report_ids::SPRING_EFFECT);
    assert_eq!(out[1], 1); // effect block index
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 800);
    assert_eq!(i16::from_le_bytes([out[4], out[5]]), -5000);
    assert_eq!(i16::from_le_bytes([out[6], out[7]]), 200);
    assert_eq!(u16::from_le_bytes([out[8], out[9]]), 100);
    Ok(())
}

/// Damper encoder: verify field placement.
#[test]
fn output_damper_field_placement() -> Result<(), Box<dyn std::error::Error>> {
    let enc = SimagicDamperEncoder::new(10.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    enc.encode(999, 7777, &mut out);

    assert_eq!(out[0], report_ids::DAMPER_EFFECT);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 999);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 7777);
    assert_eq!(out[6], 0, "reserved byte must be zero");
    assert_eq!(out[7], 0, "reserved byte must be zero");
    Ok(())
}

/// Friction encoder: verify field placement.
#[test]
fn output_friction_field_placement() -> Result<(), Box<dyn std::error::Error>> {
    let enc = SimagicFrictionEncoder::new(10.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    enc.encode(512, 9999, &mut out);

    assert_eq!(out[0], report_ids::FRICTION_EFFECT);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 512);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 9999);
    assert_eq!(&out[6..10], &[0, 0, 0, 0], "reserved bytes must be zero");
    Ok(())
}

/// Sine effect: frequency clamping below minimum.
#[test]
fn output_sine_frequency_clamp_min() {
    let report = simagic::build_sine_effect(500, 0.01, 0);
    // Clamped to 0.1 Hz → 0.1 * 100 = 10
    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 10, "frequency below 0.1 must clamp to 10");
}

/// Sine effect: frequency clamping above maximum.
#[test]
fn output_sine_frequency_clamp_max() {
    let report = simagic::build_sine_effect(500, 100.0, 0);
    // Clamped to 20.0 Hz → 20.0 * 100 = 2000
    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 2000, "frequency above 20.0 must clamp to 2000");
}

/// Square effect: duty cycle clamping above 100.
#[test]
fn output_square_duty_clamp() {
    let report = simagic::build_square_effect(500, 1.0, 200);
    let duty = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(duty, 100, "duty cycle above 100 must clamp to 100");
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 3  Device identification via PID
// ═══════════════════════════════════════════════════════════════════════════════

/// Every wheelbase PID returns a wheelbase identity with FFB support and torque.
#[test]
fn identify_all_wheelbases() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbases = [
        (product_ids::EVO_SPORT, "EVO Sport", 9.0),
        (product_ids::EVO, "EVO", 12.0),
        (product_ids::EVO_PRO, "EVO Pro", 18.0),
    ];
    for (pid, name_fragment, expected_torque) in wheelbases {
        let id = simagic::identify_device(pid);
        assert_eq!(id.product_id, pid);
        assert!(
            id.name.contains(name_fragment),
            "expected '{}' in '{}'",
            name_fragment,
            id.name
        );
        assert_eq!(id.category, SimagicDeviceCategory::Wheelbase);
        assert!(id.supports_ffb);
        let torque = id
            .max_torque_nm
            .ok_or(format!("no torque for {name_fragment}"))?;
        assert!(
            (torque - expected_torque).abs() < 0.01,
            "{name_fragment}: expected {expected_torque}, got {torque}"
        );
    }
    Ok(())
}

/// Pedals, shifters, handbrake, and rims have no FFB and no torque.
#[test]
fn identify_all_peripherals() {
    // Only the TB-RS Handbrake (0x0A04) is a verified peripheral.
    // All other peripheral PIDs are fabricated and now resolve to Unknown.
    let id = simagic::identify_device(product_ids::HANDBRAKE);
    assert_eq!(
        id.category,
        SimagicDeviceCategory::Handbrake,
        "Handbrake 0x0A04"
    );
    assert!(!id.supports_ffb, "Handbrake should not support FFB");
    assert!(
        id.max_torque_nm.is_none(),
        "Handbrake should have no torque"
    );
}

/// Fabricated P1000 and P1000A PIDs now resolve to Unknown.
#[test]
fn p1000_and_p1000a_resolve_to_unknown() {
    let a = simagic::identify_device(product_ids::P1000_PEDALS);
    let b = simagic::identify_device(product_ids::P1000A_PEDALS);
    assert_eq!(a.category, SimagicDeviceCategory::Unknown);
    assert_eq!(b.category, SimagicDeviceCategory::Unknown);
}

/// Fabricated rim PIDs now resolve to Unknown.
#[test]
fn all_rims_resolve_to_unknown() {
    let rim_pids = [
        product_ids::RIM_WR1,
        product_ids::RIM_GT1,
        product_ids::RIM_GT_NEO,
        product_ids::RIM_FORMULA,
    ];
    for &pid in &rim_pids {
        let id = simagic::identify_device(pid);
        assert_eq!(
            id.category,
            SimagicDeviceCategory::Unknown,
            "PID {pid:#06x}"
        );
    }
}

/// Unknown PID returns category Unknown, no FFB.
#[test]
fn identify_unknown_pid() {
    let id = simagic::identify_device(0xDEAD);
    assert_eq!(id.category, SimagicDeviceCategory::Unknown);
    assert!(!id.supports_ffb);
    assert!(id.max_torque_nm.is_none());
    assert_eq!(id.product_id, 0xDEAD);
}

/// SimagicModel::from_pid agrees with identify_device for confirmed PIDs.
#[test]
fn model_from_pid_consistent_with_identify() {
    let confirmed_pids = [
        product_ids::EVO_SPORT,
        product_ids::EVO,
        product_ids::EVO_PRO,
        product_ids::HANDBRAKE,
    ];
    for pid in confirmed_pids {
        let model = SimagicModel::from_pid(pid);
        let identity = simagic::identify_device(pid);

        let model_is_wheelbase = model.max_torque_nm() > 0.0;
        let identity_is_wheelbase = identity.category == SimagicDeviceCategory::Wheelbase;
        assert_eq!(
            model_is_wheelbase, identity_is_wheelbase,
            "PID {pid:#06x}: model={model:?} vs identity category={:?}",
            identity.category
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 4  Torque encoding precision and safety limits
// ═══════════════════════════════════════════════════════════════════════════════

/// Verify LSB-level precision: 1 Nm with max 10 Nm → exactly 1000 magnitude.
#[test]
fn torque_precision_one_tenth_max() {
    let enc = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(1.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 1000, "1.0/10.0 = 0.1 → 1000");
}

/// Verify that very small max_torque is clamped to 0.01.
#[test]
fn torque_max_torque_floor_protects_against_zero() {
    let enc = SimagicConstantForceEncoder::new(0.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.005, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    // 0.005 / 0.01 = 0.5 → 5000
    assert_eq!(mag, 5000, "zero max_torque should floor to 0.01");
}

/// Negative max_torque is also clamped to 0.01.
#[test]
fn torque_negative_max_torque_clamped() {
    let enc = SimagicConstantForceEncoder::new(-5.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode(0.01, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000, "negative max_torque floors to 0.01");
}

/// Saturation: torque exceeding max_torque saturates at ±10000.
#[test]
fn torque_saturation_positive_and_negative() {
    let enc = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

    enc.encode(999.0, &mut out);
    assert_eq!(i16::from_le_bytes([out[3], out[4]]), 10000);

    enc.encode(-999.0, &mut out);
    assert_eq!(i16::from_le_bytes([out[3], out[4]]), -10000);
}

/// Exact full-scale: max_torque → 10000, -max_torque → -10000.
#[test]
fn torque_exact_full_scale() {
    for max in [7.0f32, 9.0, 10.0, 12.0, 15.0, 18.0, 23.0, 25.0] {
        let enc = SimagicConstantForceEncoder::new(max);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];

        enc.encode(max, &mut out);
        assert_eq!(i16::from_le_bytes([out[3], out[4]]), 10000, "max={max}");

        enc.encode(-max, &mut out);
        assert_eq!(i16::from_le_bytes([out[3], out[4]]), -10000, "max={max}");
    }
}

/// Zero torque always produces zero magnitude regardless of max_torque.
#[test]
fn torque_zero_always_zero() {
    for max in [0.01f32, 1.0, 10.0, 50.0] {
        let enc = SimagicConstantForceEncoder::new(max);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(0.0, &mut out);
        assert_eq!(i16::from_le_bytes([out[3], out[4]]), 0, "max={max}");
    }
}

/// Torque encoding is symmetric: encode(x) == -encode(-x).
#[test]
fn torque_encoding_symmetric() {
    let enc = SimagicConstantForceEncoder::new(15.0);
    let test_values = [0.1f32, 1.0, 5.0, 7.5, 14.9, 15.0, 30.0];
    for val in test_values {
        let mut out_pos = [0u8; CONSTANT_FORCE_REPORT_LEN];
        let mut out_neg = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(val, &mut out_pos);
        enc.encode(-val, &mut out_neg);
        let mag_pos = i16::from_le_bytes([out_pos[3], out_pos[4]]);
        let mag_neg = i16::from_le_bytes([out_neg[3], out_neg[4]]);
        assert_eq!(mag_pos, -mag_neg, "symmetry failed for val={val}");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 5  Motor enable/disable patterns
// ═══════════════════════════════════════════════════════════════════════════════

/// encode_zero produces a report with magnitude bytes all zero (motor disable).
#[test]
fn motor_disable_via_encode_zero() {
    let enc = SimagicConstantForceEncoder::new(15.0);
    let mut out = [0xFFu8; CONSTANT_FORCE_REPORT_LEN];
    enc.encode_zero(&mut out);

    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
    assert_eq!(out[1], 1); // effect block index still present
    // Bytes 2-7 should be zero (magnitude + reserved)
    assert_eq!(&out[2..], &[0u8; 6]);
}

/// Spring encode_zero disables spring effect.
#[test]
fn spring_disable_via_encode_zero() {
    let enc = SimagicSpringEncoder::new(12.0);
    let mut out = [0xFFu8; SPRING_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], report_ids::SPRING_EFFECT);
    assert_eq!(
        u16::from_le_bytes([out[2], out[3]]),
        0,
        "strength must be 0"
    );
    assert_eq!(
        i16::from_le_bytes([out[4], out[5]]),
        0,
        "steering must be 0"
    );
    assert_eq!(i16::from_le_bytes([out[6], out[7]]), 0, "center must be 0");
    assert_eq!(
        u16::from_le_bytes([out[8], out[9]]),
        0,
        "deadzone must be 0"
    );
}

/// Damper encode_zero disables damper effect.
#[test]
fn damper_disable_via_encode_zero() {
    let enc = SimagicDamperEncoder::new(12.0);
    let mut out = [0xFFu8; DAMPER_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], report_ids::DAMPER_EFFECT);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 0);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 0);
}

/// Friction encode_zero disables friction effect.
#[test]
fn friction_disable_via_encode_zero() {
    let enc = SimagicFrictionEncoder::new(12.0);
    let mut out = [0xFFu8; FRICTION_REPORT_LEN];
    enc.encode_zero(&mut out);
    assert_eq!(out[0], report_ids::FRICTION_EFFECT);
    assert_eq!(u16::from_le_bytes([out[2], out[3]]), 0);
    assert_eq!(u16::from_le_bytes([out[4], out[5]]), 0);
}

/// Device gain 0x00 disables all FFB output.
#[test]
fn device_gain_zero_disables_ffb() {
    let report = simagic::build_device_gain(0x00);
    assert_eq!(report[0], report_ids::DEVICE_GAIN);
    assert_eq!(report[1], 0x00, "gain=0 means no FFB output");
    // Remaining bytes should be zero
    assert_eq!(&report[2..], &[0u8; 6]);
}

/// Device gain 0xFF enables full FFB output.
#[test]
fn device_gain_full_enables_ffb() {
    let report = simagic::build_device_gain(0xFF);
    assert_eq!(report[1], 0xFF);
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 6  Edge cases: boundary values, report size validation
// ═══════════════════════════════════════════════════════════════════════════════

/// Empty input report (0 bytes) returns None.
#[test]
fn input_empty_returns_none() {
    assert!(simagic::parse_input_report(&[]).is_none());
}

/// 16-byte report is too short (minimum is 17).
#[test]
fn input_16_bytes_too_short() {
    assert!(simagic::parse_input_report(&[0u8; 16]).is_none());
}

/// Exactly 17 bytes is the minimum valid report.
#[test]
fn input_exactly_17_bytes_valid() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0u8; 17];
    let state = simagic::parse_input_report(&data).ok_or("17 bytes should parse")?;
    assert_eq!(state.quick_release, QuickReleaseStatus::Unknown);
    assert!(state.firmware_version.is_none());
    Ok(())
}

/// 19 bytes: enough for basic fields but no quick release byte at offset 19.
#[test]
fn input_19_bytes_no_quick_release() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0u8; 19];
    let state = simagic::parse_input_report(&data).ok_or("19 bytes should parse")?;
    assert_eq!(state.quick_release, QuickReleaseStatus::Unknown);
    assert!(state.firmware_version.is_none());
    Ok(())
}

/// 20 bytes: quick release available but no firmware version.
#[test]
fn input_20_bytes_with_quick_release() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 20];
    data[19] = 1; // detached
    let state = simagic::parse_input_report(&data).ok_or("20 bytes should parse")?;
    assert_eq!(state.quick_release, QuickReleaseStatus::Detached);
    assert!(state.firmware_version.is_none());
    Ok(())
}

/// 23 bytes: firmware version available.
#[test]
fn input_23_bytes_with_firmware() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 23];
    data[20] = 3;
    data[21] = 14;
    data[22] = 159;
    let state = simagic::parse_input_report(&data).ok_or("23 bytes should parse")?;
    assert_eq!(state.firmware_version, Some((3, 14, 159)));
    Ok(())
}

/// Hat switch: upper nibble is masked off.
#[test]
fn input_hat_upper_nibble_masked() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![0u8; 64];
    data[12] = 0xF3; // upper nibble = 0xF, lower = 0x3
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x03, "upper nibble should be masked");
    Ok(())
}

/// Gear values 0–8 map to known gears; 9+ maps to Unknown.
#[test]
fn gear_raw_boundary_values() {
    assert_eq!(SimagicGear::from_raw(0), SimagicGear::Neutral);
    assert_eq!(SimagicGear::from_raw(1), SimagicGear::First);
    assert_eq!(SimagicGear::from_raw(8), SimagicGear::Eighth);
    assert_eq!(SimagicGear::from_raw(9), SimagicGear::Unknown);
    assert_eq!(SimagicGear::from_raw(0xFF), SimagicGear::Unknown);
}

/// Quick release status boundary values.
#[test]
fn quick_release_boundary_values() {
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
        QuickReleaseStatus::from_raw(0xFF),
        QuickReleaseStatus::Unknown
    );
}

/// Pedal normalization: u16::MAX raw → 1.0, 0 raw → 0.0.
#[test]
fn pedal_normalize_boundary() {
    let raw = SimagicPedalAxesRaw {
        throttle: u16::MAX,
        brake: 0,
        clutch: u16::MAX / 2,
        handbrake: 1,
    };
    let norm = raw.normalize();
    assert!((norm.throttle - 1.0).abs() < 0.0001);
    assert!(norm.brake.abs() < 0.0001);
    assert!((norm.clutch - 0.5).abs() < 0.001);
    assert!(norm.handbrake > 0.0 && norm.handbrake < 0.001);
}

/// Rotation range: encode u16 boundary values.
#[test]
fn rotation_range_boundary_values() {
    let r0 = simagic::build_rotation_range(0);
    assert_eq!(u16::from_le_bytes([r0[2], r0[3]]), 0);

    let r_max = simagic::build_rotation_range(u16::MAX);
    assert_eq!(u16::from_le_bytes([r_max[2], r_max[3]]), u16::MAX);
}

/// LED report boundary values.
#[test]
fn led_report_boundary_values() {
    let led_zero = simagic::build_led_report(0x00);
    assert_eq!(led_zero[1], 0x00);

    let led_max = simagic::build_led_report(0xFF);
    assert_eq!(led_max[1], 0xFF);
}

/// Report sizes are correct constants.
#[test]
fn report_size_constants() {
    assert_eq!(CONSTANT_FORCE_REPORT_LEN, 8);
    assert_eq!(SPRING_REPORT_LEN, 10);
    assert_eq!(DAMPER_REPORT_LEN, 8);
    assert_eq!(FRICTION_REPORT_LEN, 10);
}

/// Encoder returned length matches the report length constant.
#[test]
fn encoder_returned_lengths() {
    let cf_enc = SimagicConstantForceEncoder::new(10.0);
    let mut cf_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    assert_eq!(cf_enc.encode(1.0, &mut cf_out), CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(cf_enc.encode_zero(&mut cf_out), CONSTANT_FORCE_REPORT_LEN);

    let sp_enc = SimagicSpringEncoder::new(10.0);
    let mut sp_out = [0u8; SPRING_REPORT_LEN];
    assert_eq!(sp_enc.encode(100, 0, 0, 0, &mut sp_out), SPRING_REPORT_LEN);
    assert_eq!(sp_enc.encode_zero(&mut sp_out), SPRING_REPORT_LEN);

    let dm_enc = SimagicDamperEncoder::new(10.0);
    let mut dm_out = [0u8; DAMPER_REPORT_LEN];
    assert_eq!(dm_enc.encode(100, 100, &mut dm_out), DAMPER_REPORT_LEN);
    assert_eq!(dm_enc.encode_zero(&mut dm_out), DAMPER_REPORT_LEN);

    let fr_enc = SimagicFrictionEncoder::new(10.0);
    let mut fr_out = [0u8; FRICTION_REPORT_LEN];
    assert_eq!(fr_enc.encode(100, 100, &mut fr_out), FRICTION_REPORT_LEN);
    assert_eq!(fr_enc.encode_zero(&mut fr_out), FRICTION_REPORT_LEN);
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 7  Property tests for torque encoding round-trips
// ═══════════════════════════════════════════════════════════════════════════════

mod prop_round_trips {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Torque within range round-trips with at most 1 LSB error.
        #[test]
        fn prop_constant_force_round_trip(
            torque_frac in -1.0f32..=1.0f32,
            max_torque in 1.0f32..=50.0f32,
        ) {
            let torque = torque_frac * max_torque;
            let enc = SimagicConstantForceEncoder::new(max_torque);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            enc.encode(torque, &mut out);
            let raw = i16::from_le_bytes([out[3], out[4]]);
            let decoded = raw as f32 / 10_000.0 * max_torque;
            let error = (torque - decoded).abs();
            let lsb = max_torque / 10_000.0;
            prop_assert!(error < lsb + 0.001,
                "torque={torque} decoded={decoded} error={error} lsb={lsb}");
        }

        /// Pedal raw → normalize → pedal_axes_raw round-trip within 1 LSB.
        #[test]
        fn prop_pedal_normalize_round_trip(
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
            handbrake in 0u16..=65535u16,
        ) {
            let raw = SimagicPedalAxesRaw { throttle, brake, clutch, handbrake };
            let norm = raw.normalize();
            // Each normalized value must be in [0.0, 1.0]
            prop_assert!(norm.throttle >= 0.0 && norm.throttle <= 1.0);
            prop_assert!(norm.brake >= 0.0 && norm.brake <= 1.0);
            prop_assert!(norm.clutch >= 0.0 && norm.clutch <= 1.0);
            prop_assert!(norm.handbrake >= 0.0 && norm.handbrake <= 1.0);

            // Reverse: normalized * u16::MAX should be within ±1 of original
            let rt_throttle = (norm.throttle * u16::MAX as f32) as u16;
            let diff = (rt_throttle as i32 - throttle as i32).unsigned_abs();
            prop_assert!(diff <= 1, "throttle round-trip diff={diff}");
        }

        /// Spring coefficient round-trips exactly through encode/decode.
        #[test]
        fn prop_spring_fields_round_trip(
            strength in 0u16..=u16::MAX,
            steering in i16::MIN..=i16::MAX,
            center in i16::MIN..=i16::MAX,
            deadzone in 0u16..=u16::MAX,
        ) {
            let enc = SimagicSpringEncoder::new(10.0);
            let mut out = [0u8; SPRING_REPORT_LEN];
            enc.encode(strength, steering, center, deadzone, &mut out);

            prop_assert_eq!(u16::from_le_bytes([out[2], out[3]]), strength);
            prop_assert_eq!(i16::from_le_bytes([out[4], out[5]]), steering);
            prop_assert_eq!(i16::from_le_bytes([out[6], out[7]]), center);
            prop_assert_eq!(u16::from_le_bytes([out[8], out[9]]), deadzone);
        }

        /// Any valid input report (≥17 bytes) parses successfully.
        #[test]
        fn prop_valid_length_always_parses(len in 17usize..=256usize) {
            let data = vec![0u8; len];
            prop_assert!(simagic::parse_input_report(&data).is_some(),
                "report of len={len} should parse");
        }

        /// Reports shorter than 17 bytes always return None.
        #[test]
        fn prop_short_report_always_none(len in 0usize..17usize) {
            let data = vec![0u8; len];
            prop_assert!(simagic::parse_input_report(&data).is_none(),
                "report of len={len} should not parse");
        }

        /// Gear from_raw for 0-8 always returns a non-Unknown variant.
        #[test]
        fn prop_valid_gear_never_unknown(gear in 0u8..=8u8) {
            prop_assert_ne!(SimagicGear::from_raw(gear), SimagicGear::Unknown);
        }

        /// Gear from_raw for 9+ always returns Unknown.
        #[test]
        fn prop_invalid_gear_always_unknown(gear in 9u8..=255u8) {
            prop_assert_eq!(SimagicGear::from_raw(gear), SimagicGear::Unknown);
        }

        /// Rotation range round-trips: encoded degrees can be decoded back.
        #[test]
        fn prop_rotation_range_round_trip(degrees in 0u16..=u16::MAX) {
            let report = simagic::build_rotation_range(degrees);
            let decoded = u16::from_le_bytes([report[2], report[3]]);
            prop_assert_eq!(decoded, degrees);
        }

        /// Device gain round-trips.
        #[test]
        fn prop_device_gain_round_trip(gain in 0u8..=255u8) {
            let report = simagic::build_device_gain(gain);
            prop_assert_eq!(report[1], gain);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 8  Known constant validation
// ═══════════════════════════════════════════════════════════════════════════════

/// Vendor IDs match documented values.
#[test]
fn const_vendor_ids() {
    assert_eq!(SIMAGIC_VENDOR_ID, 0x3670);
    assert_eq!(SIMAGIC_LEGACY_VENDOR_ID, 0x0483);
}

/// Legacy PID matches documented value.
#[test]
fn const_legacy_pid() {
    assert_eq!(SIMAGIC_LEGACY_PID, 0x0522);
}

/// Report ID constants match their documented values.
#[test]
fn const_report_ids() {
    assert_eq!(report_ids::STANDARD_INPUT, 0x01);
    assert_eq!(report_ids::EXTENDED_INPUT, 0x02);
    assert_eq!(report_ids::DEVICE_INFO, 0x03);
    assert_eq!(report_ids::CONSTANT_FORCE, 0x11);
    assert_eq!(report_ids::SPRING_EFFECT, 0x12);
    assert_eq!(report_ids::DAMPER_EFFECT, 0x13);
    assert_eq!(report_ids::FRICTION_EFFECT, 0x14);
    assert_eq!(report_ids::SINE_EFFECT, 0x15);
    assert_eq!(report_ids::SQUARE_EFFECT, 0x16);
    assert_eq!(report_ids::TRIANGLE_EFFECT, 0x17);
    assert_eq!(report_ids::ROTATION_RANGE, 0x20);
    assert_eq!(report_ids::DEVICE_GAIN, 0x21);
    assert_eq!(report_ids::LED_CONTROL, 0x30);
    assert_eq!(report_ids::QUICK_RELEASE_STATUS, 0x40);
}

/// FFB effect type report IDs match their documented values.
#[test]
fn const_ffb_effect_report_ids() {
    assert_eq!(SimagicFfbEffectType::Constant.report_id(), 0x11);
    assert_eq!(SimagicFfbEffectType::Spring.report_id(), 0x12);
    assert_eq!(SimagicFfbEffectType::Damper.report_id(), 0x13);
    assert_eq!(SimagicFfbEffectType::Friction.report_id(), 0x14);
    assert_eq!(SimagicFfbEffectType::Sine.report_id(), 0x15);
    assert_eq!(SimagicFfbEffectType::Square.report_id(), 0x16);
    assert_eq!(SimagicFfbEffectType::Triangle.report_id(), 0x17);
}

/// FFB effect type report IDs align with the report_ids module constants.
#[test]
fn const_effect_type_matches_report_ids_module() {
    assert_eq!(
        SimagicFfbEffectType::Constant.report_id(),
        report_ids::CONSTANT_FORCE
    );
    assert_eq!(
        SimagicFfbEffectType::Spring.report_id(),
        report_ids::SPRING_EFFECT
    );
    assert_eq!(
        SimagicFfbEffectType::Damper.report_id(),
        report_ids::DAMPER_EFFECT
    );
    assert_eq!(
        SimagicFfbEffectType::Friction.report_id(),
        report_ids::FRICTION_EFFECT
    );
    assert_eq!(
        SimagicFfbEffectType::Sine.report_id(),
        report_ids::SINE_EFFECT
    );
    assert_eq!(
        SimagicFfbEffectType::Square.report_id(),
        report_ids::SQUARE_EFFECT
    );
    assert_eq!(
        SimagicFfbEffectType::Triangle.report_id(),
        report_ids::TRIANGLE_EFFECT
    );
}

/// All known EVO-generation product IDs are distinct.
#[test]
fn const_evo_pids_distinct() {
    let pids = [
        product_ids::EVO_SPORT,
        product_ids::EVO,
        product_ids::EVO_PRO,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(pids[i], pids[j], "EVO PIDs at index {i} and {j} collide");
        }
    }
}

/// Report IDs do not collide with each other.
#[test]
fn const_report_ids_no_collision() {
    let ids = [
        report_ids::STANDARD_INPUT,
        report_ids::EXTENDED_INPUT,
        report_ids::DEVICE_INFO,
        report_ids::CONSTANT_FORCE,
        report_ids::SPRING_EFFECT,
        report_ids::DAMPER_EFFECT,
        report_ids::FRICTION_EFFECT,
        report_ids::SINE_EFFECT,
        report_ids::SQUARE_EFFECT,
        report_ids::TRIANGLE_EFFECT,
        report_ids::ROTATION_RANGE,
        report_ids::DEVICE_GAIN,
        report_ids::LED_CONTROL,
        report_ids::QUICK_RELEASE_STATUS,
    ];
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "report IDs at index {i} and {j} collide");
        }
    }
}

/// SimagicModel max_torque values match identify_device for EVO wheelbases.
#[test]
fn const_model_torque_matches_identity() -> Result<(), Box<dyn std::error::Error>> {
    let wheelbases = [
        (product_ids::EVO_SPORT, SimagicModel::EvoSport),
        (product_ids::EVO, SimagicModel::Evo),
        (product_ids::EVO_PRO, SimagicModel::EvoPro),
    ];
    for (pid, model) in wheelbases {
        let identity = simagic::identify_device(pid);
        let identity_torque = identity.max_torque_nm.ok_or("no torque")?;
        let model_torque = model.max_torque_nm();
        assert!(
            (identity_torque - model_torque).abs() < 0.01,
            "PID {pid:#06x}: identity={identity_torque} model={model_torque}"
        );
    }
    Ok(())
}
