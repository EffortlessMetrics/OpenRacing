//! Property-based tests for PXN / Lite Star device identification.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero and unique
//! - `is_pxn` correctness for arbitrary VID/PID pairs
//! - `product_name` consistency for known and unknown PIDs
//! - Robustness: arbitrary inputs never panic

use proptest::prelude::*;
use racing_wheel_hid_pxn_protocol::{
    is_pxn, product_name, PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE,
    PRODUCT_V12_LITE_2, VENDOR_ID,
};

/// All known PXN / Lite Star product IDs.
const ALL_PIDS: [u16; 5] = [
    PRODUCT_V10,
    PRODUCT_V12,
    PRODUCT_V12_LITE,
    PRODUCT_V12_LITE_2,
    PRODUCT_GT987,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(VENDOR_ID != 0,
            "VENDOR_ID must not be zero");
    }

    /// VENDOR_ID must always equal 0x11FF (Lite Star).
    #[test]
    fn prop_vendor_id_value(_unused: u8) {
        prop_assert_eq!(VENDOR_ID, 0x11FFu16,
            "VENDOR_ID must be 0x11FF (Lite Star)");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..5usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_pids_unique(idx_a in 0usize..5usize, idx_b in 0usize..5usize) {
        if idx_a != idx_b {
            prop_assert!(ALL_PIDS[idx_a] != ALL_PIDS[idx_b],
                "PIDs at index {idx_a} and {idx_b} must differ, both are {:#06x}",
                ALL_PIDS[idx_a]);
        }
    }

    /// is_pxn must return true for all known PIDs with the correct VID.
    #[test]
    fn prop_is_pxn_known_devices(idx in 0usize..5usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(is_pxn(VENDOR_ID, pid),
            "is_pxn(VENDOR_ID, {pid:#06x}) must return true");
    }

    /// is_pxn must return false for wrong VID.
    #[test]
    fn prop_is_pxn_wrong_vid(
        vid in any::<u16>().prop_filter("not PXN VID", |v| *v != VENDOR_ID),
        pid: u16,
    ) {
        prop_assert!(!is_pxn(vid, pid),
            "is_pxn({vid:#06x}, {pid:#06x}) must return false for wrong VID");
    }

    /// is_pxn with the correct VID must return false for unknown PIDs.
    #[test]
    fn prop_is_pxn_unknown_pid(pid: u16) {
        let is_known = ALL_PIDS.contains(&pid);
        prop_assert_eq!(is_pxn(VENDOR_ID, pid), is_known,
            "is_pxn(VENDOR_ID, {pid:#06x}) must be {is_known}");
    }

    /// product_name must return Some for all known PIDs.
    #[test]
    fn prop_product_name_known(idx in 0usize..5usize) {
        let pid = ALL_PIDS[idx];
        let name = product_name(pid);
        prop_assert!(name.is_some(),
            "product_name({pid:#06x}) must return Some");
    }

    /// product_name must return None for unknown PIDs.
    #[test]
    fn prop_product_name_unknown(pid: u16) {
        if !ALL_PIDS.contains(&pid) {
            prop_assert!(product_name(pid).is_none(),
                "product_name({pid:#06x}) must return None for unknown PID");
        }
    }

    /// product_name must contain "PXN" or "Lite Star" for all known PIDs.
    #[test]
    fn prop_product_name_contains_brand(idx in 0usize..5usize) {
        let pid = ALL_PIDS[idx];
        if let Some(name) = product_name(pid) {
            prop_assert!(name.contains("PXN") || name.contains("Lite Star"),
                "product_name({pid:#06x}) must contain 'PXN' or 'Lite Star', got '{name}'");
        }
    }

    /// product_name must not be empty for known PIDs.
    #[test]
    fn prop_product_name_non_empty(idx in 0usize..5usize) {
        let pid = ALL_PIDS[idx];
        if let Some(name) = product_name(pid) {
            prop_assert!(!name.is_empty(),
                "product_name({pid:#06x}) must not be empty");
        }
    }

    /// Robustness: is_pxn must not panic for any arbitrary VID/PID pair.
    #[test]
    fn prop_is_pxn_never_panics(vid: u16, pid: u16) {
        // Just call the function; proptest will catch any panic.
        let _ = is_pxn(vid, pid);
    }

    /// Robustness: product_name must not panic for any arbitrary PID.
    #[test]
    fn prop_product_name_never_panics(pid: u16) {
        // Just call the function; proptest will catch any panic.
        let _ = product_name(pid);
    }

    /// product_name and is_pxn must agree: if product_name returns Some,
    /// then is_pxn(VENDOR_ID, pid) must be true, and vice versa.
    #[test]
    fn prop_product_name_consistent_with_is_pxn(pid: u16) {
        let has_name = product_name(pid).is_some();
        let is_known = is_pxn(VENDOR_ID, pid);
        prop_assert_eq!(has_name, is_known,
            "product_name and is_pxn must agree for PID {pid:#06x}: \
             has_name={has_name}, is_known={is_known}");
    }
}
