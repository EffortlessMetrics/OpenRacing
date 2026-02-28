//! Fuzzes the GRID 2019 UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_grid_2019
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{Grid2019Adapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = Grid2019Adapter::new();
    let _ = adapter.normalize(data);
});
