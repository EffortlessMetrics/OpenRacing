//! Property-based tests for VRS device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - Device identity classification determinism and max_torque_nm bounds
//! - identify_device name is non-empty and torque is non-negative
//! - Round-trip: PID → identity → PID preserves identity

use proptest::prelude::*;
use racing_wheel_hid_vrs_protocol::{
    VRS_PRODUCT_ID, VRS_VENDOR_ID, identify_device, is_wheelbase_product, product_ids,
};

/// All known VRS product IDs.
const ALL_PIDS: [u16; 8] = [
    product_ids::DIRECTFORCE_PRO,
    product_ids::DIRECTFORCE_PRO_V2,
    product_ids::R295,
    product_ids::PEDALS,
    product_ids::PEDALS_V1,
    product_ids::PEDALS_V2,
    product_ids::HANDBRAKE,
    product_ids::SHIFTER,
];

/// Wheelbase-only PIDs.
const WHEELBASE_PIDS: [u16; 3] = [
    product_ids::DIRECTFORCE_PRO,
    product_ids::DIRECTFORCE_PRO_V2,
    product_ids::R295,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// VRS_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(VRS_VENDOR_ID != 0,
            "VRS_VENDOR_ID must not be zero");
    }

    /// VID must match the expected VRS vendor ID (0x0483).
    #[test]
    fn prop_vendor_id_matches_expected(_unused: u8) {
        prop_assert_eq!(VRS_VENDOR_ID, 0x0483,
            "VRS_VENDOR_ID must be 0x0483");
    }

    /// VRS_PRODUCT_ID alias must match DIRECTFORCE_PRO.
    #[test]
    fn prop_product_id_alias(_unused: u8) {
        prop_assert_eq!(VRS_PRODUCT_ID, product_ids::DIRECTFORCE_PRO,
            "VRS_PRODUCT_ID must equal product_ids::DIRECTFORCE_PRO");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..8usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_known_pids_unique(a in 0usize..8usize, b in 0usize..8usize) {
        if a != b {
            prop_assert!(ALL_PIDS[a] != ALL_PIDS[b],
                "PIDs at index {a} and {b} must differ, both are {:#06x}",
                ALL_PIDS[a]);
        }
    }

    /// identify_device must return non-negative and finite torque when present.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let identity = identify_device(pid);
        if let Some(torque) = identity.max_torque_nm {
            prop_assert!(torque >= 0.0,
                "identify_device({pid:#06x}).max_torque_nm must be >= 0, got {torque}");
            prop_assert!(torque.is_finite(),
                "identify_device({pid:#06x}).max_torque_nm must be finite, got {torque}");
        }
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_wheelbase_torque_positive(idx in 0usize..3usize) {
        let pid = WHEELBASE_PIDS[idx];
        let identity = identify_device(pid);
        prop_assert!(identity.max_torque_nm.is_some(),
            "Wheelbase PID {pid:#06x} must have max_torque_nm");
        if let Some(torque) = identity.max_torque_nm {
            prop_assert!(torque > 0.0,
                "Wheelbase PID {pid:#06x} must have positive torque, got {torque}");
        }
    }

    /// identify_device name must be non-empty for any PID.
    #[test]
    fn prop_identify_name_non_empty(pid: u16) {
        let identity = identify_device(pid);
        prop_assert!(!identity.name.is_empty(),
            "identify_device({pid:#06x}).name must not be empty");
    }

    /// identify_device name must contain "VRS" for all known PIDs.
    #[test]
    fn prop_identify_name_contains_brand(idx in 0usize..8usize) {
        let pid = ALL_PIDS[idx];
        let identity = identify_device(pid);
        prop_assert!(identity.name.contains("VRS"),
            "identify_device({pid:#06x}).name must contain 'VRS', got '{}'",
            identity.name);
    }

    /// is_wheelbase_product must return true for all wheelbase PIDs.
    #[test]
    fn prop_wheelbase_classification(idx in 0usize..3usize) {
        let pid = WHEELBASE_PIDS[idx];
        prop_assert!(is_wheelbase_product(pid),
            "PID {pid:#06x} must be classified as wheelbase");
    }

    /// Wheelbase identity must report supports_ffb = true.
    #[test]
    fn prop_wheelbase_supports_ffb(idx in 0usize..3usize) {
        let pid = WHEELBASE_PIDS[idx];
        let identity = identify_device(pid);
        prop_assert!(identity.supports_ffb,
            "Wheelbase PID {pid:#06x} must support FFB");
    }

    /// Round-trip: PID → identity → PID must preserve product_id.
    #[test]
    fn prop_roundtrip_pid_identity(idx in 0usize..8usize) {
        let pid = ALL_PIDS[idx];
        let identity = identify_device(pid);
        prop_assert_eq!(identity.product_id, pid);
    }
}
