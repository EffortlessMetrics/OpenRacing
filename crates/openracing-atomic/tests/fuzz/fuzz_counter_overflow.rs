//! Fuzz targets for openracing-atomic.
//!
//! These targets are designed to be used with `cargo fuzz`.
//!
//! # Usage
//!
//! ```bash
//! cd crates/openracing-atomic
//! cargo fuzz run fuzz_counter_overflow
//! ```

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }

    let amount = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);

    let counters = openracing_atomic::AtomicCounters::new();
    counters.inc_tick_by(amount);
    counters.inc_missed_tick_by(amount);
    counters.inc_safety_event();

    let _ = counters.total_ticks();
    let _ = counters.missed_ticks();
    let _ = counters.safety_events();
});
