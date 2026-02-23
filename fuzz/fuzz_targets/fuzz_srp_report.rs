//! Fuzzes the SR-P standalone USB report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_srp_report

#![deny(static_mut_refs)]
#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_srp::parse_srp_usb_report_best_effort;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_srp_usb_report_best_effort(data);
});
