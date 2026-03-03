//! Property-based fuzz tests for WRC Generations UDP packet parsing.
//!
//! Ensures the parser never panics on arbitrary or random input.

use proptest::prelude::*;
use racing_wheel_telemetry_wrc_generations::{TelemetryAdapter, WrcGenerationsAdapter};

const MIN_PACKET_SIZE: usize = 264;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..1024)
    ) {
        let adapter = WrcGenerationsAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly 264 bytes (Codemasters Mode 1 minimum) filled
    /// with random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), MIN_PACKET_SIZE..=MIN_PACKET_SIZE)
    ) {
        let adapter = WrcGenerationsAdapter::new();
        let _ = adapter.normalize(&data);
    }
}
