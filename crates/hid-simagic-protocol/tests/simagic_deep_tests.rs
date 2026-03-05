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
    ids::{
        SIMAGIC_LEGACY_PID, SIMAGIC_LEGACY_VENDOR_ID, SIMAGIC_VENDOR_ID, product_ids, report_ids,
    },
    settings::{
        self, AngleLockStrength, Settings1, Settings2, Settings3, Settings4,
        decode_ring_light, encode_ring_light, encode_settings1, encode_settings2, encode_settings3,
        encode_settings4, parse_status1,
    },
    types::{
        QuickReleaseStatus, SimagicDeviceCategory, SimagicFfbEffectType, SimagicGear, SimagicModel,
        SimagicPedalAxesRaw,
    },
    wire,
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

// ═══════════════════════════════════════════════════════════════════════════════
// § 11  VID/PID constant validation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn vid_modern_simagic_is_3670() {
    assert_eq!(SIMAGIC_VENDOR_ID, 0x3670);
}

#[test]
fn vid_legacy_stmicro_is_0483() {
    assert_eq!(SIMAGIC_LEGACY_VENDOR_ID, 0x0483);
}

#[test]
fn legacy_pid_shared_0522() {
    // M10, Alpha Mini, Alpha, Alpha Ultimate all share this PID
    assert_eq!(SIMAGIC_LEGACY_PID, 0x0522);
}

#[test]
fn evo_generation_pids_are_contiguous() {
    // EVO Sport/EVO/EVO Pro have contiguous PIDs 0x0500–0x0502
    assert_eq!(product_ids::EVO_SPORT, 0x0500);
    assert_eq!(product_ids::EVO, 0x0501);
    assert_eq!(product_ids::EVO_PRO, 0x0502);
}

#[test]
fn legacy_pid_resolves_to_unknown_in_identify() {
    // Legacy PID 0x0522 is not in identify_device (which handles VID 0x3670 PIDs only)
    let identity = simagic::identify_device(SIMAGIC_LEGACY_PID);
    assert_eq!(identity.category, SimagicDeviceCategory::Unknown);
}

#[test]
fn all_evo_wheelbases_have_ffb_support() {
    for pid in [
        product_ids::EVO_SPORT,
        product_ids::EVO,
        product_ids::EVO_PRO,
    ] {
        let identity = simagic::identify_device(pid);
        assert!(
            identity.supports_ffb,
            "{} (PID {pid:#06x}) should support FFB",
            identity.name
        );
    }
}

#[test]
fn handbrake_does_not_support_ffb() {
    let identity = simagic::identify_device(product_ids::HANDBRAKE);
    assert!(!identity.supports_ffb);
    assert_eq!(identity.category, SimagicDeviceCategory::Handbrake);
}

#[test]
fn all_evo_wheelbases_have_torque_values() {
    let pids_and_torques = [
        (product_ids::EVO_SPORT, 9.0f32),
        (product_ids::EVO, 12.0),
        (product_ids::EVO_PRO, 18.0),
    ];
    for (pid, expected) in pids_and_torques {
        let identity = simagic::identify_device(pid);
        assert_eq!(
            identity.max_torque_nm,
            Some(expected),
            "PID {pid:#06x} torque mismatch"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 12  Settings encoding and decoding
// ═══════════════════════════════════════════════════════════════════════════════

// ── 12.1  Settings1 encoding ────────────────────────────────────────────────

#[test]
fn settings1_encode_report_id_and_page() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: 900,
        ff_strength: 50,
        wheel_rotation_speed: 30,
        mechanical_centering: 40,
        mechanical_damper: 50,
        center_damper: 60,
        mechanical_friction: 70,
        game_centering: 100,
        game_inertia: 110,
        game_damper: 120,
        game_friction: 130,
    };
    let buf = encode_settings1(&s);
    assert_eq!(buf[0], settings::SET_REPORT_ID);
    assert_eq!(buf[1], 0x01);
    assert_eq!(buf.len(), 64);
    Ok(())
}

#[test]
fn settings1_encode_angle_le16() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: 1440,
        ff_strength: 0,
        wheel_rotation_speed: 0,
        mechanical_centering: 0,
        mechanical_damper: 0,
        center_damper: 0,
        mechanical_friction: 0,
        game_centering: 0,
        game_inertia: 0,
        game_damper: 0,
        game_friction: 0,
    };
    let buf = encode_settings1(&s);
    let angle = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(angle, 1440);
    Ok(())
}

#[test]
fn settings1_sanitize_clamps_all_fields() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: 10_000,         // above MAX_ANGLE
        ff_strength: 200,          // above 100
        wheel_rotation_speed: 255, // above 100
        mechanical_centering: 255,
        mechanical_damper: 255,
        center_damper: 255,
        mechanical_friction: 255,
        game_centering: 255, // above 200
        game_inertia: 255,
        game_damper: 255,
        game_friction: 255,
    };
    let buf = encode_settings1(&s);
    let angle = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(angle, settings::MAX_ANGLE);
    let strength = i16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(strength, 100);
    assert_eq!(buf[7], 100); // wheel_rotation_speed
    assert_eq!(buf[8], 100); // mechanical_centering
    assert_eq!(buf[12], 200); // game_centering
    Ok(())
}

#[test]
fn settings1_negative_ff_strength_encoded() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: 900,
        ff_strength: -75,
        wheel_rotation_speed: 0,
        mechanical_centering: 0,
        mechanical_damper: 0,
        center_damper: 0,
        mechanical_friction: 0,
        game_centering: 0,
        game_inertia: 0,
        game_damper: 0,
        game_friction: 0,
    };
    let buf = encode_settings1(&s);
    let strength = i16::from_le_bytes([buf[4], buf[5]]);
    assert_eq!(strength, -75);
    Ok(())
}

#[test]
fn settings1_unknown_offset_06_is_0x02() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: 900,
        ff_strength: 0,
        wheel_rotation_speed: 0,
        mechanical_centering: 0,
        mechanical_damper: 0,
        center_damper: 0,
        mechanical_friction: 0,
        game_centering: 0,
        game_inertia: 0,
        game_damper: 0,
        game_friction: 0,
    };
    let buf = encode_settings1(&s);
    assert_eq!(buf[6], 0x02);
    Ok(())
}

// ── 12.2  Settings2 encoding ────────────────────────────────────────────────

#[test]
fn settings2_encode_page_header() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings2 {
        angle_lock: 540,
        feedback_detail: 50,
        angle_lock_strength: 1,
        mechanical_inertia: 30,
    };
    let buf = encode_settings2(&s, 900);
    assert_eq!(buf[0], settings::SET_REPORT_ID);
    assert_eq!(buf[1], 0x02);
    Ok(())
}

#[test]
fn settings2_angle_lock_clamped_to_max_angle() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings2 {
        angle_lock: 5000,
        feedback_detail: 0,
        angle_lock_strength: 0,
        mechanical_inertia: 0,
    };
    let buf = encode_settings2(&s, 900);
    let lock = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(lock, 900);
    Ok(())
}

#[test]
fn settings2_angle_lock_strength_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings2 {
        angle_lock: 540,
        feedback_detail: 0,
        angle_lock_strength: 10, // above 2
        mechanical_inertia: 0,
    };
    let buf = encode_settings2(&s, 900);
    assert_eq!(buf[6], 2); // clamped to max
    Ok(())
}

// ── 12.3  Settings3 (ring light) ────────────────────────────────────────────

#[test]
fn settings3_encode_header() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings3 {
        ring_light_enabled: true,
        ring_light_brightness: 75,
    };
    let buf = encode_settings3(&s);
    assert_eq!(buf[0], settings::SET_REPORT_ID);
    assert_eq!(buf[1], 0x10);
    assert_eq!(buf[2], 0x38);
    assert_eq!(buf[3], 0x00);
    assert_eq!(buf[4], 0x01);
    Ok(())
}

#[test]
fn settings3_ring_light_byte_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings3 {
        ring_light_enabled: true,
        ring_light_brightness: 75,
    };
    let buf = encode_settings3(&s);
    assert_eq!(buf[5], 0x80 | 75); // enabled bit + brightness
    Ok(())
}

#[test]
fn settings3_ring_light_disabled() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings3 {
        ring_light_enabled: false,
        ring_light_brightness: 50,
    };
    let buf = encode_settings3(&s);
    assert_eq!(buf[5], 50); // no enable bit
    Ok(())
}

// ── 12.4  Settings4 (filter/slew) ──────────────────────────────────────────

#[test]
fn settings4_encode_header() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings4 {
        filter_level: 10,
        slew_rate_control: 50,
    };
    let buf = encode_settings4(&s);
    assert_eq!(buf[0], settings::SET_REPORT_ID);
    assert_eq!(buf[1], 0x10);
    assert_eq!(buf[2], 0x39);
    assert_eq!(buf[4], 0x07);
    Ok(())
}

#[test]
fn settings4_filter_level_clamped() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings4 {
        filter_level: 100,      // above 20
        slew_rate_control: 200, // above 100
    };
    let buf = encode_settings4(&s);
    assert_eq!(buf[7], 20); // clamped
    assert_eq!(buf[9], 100); // clamped
    Ok(())
}

// ── 12.5  Ring light encode/decode round-trip ──────────────────────────────

#[test]
fn ring_light_roundtrip_all_values() -> Result<(), Box<dyn std::error::Error>> {
    for enabled in [true, false] {
        for brightness in [0u8, 1, 50, 99, 100] {
            let encoded = encode_ring_light(enabled, brightness);
            let (dec_en, dec_br) = decode_ring_light(encoded);
            assert_eq!(dec_en, enabled);
            assert_eq!(dec_br, brightness);
        }
    }
    Ok(())
}

#[test]
fn ring_light_encode_clamps_over_100() {
    let encoded = encode_ring_light(true, 200);
    let (enabled, brightness) = decode_ring_light(encoded);
    assert!(enabled);
    assert_eq!(brightness, 100);
}

// ── 12.6  AngleLockStrength ────────────────────────────────────────────────

#[test]
fn angle_lock_strength_all_variants() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        AngleLockStrength::from_byte(0),
        Some(AngleLockStrength::Soft)
    );
    assert_eq!(
        AngleLockStrength::from_byte(1),
        Some(AngleLockStrength::Normal)
    );
    assert_eq!(
        AngleLockStrength::from_byte(2),
        Some(AngleLockStrength::Firm)
    );
    assert_eq!(AngleLockStrength::from_byte(3), None);
    assert_eq!(AngleLockStrength::from_byte(255), None);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 13  Status1 parsing and round-trip
// ═══════════════════════════════════════════════════════════════════════════════

fn make_status1_report(f: impl FnOnce(&mut [u8; 64])) -> [u8; 64] {
    let mut buf = [0u8; 64];
    buf[0] = settings::GET_REPORT_ID;
    f(&mut buf);
    buf
}

#[test]
fn status1_parse_minimal_valid() -> Result<(), Box<dyn std::error::Error>> {
    let report = make_status1_report(|buf| {
        buf[2..4].copy_from_slice(&900u16.to_le_bytes()); // max_angle
        buf[4..6].copy_from_slice(&50i16.to_le_bytes()); // ff_strength
        buf[7] = 30; // wheel_rotation_speed
        buf[8] = 40; // mechanical_centering
        buf[9] = 50; // mechanical_damper
        buf[10] = 60; // center_damper
        buf[11] = 70; // mechanical_friction
        buf[12] = 80; // game_centering
        buf[13] = 90; // game_inertia
        buf[14] = 100; // game_damper
        buf[15] = 110; // game_friction
        buf[16..18].copy_from_slice(&540u16.to_le_bytes()); // angle_lock
        buf[18] = 75; // feedback_detail
        buf[20] = 1; // angle_lock_strength
        buf[22] = 25; // mechanical_inertia
        buf[47] = encode_ring_light(true, 80); // ring_light
        buf[50] = 15; // filter_level
        buf[52] = 60; // slew_rate_control
    });

    let status = parse_status1(&report).ok_or("failed to parse status1")?;
    assert_eq!(status.max_angle, 900);
    assert_eq!(status.ff_strength, 50);
    assert_eq!(status.wheel_rotation_speed, 30);
    assert_eq!(status.mechanical_centering, 40);
    assert_eq!(status.mechanical_damper, 50);
    assert_eq!(status.center_damper, 60);
    assert_eq!(status.mechanical_friction, 70);
    assert_eq!(status.game_centering, 80);
    assert_eq!(status.game_inertia, 90);
    assert_eq!(status.game_damper, 100);
    assert_eq!(status.game_friction, 110);
    assert_eq!(status.angle_lock, 540);
    assert_eq!(status.feedback_detail, 75);
    assert_eq!(status.angle_lock_strength, 1);
    assert_eq!(status.mechanical_inertia, 25);
    assert_eq!(status.filter_level, 15);
    assert_eq!(status.slew_rate_control, 60);
    Ok(())
}

#[test]
fn status1_parse_wrong_report_id_returns_none() {
    let mut report = [0u8; 64];
    report[0] = 0x80; // SET_REPORT_ID, not GET_REPORT_ID
    assert!(parse_status1(&report).is_none());
}

#[test]
fn status1_parse_too_short_returns_none() {
    let mut report = [0u8; 52]; // needs at least 53
    report[0] = settings::GET_REPORT_ID;
    assert!(parse_status1(&report).is_none());
}

#[test]
fn status1_to_settings1_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = make_status1_report(|buf| {
        buf[2..4].copy_from_slice(&1080u16.to_le_bytes());
        buf[4..6].copy_from_slice(&(-25i16).to_le_bytes());
        buf[7] = 50;
        buf[8] = 60;
        buf[9] = 70;
        buf[10] = 80;
        buf[11] = 90;
        buf[12] = 150;
        buf[13] = 160;
        buf[14] = 170;
        buf[15] = 180;
    });
    let status = parse_status1(&report).ok_or("parse failed")?;
    let s1: Settings1 = (&status).into();
    assert_eq!(s1.max_angle, 1080);
    assert_eq!(s1.ff_strength, -25);
    assert_eq!(s1.wheel_rotation_speed, 50);
    assert_eq!(s1.game_centering, 150);
    Ok(())
}

#[test]
fn status1_to_settings2_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = make_status1_report(|buf| {
        buf[16..18].copy_from_slice(&720u16.to_le_bytes());
        buf[18] = 85;
        buf[20] = 2;
        buf[22] = 45;
    });
    let status = parse_status1(&report).ok_or("parse failed")?;
    let s2: Settings2 = (&status).into();
    assert_eq!(s2.angle_lock, 720);
    assert_eq!(s2.feedback_detail, 85);
    assert_eq!(s2.angle_lock_strength, 2);
    assert_eq!(s2.mechanical_inertia, 45);
    Ok(())
}

#[test]
fn status1_to_settings3_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = make_status1_report(|buf| {
        buf[47] = encode_ring_light(true, 60);
    });
    let status = parse_status1(&report).ok_or("parse failed")?;
    let s3: Settings3 = (&status).into();
    assert!(s3.ring_light_enabled);
    assert_eq!(s3.ring_light_brightness, 60);
    Ok(())
}

#[test]
fn status1_to_settings4_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let report = make_status1_report(|buf| {
        buf[50] = 12;
        buf[52] = 88;
    });
    let status = parse_status1(&report).ok_or("parse failed")?;
    let s4: Settings4 = (&status).into();
    assert_eq!(s4.filter_level, 12);
    assert_eq!(s4.slew_rate_control, 88);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 14  Wire format encoding (kernel protocol)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn wire_constant_force_report() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_constant(0x7FFF);
    assert_eq!(buf[0], wire::report_type::SET_CONSTANT);
    assert_eq!(buf[1], wire::block_id::CONSTANT);
    let mag = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(mag, 10000); // rescale_signed_to_10k(0x7FFF) = 10000
    assert_eq!(buf.len(), wire::REPORT_SIZE);
    Ok(())
}

#[test]
fn wire_constant_force_zero() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_constant(0);
    let mag = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(mag, 0);
    Ok(())
}

#[test]
fn wire_constant_force_negative() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_constant(i16::MIN);
    let mag = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(mag, -10000);
    Ok(())
}

#[test]
fn wire_set_effect_structure() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_set_effect(wire::block_id::CONSTANT, 500);
    assert_eq!(buf[0], wire::report_type::SET_EFFECT);
    assert_eq!(buf[1], wire::block_id::CONSTANT);
    assert_eq!(buf[2], 0x01);
    let dur = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(dur, 500);
    assert_eq!(buf[9], 0xFF); // gain
    assert_eq!(buf[10], 0xFF); // trigger button
    assert_eq!(buf[11], 0x04);
    assert_eq!(buf[12], 0x3F);
    Ok(())
}

#[test]
fn wire_set_effect_zero_duration_infinite() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_set_effect(wire::block_id::SINE, 0);
    let dur = u16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(dur, 0xFFFF);
    Ok(())
}

#[test]
fn wire_condition_spring() -> Result<(), Box<dyn std::error::Error>> {
    let params = wire::ConditionParams {
        center: 0,
        right_coeff: 0x4000,
        left_coeff: -0x4000,
        right_saturation: 0xFFFF,
        left_saturation: 0x8000,
        deadband: 0x1000,
    };
    let buf = wire::encode_condition(wire::block_id::SPRING, &params);
    assert_eq!(buf[0], wire::report_type::SET_CONDITION);
    assert_eq!(buf[1], wire::block_id::SPRING);
    assert_eq!(buf[2], 0x00);
    let center = i16::from_le_bytes([buf[3], buf[4]]);
    assert_eq!(center, 0);
    Ok(())
}

#[test]
fn wire_condition_all_effect_types() -> Result<(), Box<dyn std::error::Error>> {
    let params = wire::ConditionParams {
        center: 100,
        right_coeff: 5000,
        left_coeff: -5000,
        right_saturation: 0x8000,
        left_saturation: 0x8000,
        deadband: 0,
    };
    for &bid in &[
        wire::block_id::SPRING,
        wire::block_id::DAMPER,
        wire::block_id::FRICTION,
        wire::block_id::INERTIA,
    ] {
        let buf = wire::encode_condition(bid, &params);
        assert_eq!(buf[0], wire::report_type::SET_CONDITION);
        assert_eq!(buf[1], bid);
    }
    Ok(())
}

#[test]
fn wire_periodic_sine() -> Result<(), Box<dyn std::error::Error>> {
    let buf = wire::encode_periodic(wire::block_id::SINE, 0x7FFF, 0, 0, 100);
    assert_eq!(buf[0], wire::report_type::SET_PERIODIC);
    assert_eq!(buf[1], wire::block_id::SINE);
    let mag = i16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(mag, 10000);
    Ok(())
}

#[test]
fn wire_effect_operation_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let start_buf = wire::encode_effect_operation(wire::block_id::CONSTANT, true, 5);
    assert_eq!(start_buf[0], wire::report_type::EFFECT_OPERATION);
    assert_eq!(start_buf[2], wire::effect_op::START);
    assert_eq!(start_buf[3], 5);

    let stop_buf = wire::encode_effect_operation(wire::block_id::CONSTANT, false, 0);
    assert_eq!(stop_buf[2], wire::effect_op::STOP);
    assert_eq!(stop_buf[3], 0);
    Ok(())
}

#[test]
fn wire_gain_full_and_zero() -> Result<(), Box<dyn std::error::Error>> {
    let full = wire::encode_gain(0xFFFF);
    assert_eq!(full[0], wire::report_type::SET_GAIN);
    assert_eq!(full[1], 0xFF);

    let zero = wire::encode_gain(0);
    assert_eq!(zero[1], 0);
    Ok(())
}

#[test]
fn wire_rescale_signed_boundary_values() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(wire::rescale_signed_to_10k(0), 0);
    assert_eq!(wire::rescale_signed_to_10k(i16::MAX), 10000);
    assert_eq!(wire::rescale_signed_to_10k(i16::MIN), -10000);
    Ok(())
}

#[test]
fn wire_all_reports_are_64_bytes() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(wire::encode_constant(0).len(), 64);
    assert_eq!(wire::encode_set_effect(0x01, 100).len(), 64);
    assert_eq!(wire::encode_periodic(0x02, 0, 0, 0, 0).len(), 64);
    assert_eq!(wire::encode_effect_operation(0x01, true, 1).len(), 64);
    assert_eq!(wire::encode_gain(0).len(), 64);
    let params = wire::ConditionParams {
        center: 0,
        right_coeff: 0,
        left_coeff: 0,
        right_saturation: 0,
        left_saturation: 0,
        deadband: 0,
    };
    assert_eq!(wire::encode_condition(0x06, &params).len(), 64);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 15  Error handling for malformed data
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn parse_input_report_all_0xff() -> Result<(), Box<dyn std::error::Error>> {
    let data = [0xFFu8; 64];
    let state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    // Should produce valid (saturated) values without panicking
    assert!((state.steering - 1.0).abs() < 0.001);
    assert!((state.throttle - 1.0).abs() < 0.001);
    assert_eq!(state.shifter.gear, SimagicGear::Unknown);
    assert_eq!(state.quick_release, QuickReleaseStatus::Unknown);
    Ok(())
}

#[test]
fn parse_input_report_alternating_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    for (i, b) in data.iter_mut().enumerate() {
        *b = if i % 2 == 0 { 0xAA } else { 0x55 };
    }
    // Should not panic; just parse whatever values emerge
    let _state = simagic::parse_input_report(&data).ok_or("parse failed")?;
    Ok(())
}

#[test]
fn parse_status1_all_zeros_except_id() {
    let mut report = [0u8; 64];
    report[0] = settings::GET_REPORT_ID;
    let status = parse_status1(&report);
    assert!(status.is_some());
}

#[test]
fn parse_status1_all_0xff() {
    let report = [0xFFu8; 64];
    // Report ID 0xFF != 0x81, so this should fail
    assert!(parse_status1(&report).is_none());
}

#[test]
fn settings1_min_angle_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: settings::MIN_ANGLE,
        ff_strength: -100,
        wheel_rotation_speed: 0,
        mechanical_centering: 0,
        mechanical_damper: 0,
        center_damper: 0,
        mechanical_friction: 0,
        game_centering: 0,
        game_inertia: 0,
        game_damper: 0,
        game_friction: 0,
    };
    let buf = encode_settings1(&s);
    let angle = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(angle, 90);
    Ok(())
}

#[test]
fn settings1_max_angle_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let s = Settings1 {
        max_angle: settings::MAX_ANGLE,
        ff_strength: 100,
        wheel_rotation_speed: 100,
        mechanical_centering: 100,
        mechanical_damper: 100,
        center_damper: 100,
        mechanical_friction: 100,
        game_centering: 200,
        game_inertia: 200,
        game_damper: 200,
        game_friction: 200,
    };
    let buf = encode_settings1(&s);
    let angle = u16::from_le_bytes([buf[2], buf[3]]);
    assert_eq!(angle, 2520);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 16  Proptest property-based tests
// ═══════════════════════════════════════════════════════════════════════════════

mod proptest_deep {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(300))]

        #[test]
        fn prop_parse_input_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = simagic::parse_input_report(&data);
        }

        #[test]
        fn prop_steering_always_in_range(raw in 0u16..=65535u16) {
            let mut data = [0u8; 64];
            data[0..2].copy_from_slice(&raw.to_le_bytes());
            if let Some(state) = simagic::parse_input_report(&data) {
                prop_assert!(state.steering >= -1.0 && state.steering <= 1.0,
                    "steering out of range: {}", state.steering);
            }
        }

        #[test]
        fn prop_pedals_always_normalized(
            throttle in 0u16..=65535u16,
            brake in 0u16..=65535u16,
            clutch in 0u16..=65535u16,
            handbrake in 0u16..=65535u16,
        ) {
            let mut data = [0u8; 64];
            data[2..4].copy_from_slice(&throttle.to_le_bytes());
            data[4..6].copy_from_slice(&brake.to_le_bytes());
            data[6..8].copy_from_slice(&clutch.to_le_bytes());
            data[8..10].copy_from_slice(&handbrake.to_le_bytes());
            if let Some(state) = simagic::parse_input_report(&data) {
                prop_assert!(state.throttle >= 0.0 && state.throttle <= 1.0);
                prop_assert!(state.brake >= 0.0 && state.brake <= 1.0);
                prop_assert!(state.clutch >= 0.0 && state.clutch <= 1.0);
                prop_assert!(state.handbrake >= 0.0 && state.handbrake <= 1.0);
            }
        }

        #[test]
        fn prop_wire_rescale_bounded(v in i16::MIN..=i16::MAX) {
            let r = wire::rescale_signed_to_10k(v);
            prop_assert!((-10000..=10000).contains(&r), "out of range: {r}");
        }

        #[test]
        fn prop_wire_rescale_monotone(a in i16::MIN..=i16::MAX, b in i16::MIN..=i16::MAX) {
            let ra = wire::rescale_signed_to_10k(a);
            let rb = wire::rescale_signed_to_10k(b);
            if a < b {
                prop_assert!(ra <= rb, "monotonicity: f({a})={ra} > f({b})={rb}");
            }
        }

        #[test]
        fn prop_wire_constant_always_valid(level in i16::MIN..=i16::MAX) {
            let buf = wire::encode_constant(level);
            prop_assert_eq!(buf[0], wire::report_type::SET_CONSTANT);
            prop_assert_eq!(buf[1], wire::block_id::CONSTANT);
            prop_assert_eq!(buf.len(), 64);
        }

        #[test]
        fn prop_wire_gain_high_byte(gain in 0u16..=u16::MAX) {
            let buf = wire::encode_gain(gain);
            prop_assert_eq!(buf[0], wire::report_type::SET_GAIN);
            prop_assert_eq!(buf[1], (gain >> 8) as u8);
        }

        #[test]
        fn prop_constant_force_encoder_magnitude_bounded(torque in -50.0f32..50.0f32) {
            let enc = SimagicConstantForceEncoder::new(10.0);
            let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
            let _ = enc.encode(torque, &mut out);
            let mag = i16::from_le_bytes([out[3], out[4]]);
            prop_assert!((-10000..=10000).contains(&mag),
                "magnitude {mag} out of ±10000 range for torque {torque}");
        }

        #[test]
        fn prop_ring_light_roundtrip(enabled in proptest::bool::ANY, brightness in 0u8..=100u8) {
            let encoded = encode_ring_light(enabled, brightness);
            let (dec_en, dec_br) = decode_ring_light(encoded);
            prop_assert_eq!(dec_en, enabled);
            prop_assert_eq!(dec_br, brightness);
        }

        #[test]
        fn prop_parse_status1_never_panics(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = parse_status1(&data);
        }

        #[test]
        fn prop_settings1_sanitize_idempotent(
            angle in 0u16..=5000u16,
            ff in -200i16..=200i16,
            speed in 0u8..=255u8,
            centering in 0u8..=255u8,
            game_c in 0u8..=255u8,
        ) {
            let mut s = Settings1 {
                max_angle: angle,
                ff_strength: ff,
                wheel_rotation_speed: speed,
                mechanical_centering: centering,
                mechanical_damper: 0,
                center_damper: 0,
                mechanical_friction: 0,
                game_centering: game_c,
                game_inertia: 0,
                game_damper: 0,
                game_friction: 0,
            };
            s.sanitize();
            let mut s2 = s;
            s2.sanitize();
            prop_assert_eq!(s.max_angle, s2.max_angle);
            prop_assert_eq!(s.ff_strength, s2.ff_strength);
            prop_assert_eq!(s.wheel_rotation_speed, s2.wheel_rotation_speed);
            prop_assert_eq!(s.game_centering, s2.game_centering);
        }

        #[test]
        fn prop_identify_device_never_panics(pid in 0u16..=u16::MAX) {
            let identity = simagic::identify_device(pid);
            prop_assert!(!identity.name.is_empty());
        }

        #[test]
        fn prop_gear_from_raw_never_panics(raw in 0u8..=255u8) {
            let _ = SimagicGear::from_raw(raw);
        }

        #[test]
        fn prop_quick_release_from_raw_never_panics(raw in 0u8..=255u8) {
            let _ = QuickReleaseStatus::from_raw(raw);
        }
    }
}
