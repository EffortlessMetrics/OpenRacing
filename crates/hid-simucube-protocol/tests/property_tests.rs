//! Property-based tests for the Simucube HID protocol.
//!
//! Uses proptest with 500 cases to verify invariants on model detection,
//! torque encoding/clamping, and output report construction.

use hid_simucube_protocol::{
    MAX_TORQUE_NM, SIMUCUBE_1_PID, SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID,
    SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_VENDOR_ID, SimucubeModel, SimucubeOutputReport,
    WheelCapabilities, WheelModel, is_simucube_device, simucube_model_from_info,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    // -- Model detection: determinism -----------------------------------------

    /// SimucubeModel::from_product_id must return the same model for the same PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid: u16) {
        let a = SimucubeModel::from_product_id(pid);
        let b = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(a, b, "model must be stable for pid={:#06x}", pid);
    }

    /// simucube_model_from_info with the correct VID must match from_product_id.
    #[test]
    fn prop_model_from_info_matches_from_pid_for_correct_vid(pid: u16) {
        let via_info = simucube_model_from_info(SIMUCUBE_VENDOR_ID, pid);
        let via_pid = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(
            via_info, via_pid,
            "simucube_model_from_info with correct VID must match from_product_id for pid={:#06x}",
            pid
        );
    }

    /// simucube_model_from_info with any VID other than SIMUCUBE_VENDOR_ID must return Unknown.
    #[test]
    fn prop_wrong_vid_always_unknown(vid: u16, pid: u16) {
        prop_assume!(vid != SIMUCUBE_VENDOR_ID);
        let model = simucube_model_from_info(vid, pid);
        prop_assert_eq!(
            model,
            SimucubeModel::Unknown,
            "wrong VID {:#06x} must always return Unknown (pid={:#06x})",
            vid,
            pid
        );
    }

    // -- Model detection: Sport / Pro / Ultimate PIDs -------------------------

    /// Known Simucube PIDs must map to the correct model variant.
    #[test]
    fn prop_known_pids_return_correct_model(idx in 0usize..4usize) {
        let pairs = [
            (SIMUCUBE_1_PID,          SimucubeModel::Simucube1),
            (SIMUCUBE_2_SPORT_PID,    SimucubeModel::Sport),
            (SIMUCUBE_2_PRO_PID,      SimucubeModel::Pro),
            (SIMUCUBE_2_ULTIMATE_PID, SimucubeModel::Ultimate),
        ];
        let (pid, expected) = pairs[idx];
        let model = SimucubeModel::from_product_id(pid);
        prop_assert_eq!(model, expected, "PID {:#06x} must return model {:?}", pid, expected);
    }

    /// is_simucube_device must return true only for SIMUCUBE_VENDOR_ID.
    #[test]
    fn prop_is_simucube_device_vid_check(vid: u16) {
        let result = is_simucube_device(vid);
        if vid == SIMUCUBE_VENDOR_ID {
            prop_assert!(result, "SIMUCUBE_VENDOR_ID must be recognized");
        } else {
            prop_assert!(!result, "VID {:#06x} must not be recognized as Simucube", vid);
        }
    }

    // -- Torque encoding: clamping --------------------------------------------

    /// with_torque must always clamp the result to +/-MAX_TORQUE_NM.
    #[test]
    fn prop_torque_clamped_to_max(torque in -200.0f32..200.0f32) {
        let report = SimucubeOutputReport::new(0).with_torque(torque);
        let clamped_nm = report.torque_cNm as f32 / 100.0;
        prop_assert!(
            (-MAX_TORQUE_NM..=MAX_TORQUE_NM).contains(&clamped_nm),
            "clamped value {} must be within +/-{}",
            clamped_nm,
            MAX_TORQUE_NM
        );
    }

    // -- Torque encoding: sign preservation -----------------------------------

    /// A non-negative torque must produce a non-negative torque_cNm.
    #[test]
    fn prop_nonneg_torque_nonneg_cnm(torque in 0.0f32..200.0f32) {
        let report = SimucubeOutputReport::new(0).with_torque(torque);
        prop_assert!(
            report.torque_cNm >= 0,
            "non-negative torque {} must give torque_cNm >= 0, got {}",
            torque,
            report.torque_cNm
        );
    }

    /// A non-positive torque must produce a non-positive torque_cNm.
    #[test]
    fn prop_nonpos_torque_nonpos_cnm(torque in -200.0f32..0.0f32) {
        let report = SimucubeOutputReport::new(0).with_torque(torque);
        prop_assert!(
            report.torque_cNm <= 0,
            "non-positive torque {} must give torque_cNm <= 0, got {}",
            torque,
            report.torque_cNm
        );
    }

    // -- Output report: build -------------------------------------------------

    /// build() must always succeed (never error) for any valid torque and sequence.
    #[test]
    fn prop_build_always_succeeds(torque in -200.0f32..200.0f32, seq: u16) {
        let report = SimucubeOutputReport::new(seq).with_torque(torque);
        let result = report.build();
        prop_assert!(result.is_ok(), "build() must always succeed");
        if let Ok(data) = result {
            prop_assert!(!data.is_empty(), "built report must be non-empty");
        }
    }

    // -- WheelCapabilities ----------------------------------------------------

    /// Wheelbase models must report a strictly positive max torque capability.
    #[test]
    fn prop_wheelbase_capabilities_positive_torque(idx in 0usize..3usize) {
        let models = [
            WheelModel::Simucube2Sport,
            WheelModel::Simucube2Pro,
            WheelModel::Simucube2Ultimate,
        ];
        let caps = WheelCapabilities::for_model(models[idx]);
        prop_assert!(
            caps.max_torque_nm > 0.0,
            "wheelbase model must have positive max torque"
        );
        prop_assert!(
            caps.encoder_resolution_bits >= 16,
            "encoder resolution must be at least 16 bits"
        );
    }
}

/// Simucube 2 Sport torque must be strictly less than Pro, which must be less than Ultimate.
#[test]
fn test_torque_ordering_sport_lt_pro_lt_ultimate() -> Result<(), Box<dyn std::error::Error>> {
    let sport = SimucubeModel::Sport.max_torque_nm();
    let pro = SimucubeModel::Pro.max_torque_nm();
    let ultimate = SimucubeModel::Ultimate.max_torque_nm();
    assert!(sport > 0.0, "Sport max torque must be positive");
    assert!(pro > sport, "Pro ({pro} Nm) must exceed Sport ({sport} Nm)");
    assert!(
        ultimate > pro,
        "Ultimate ({ultimate} Nm) must exceed Pro ({pro} Nm)"
    );
    Ok(())
}
