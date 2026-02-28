//! Fuzzes the OpenFFBoard torque encoder, feature reports, and device identification.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_openffboard_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_openffboard_protocol::{
    OpenFFBoardTorqueEncoder, build_enable_ffb, build_set_gain, is_openffboard_product,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on any finite or non-finite float input.
    if data.len() >= 4 {
        let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
        let torque = f32::from_le_bytes(torque_bytes);
        let enc = OpenFFBoardTorqueEncoder;
        let _ = enc.encode(torque);
    }

    // Feature report helpers.
    if let Some(&gain) = data.first() {
        let _ = build_set_gain(gain);
        let _ = build_enable_ffb(gain != 0);
    }

    // Device identification with arbitrary PID.
    if data.len() >= 2 {
        let pid = u16::from_le_bytes([data[0], data[1]]);
        let _ = is_openffboard_product(pid);
    }
});
