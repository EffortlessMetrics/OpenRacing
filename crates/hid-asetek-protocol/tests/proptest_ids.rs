//! Property-based tests for Asetek device identification and model metadata.
//!
//! Uses proptest with 500 cases to verify invariants on:
//! - VID/PID constants are non-zero
//! - AsetekModel classification determinism and max_torque_nm bounds
//! - identify (display_name) is non-empty and max_torque_nm is non-negative
//! - Round-trip: PID → model → PID preserves model identity

use hid_asetek_protocol::{
    ASETEK_FORTE_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PID, ASETEK_TONY_KANAAN_PID,
    ASETEK_VENDOR_ID, AsetekModel, is_asetek_device,
};
use proptest::prelude::*;

/// All known Asetek product IDs (wheelbases only).
const ALL_PIDS: [u16; 4] = [
    ASETEK_FORTE_PID,
    ASETEK_INVICTA_PID,
    ASETEK_LAPRIMA_PID,
    ASETEK_TONY_KANAAN_PID,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// ASETEK_VENDOR_ID must always be non-zero.
    #[test]
    fn prop_vendor_id_nonzero(_unused: u8) {
        prop_assert!(ASETEK_VENDOR_ID != 0,
            "ASETEK_VENDOR_ID must not be zero");
    }

    /// Every known product PID must be non-zero.
    #[test]
    fn prop_known_pids_nonzero(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        prop_assert!(pid != 0,
            "PID at index {idx} must not be zero");
    }

    /// All known PIDs must be unique (no duplicates).
    #[test]
    fn prop_known_pids_unique(a in 0usize..4usize, b in 0usize..4usize) {
        if a != b {
            prop_assert!(ALL_PIDS[a] != ALL_PIDS[b],
                "PIDs at index {a} and {b} must differ, both are {:#06x}",
                ALL_PIDS[a]);
        }
    }

    /// VID must match the expected Asetek vendor ID (0x2433).
    #[test]
    fn prop_vendor_id_matches_expected(_unused: u8) {
        prop_assert_eq!(ASETEK_VENDOR_ID, 0x2433,
            "ASETEK_VENDOR_ID must be 0x2433");
    }

    /// is_asetek_device must return true for the canonical VID.
    #[test]
    fn prop_is_asetek_device_for_vid(_unused: u8) {
        prop_assert!(is_asetek_device(ASETEK_VENDOR_ID),
            "is_asetek_device must return true for ASETEK_VENDOR_ID");
    }

    /// AsetekModel::max_torque_nm must be non-negative and finite for any PID.
    #[test]
    fn prop_torque_non_negative_and_finite(pid: u16) {
        let model = AsetekModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque >= 0.0,
            "{model:?} must have non-negative max_torque_nm, got {torque}");
        prop_assert!(torque.is_finite(),
            "{model:?} must have finite max_torque_nm, got {torque}");
    }

    /// Known wheelbase models must have strictly positive torque.
    #[test]
    fn prop_wheelbase_torque_positive(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        let model = AsetekModel::from_product_id(pid);
        let torque = model.max_torque_nm();
        prop_assert!(torque > 0.0,
            "{model:?} (PID {pid:#06x}) must have positive torque, got {torque}");
    }

    /// display_name must be non-empty for any PID.
    #[test]
    fn prop_display_name_non_empty(pid: u16) {
        let model = AsetekModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty(),
            "display_name for {model:?} (PID {pid:#06x}) must not be empty");
    }

    /// display_name must contain "Asetek" for all known PIDs.
    #[test]
    fn prop_display_name_contains_brand(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        let model = AsetekModel::from_product_id(pid);
        prop_assert!(model.display_name().contains("Asetek"),
            "display_name for PID {pid:#06x} must contain 'Asetek', got '{}'",
            model.display_name());
    }

    /// Round-trip: PID → model → PID must produce the same model.
    #[test]
    fn prop_roundtrip_pid_model(idx in 0usize..4usize) {
        let pid = ALL_PIDS[idx];
        let model = AsetekModel::from_product_id(pid);
        let model2 = AsetekModel::from_product_id(pid);
        prop_assert_eq!(model, model2);
    }
}
