//! Fuzzes the VRS DirectForce Pro HID input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_vrs_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_vrs_protocol::parse_input_report;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_input_report(data);
});
