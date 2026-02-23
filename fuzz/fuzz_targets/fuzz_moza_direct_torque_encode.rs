//! Fuzzes the Moza direct torque encoder with arbitrary torque values.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_direct_torque_encode

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_moza_protocol::MozaDirectTorqueEncoder;

fuzz_target!(|data: &[u8]| {
    if data.len() < 6 {
        return;
    }
    let torque_bytes: [u8; 4] = data[0..4].try_into().unwrap_or([0u8; 4]);
    let seq_bytes: [u8; 2] = data[4..6].try_into().unwrap_or([0u8; 2]);

    let torque_nm = f32::from_le_bytes(torque_bytes);
    let seq = u16::from_le_bytes(seq_bytes);

    // Must never panic on any finite or non-finite float input.
    let encoder = MozaDirectTorqueEncoder::new(5.5);
    let mut out = [0u8; 8];
    let _ = encoder.encode(torque_nm, seq, &mut out);
});
