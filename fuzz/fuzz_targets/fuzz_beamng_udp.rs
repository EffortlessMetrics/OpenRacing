//! Fuzzes the BeamNG.drive OutGauge UDP telemetry packet normalizer.
//!
//! BeamNG uses the LFS OutGauge binary format on UDP port 4444.
//!
//! Run with:
//!   cargo +nightly fuzz run fuzz_beamng_udp
#![no_main]
use libfuzzer_sys::fuzz_target;
use racing_wheel_telemetry_adapters::{BeamNGAdapter, TelemetryAdapter};

fuzz_target!(|data: &[u8]| {
    // Must never panic on arbitrary bytes — errors are expected, panics are not.
    let adapter = BeamNGAdapter::new();
    let _ = adapter.normalize(data);
});
