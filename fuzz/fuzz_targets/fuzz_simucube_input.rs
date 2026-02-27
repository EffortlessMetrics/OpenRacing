//! Fuzzes the Simucube HID input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simucube_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_simucube_protocol::SimucubeInputReport;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = SimucubeInputReport::parse(data);
});
