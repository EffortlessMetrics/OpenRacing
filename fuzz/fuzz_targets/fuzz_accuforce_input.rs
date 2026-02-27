//! Fuzzes the AccuForce HID device classification functions.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_accuforce_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_accuforce_protocol::{is_accuforce, is_accuforce_pid};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    if data.len() < 4 {
        return;
    }
    let vid = u16::from_le_bytes([data[0], data[1]]);
    let pid = u16::from_le_bytes([data[2], data[3]]);
    let _ = is_accuforce(vid, pid);
    let _ = is_accuforce_pid(pid);
});
