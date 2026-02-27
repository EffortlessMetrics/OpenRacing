//! Fuzzes the Heusinkveld pedal HID input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_heusinkveld_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_heusinkveld_protocol::HeusinkveldInputReport;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = HeusinkveldInputReport::parse(data);
});
