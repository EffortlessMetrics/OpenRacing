//! Integration tests for the AccuForce protocol crate.

use crate::ids::{VENDOR_ID, PID_ACCUFORCE_PRO, is_accuforce, is_accuforce_pid};
use crate::types::{AccuForceModel, DeviceInfo};

const ALL_KNOWN_PIDS: &[u16] = &[PID_ACCUFORCE_PRO];

#[test]
fn all_known_pids_are_recognised() {
    for &pid in ALL_KNOWN_PIDS {
        assert!(
            is_accuforce_pid(pid),
            "PID 0x{pid:04X} must be recognised as an AccuForce device"
        );
    }
}

#[test]
fn all_known_pids_resolve_to_non_unknown_model() {
    for &pid in ALL_KNOWN_PIDS {
        let model = AccuForceModel::from_product_id(pid);
        assert_ne!(
            model,
            AccuForceModel::Unknown,
            "PID 0x{pid:04X} must resolve to a named AccuForce model"
        );
    }
}

#[test]
fn unknown_pid_not_recognised() {
    let unknown = [0x0000u16, 0x0001, 0x1234, 0xDEAD, 0xFFFF];
    for &pid in &unknown {
        assert!(
            !is_accuforce_pid(pid),
            "PID 0x{pid:04X} must not be recognised"
        );
    }
}

#[test]
fn wrong_vid_not_recognised() {
    assert!(!is_accuforce(0x16D0, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(0x1DD2, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(0x0000, PID_ACCUFORCE_PRO));
}

#[test]
fn device_info_model_matches_pid_function() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_eq!(info.model, AccuForceModel::from_product_id(PID_ACCUFORCE_PRO));
}

#[test]
fn vendor_id_is_nxp_usb() {
    assert_eq!(VENDOR_ID, 0x1FC9);
}

// ── Proptest property tests ───────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(512))]

    /// `is_accuforce_pid` and `AccuForceModel::from_product_id` must agree:
    /// a recognised PID must not resolve to `Unknown`.
    #[test]
    fn prop_recognised_pid_resolves_to_named_model(pid in any::<u16>()) {
        if is_accuforce_pid(pid) {
            prop_assert_ne!(
                AccuForceModel::from_product_id(pid),
                AccuForceModel::Unknown,
                "recognised PID 0x{:04X} must not resolve to Unknown",
                pid
            );
        }
    }

    /// An unrecognised PID must resolve to `Unknown`.
    #[test]
    fn prop_unrecognised_pid_resolves_to_unknown(pid in any::<u16>()) {
        if !is_accuforce_pid(pid) {
            prop_assert_eq!(
                AccuForceModel::from_product_id(pid),
                AccuForceModel::Unknown,
                "unrecognised PID 0x{:04X} must resolve to Unknown",
                pid
            );
        }
    }

    /// Display names must never be empty for any PID.
    #[test]
    fn prop_display_name_non_empty(pid in any::<u16>()) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty());
    }

    /// max_torque_nm must always be positive.
    #[test]
    fn prop_max_torque_positive(pid in any::<u16>()) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(model.max_torque_nm() > 0.0);
    }
}
