//! Fuzzes the GRID Autosport UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_grid_autosport
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{GridAutosportAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = GridAutosportAdapter::new();
    let _ = adapter.normalize(data);
});
