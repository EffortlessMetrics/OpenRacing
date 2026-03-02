//! Property-based tests for the PXN HID protocol.
//!
//! Verifies invariants across a wide range of inputs using `proptest`.
//! Complements `proptest_ids.rs` with `prop_oneof!`-style PID selection
//! and additional consistency checks following the workspace test pattern.

use proptest::prelude::*;
use racing_wheel_hid_pxn_protocol as pxn;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    /// is_pxn returns true for the official VID + known PIDs.
    #[test]
    fn prop_is_pxn_correct(pid in prop_oneof![
        Just(pxn::PRODUCT_V10),
        Just(pxn::PRODUCT_V12),
        Just(pxn::PRODUCT_V12_LITE),
        Just(pxn::PRODUCT_V12_LITE_2),
        Just(pxn::PRODUCT_GT987),
    ]) {
        prop_assert!(pxn::is_pxn(pxn::VENDOR_ID, pid));
    }

    /// is_pxn returns false for any non-PXN VID.
    #[test]
    fn prop_is_pxn_wrong_vid(vid in 0u16..=u16::MAX, pid in 0u16..=u16::MAX) {
        if vid != pxn::VENDOR_ID {
            prop_assert!(
                !pxn::is_pxn(vid, pid),
                "non-PXN VID {vid:#06X} must not be recognised"
            );
        }
    }

    /// is_pxn returns false for unknown PIDs even with the correct VID.
    #[test]
    fn prop_is_pxn_unknown_pid(pid in 0u16..=u16::MAX) {
        if pid != pxn::PRODUCT_V10
            && pid != pxn::PRODUCT_V12
            && pid != pxn::PRODUCT_V12_LITE
            && pid != pxn::PRODUCT_V12_LITE_2
            && pid != pxn::PRODUCT_GT987
        {
            prop_assert!(!pxn::is_pxn(pxn::VENDOR_ID, pid));
        }
    }

    /// product_name returns Some for all known PIDs.
    #[test]
    fn prop_product_name_known(pid in prop_oneof![
        Just(pxn::PRODUCT_V10),
        Just(pxn::PRODUCT_V12),
        Just(pxn::PRODUCT_V12_LITE),
        Just(pxn::PRODUCT_V12_LITE_2),
        Just(pxn::PRODUCT_GT987),
    ]) {
        prop_assert!(
            pxn::product_name(pid).is_some(),
            "product_name({pid:#06X}) must return Some for known PID"
        );
    }

    /// product_name returns None for unknown PIDs.
    #[test]
    fn prop_product_name_unknown(pid in 0u16..=u16::MAX) {
        if pid != pxn::PRODUCT_V10
            && pid != pxn::PRODUCT_V12
            && pid != pxn::PRODUCT_V12_LITE
            && pid != pxn::PRODUCT_V12_LITE_2
            && pid != pxn::PRODUCT_GT987
        {
            prop_assert!(
                pxn::product_name(pid).is_none(),
                "product_name({pid:#06X}) must return None for unknown PID"
            );
        }
    }

    /// product_name must contain "PXN" or "Lite Star" for all known PIDs.
    #[test]
    fn prop_product_name_contains_brand(pid in prop_oneof![
        Just(pxn::PRODUCT_V10),
        Just(pxn::PRODUCT_V12),
        Just(pxn::PRODUCT_V12_LITE),
        Just(pxn::PRODUCT_V12_LITE_2),
        Just(pxn::PRODUCT_GT987),
    ]) {
        if let Some(name) = pxn::product_name(pid) {
            prop_assert!(
                name.contains("PXN") || name.contains("Lite Star"),
                "product_name({pid:#06X}) must contain 'PXN' or 'Lite Star', got '{name}'"
            );
        }
    }

    /// product_name and is_pxn must agree: if product_name returns Some,
    /// then is_pxn(VENDOR_ID, pid) must be true, and vice versa.
    #[test]
    fn prop_product_name_consistent_with_is_pxn(pid in 0u16..=u16::MAX) {
        let has_name = pxn::product_name(pid).is_some();
        let known = pxn::is_pxn(pxn::VENDOR_ID, pid);
        prop_assert!(
            has_name == known,
            "product_name and is_pxn must agree for PID {:#06X}", pid
        );
    }

    /// product_name must not be empty for known PIDs.
    #[test]
    fn prop_product_name_non_empty(pid in prop_oneof![
        Just(pxn::PRODUCT_V10),
        Just(pxn::PRODUCT_V12),
        Just(pxn::PRODUCT_V12_LITE),
        Just(pxn::PRODUCT_V12_LITE_2),
        Just(pxn::PRODUCT_GT987),
    ]) {
        if let Some(name) = pxn::product_name(pid) {
            prop_assert!(
                !name.is_empty(),
                "product_name({pid:#06X}) must not be empty"
            );
        }
    }

    /// VID constant is always 0x11FF (Lite Star), regardless of context.
    #[test]
    fn prop_vendor_id_constant_is_pxn(_pid in any::<u16>()) {
        prop_assert_eq!(
            pxn::VENDOR_ID,
            0x11FFu16,
            "VENDOR_ID must always be 0x11FF"
        );
    }
}
