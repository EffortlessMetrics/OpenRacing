//! Fuzzes the Fanatec HID input report parsers.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_fanatec_input
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_fanatec_protocol::{parse_extended_report, parse_pedal_report, parse_standard_report};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_standard_report(data);
    let _ = parse_extended_report(data);
    let _ = parse_pedal_report(data);
});
