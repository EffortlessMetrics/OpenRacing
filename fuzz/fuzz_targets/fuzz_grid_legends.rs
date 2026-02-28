//! Fuzzes the GRID Legends UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_grid_legends
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{GridLegendsAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = GridLegendsAdapter::new();
    let _ = adapter.normalize(data);
});
