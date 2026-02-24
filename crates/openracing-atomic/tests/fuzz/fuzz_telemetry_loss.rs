//! Fuzz target for telemetry loss percentage calculation.
//!
//! Tests that telemetry loss percentage calculation handles all inputs correctly.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (u64, u64)| {
    let (received, lost) = data;

    let counters = openracing_atomic::AtomicCounters::new();

    for _ in 0..received {
        counters.inc_telemetry_received();
    }
    for _ in 0..lost {
        counters.inc_telemetry_lost();
    }

    let pct = counters.telemetry_loss_percent();
    assert!(pct >= 0.0 && pct <= 100.0);
});
