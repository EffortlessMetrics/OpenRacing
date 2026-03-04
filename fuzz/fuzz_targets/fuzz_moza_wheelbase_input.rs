//! Fuzzes the standalone Moza wheelbase report parsers from the
//! `racing-wheel-moza-wheelbase-report` crate.
//!
//! Covers `parse_wheelbase_report`, `parse_wheelbase_input_report`,
//! `parse_wheelbase_pedal_axes`, and `parse_axis` with arbitrary bytes.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_moza_wheelbase_input

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_moza_wheelbase_report::{
    parse_axis, parse_wheelbase_input_report, parse_wheelbase_pedal_axes, parse_wheelbase_report,
};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes.
    let _ = parse_wheelbase_report(data);
    let _ = parse_wheelbase_input_report(data);
    let _ = parse_wheelbase_pedal_axes(data);

    // parse_axis with arbitrary start offset.
    if data.len() >= 2 {
        let start = data[0] as usize;
        let _ = parse_axis(&data[1..], start);
    }
});
