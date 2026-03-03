//! Property-based fuzz tests for LFS OutGauge packet parsing.
//!
//! Ensures the parser never panics on arbitrary or random input.

use proptest::prelude::*;
use racing_wheel_telemetry_lfs::{LFSAdapter, TelemetryAdapter};

const OUTGAUGE_PACKET_SIZE: usize = 96;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Arbitrary random bytes of any length must never cause a panic.
    #[test]
    fn prop_random_bytes_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..256)
    ) {
        let adapter = LFSAdapter::new();
        let _ = adapter.normalize(&data);
    }

    /// A buffer of exactly 96 bytes (OutGauge packet size) filled with
    /// random content must not panic.
    #[test]
    fn prop_valid_size_random_content(
        data in proptest::collection::vec(any::<u8>(), OUTGAUGE_PACKET_SIZE..=OUTGAUGE_PACKET_SIZE)
    ) {
        let adapter = LFSAdapter::new();
        let _ = adapter.normalize(&data);
    }
}
