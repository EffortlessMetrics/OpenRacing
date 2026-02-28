//! Fuzzes the Thrustmaster HID input report parser, constant-force encoder, and device identification.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_thrustmaster_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, Model, ThrustmasterConstantForceEncoder, identify_device, input::parse_pedal_report,
    is_pedal_product, is_wheel_product, parse_input_report,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_input_report(data);
    let _ = parse_pedal_report(data);

    // If we have at least 4 bytes, reinterpret first 4 as an f32 torque value
    // and verify the encoder never panics.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let enc = ThrustmasterConstantForceEncoder::new(6.0);
        let mut out = [0u8; EFFECT_REPORT_LEN];
        enc.encode(torque, &mut out);
    }

    // Device identification with arbitrary PID.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = identify_device(pid);
        let _ = is_wheel_product(pid);
        let _ = is_pedal_product(pid);
        let _ = Model::from_product_id(pid);
    }
});
