//! Fuzzes the Thrustmaster packet-level parsing: input reports, pedal reports,
//! effect encoders, and command round-trips.
//!
//! Complements `fuzz_thrustmaster_input` (input only) and
//! `fuzz_thrustmaster_output` (output only) by exercising full round-trip
//! paths: build a command from fuzz data, then feed the encoded bytes back
//! into the input parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_thrustmaster_packet

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, Model, ThrustmasterConstantForceEncoder, build_actuator_enable,
    build_damper_effect, build_device_gain, build_friction_effect, build_set_range_report,
    build_spring_effect, identify_device, input::parse_pedal_report, is_pedal_product,
    is_wheel_product, parse_input_report,
};

fuzz_target!(|data: &[u8]| {
    // Parse raw bytes as both wheel and pedal input reports.
    let _ = parse_input_report(data);
    let _ = parse_pedal_report(data);

    // Round-trip: build effect reports and feed them back to the input parser.
    if data.len() >= 4 {
        let center = i16::from_le_bytes([data[0], data[1]]);
        let stiffness = u16::from_le_bytes([data[2], data[3]]);
        let spring = build_spring_effect(center, stiffness);
        let _ = parse_input_report(&spring);
    }

    if data.len() >= 4 {
        let minimum = u16::from_le_bytes([data[0], data[1]]);
        let maximum = u16::from_le_bytes([data[2], data[3]]);
        let friction = build_friction_effect(minimum, maximum);
        let _ = parse_input_report(&friction);
    }

    if data.len() >= 2 {
        let damping = u16::from_le_bytes([data[0], data[1]]);
        let damper = build_damper_effect(damping);
        let _ = parse_input_report(&damper);

        let degrees = u16::from_le_bytes([data[0], data[1]]);
        let range = build_set_range_report(degrees);
        let _ = parse_input_report(&range);
    }

    // Actuator enable/disable and gain round-trips.
    if let Some(&b) = data.first() {
        let actuator = build_actuator_enable(b != 0);
        let _ = parse_input_report(&actuator);

        let gain = build_device_gain(b);
        let _ = parse_input_report(&gain);
    }

    // Constant-force encoder round-trip.
    if data.len() >= 4 {
        let torque = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(torque, &mut out);
        let _ = parse_input_report(&out);
        let _ = parse_pedal_report(&out);
    }

    // Device identification from arbitrary PID values.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = identify_device(pid);
        let _ = is_wheel_product(pid);
        let _ = is_pedal_product(pid);
        let _ = Model::from_product_id(pid);
    }
});
