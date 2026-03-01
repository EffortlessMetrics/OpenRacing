//! Property-based tests for Heusinkveld device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - All known PIDs are unique (no duplicates)
//! - HeusinkveldModel classification and max_load_kg bounds
//! - Round-trip: PID → model → display_name is consistent

use hid_heusinkveld_protocol::{
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
    HeusinkveldModel, heusinkveld_model_from_info, is_heusinkveld_device,
};
use proptest::prelude::*;

/// All known Heusinkveld product IDs.
const ALL_PIDS: [u16; 3] = [
    HEUSINKVELD_SPRINT_PID,
    HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_PRO_PID,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// HEUSINKVELD_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(HEUSINKVELD_VENDOR_ID != 0,
            "HEUSINKVELD_VENDOR_ID must not be zero");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_pids_unique(idx_a in 0usize..3usize, idx_b in 0usize..3usize) {
        if idx_a != idx_b {
            prop_assert!(ALL_PIDS[idx_a] != ALL_PIDS[idx_b],
                "PIDs at index {idx_a} and {idx_b} must differ, both are {:#06x}",
                ALL_PIDS[idx_a]);
        }
    }

    /// HEUSINKVELD_VENDOR_ID must match expected value (0x04D8).
    #[test]
    fn prop_vendor_id_value(_unused: u8) {
        prop_assert_eq!(HEUSINKVELD_VENDOR_ID, 0x04D8,
            "HEUSINKVELD_VENDOR_ID must be 0x04D8");
    }

    /// HeusinkveldModel::from_product_id must return a known variant for all known PIDs.
    #[test]
    fn prop_known_pid_resolves_to_model(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        prop_assert!(model != HeusinkveldModel::Unknown,
            "HeusinkveldModel::from_product_id({pid:#06x}) must not return Unknown");
    }

    /// HeusinkveldModel::max_load_kg must be positive and finite for all known models.
    #[test]
    fn prop_max_load_positive_and_finite(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        let load = model.max_load_kg();
        prop_assert!(load > 0.0,
            "{model:?} must have positive max_load_kg, got {load}");
        prop_assert!(load.is_finite(),
            "{model:?} must have finite max_load_kg, got {load}");
    }

    /// HeusinkveldModel::pedal_count must be in [2, 3] for all known models.
    #[test]
    fn prop_pedal_count_range(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        let count = model.pedal_count();
        prop_assert!((2..=3).contains(&count),
            "{model:?} must have pedal_count in [2, 3], got {count}");
    }

    /// display_name must be non-empty for all known PIDs.
    #[test]
    fn prop_display_name_non_empty(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        let name = model.display_name();
        prop_assert!(!name.is_empty(),
            "display_name for PID {pid:#06x} must not be empty");
    }

    /// display_name must contain "Heusinkveld" for all known PIDs.
    #[test]
    fn prop_display_name_contains_brand(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        let name = model.display_name();
        prop_assert!(name.contains("Heusinkveld"),
            "display_name for PID {pid:#06x} must contain 'Heusinkveld', got '{name}'");
    }

    /// is_heusinkveld_device must return true for the correct VID.
    #[test]
    fn prop_is_heusinkveld_device(_unused: u8) {
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_VENDOR_ID),
            "is_heusinkveld_device must return true for HEUSINKVELD_VENDOR_ID");
    }

    /// heusinkveld_model_from_info must return a known model for known VID+PID pairs.
    #[test]
    fn prop_model_from_info_known(idx in 0usize..3usize) {
        let pid = ALL_PIDS[idx];
        let model = heusinkveld_model_from_info(HEUSINKVELD_VENDOR_ID, pid);
        prop_assert!(model != HeusinkveldModel::Unknown,
            "heusinkveld_model_from_info(VID, {pid:#06x}) must not return Unknown");
    }

    /// heusinkveld_model_from_info must return Unknown for wrong VID.
    #[test]
    fn prop_model_from_info_wrong_vid(vid in 0u16..=u16::MAX) {
        prop_assume!(vid != HEUSINKVELD_VENDOR_ID);
        for &pid in &ALL_PIDS {
            let model = heusinkveld_model_from_info(vid, pid);
            prop_assert!(model == HeusinkveldModel::Unknown,
                "heusinkveld_model_from_info({:#06x}, {:#06x}) must return Unknown for wrong VID",
                vid, pid);
        }
    }

    /// Random PIDs should resolve to Unknown.
    #[test]
    fn prop_unknown_pid_returns_unknown(pid: u16) {
        prop_assume!(!ALL_PIDS.contains(&pid));
        let model = HeusinkveldModel::from_product_id(pid);
        prop_assert!(model == HeusinkveldModel::Unknown,
            "HeusinkveldModel::from_product_id({:#06x}) must return Unknown for unknown PID",
            pid);
    }
}
