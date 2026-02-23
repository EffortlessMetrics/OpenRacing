//! Fuzzes the Moza HBP standalone report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_hbp_report

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_moza_protocol::{parse_hbp_report, product_ids};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_hbp_report(product_ids::HBP_HANDBRAKE, data);
});
