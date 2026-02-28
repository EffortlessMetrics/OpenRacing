//! Fuzzes the Dakar Desert Rally UDP telemetry packet normalizer.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_dakar_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{DakarDesertRallyAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = DakarDesertRallyAdapter::new();
    let _ = adapter.normalize(data);
});
