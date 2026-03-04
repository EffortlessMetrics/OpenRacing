//! Fuzzes the standalone HBP (handbrake) USB report parser from the `racing-wheel-hbp` crate.
//!
//! Exercises `parse_hbp_usb_report_best_effort` and `parse_axis` with arbitrary
//! bytes, covering report-ID–prefixed, raw 2-byte, and raw 3+ byte layouts.
//! Also round-trips through `.normalize()` when parsing succeeds.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_hbp_usb_report

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hbp::{parse_axis, parse_hbp_usb_report_best_effort};

fuzz_target!(|data: &[u8]| {
    // Main entry point: must never panic on arbitrary bytes.
    if let Some(raw) = parse_hbp_usb_report_best_effort(data) {
        // Normalize round-trip must also be safe.
        let _ = raw.normalize();
    }

    // parse_axis with arbitrary offset must never panic.
    if data.len() >= 2 {
        let start = data[0] as usize;
        let _ = parse_axis(&data[1..], start);
    }
});
