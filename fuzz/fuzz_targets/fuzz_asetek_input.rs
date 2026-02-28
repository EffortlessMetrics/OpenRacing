//! Fuzzes the Asetek HID input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_asetek_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_asetek_protocol::AsetekInputReport;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = AsetekInputReport::parse(data);
});
