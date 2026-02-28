//! Fuzzes the Race Driver: GRID UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_race_driver_grid
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{RaceDriverGridAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = RaceDriverGridAdapter::new();
    let _ = adapter.normalize(data);
});
