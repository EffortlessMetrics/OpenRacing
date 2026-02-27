//! Fuzzes the SimpleMotion V2 feedback report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simplemotion
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_simplemotion_v2::parse_feedback_report;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_feedback_report(data);
});
