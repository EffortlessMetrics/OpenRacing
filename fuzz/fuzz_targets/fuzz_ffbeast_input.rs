//! Fuzzes the FFBeast HID torque encoder.
//!
//! Encodes arbitrary bytes as f32 torque values and feature report inputs.
//! Must never panic.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_ffbeast_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_ffbeast_protocol::output::{
    FFBeastTorqueEncoder, build_enable_ffb, build_set_gain,
};

fuzz_target!(|data: &[u8]| {
    let enc = FFBeastTorqueEncoder;

    // Interpret first 4 bytes as f32 torque and encode.
    if let Some(bytes) = data.get(..4) {
        let torque = f32::from_le_bytes(bytes.try_into().unwrap());
        let _ = enc.encode(torque);
    }

    // Use next byte as gain for feature report helpers.
    if let Some(&gain) = data.get(4) {
        let _ = build_set_gain(gain);
        let _ = build_enable_ffb(gain != 0);
    }
});
