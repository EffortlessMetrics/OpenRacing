//! Fuzzes the PXN HID input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_pxn_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_pxn_protocol::{is_pxn_device, parse, product_name};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.

    // parse() must never panic regardless of input size or content.
    let _ = parse(data);

    // ID functions must never panic for any VID/PID pair.
    if data.len() >= 4 {
        let vid = u16::from_le_bytes([data[0], data[1]]);
        let pid = u16::from_le_bytes([data[2], data[3]]);
        let _ = is_pxn_device(vid, pid);
        let _ = product_name(pid);
    }
});
