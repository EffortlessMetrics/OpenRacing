//! End-to-end fuzz of F1_25Adapter::normalize().
//!
//! This target feeds arbitrary bytes into the top-level normalize() entry
//! point to ensure no panic, OOM, or undefined behaviour occurs regardless
//! of what the network delivers.
//!
//! Run with:
//!   cargo fuzz run fuzz_f1_25_normalize

#![no_main]

use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::f1_25::F1_25Adapter;
use racing_wheel_telemetry_adapters::TelemetryAdapter;

fuzz_target!(|data: &[u8]| {
    let adapter = F1_25Adapter::new();
    // Must never panic — errors (Err(_)) are fine.
    let _ = adapter.normalize(data);
});
