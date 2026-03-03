//! Fuzzes the Simagic HID output report builders and effect encoders.
//!
//! Covers: rotation range, device gain, LED, periodic effects (sine/square/triangle),
//! and spring/damper/friction encoders. Must never panic on arbitrary input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simagic_output
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_simagic_protocol::{
    DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN, SimagicDamperEncoder,
    SimagicFrictionEncoder, SimagicSpringEncoder, build_device_gain, build_led_report,
    build_rotation_range, build_sine_effect, build_square_effect, build_triangle_effect,
};

fuzz_target!(|data: &[u8]| {
    // Rotation range with arbitrary degrees.
    if data.len() >= 2 {
        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_rotation_range(degrees);
    }

    // Device gain and LED with arbitrary byte.
    if let Some(&b) = data.first() {
        let _ = build_device_gain(b);
        let _ = build_led_report(b);
    }

    // Periodic effects with arbitrary parameters.
    if data.len() >= 8 {
        let amplitude = u16::from_le_bytes([data[0], data[1]]);
        let freq = f32::from_le_bytes([data[2], data[3], data[4], data[5]]);
        let phase = u16::from_le_bytes([data[6], data[7]]);
        let _ = build_sine_effect(amplitude, freq, phase);
        let _ = build_square_effect(amplitude, freq, phase);
        let _ = build_triangle_effect(amplitude, freq);
    }

    // Spring encoder with arbitrary parameters.
    if data.len() >= 10 {
        let enc = SimagicSpringEncoder::new(15.0);
        let strength = u16::from_le_bytes([data[0], data[1]]);
        let position = i16::from_le_bytes([data[2], data[3]]);
        let offset = i16::from_le_bytes([data[4], data[5]]);
        let deadzone = u16::from_le_bytes([data[6], data[7]]);
        let mut out = [0u8; SPRING_REPORT_LEN];
        enc.encode(strength, position, offset, deadzone, &mut out);
        enc.encode_zero(&mut out);
    }

    // Damper encoder with arbitrary parameters.
    if data.len() >= 4 {
        let enc = SimagicDamperEncoder::new(15.0);
        let strength = u16::from_le_bytes([data[0], data[1]]);
        let velocity = u16::from_le_bytes([data[2], data[3]]);
        let mut out = [0u8; DAMPER_REPORT_LEN];
        enc.encode(strength, velocity, &mut out);
        enc.encode_zero(&mut out);
    }

    // Friction encoder with arbitrary parameters.
    if data.len() >= 4 {
        let enc = SimagicFrictionEncoder::new(15.0);
        let coefficient = u16::from_le_bytes([data[0], data[1]]);
        let velocity = u16::from_le_bytes([data[2], data[3]]);
        let mut out = [0u8; FRICTION_REPORT_LEN];
        enc.encode(coefficient, velocity, &mut out);
        enc.encode_zero(&mut out);
    }
});
