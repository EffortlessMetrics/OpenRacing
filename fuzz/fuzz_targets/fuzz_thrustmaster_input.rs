//! Fuzzes the Thrustmaster HID input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_thrustmaster_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_thrustmaster_protocol::{input::parse_pedal_report, parse_input_report};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_input_report(data);
    let _ = parse_pedal_report(data);
});
