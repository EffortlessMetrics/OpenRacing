//! Fuzzes the VRS DirectForce Pro HID input report parser, FFB encoder, and device identification.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_vrs_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, VrsConstantForceEncoder, identify_device, is_wheelbase_product,
    parse_input_report,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_input_report(data);

    // Constant-force encoder with arbitrary torque values.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let enc = VrsConstantForceEncoder::new(11.0);
        let mut out = [0u8; CONSTANT_FORCE_REPORT_LEN];
        enc.encode(torque, &mut out);
        enc.encode_zero(&mut out);
    }

    // Device identification with arbitrary PID.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = identify_device(pid);
        let _ = is_wheelbase_product(pid);
    }
});
