//! Fuzzes the Simagic HID input report parser and constant-force encoder.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simagic_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_simagic_protocol::{
    SimagicConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN, identify_device,
    is_wheelbase_product, parse_input_report,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_input_report(data);

    // If we have at least 4 bytes, reinterpret first 4 as an f32 torque value
    // and verify the encoder never panics.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let enc = SimagicConstantForceEncoder::new(15.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        enc.encode_zero(&mut out);
    }

    // Reinterpret first 2 bytes as a PID and verify device identification never panics.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = identify_device(pid);
        let _ = is_wheelbase_product(pid);
    }
});