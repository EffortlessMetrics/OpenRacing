//! Fuzzes the SimHub generic JSON UDP bridge parser.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_simhub_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::simhub::parse_simhub_packet;

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let _ = parse_simhub_packet(data);
});
