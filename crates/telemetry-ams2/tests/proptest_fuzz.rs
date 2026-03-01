//! Property-based fuzz tests for AMS2/PCars2 shared memory parsing.
//!
//! Ensures the parser never panics on arbitrary or random input.

use proptest::prelude::*;
use racing_wheel_telemetry_ams2::{AMS2Adapter, TelemetryAdapter};
use racing_wheel_telemetry_adapters::ams2::AMS2SharedMemory;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let adapter = AMS2Adapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly `size_of::<AMS2SharedMemory>()` bytes filled with
    /// random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(
            any::<u8>(),
            std::mem::size_of::<AMS2SharedMemory>()..=std::mem::size_of::<AMS2SharedMemory>()
        )
    ) {
        let adapter = AMS2Adapter::new();
        let _ = adapter.normalize(&data);
    }
}
