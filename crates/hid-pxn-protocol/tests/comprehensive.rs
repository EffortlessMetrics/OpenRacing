//! Comprehensive tests for the PXN HID protocol crate.
//!
//! Covers: device identification via PID, known constant validation,
//! edge cases (boundary values, invalid data), and property tests.
//!
//! Note: PXN crate currently only exposes IDs (no input/output report
//! parsing), so tests focus on identification and constant correctness.

use racing_wheel_hid_pxn_protocol::{
    is_pxn, product_name, PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE,
    PRODUCT_V12_LITE_2, VENDOR_ID,
};

// ---------------------------------------------------------------------------
// 1. Known constant validation
// ---------------------------------------------------------------------------

#[test]
fn constants_vendor_id() {
    assert_eq!(
        VENDOR_ID, 0x11FF,
        "PXN/Lite Star VID must match Linux kernel hid-ids.h"
    );
}

#[test]
fn constants_product_v10() {
    assert_eq!(PRODUCT_V10, 0x3245);
}

#[test]
fn constants_product_v12() {
    assert_eq!(PRODUCT_V12, 0x1212);
}

#[test]
fn constants_product_v12_lite() {
    assert_eq!(PRODUCT_V12_LITE, 0x1112);
}

#[test]
fn constants_product_v12_lite_2() {
    assert_eq!(PRODUCT_V12_LITE_2, 0x1211);
}

#[test]
fn constants_product_gt987() {
    assert_eq!(PRODUCT_GT987, 0x2141);
}

#[test]
fn all_pids_distinct() {
    let pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for i in 0..pids.len() {
        for j in (i + 1)..pids.len() {
            assert_ne!(
                pids[i], pids[j],
                "PIDs at index {i} and {j} must be distinct"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Device identification via PID
// ---------------------------------------------------------------------------

#[test]
fn is_pxn_accepts_all_known_pids() {
    let known = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in known {
        assert!(
            is_pxn(VENDOR_ID, pid),
            "PID 0x{pid:04X} should be recognised"
        );
    }
}

#[test]
fn is_pxn_rejects_wrong_vid() {
    assert!(!is_pxn(0x0000, PRODUCT_V10));
    assert!(!is_pxn(0xFFFF, PRODUCT_V12));
    assert!(!is_pxn(0x3416, PRODUCT_V10)); // Cammus VID
    assert!(!is_pxn(0x0483, PRODUCT_V10)); // VRS/STM VID
}

#[test]
fn is_pxn_rejects_unknown_pid() {
    assert!(!is_pxn(VENDOR_ID, 0x0000));
    assert!(!is_pxn(VENDOR_ID, 0xFFFF));
    assert!(!is_pxn(VENDOR_ID, 0x0001));
    assert!(!is_pxn(VENDOR_ID, 0x3244)); // one below V10
    assert!(!is_pxn(VENDOR_ID, 0x3246)); // one above V10
}

#[test]
fn is_pxn_both_wrong() {
    assert!(!is_pxn(0x0000, 0x0000));
    assert!(!is_pxn(0xFFFF, 0xFFFF));
}

// ---------------------------------------------------------------------------
// 3. Product names
// ---------------------------------------------------------------------------

#[test]
fn product_name_v10() {
    assert_eq!(product_name(PRODUCT_V10), Some("PXN V10"));
}

#[test]
fn product_name_v12() {
    assert_eq!(product_name(PRODUCT_V12), Some("PXN V12"));
}

#[test]
fn product_name_v12_lite() {
    assert_eq!(product_name(PRODUCT_V12_LITE), Some("PXN V12 Lite"));
}

#[test]
fn product_name_v12_lite_2() {
    assert_eq!(product_name(PRODUCT_V12_LITE_2), Some("PXN V12 Lite (SE)"));
}

#[test]
fn product_name_gt987() {
    assert_eq!(product_name(PRODUCT_GT987), Some("Lite Star GT987 FF"));
}

#[test]
fn product_name_unknown_returns_none() {
    assert_eq!(product_name(0x0000), None);
    assert_eq!(product_name(0xFFFF), None);
    assert_eq!(product_name(0x0001), None);
}

#[test]
fn product_name_consistency_with_is_pxn() {
    let all_pids = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in all_pids {
        assert!(
            is_pxn(VENDOR_ID, pid),
            "PID 0x{pid:04X} recognised by is_pxn"
        );
        assert!(
            product_name(pid).is_some(),
            "PID 0x{pid:04X} should have a product name"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. Edge cases: adjacent PID values
// ---------------------------------------------------------------------------

#[test]
fn adjacent_pids_not_recognised() {
    let known = [
        PRODUCT_V10,
        PRODUCT_V12,
        PRODUCT_V12_LITE,
        PRODUCT_V12_LITE_2,
        PRODUCT_GT987,
    ];
    for pid in known {
        if pid > 0 {
            let below = pid - 1;
            // Skip if the adjacent PID is itself a known product
            if !known.contains(&below) {
                assert!(
                    !is_pxn(VENDOR_ID, below),
                    "PID 0x{below:04X} (one below 0x{pid:04X}) should not be recognised",
                );
            }
        }
        if pid < u16::MAX {
            let above = pid + 1;
            if !known.contains(&above) {
                assert!(
                    !is_pxn(VENDOR_ID, above),
                    "PID 0x{above:04X} (one above 0x{pid:04X}) should not be recognised",
                );
            }
        }
    }
}

#[test]
fn vendor_id_boundary() {
    // VID -1 and +1 should not match
    assert!(!is_pxn(VENDOR_ID.wrapping_sub(1), PRODUCT_V10));
    assert!(!is_pxn(VENDOR_ID.wrapping_add(1), PRODUCT_V10));
}

// ---------------------------------------------------------------------------
// 5. Property tests
// ---------------------------------------------------------------------------

mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(1000))]

        #[test]
        fn prop_is_pxn_requires_correct_vid(vid in 0u16..=65535u16, pid in 0u16..=65535u16) {
            if is_pxn(vid, pid) {
                prop_assert_eq!(vid, VENDOR_ID);
                prop_assert!(product_name(pid).is_some());
            }
        }

        #[test]
        fn prop_product_name_consistent_with_is_pxn(pid in 0u16..=65535u16) {
            if let Some(_name) = product_name(pid) {
                prop_assert!(is_pxn(VENDOR_ID, pid));
            }
        }

        #[test]
        fn prop_wrong_vid_always_false(vid in 0u16..=65535u16, pid in 0u16..=65535u16) {
            if vid != VENDOR_ID {
                prop_assert!(!is_pxn(vid, pid));
            }
        }

        #[test]
        fn prop_product_name_returns_non_empty_for_known(pid in 0u16..=65535u16) {
            if let Some(name) = product_name(pid) {
                prop_assert!(!name.is_empty());
            }
        }
    }
}
