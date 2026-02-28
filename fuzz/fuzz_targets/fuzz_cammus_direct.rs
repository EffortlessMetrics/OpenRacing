//! Fuzzes the Cammus FFB direct torque encoder.
//!
//! Verifies encode_torque and encode_stop never panic on any input,
//! including NaN, Inf, and arbitrary float bit patterns.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_cammus_direct
#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_cammus_protocol::{encode_stop, encode_torque};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let torque_bytes: [u8; 4] = data[0..4].try_into().unwrap_or([0u8; 4]);
    let torque = f32::from_le_bytes(torque_bytes);
    // Must never panic on any float input, including NaN and Inf.
    let _ = encode_torque(torque);
    let _ = encode_stop();
});
