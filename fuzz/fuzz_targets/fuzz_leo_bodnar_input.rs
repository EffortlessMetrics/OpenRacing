//! Fuzzes the Leo Bodnar HID device classification functions.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_leo_bodnar_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_leo_bodnar_protocol::{is_leo_bodnar, is_leo_bodnar_device, is_leo_bodnar_ffb_pid};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    if data.len() < 4 {
        return;
    }
    let vid = u16::from_le_bytes([data[0], data[1]]);
    let pid = u16::from_le_bytes([data[2], data[3]]);
    let _ = is_leo_bodnar(vid, pid);
    let _ = is_leo_bodnar_device(pid);
    let _ = is_leo_bodnar_ffb_pid(pid);
});
