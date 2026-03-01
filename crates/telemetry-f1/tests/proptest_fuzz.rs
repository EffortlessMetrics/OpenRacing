//! Property-based fuzz tests for F1 telemetry packet parsing.
//!
//! Ensures the parser never panics on arbitrary or random input.

use proptest::prelude::*;
use racing_wheel_telemetry_f1::{F1NativeAdapter, TelemetryAdapter};

const F1_PACKET_MAX: usize = 2048;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..F1_PACKET_MAX)
    ) {
        let adapter = F1NativeAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly 1349 bytes (min car-telemetry packet size: header 29 + 22×60)
    /// filled with random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), 1349..=1349)
    ) {
        let adapter = F1NativeAdapter::new();
        let _ = adapter.normalize(&data);
    }
}
