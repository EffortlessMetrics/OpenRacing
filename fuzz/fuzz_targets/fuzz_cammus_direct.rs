//! Fuzzes the Cammus FFB direct torque encoder, input parser, and device identification.
//!
//! Verifies encode_torque, encode_stop, parse, and identification never panic
//! on any input, including NaN, Inf, and arbitrary float bit patterns.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_cammus_direct
#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_cammus_protocol::{
    CammusModel, encode_stop, encode_torque, is_cammus, parse, product_name,
};

fuzz_target!(|data: &[u8]| {
    // Input report parsing with arbitrary bytes.
    let _ = parse(data);

    // Must never panic on any float input, including NaN and Inf.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let _ = encode_torque(torque);
    }
    let _ = encode_stop();

    // Device identification with arbitrary VID/PID.
    if data.len() >= 4 {
        let vid = u16::from_le_bytes([data[0], data[1]]);
        let pid = u16::from_le_bytes([data[2], data[3]]);
        let _ = is_cammus(vid, pid);
        let _ = product_name(pid);
        let _ = CammusModel::from_pid(pid);
    }
});
