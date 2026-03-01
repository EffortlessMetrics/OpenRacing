//! Property-based fuzz tests for RaceRoom shared memory parsing.
//!
//! Ensures the parser never panics on arbitrary or random input.

use proptest::prelude::*;
use racing_wheel_telemetry_raceroom::{RaceRoomAdapter, TelemetryAdapter};

const R3E_VIEW_SIZE: usize = 4096;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..8192)
    ) {
        let adapter = RaceRoomAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly 4096 bytes (R3E shared memory view size) filled
    /// with random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), R3E_VIEW_SIZE..=R3E_VIEW_SIZE)
    ) {
        let adapter = RaceRoomAdapter::new();
        let _ = adapter.normalize(&data);
    }
}
