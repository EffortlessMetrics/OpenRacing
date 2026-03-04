//! Deep protocol coverage tests for the Simagic HID protocol crate.
//!
//! Covers: all command types, all report types, state machine transitions
//! (gear/QR/hat), error handling, encoder edge cases, and round-trip validation.
//!
//! # Sources
//!
//! - JacKeTUs/simagic-ff `hid-simagic.c` (GPL-2.0): real wire protocol
//! - JacKeTUs/simagic-ff `hid-simagic.h`: VID/PID and struct definitions
//! - JacKeTUs/linux-steering-wheels: compatibility table
//! - JacKeTUs/simracing-hwdb: udev database (handbrake PID)

use racing_wheel_hid_simagic_protocol::{
    self as simagic, CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN,
    SPRING_REPORT_LEN, SimagicConstantForceEncoder, SimagicDamperEncoder, SimagicFrictionEncoder,
    SimagicSpringEncoder,
    ids::{product_ids, report_ids},
    types::{
        QuickReleaseStatus, SimagicDeviceCategory, SimagicFfbEffectType, SimagicGear, SimagicModel,
        SimagicPedalAxesRaw,
    },
};

// ═══════════════════════════════════════════════════════════════════════════════
// § 1  All command types — output report construction
// ═══════════════════════════════════════════════════════════════════════════════

// ── 1.1  Constant force command ─────────────────────────────────────────────

#[test]
fn constant_force_full_positive() {
    let enc = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode(10.0, &mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
    assert_eq!(out[1], 1);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 10000);
}

#[test]
fn constant_force_full_negative() {
    let enc = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let _ = enc.encode(-10.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -10000);
}

#[test]
fn constant_force_zero() {
    let enc = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let len = enc.encode_zero(&mut out);
    assert_eq!(len, CONSTANT_FORCE_REPORT_LEN);
    assert_eq!(out[0], report_ids::CONSTANT_FORCE);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 0);
}

#[test]
fn constant_force_quarter_torque() {
    let enc = SimagicConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let _ = enc.encode(5.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, 2500, "5/20 = 0.25 → 2500");
}

#[test]
fn constant_force_three_quarter_negative() {
    let enc = SimagicConstantForceEncoder::new(20.0);
    let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let _ = enc.encode(-15.0, &mut out);
    let mag = i16::from_le_bytes([out[3], out[4]]);
    assert_eq!(mag, -7500, "-15/20 = -0.75 → -7500");
}

#[test]
fn constant_force_encode_clears_buffer() {
    let enc = SimagicConstantForceEncoder::new(10.0);
    let mut out = [0xFF_u8; CONSTANT_FORCE_REPORT_LEN];
    let _ = enc.encode(0.0, &mut out);
    // Reserved bytes should be zeroed
    assert_eq!(out[5], 0);
    assert_eq!(out[6], 0);
    assert_eq!(out[7], 0);
}

// ── 1.2  Spring effect command ──────────────────────────────────────────────

#[test]
fn spring_effect_full_parameters() {
    let enc = SimagicSpringEncoder::new(15.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let len = enc.encode(1000, 16000, -500, 100, &mut out);
    assert_eq!(len, SPRING_REPORT_LEN);
    assert_eq!(out[0], report_ids::SPRING_EFFECT);
    assert_eq!(out[1], 1);

    let strength = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(strength, 1000);

    let steering = i16::from_le_bytes([out[4], out[5]]);
    assert_eq!(steering, 16000);

    let center = i16::from_le_bytes([out[6], out[7]]);
    assert_eq!(center, -500);

    let deadzone = u16::from_le_bytes([out[8], out[9]]);
    assert_eq!(deadzone, 100);
}

#[test]
fn spring_effect_zero_disables() {
    let enc = SimagicSpringEncoder::new(15.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let len = enc.encode_zero(&mut out);
    assert_eq!(len, SPRING_REPORT_LEN);
    assert_eq!(out[0], report_ids::SPRING_EFFECT);

    let strength = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(strength, 0);
    let deadzone = u16::from_le_bytes([out[8], out[9]]);
    assert_eq!(deadzone, 0);
}

#[test]
fn spring_effect_negative_steering() {
    let enc = SimagicSpringEncoder::new(10.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let _ = enc.encode(500, -32768, 0, 50, &mut out);

    let steering = i16::from_le_bytes([out[4], out[5]]);
    assert_eq!(steering, -32768, "must handle i16::MIN");
}

#[test]
fn spring_effect_max_steering() {
    let enc = SimagicSpringEncoder::new(10.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let _ = enc.encode(500, 32767, 0, 50, &mut out);

    let steering = i16::from_le_bytes([out[4], out[5]]);
    assert_eq!(steering, 32767, "must handle i16::MAX");
}

// ── 1.3  Damper effect command ──────────────────────────────────────────────

#[test]
fn damper_effect_full_parameters() {
    let enc = SimagicDamperEncoder::new(15.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    let len = enc.encode(800, 7500, &mut out);
    assert_eq!(len, DAMPER_REPORT_LEN);
    assert_eq!(out[0], report_ids::DAMPER_EFFECT);
    assert_eq!(out[1], 1);

    let strength = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(strength, 800);

    let velocity = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(velocity, 7500);
}

#[test]
fn damper_effect_zero_disables() {
    let enc = SimagicDamperEncoder::new(15.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    let len = enc.encode_zero(&mut out);
    assert_eq!(len, DAMPER_REPORT_LEN);
    let strength = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(strength, 0);
}

#[test]
fn damper_effect_max_values() {
    let enc = SimagicDamperEncoder::new(10.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    let _ = enc.encode(u16::MAX, u16::MAX, &mut out);

    let strength = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(strength, u16::MAX);

    let velocity = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(velocity, u16::MAX);
}

// ── 1.4  Friction effect command ────────────────────────────────────────────

#[test]
fn friction_effect_full_parameters() {
    let enc = SimagicFrictionEncoder::new(15.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    let len = enc.encode(600, 3000, &mut out);
    assert_eq!(len, FRICTION_REPORT_LEN);
    assert_eq!(out[0], report_ids::FRICTION_EFFECT);
    assert_eq!(out[1], 1);

    let coefficient = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(coefficient, 600);

    let velocity = u16::from_le_bytes([out[4], out[5]]);
    assert_eq!(velocity, 3000);
}

#[test]
fn friction_effect_zero_disables() {
    let enc = SimagicFrictionEncoder::new(15.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    let len = enc.encode_zero(&mut out);
    assert_eq!(len, FRICTION_REPORT_LEN);
    let coefficient = u16::from_le_bytes([out[2], out[3]]);
    assert_eq!(coefficient, 0);
}

#[test]
fn friction_effect_reserved_bytes_zeroed() {
    let enc = SimagicFrictionEncoder::new(10.0);
    let mut out = [0xFF_u8; FRICTION_REPORT_LEN];
    let _ = enc.encode(500, 2000, &mut out);
    assert_eq!(out[6], 0, "reserved bytes must be zeroed");
    assert_eq!(out[7], 0);
    assert_eq!(out[8], 0);
    assert_eq!(out[9], 0);
}

// ── 1.5  Rotation range command ─────────────────────────────────────────────

#[test]
fn rotation_range_common_values() {
    let test_cases: &[(u16, &str)] = &[
        (180, "F1-style 180°"),
        (360, "Rally 360°"),
        (540, "GT 540°"),
        (900, "Standard 900°"),
        (1080, "Extended 1080°"),
        (1440, "Truck 1440°"),
        (2520, "Maximum 2520°"),
    ];

    for &(degrees, label) in test_cases {
        let report = simagic::build_rotation_range(degrees);
        assert_eq!(report[0], report_ids::ROTATION_RANGE, "{label}");
        assert_eq!(report[1], 0x00, "{label}");
        let reported = u16::from_le_bytes([report[2], report[3]]);
        assert_eq!(reported, degrees, "{label}");
    }
}

#[test]
fn rotation_range_zero() {
    let report = simagic::build_rotation_range(0);
    assert_eq!(report[0], report_ids::ROTATION_RANGE);
    let reported = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(reported, 0);
}

#[test]
fn rotation_range_max_u16() {
    let report = simagic::build_rotation_range(u16::MAX);
    let reported = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(reported, u16::MAX);
}

// ── 1.6  Device gain command ────────────────────────────────────────────────

#[test]
fn device_gain_full_range() {
    let test_cases: &[(u8, &str)] = &[
        (0x00, "0% gain"),
        (0x40, "25% gain"),
        (0x80, "50% gain"),
        (0xBF, "~75% gain"),
        (0xFF, "100% gain"),
    ];

    for &(gain, label) in test_cases {
        let report = simagic::build_device_gain(gain);
        assert_eq!(report[0], report_ids::DEVICE_GAIN, "{label}");
        assert_eq!(report[1], gain, "{label}");
        // Remaining bytes must be zero
        for (i, &byte) in report[2..].iter().enumerate() {
            assert_eq!(byte, 0, "{label}: byte {i} not zero");
        }
    }
}

// ── 1.7  LED control command ────────────────────────────────────────────────

#[test]
fn led_control_all_patterns() {
    for pattern in [0x00_u8, 0x01, 0x0F, 0x55, 0xAA, 0xFF] {
        let report = simagic::build_led_report(pattern);
        assert_eq!(report[0], report_ids::LED_CONTROL);
        assert_eq!(report[1], pattern);
        for &byte in &report[2..] {
            assert_eq!(byte, 0, "pattern {pattern:#04x}: reserved byte not zero");
        }
    }
}

// ── 1.8  Sine effect command ────────────────────────────────────────────────

#[test]
fn sine_effect_standard() {
    let report = simagic::build_sine_effect(500, 5.0, 180);
    assert_eq!(report[0], report_ids::SINE_EFFECT);
    assert_eq!(report[1], 0x01, "effect block index");

    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 500);

    // 5.0 Hz * 100 = 500
    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 500);

    let phase = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(phase, 180);
}

#[test]
fn sine_effect_frequency_clamping() {
    // Below minimum (0.1 Hz)
    let report = simagic::build_sine_effect(100, 0.01, 0);
    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 10, "0.1 Hz * 100 = 10");

    // Above maximum (20 Hz)
    let report = simagic::build_sine_effect(100, 100.0, 0);
    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 2000, "20.0 Hz * 100 = 2000");
}

#[test]
fn sine_effect_zero_amplitude() {
    let report = simagic::build_sine_effect(0, 1.0, 0);
    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 0);
}

// ── 1.9  Square effect command ──────────────────────────────────────────────

#[test]
fn square_effect_standard() {
    let report = simagic::build_square_effect(750, 3.0, 50);
    assert_eq!(report[0], report_ids::SQUARE_EFFECT);
    assert_eq!(report[1], 0x01);

    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 750);

    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 300, "3.0 Hz * 100 = 300");

    let duty = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(duty, 50);
}

#[test]
fn square_effect_duty_cycle_clamped() {
    let report = simagic::build_square_effect(100, 1.0, 200);
    let duty = u16::from_le_bytes([report[6], report[7]]);
    assert_eq!(duty, 100, "duty cycle must be clamped to 100");
}

// ── 1.10  Triangle effect command ───────────────────────────────────────────

#[test]
fn triangle_effect_standard() {
    let report = simagic::build_triangle_effect(400, 2.5);
    assert_eq!(report[0], report_ids::TRIANGLE_EFFECT);
    assert_eq!(report[1], 0x01);

    let amplitude = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(amplitude, 400);

    let freq = u16::from_le_bytes([report[4], report[5]]);
    assert_eq!(freq, 250, "2.5 Hz * 100 = 250");

    // Reserved bytes must be zero
    assert_eq!(report[6], 0);
    assert_eq!(report[7], 0);
    assert_eq!(report[8], 0);
    assert_eq!(report[9], 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 2  All report types — input report parsing
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper to build a 64-byte input report buffer.
fn make_input(f: impl FnOnce(&mut [u8; 64])) -> [u8; 64] {
    let mut data = [0u8; 64];
    f(&mut data);
    data
}

// ── 2.1  Standard input report ──────────────────────────────────────────────

#[test]
fn parse_standard_input_all_center() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_input(|d| {
        d[0..2].copy_from_slice(&0x8000_u16.to_le_bytes()); // steering center
    });
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.steering.abs() < 0.001);
    assert!(state.throttle.abs() < 0.001);
    assert!(state.brake.abs() < 0.001);
    assert!(state.clutch.abs() < 0.001);
    assert!(state.handbrake.abs() < 0.001);
    Ok(())
}

#[test]
fn parse_standard_input_all_max() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_input(|d| {
        d[0..2].copy_from_slice(&0xFFFF_u16.to_le_bytes()); // full right
        d[2..4].copy_from_slice(&0xFFFF_u16.to_le_bytes()); // full throttle
        d[4..6].copy_from_slice(&0xFFFF_u16.to_le_bytes()); // full brake
        d[6..8].copy_from_slice(&0xFFFF_u16.to_le_bytes()); // full clutch
        d[8..10].copy_from_slice(&0xFFFF_u16.to_le_bytes()); // full handbrake
        d[10..12].copy_from_slice(&0xFFFF_u16.to_le_bytes()); // all buttons
    });
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!((state.steering - 1.0).abs() < 0.001);
    assert!((state.throttle - 1.0).abs() < 0.001);
    assert!((state.brake - 1.0).abs() < 0.001);
    assert!((state.clutch - 1.0).abs() < 0.001);
    assert!((state.handbrake - 1.0).abs() < 0.001);
    assert_eq!(state.buttons, 0xFFFF);
    Ok(())
}

// ── 2.2  Button report ──────────────────────────────────────────────────────

#[test]
fn parse_individual_buttons() -> Result<(), Box<dyn std::error::Error>> {
    for bit in 0..16_u16 {
        let data = make_input(|d| {
            let mask = 1_u16 << bit;
            d[10..12].copy_from_slice(&mask.to_le_bytes());
        });
        let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!((state.buttons >> bit) & 1, 1, "button {bit} should be set");
        // All other buttons should be clear
        assert_eq!(
            state.buttons & !(1 << bit),
            0,
            "only button {bit} should be set"
        );
    }
    Ok(())
}

#[test]
fn parse_no_buttons() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_input(|_| {});
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.buttons, 0);
    Ok(())
}

// ── 2.3  Hat switch report ──────────────────────────────────────────────────

#[test]
fn parse_hat_all_directions() -> Result<(), Box<dyn std::error::Error>> {
    // Standard USB HID hat encoding: 0=N, 1=NE, 2=E, ..., 7=NW, 8=neutral
    let directions: &[(u8, &str)] = &[
        (0x00, "North"),
        (0x01, "NorthEast"),
        (0x02, "East"),
        (0x03, "SouthEast"),
        (0x04, "South"),
        (0x05, "SouthWest"),
        (0x06, "West"),
        (0x07, "NorthWest"),
        (0x08, "Neutral"),
    ];

    for &(value, label) in directions {
        let data = make_input(|d| {
            d[12] = value;
        });
        let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.hat, value, "hat direction {label}");
    }
    Ok(())
}

#[test]
fn parse_hat_masked_to_nibble() -> Result<(), Box<dyn std::error::Error>> {
    // Hat is masked with 0x0F, so upper nibble should be ignored
    let data = make_input(|d| {
        d[12] = 0xF2; // upper nibble = 0xF, lower nibble = 2 (East)
    });
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.hat, 0x02, "upper nibble must be masked off");
    Ok(())
}

// ── 2.4  Rotary encoder report ──────────────────────────────────────────────

#[test]
fn parse_rotary_encoders_full_range() -> Result<(), Box<dyn std::error::Error>> {
    let test_values: &[(u8, u8)] = &[(0, 0), (1, 255), (128, 128), (255, 0)];

    for &(r1, r2) in test_values {
        let data = make_input(|d| {
            d[13] = r1;
            d[14] = r2;
        });
        let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
        assert_eq!(state.rotary1, r1);
        assert_eq!(state.rotary2, r2);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 3  State machine transitions
// ═══════════════════════════════════════════════════════════════════════════════

// ── 3.1  Gear state transitions ─────────────────────────────────────────────

#[test]
fn gear_transitions_sequential_up() {
    let gears = [
        (0, SimagicGear::Neutral),
        (1, SimagicGear::First),
        (2, SimagicGear::Second),
        (3, SimagicGear::Third),
        (4, SimagicGear::Fourth),
        (5, SimagicGear::Fifth),
        (6, SimagicGear::Sixth),
        (7, SimagicGear::Seventh),
        (8, SimagicGear::Eighth),
    ];

    for &(raw, expected) in &gears {
        assert_eq!(SimagicGear::from_raw(raw), expected);
    }
}

#[test]
fn gear_transition_neutral_to_first_and_back() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];

    data[15] = 0; // neutral
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.shifter.gear, SimagicGear::Neutral);

    data[15] = 1; // shift to first
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.shifter.gear, SimagicGear::First);

    data[15] = 0; // back to neutral
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.shifter.gear, SimagicGear::Neutral);
    Ok(())
}

#[test]
fn gear_unknown_values() {
    // Values > 8 should be Unknown
    for raw in [9_u8, 10, 50, 127, 200, 255] {
        assert_eq!(
            SimagicGear::from_raw(raw),
            SimagicGear::Unknown,
            "raw value {raw} must be Unknown"
        );
    }
}

// ── 3.2  Quick release state transitions ────────────────────────────────────

#[test]
fn quick_release_transitions() {
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

#[test]
fn quick_release_attach_detach_cycle() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];

    // Attached
    data[19] = 0;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.quick_release, QuickReleaseStatus::Attached);

    // Detach
    data[19] = 1;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.quick_release, QuickReleaseStatus::Detached);

    // Re-attach
    data[19] = 0;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.quick_release, QuickReleaseStatus::Attached);
    Ok(())
}

// ── 3.3  Shifter flag transitions ───────────────────────────────────────────

#[test]
fn shifter_flags_individual() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];

    // Clutch in range only
    data[16] = 0x01;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.shifter.clutch_in_range);
    assert!(!state.shifter.sequential_up_pressed);
    assert!(!state.shifter.sequential_down_pressed);

    // Sequential up only
    data[16] = 0x02;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state.shifter.clutch_in_range);
    assert!(state.shifter.sequential_up_pressed);
    assert!(!state.shifter.sequential_down_pressed);

    // Sequential down only
    data[16] = 0x04;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state.shifter.clutch_in_range);
    assert!(!state.shifter.sequential_up_pressed);
    assert!(state.shifter.sequential_down_pressed);

    // No flags
    data[16] = 0x00;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state.shifter.clutch_in_range);
    assert!(!state.shifter.sequential_up_pressed);
    assert!(!state.shifter.sequential_down_pressed);

    // All flags
    data[16] = 0x07;
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.shifter.clutch_in_range);
    assert!(state.shifter.sequential_up_pressed);
    assert!(state.shifter.sequential_down_pressed);
    Ok(())
}

#[test]
fn shifter_flags_ignore_upper_bits() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[16] = 0xF8; // upper 5 bits set, lower 3 clear
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(!state.shifter.clutch_in_range);
    assert!(!state.shifter.sequential_up_pressed);
    assert!(!state.shifter.sequential_down_pressed);
    Ok(())
}

// ── 3.4  Firmware version transitions ───────────────────────────────────────

#[test]
fn firmware_version_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_input(|d| {
        d[20] = 2; // major
        d[21] = 15; // minor
        d[22] = 99; // patch
    });
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.firmware_version, Some((2, 15, 99)));
    Ok(())
}

#[test]
fn firmware_version_zero() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_input(|_| {});
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.firmware_version, Some((0, 0, 0)));
    Ok(())
}

#[test]
fn firmware_version_max_values() -> Result<(), Box<dyn std::error::Error>> {
    let data = make_input(|d| {
        d[20] = 255;
        d[21] = 255;
        d[22] = 255;
    });
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert_eq!(state.firmware_version, Some((255, 255, 255)));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 4  Error handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_empty_report_returns_none() {
    assert!(simagic::parse_input_report(&[]).is_none());
}

#[test]
fn parse_single_byte_returns_none() {
    assert!(simagic::parse_input_report(&[0x01]).is_none());
}

#[test]
fn parse_16_bytes_returns_none() {
    let data = [0u8; 16];
    assert!(simagic::parse_input_report(&data).is_none());
}

#[test]
fn parse_17_bytes_returns_some() {
    let data = [0u8; 17];
    assert!(simagic::parse_input_report(&data).is_some());
}

#[test]
fn parse_oversized_report_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    // 256 bytes — much larger than standard 64-byte report
    let data = vec![0u8; 256];
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    assert!(state.steering.is_finite());
    Ok(())
}

#[test]
fn unknown_pid_identification_graceful() {
    let identity = simagic::identify_device(0x0000);
    assert_eq!(identity.category, SimagicDeviceCategory::Unknown);
    assert!(!identity.supports_ffb);

    let identity = simagic::identify_device(0xFFFF);
    assert_eq!(identity.category, SimagicDeviceCategory::Unknown);
}

#[test]
fn model_from_unknown_pid_graceful() {
    let model = SimagicModel::from_pid(0x0000);
    assert_eq!(model, SimagicModel::Unknown);
    // Unknown model should still return a safe torque value
    assert!(model.max_torque_nm() >= 0.0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 5  Pedal axes round-trip and normalization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn pedal_axes_raw_round_trip() {
    let raw = SimagicPedalAxesRaw {
        throttle: 32768,
        brake: 16384,
        clutch: 49152,
        handbrake: 0,
    };

    let normalized = raw.normalize();
    assert!((normalized.throttle - 0.5).abs() < 0.001);
    assert!((normalized.brake - 0.25).abs() < 0.001);
    assert!((normalized.clutch - 0.75).abs() < 0.001);
    assert!(normalized.handbrake.abs() < 0.001);
}

#[test]
fn pedal_axes_raw_full_range() {
    let min = SimagicPedalAxesRaw {
        throttle: 0,
        brake: 0,
        clutch: 0,
        handbrake: 0,
    };
    let max = SimagicPedalAxesRaw {
        throttle: u16::MAX,
        brake: u16::MAX,
        clutch: u16::MAX,
        handbrake: u16::MAX,
    };

    let norm_min = min.normalize();
    assert!(norm_min.throttle.abs() < 0.001);

    let norm_max = max.normalize();
    assert!((norm_max.throttle - 1.0).abs() < 0.001);
}

#[test]
fn pedal_axes_raw_from_input_state() {
    let state = simagic::SimagicInputState {
        throttle: 1.0,
        brake: 0.0,
        clutch: 0.5,
        handbrake: 0.25,
        ..Default::default()
    };
    let raw = state.pedal_axes_raw();
    assert_eq!(raw.throttle, u16::MAX);
    assert_eq!(raw.brake, 0);
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 6  SimagicModel coverage
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_models_have_non_negative_torque() {
    let models = [
        SimagicModel::EvoSport,
        SimagicModel::Evo,
        SimagicModel::EvoPro,
        SimagicModel::AlphaEvo,
        SimagicModel::Neo,
        SimagicModel::NeoMini,
        SimagicModel::P1000,
        SimagicModel::P2000,
        SimagicModel::ShifterH,
        SimagicModel::ShifterSeq,
        SimagicModel::Handbrake,
        SimagicModel::Unknown,
    ];

    for model in models {
        assert!(
            model.max_torque_nm() >= 0.0,
            "{model:?} has negative torque"
        );
    }
}

#[test]
fn wheelbase_models_have_positive_torque() {
    let wheelbases = [
        SimagicModel::EvoSport,
        SimagicModel::Evo,
        SimagicModel::EvoPro,
        SimagicModel::AlphaEvo,
        SimagicModel::Neo,
        SimagicModel::NeoMini,
    ];

    for model in wheelbases {
        assert!(
            model.max_torque_nm() > 0.0,
            "{model:?} should have positive torque"
        );
    }
}

#[test]
fn accessory_models_have_zero_torque() {
    let accessories = [
        SimagicModel::P1000,
        SimagicModel::P2000,
        SimagicModel::ShifterH,
        SimagicModel::ShifterSeq,
        SimagicModel::Handbrake,
    ];

    for model in accessories {
        assert!(
            (model.max_torque_nm()).abs() < f32::EPSILON,
            "{model:?} should have zero torque"
        );
    }
}

#[test]
fn model_from_pid_all_verified_pids() {
    let confirmed: &[(u16, SimagicModel)] = &[
        (product_ids::EVO_SPORT, SimagicModel::EvoSport),
        (product_ids::EVO, SimagicModel::Evo),
        (product_ids::EVO_PRO, SimagicModel::EvoPro),
        (product_ids::HANDBRAKE, SimagicModel::Handbrake),
    ];

    for &(pid, expected) in confirmed {
        assert_eq!(SimagicModel::from_pid(pid), expected, "PID {pid:#06x}");
    }

    // Fabricated PIDs now resolve to Unknown
    let fabricated = [
        product_ids::ALPHA_EVO,
        product_ids::NEO,
        product_ids::NEO_MINI,
        product_ids::P1000_PEDALS,
        product_ids::P1000A_PEDALS,
        product_ids::P2000_PEDALS,
        product_ids::SHIFTER_H,
        product_ids::SHIFTER_SEQ,
    ];
    for pid in fabricated {
        assert_eq!(
            SimagicModel::from_pid(pid),
            SimagicModel::Unknown,
            "fabricated PID {pid:#06x} should resolve to Unknown"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 7  Report ID uniqueness and allocation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_report_ids_unique() {
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
            assert_ne!(ids[i], ids[j], "duplicate report ID {:#04x}", ids[i]);
        }
    }
}

#[test]
fn report_ids_non_zero() {
    // Report ID 0x00 is reserved in HID spec
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

    for &id in &ids {
        assert_ne!(id, 0x00, "report ID 0x00 is reserved in HID spec");
    }
}

#[test]
fn effect_type_report_ids_match_report_id_constants() {
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

// ═══════════════════════════════════════════════════════════════════════════════
// § 8  Encoder constructor safety
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn spring_encoder_zero_max_torque() {
    // Should not panic; max_torque clamped to 0.01
    let enc = SimagicSpringEncoder::new(0.0);
    let mut out = [0u8; SPRING_REPORT_LEN];
    let len = enc.encode(500, 0, 0, 0, &mut out);
    assert_eq!(len, SPRING_REPORT_LEN);
}

#[test]
fn damper_encoder_zero_max_torque() {
    let enc = SimagicDamperEncoder::new(0.0);
    let mut out = [0u8; DAMPER_REPORT_LEN];
    let len = enc.encode(500, 1000, &mut out);
    assert_eq!(len, DAMPER_REPORT_LEN);
}

#[test]
fn friction_encoder_zero_max_torque() {
    let enc = SimagicFrictionEncoder::new(0.0);
    let mut out = [0u8; FRICTION_REPORT_LEN];
    let len = enc.encode(500, 1000, &mut out);
    assert_eq!(len, FRICTION_REPORT_LEN);
}

#[test]
fn encoders_negative_max_torque() {
    // All encoder constructors should handle negative max_torque gracefully
    let cf = SimagicConstantForceEncoder::new(-10.0);
    let sp = SimagicSpringEncoder::new(-10.0);
    let da = SimagicDamperEncoder::new(-10.0);
    let fr = SimagicFrictionEncoder::new(-10.0);

    let mut cf_out = [0u8; CONSTANT_FORCE_REPORT_LEN];
    let mut sp_out = [0u8; SPRING_REPORT_LEN];
    let mut da_out = [0u8; DAMPER_REPORT_LEN];
    let mut fr_out = [0u8; FRICTION_REPORT_LEN];

    let _ = cf.encode(1.0, &mut cf_out);
    let _ = sp.encode(500, 0, 0, 0, &mut sp_out);
    let _ = da.encode(500, 1000, &mut da_out);
    let _ = fr.encode(500, 1000, &mut fr_out);

    // Must produce valid report IDs
    assert_eq!(cf_out[0], report_ids::CONSTANT_FORCE);
    assert_eq!(sp_out[0], report_ids::SPRING_EFFECT);
    assert_eq!(da_out[0], report_ids::DAMPER_EFFECT);
    assert_eq!(fr_out[0], report_ids::FRICTION_EFFECT);
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 9  Category coverage
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_verified_device_categories_reachable() {
    // Only verified PIDs produce known categories; fabricated PIDs resolve to Unknown.
    let categories_found: Vec<SimagicDeviceCategory> = [
        product_ids::EVO_SPORT,
        product_ids::HANDBRAKE,
        product_ids::P1000_PEDALS, // fabricated → Unknown
        0xDEAD,                    // unknown
    ]
    .iter()
    .map(|&pid| simagic::identify_device(pid).category)
    .collect();

    assert!(categories_found.contains(&SimagicDeviceCategory::Wheelbase));
    assert!(categories_found.contains(&SimagicDeviceCategory::Handbrake));
    assert!(categories_found.contains(&SimagicDeviceCategory::Unknown));
    // Fabricated peripheral PIDs no longer produce Pedals/Shifter/Rim categories
    assert!(!categories_found.contains(&SimagicDeviceCategory::Pedals));
    assert!(!categories_found.contains(&SimagicDeviceCategory::Shifter));
    assert!(!categories_found.contains(&SimagicDeviceCategory::Rim));
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 10  SimagicInputState default and empty
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn input_state_empty_is_default() {
    let empty = simagic::SimagicInputState::empty();
    let default = simagic::SimagicInputState::default();

    assert!((empty.steering - default.steering).abs() < f32::EPSILON);
    assert!((empty.throttle - default.throttle).abs() < f32::EPSILON);
    assert!((empty.brake - default.brake).abs() < f32::EPSILON);
    assert_eq!(empty.buttons, default.buttons);
    assert_eq!(empty.hat, default.hat);
    assert_eq!(empty.firmware_version, default.firmware_version);
}

#[test]
fn input_state_default_values() {
    let state = simagic::SimagicInputState::default();
    assert!((state.steering).abs() < f32::EPSILON);
    assert!((state.throttle).abs() < f32::EPSILON);
    assert!((state.brake).abs() < f32::EPSILON);
    assert!((state.clutch).abs() < f32::EPSILON);
    assert!((state.handbrake).abs() < f32::EPSILON);
    assert_eq!(state.buttons, 0);
    assert_eq!(state.hat, 0);
    assert_eq!(state.rotary1, 0);
    assert_eq!(state.rotary2, 0);
    assert_eq!(state.shifter.gear, SimagicGear::Neutral);
    assert_eq!(state.quick_release, QuickReleaseStatus::Unknown);
    assert_eq!(state.firmware_version, None);
}
