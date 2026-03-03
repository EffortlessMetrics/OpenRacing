//! Property-based tests for OpenFFBoard device identification constants.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero (valid USB IDs)
//! - `is_openffboard_product` recognises only known PIDs
//! - PID recognition is deterministic for arbitrary inputs

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::{
    is_openffboard_product, OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT,
    OPENFFBOARD_VENDOR_ID,
};

/// All known OpenFFBoard product IDs.
const ALL_PIDS: [u16; 2] = [OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// OPENFFBOARD_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(OPENFFBOARD_VENDOR_ID != 0,
            "OPENFFBOARD_VENDOR_ID must not be zero");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..2usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {} must not be zero", idx);
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_known_pids_unique(
        i in 0usize..2usize,
        j in 0usize..2usize,
    ) {
        if i != j {
            prop_assert_ne!(ALL_PIDS[i], ALL_PIDS[j],
                "PIDs at index {} and {} must differ", i, j);
        }
    }

    /// is_openffboard_product must return true only for the known PIDs.
    #[test]
    fn prop_recognition_matches_known_set(pid: u16) {
        let expected = ALL_PIDS.contains(&pid);
        let actual = is_openffboard_product(pid);
        prop_assert_eq!(actual, expected,
            "is_openffboard_product(0x{:04X}) = {}, expected {}",
            pid, actual, expected);
    }

    /// is_openffboard_product must be deterministic for any PID.
    #[test]
    fn prop_recognition_deterministic(pid: u16) {
        let a = is_openffboard_product(pid);
        let b = is_openffboard_product(pid);
        prop_assert_eq!(a, b,
            "is_openffboard_product must be deterministic for pid=0x{:04X}", pid);
    }
}
