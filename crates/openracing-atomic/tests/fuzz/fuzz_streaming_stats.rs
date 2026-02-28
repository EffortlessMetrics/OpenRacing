//! Fuzz target for streaming stats overflow handling.
//!
//! Tests that streaming stats handles overflow gracefully.

#![no_main]

use libfuzzer_sys::fuzz_target;
use openracing_atomic::StreamingStats;

fuzz_target!(|data: Vec<u64>| {
    let mut stats = StreamingStats::new();

    for &value in &data {
        stats.record(value);
    }

    if !data.is_empty() {
        assert_eq!(stats.count(), data.len() as u64);
    }

    let _ = stats.mean();
    let _ = stats.min();
    let _ = stats.max();
});
