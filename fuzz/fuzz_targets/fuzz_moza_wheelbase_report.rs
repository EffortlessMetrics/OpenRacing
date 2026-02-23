//! Fuzzes the Moza wheelbase input report parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_wheelbase_report

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_hid_moza_protocol::{MozaProtocol, product_ids};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let protocol = MozaProtocol::new(product_ids::R5_V1);
    let _ = protocol.parse_input_state(data);
});
