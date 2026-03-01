//! Fuzzes the OpenFFBoard torque encoder round-trip with edge-case float values.
//!
//! Extends coverage of the existing fuzz_openffboard_input target by exercising
//! the encoder with structured arbitrary floats (NaN, infinity, subnormals).
//! Must never panic on any input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_openffboard_encode
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_openffboard_protocol::{OpenFFBoardTorqueEncoder, build_enable_ffb, build_set_gain};

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }

    let torque = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let enc = OpenFFBoardTorqueEncoder;
    let encoded = enc.encode(torque);

    // Re-encode the output bytes as a float to exercise round-trip paths.
    if encoded.len() >= 4 {
        let reinterpreted = f32::from_le_bytes([encoded[1], encoded[2], encoded[3], encoded[4]]);
        let _ = enc.encode(reinterpreted);
    }

    // Feature reports with all byte values from fuzz input.
    for &b in data.iter().take(8) {
        let _ = build_set_gain(b);
        let _ = build_enable_ffb(b != 0);
    }
});
