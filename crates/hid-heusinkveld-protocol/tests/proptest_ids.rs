//! Property-based tests for Heusinkveld device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - All known PIDs are unique (no duplicates)
//! - HeusinkveldModel classification and max_load_kg bounds
//! - Round-trip: PID → model → display_name is consistent

use hid_heusinkveld_protocol::{
    HEUSINKVELD_HANDBRAKE_V1_PID, HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID,
    HEUSINKVELD_HANDBRAKE_V2_PID, HEUSINKVELD_LEGACY_SPRINT_PID, HEUSINKVELD_LEGACY_ULTIMATE_PID,
    HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID, HEUSINKVELD_SHIFTER_PID,
    HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_VENDOR_ID, HeusinkveldModel, is_heusinkveld_device,
};
use proptest::prelude::*;

/// All known Heusinkveld product IDs (across all VIDs).
const ALL_PIDS: [u16; 8] = [
    HEUSINKVELD_SPRINT_PID,
    HEUSINKVELD_HANDBRAKE_V2_PID,
    HEUSINKVELD_ULTIMATE_PID,
    HEUSINKVELD_LEGACY_SPRINT_PID,
    HEUSINKVELD_LEGACY_ULTIMATE_PID,
    HEUSINKVELD_PRO_PID,
    HEUSINKVELD_HANDBRAKE_V1_PID,
    HEUSINKVELD_SHIFTER_PID,
];

/// All known VID/PID pairs.
const ALL_VID_PID_PAIRS: [(u16, u16); 8] = [
    (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
    (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID),
    (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V2_PID),
    (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID),
    (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_ULTIMATE_PID),
    (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID),
    (HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID, HEUSINKVELD_HANDBRAKE_V1_PID),
    (HEUSINKVELD_SHIFTER_VENDOR_ID, HEUSINKVELD_SHIFTER_PID),
];

/// All `HeusinkveldModel` enum variants (for exhaustive variant-level tests).
const ALL_VARIANTS: [HeusinkveldModel; 7] = [
    HeusinkveldModel::Sprint,
    HeusinkveldModel::Ultimate,
    HeusinkveldModel::Pro,
    HeusinkveldModel::HandbrakeV1,
    HeusinkveldModel::HandbrakeV2,
    HeusinkveldModel::SequentialShifter,
    HeusinkveldModel::Unknown,
];

/// Pedal-only PIDs (for load/pedal-count tests).
const PEDAL_PIDS: [(u16, u16); 5] = [
    (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID),
    (HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID),
    (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_SPRINT_PID),
    (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_LEGACY_ULTIMATE_PID),
    (HEUSINKVELD_LEGACY_VENDOR_ID, HEUSINKVELD_PRO_PID),
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// HEUSINKVELD_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(HEUSINKVELD_VENDOR_ID != 0,
            "HEUSINKVELD_VENDOR_ID must not be zero");
        prop_assert!(HEUSINKVELD_LEGACY_VENDOR_ID != 0,
            "HEUSINKVELD_LEGACY_VENDOR_ID must not be zero");
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
    fn prop_pids_unique(idx_a in 0usize..8usize, idx_b in 0usize..8usize) {
        if idx_a != idx_b {
            prop_assert!(ALL_PIDS[idx_a] != ALL_PIDS[idx_b],
                "PIDs at index {idx_a} and {idx_b} must differ, both are {:#06x}",
                ALL_PIDS[idx_a]);
        }
    }

    /// HEUSINKVELD_VENDOR_ID must match expected value (0x30B7).
    #[test]
    fn prop_vendor_id_value(_unused: u8) {
        prop_assert_eq!(HEUSINKVELD_VENDOR_ID, 0x30B7,
            "HEUSINKVELD_VENDOR_ID must be 0x30B7");
    }

    /// HeusinkveldModel::from_product_id must return a known variant for all known PIDs.
    #[test]
    fn prop_known_pid_resolves_to_model(idx in 0usize..8usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        prop_assert!(model != HeusinkveldModel::Unknown,
            "HeusinkveldModel::from_product_id({pid:#06x}) must not return Unknown");
    }

    /// HeusinkveldModel::max_load_kg must be positive and finite for pedal models.
    #[test]
    fn prop_max_load_positive_and_finite(idx in 0usize..5usize) {
        let (vid, pid) = PEDAL_PIDS[idx];
        let model = HeusinkveldModel::from_vid_pid(vid, pid);
        let load = model.max_load_kg();
        prop_assert!(load > 0.0,
            "{model:?} must have positive max_load_kg, got {load}");
        prop_assert!(load.is_finite(),
            "{model:?} must have finite max_load_kg, got {load}");
    }

    /// HeusinkveldModel::pedal_count must be in [2, 3] for pedal models.
    #[test]
    fn prop_pedal_count_range(idx in 0usize..5usize) {
        let (vid, pid) = PEDAL_PIDS[idx];
        let model = HeusinkveldModel::from_vid_pid(vid, pid);
        let count = model.pedal_count();
        prop_assert!((2..=3).contains(&count),
            "{model:?} must have pedal_count in [2, 3], got {count}");
    }

    /// display_name must be non-empty for all known PIDs.
    #[test]
    fn prop_display_name_non_empty(idx in 0usize..8usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        let name = model.display_name();
        prop_assert!(!name.is_empty(),
            "display_name for PID {pid:#06x} must not be empty");
    }

    /// display_name must contain "Heusinkveld" for all known PIDs.
    #[test]
    fn prop_display_name_contains_brand(idx in 0usize..8usize) {
        let pid = ALL_PIDS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        let name = model.display_name();
        prop_assert!(name.contains("Heusinkveld"),
            "display_name for PID {pid:#06x} must contain 'Heusinkveld', got '{name}'");
    }

    /// is_heusinkveld_device must return true for all known VIDs.
    #[test]
    fn prop_is_heusinkveld_device(_unused: u8) {
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_VENDOR_ID),
            "is_heusinkveld_device must return true for current VID");
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_LEGACY_VENDOR_ID),
            "is_heusinkveld_device must return true for legacy VID");
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_HANDBRAKE_V1_VENDOR_ID),
            "is_heusinkveld_device must return true for handbrake V1 VID");
        prop_assert!(is_heusinkveld_device(HEUSINKVELD_SHIFTER_VENDOR_ID),
            "is_heusinkveld_device must return true for shifter VID");
    }

    /// Random PIDs that are not in ALL_PIDS should resolve to Unknown.
    #[test]
    fn prop_unknown_pid_returns_unknown(pid: u16) {
        prop_assume!(!ALL_PIDS.contains(&pid));
        let model = HeusinkveldModel::from_product_id(pid);
        prop_assert!(model == HeusinkveldModel::Unknown,
            "HeusinkveldModel::from_product_id({:#06x}) must return Unknown for unknown PID",
            pid);
    }

    // ── from_product_id consistency with is_heusinkveld_device ────────────────

    /// For every known VID/PID pair, from_product_id must return non-Unknown,
    /// and is_heusinkveld_device must recognise the VID.
    #[test]
    fn prop_from_product_id_consistent_with_is_heusinkveld(idx in 0usize..8usize) {
        let (vid, pid) = ALL_VID_PID_PAIRS[idx];
        let model = HeusinkveldModel::from_product_id(pid);
        prop_assert_ne!(model, HeusinkveldModel::Unknown,
            "from_product_id({:#06x}) must not return Unknown for known PID", pid);
        prop_assert!(is_heusinkveld_device(vid),
            "is_heusinkveld_device({:#06x}) must be true for VID paired with known PID {:#06x}",
            vid, pid);
    }

    /// For any random VID/PID, if from_vid_pid returns non-Unknown then
    /// is_heusinkveld_device must also return true for that VID.
    #[test]
    fn prop_from_vid_pid_implies_is_heusinkveld(vid: u16, pid: u16) {
        let model = HeusinkveldModel::from_vid_pid(vid, pid);
        if model != HeusinkveldModel::Unknown {
            prop_assert!(is_heusinkveld_device(vid),
                "from_vid_pid({:#06x}, {:#06x}) = {model:?} but is_heusinkveld_device is false",
                vid, pid);
        }
    }

    // ── All model variants: display_name non-empty ───────────────────────────

    /// display_name must be non-empty for every HeusinkveldModel variant.
    #[test]
    fn prop_all_variants_display_name_non_empty(idx in 0usize..7usize) {
        let variant = ALL_VARIANTS[idx];
        let name = variant.display_name();
        prop_assert!(!name.is_empty(),
            "{variant:?} must have a non-empty display_name");
        prop_assert!(name.contains("Heusinkveld"),
            "{variant:?} display_name '{name}' must contain 'Heusinkveld'");
    }
}
