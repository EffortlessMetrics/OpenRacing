//! Fuzzes the RaceRoom Racing Experience telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_raceroom
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{RaceRoomAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = RaceRoomAdapter::new();
    let _ = adapter.normalize(data);
});
