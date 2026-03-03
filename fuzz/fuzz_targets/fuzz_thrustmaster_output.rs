//! Fuzzes the Thrustmaster HID output report builders and effect encoders.
//!
//! Covers: kernel range/gain/autocenter commands, spring/damper/friction effects,
//! actuator enable, T150-family encoding, and the constant-force zero-torque path.
//! Must never panic on arbitrary input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_thrustmaster_output
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, ThrustmasterConstantForceEncoder, build_actuator_enable,
    build_damper_effect, build_device_gain, build_friction_effect,
    build_kernel_autocenter_commands, build_kernel_gain_command, build_kernel_range_command,
    build_set_range_report, build_spring_effect, encode_gain_t150, encode_play_effect_t150,
    encode_range_t150, encode_stop_effect_t150,
};

fuzz_target!(|data: &[u8]| {
    // Range commands with arbitrary degree values.
    if data.len() >= 2 {
        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_set_range_report(degrees);
        let _ = build_kernel_range_command(degrees);
    }

    // Gain commands.
    if data.len() >= 2 {
        let _ = build_device_gain(data[0]);
        let gain16 = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_kernel_gain_command(gain16);
    }

    // Autocenter commands.
    if data.len() >= 2 {
        let value = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_kernel_autocenter_commands(value);
    }

    // Spring effect with arbitrary center offset and stiffness.
    if data.len() >= 4 {
        let center = i16::from_le_bytes([data[0], data[1]]);
        let stiffness = u16::from_le_bytes([data[2], data[3]]);
        let _ = build_spring_effect(center, stiffness);
    }

    // Damper effect.
    if data.len() >= 2 {
        let damping = u16::from_le_bytes([data[0], data[1]]);
        let _ = build_damper_effect(damping);
    }

    // Friction effect.
    if data.len() >= 4 {
        let minimum = u16::from_le_bytes([data[0], data[1]]);
        let maximum = u16::from_le_bytes([data[2], data[3]]);
        let _ = build_friction_effect(minimum, maximum);
    }

    // Actuator enable with arbitrary boolean.
    if let Some(&b) = data.first() {
        let _ = build_actuator_enable(b != 0);
    }

    // T150-family encoding with arbitrary values.
    if data.len() >= 2 {
        let range_value = u16::from_le_bytes([data[0], data[1]]);
        let _ = encode_range_t150(range_value);
    }
    if let Some(&gain) = data.first() {
        let _ = encode_gain_t150(gain);
    }
    if data.len() >= 3 {
        let _ = encode_play_effect_t150(data[0], data[1], data[2]);
        let _ = encode_stop_effect_t150(data[0]);
    }

    // Constant-force encoder: encode_zero path.
    {
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode_zero(&mut out);
    }
});
