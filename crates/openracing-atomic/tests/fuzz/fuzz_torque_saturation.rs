//! Fuzz target for torque saturation percentage calculation.
//!
//! Tests that torque saturation percentage calculation handles all inputs correctly.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (u64, u64)| {
    let (saturated, not_saturated) = data;

    let counters = openracing_atomic::AtomicCounters::new();

    for _ in 0..saturated {
        counters.record_torque_saturation(true);
    }
    for _ in 0..not_saturated {
        counters.record_torque_saturation(false);
    }

    let pct = counters.torque_saturation_percent();
    assert!(pct >= 0.0 && pct <= 100.0);
});
