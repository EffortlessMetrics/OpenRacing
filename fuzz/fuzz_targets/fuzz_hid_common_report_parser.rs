//! Fuzzes the HID common report parser with arbitrary byte sequences.
//!
//! The `ReportParser` is used across all HID protocol crates to deserialise
//! raw device reports.  This target exercises every read method to ensure
//! none of them panic on malformed input.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_hid_common_report_parser
#![no_main]
use libfuzzer_sys::fuzz_target;
use openracing_hid_common::ReportParser;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let mut parser = ReportParser::from_slice(data);

    // Exercise every primitive read method.
    let _ = parser.read_u8();
    let _ = parser.read_i8();
    let _ = parser.read_u16_le();
    let _ = parser.read_u16_be();
    let _ = parser.read_i16_le();
    let _ = parser.read_u32_le();
    let _ = parser.read_i32_le();
    let _ = parser.read_f32_le();
    let _ = parser.read_bytes(4);
    let _ = parser.peek_u8();

    // Reset and try a different read order.
    parser.reset();
    let _ = parser.read_f32_le();
    parser.skip(2);
    let _ = parser.read_u16_le();
    let _ = parser.remaining();
});
