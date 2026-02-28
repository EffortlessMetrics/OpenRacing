//! Fuzzes the OpenFFBoard torque encoder with arbitrary float inputs.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_openffboard_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_openffboard_protocol::OpenFFBoardTorqueEncoder;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let torque_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
    let torque = f32::from_le_bytes(torque_bytes);

    // Must never panic on any finite or non-finite float input.
    let enc = OpenFFBoardTorqueEncoder;
    let _ = enc.encode(torque);
});
