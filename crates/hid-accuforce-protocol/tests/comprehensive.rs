//! Comprehensive tests for the AccuForce HID protocol crate.
//!
//! Covers: device identification, report constants, type classification,
//! edge cases, property tests, and constant validation.

use proptest::prelude::*;
use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_ACCUFORCE_PRO,
    RECOMMENDED_B_INTERVAL_MS, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// ── Constant validation ──────────────────────────────────────────────────────

#[test]
fn vendor_id_is_nxp() {
    assert_eq!(VENDOR_ID, 0x1FC9);
}

#[test]
fn accuforce_pro_pid() {
    assert_eq!(PID_ACCUFORCE_PRO, 0x804C);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn max_report_bytes_within_usb_full_speed() {
    assert!(MAX_REPORT_BYTES <= 64);
    assert!(MAX_REPORT_BYTES > 0);
}

#[test]
fn hid_pid_usage_page_standard() {
    assert_eq!(HID_PID_USAGE_PAGE, 0x000F);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn recommended_interval_positive() {
    assert!(RECOMMENDED_B_INTERVAL_MS > 0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn recommended_interval_reasonable_range() {
    // USB polling intervals typically 1-255 ms; u8 guarantees <=255
    assert!(RECOMMENDED_B_INTERVAL_MS >= 1);
}

// ── Device identification: is_accuforce ──────────────────────────────────────

#[test]
fn is_accuforce_correct_vid_pid() {
    assert!(is_accuforce(VENDOR_ID, PID_ACCUFORCE_PRO));
}

#[test]
fn is_accuforce_wrong_vid() {
    assert!(!is_accuforce(0x0000, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(0xFFFF, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(0x045B, PID_ACCUFORCE_PRO)); // FFBeast
    assert!(!is_accuforce(0x2433, PID_ACCUFORCE_PRO)); // Asetek
    assert!(!is_accuforce(0x16D0, PID_ACCUFORCE_PRO)); // Simucube
}

#[test]
fn is_accuforce_wrong_pid() {
    assert!(!is_accuforce(VENDOR_ID, 0x0000));
    assert!(!is_accuforce(VENDOR_ID, 0xFFFF));
    assert!(!is_accuforce(VENDOR_ID, 0x804B)); // one below
    assert!(!is_accuforce(VENDOR_ID, 0x804D)); // one above
}

#[test]
fn is_accuforce_wrong_vid_and_pid() {
    assert!(!is_accuforce(0x0000, 0x0000));
    assert!(!is_accuforce(0xFFFF, 0xFFFF));
}

// ── Device identification: is_accuforce_pid ──────────────────────────────────

#[test]
fn is_accuforce_pid_known() {
    assert!(is_accuforce_pid(PID_ACCUFORCE_PRO));
}

#[test]
fn is_accuforce_pid_unknown() {
    assert!(!is_accuforce_pid(0x0000));
    assert!(!is_accuforce_pid(0xFFFF));
    assert!(!is_accuforce_pid(0x804B));
    assert!(!is_accuforce_pid(0x804D));
}

#[test]
fn is_accuforce_pid_agrees_with_is_accuforce() {
    // With the correct VID, is_accuforce should agree with is_accuforce_pid
    let test_pids: &[u16] = &[0x0000, 0x0001, PID_ACCUFORCE_PRO, 0x804B, 0x804D, 0xFFFF];
    for &pid in test_pids {
        assert_eq!(
            is_accuforce(VENDOR_ID, pid),
            is_accuforce_pid(pid),
            "disagreement for PID {pid:#06X}"
        );
    }
}

// ── AccuForceModel ───────────────────────────────────────────────────────────

#[test]
fn model_from_known_pid() {
    assert_eq!(
        AccuForceModel::from_product_id(PID_ACCUFORCE_PRO),
        AccuForceModel::Pro
    );
}

#[test]
fn model_from_unknown_pid() {
    assert_eq!(
        AccuForceModel::from_product_id(0x0000),
        AccuForceModel::Unknown
    );
    assert_eq!(
        AccuForceModel::from_product_id(0xFFFF),
        AccuForceModel::Unknown
    );
    assert_eq!(
        AccuForceModel::from_product_id(0x804B),
        AccuForceModel::Unknown
    );
}

#[test]
fn model_display_name_pro() {
    assert_eq!(
        AccuForceModel::Pro.display_name(),
        "SimExperience AccuForce Pro"
    );
}

#[test]
fn model_display_name_unknown() {
    assert_eq!(
        AccuForceModel::Unknown.display_name(),
        "SimExperience AccuForce (unknown model)"
    );
}

#[test]
fn model_display_names_non_empty() {
    assert!(!AccuForceModel::Pro.display_name().is_empty());
    assert!(!AccuForceModel::Unknown.display_name().is_empty());
}

#[test]
fn model_display_names_contain_accuforce() {
    assert!(AccuForceModel::Pro.display_name().contains("AccuForce"));
    assert!(AccuForceModel::Unknown.display_name().contains("AccuForce"));
}

#[test]
fn model_max_torque_pro() {
    assert_eq!(AccuForceModel::Pro.max_torque_nm(), 7.0);
}

#[test]
fn model_max_torque_unknown() {
    assert_eq!(AccuForceModel::Unknown.max_torque_nm(), 7.0);
}

#[test]
fn model_max_torque_positive_and_finite() {
    assert!(AccuForceModel::Pro.max_torque_nm() > 0.0);
    assert!(AccuForceModel::Pro.max_torque_nm().is_finite());
    assert!(AccuForceModel::Unknown.max_torque_nm() > 0.0);
    assert!(AccuForceModel::Unknown.max_torque_nm().is_finite());
}

#[test]
fn model_equality() {
    assert_eq!(AccuForceModel::Pro, AccuForceModel::Pro);
    assert_eq!(AccuForceModel::Unknown, AccuForceModel::Unknown);
    assert_ne!(AccuForceModel::Pro, AccuForceModel::Unknown);
}

#[test]
fn model_copy_clone() {
    let model = AccuForceModel::Pro;
    let copied = model;
    #[allow(clippy::clone_on_copy)]
    let cloned = model.clone();
    assert_eq!(model, copied);
    assert_eq!(model, cloned);
}

#[test]
fn model_debug_output() {
    let debug_str = format!("{:?}", AccuForceModel::Pro);
    assert!(!debug_str.is_empty());
    let debug_str_unknown = format!("{:?}", AccuForceModel::Unknown);
    assert!(!debug_str_unknown.is_empty());
}

// ── DeviceInfo ───────────────────────────────────────────────────────────────

#[test]
fn device_info_from_valid_vid_pid() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_eq!(info.vendor_id, VENDOR_ID);
    assert_eq!(info.product_id, PID_ACCUFORCE_PRO);
    assert_eq!(info.model, AccuForceModel::Pro);
}

#[test]
fn device_info_from_wrong_vid() {
    let info = DeviceInfo::from_vid_pid(0x0000, PID_ACCUFORCE_PRO);
    assert_eq!(info.vendor_id, 0x0000);
    assert_eq!(info.product_id, PID_ACCUFORCE_PRO);
    // Model is resolved from PID only
    assert_eq!(info.model, AccuForceModel::Pro);
}

#[test]
fn device_info_from_unknown_pid() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, 0xDEAD);
    assert_eq!(info.vendor_id, VENDOR_ID);
    assert_eq!(info.product_id, 0xDEAD);
    assert_eq!(info.model, AccuForceModel::Unknown);
}

#[test]
fn device_info_preserves_arbitrary_vid_pid() {
    let info = DeviceInfo::from_vid_pid(0x1234, 0x5678);
    assert_eq!(info.vendor_id, 0x1234);
    assert_eq!(info.product_id, 0x5678);
}

#[test]
fn device_info_equality() {
    let a = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    let b = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    assert_eq!(a, b);
}

#[test]
fn device_info_inequality_different_vid() {
    let a = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    let b = DeviceInfo::from_vid_pid(0x0000, PID_ACCUFORCE_PRO);
    assert_ne!(a, b);
}

#[test]
fn device_info_inequality_different_pid() {
    let a = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    let b = DeviceInfo::from_vid_pid(VENDOR_ID, 0x0000);
    assert_ne!(a, b);
}

#[test]
fn device_info_copy_clone() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    let copied = info;
    #[allow(clippy::clone_on_copy)]
    let cloned = info.clone();
    assert_eq!(info, copied);
    assert_eq!(info, cloned);
}

#[test]
fn device_info_debug() {
    let info = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    let debug_str = format!("{:?}", info);
    assert!(!debug_str.is_empty());
}

// ── Edge cases ───────────────────────────────────────────────────────────────

#[test]
fn vid_pid_boundary_values() {
    // Min/max u16 boundary
    assert!(!is_accuforce(0x0000, 0x0000));
    assert!(!is_accuforce(u16::MAX, u16::MAX));
    assert!(!is_accuforce(u16::MIN, PID_ACCUFORCE_PRO));
    assert!(!is_accuforce(u16::MAX, PID_ACCUFORCE_PRO));
}

#[test]
fn device_info_boundary_vid_pid() {
    let info_min = DeviceInfo::from_vid_pid(u16::MIN, u16::MIN);
    assert_eq!(info_min.vendor_id, u16::MIN);
    assert_eq!(info_min.product_id, u16::MIN);
    assert_eq!(info_min.model, AccuForceModel::Unknown);

    let info_max = DeviceInfo::from_vid_pid(u16::MAX, u16::MAX);
    assert_eq!(info_max.vendor_id, u16::MAX);
    assert_eq!(info_max.product_id, u16::MAX);
    assert_eq!(info_max.model, AccuForceModel::Unknown);
}

#[test]
fn no_other_vendor_products_match() {
    // Common wheelbase VIDs should not match AccuForce
    let other_vids: &[u16] = &[
        0x045B, // FFBeast / Renesas
        0x2433, // Asetek
        0x16D0, // Simucube / MCS
        0x1DD2, // Leo Bodnar
        0x0EB7, // Fanatec / Endor
        0x346E, // Moza
    ];
    for &vid in other_vids {
        assert!(
            !is_accuforce(vid, PID_ACCUFORCE_PRO),
            "VID {vid:#06X} should not match AccuForce"
        );
    }
}

// ── Property tests ───────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(500))]

    #[test]
    fn prop_is_accuforce_wrong_vid_never_matches(vid: u16, pid: u16) {
        if vid != VENDOR_ID {
            prop_assert!(!is_accuforce(vid, pid));
        }
    }

    #[test]
    fn prop_is_accuforce_unknown_pid_never_matches(pid: u16) {
        if pid != PID_ACCUFORCE_PRO {
            prop_assert!(!is_accuforce(VENDOR_ID, pid));
        }
    }

    #[test]
    fn prop_is_accuforce_pid_agrees(pid: u16) {
        prop_assert_eq!(
            is_accuforce(VENDOR_ID, pid),
            is_accuforce_pid(pid)
        );
    }

    #[test]
    fn prop_recognised_pid_not_unknown(pid: u16) {
        if is_accuforce_pid(pid) {
            prop_assert_ne!(AccuForceModel::from_product_id(pid), AccuForceModel::Unknown);
        }
    }

    #[test]
    fn prop_unrecognised_pid_is_unknown(pid: u16) {
        if !is_accuforce_pid(pid) {
            prop_assert_eq!(AccuForceModel::from_product_id(pid), AccuForceModel::Unknown);
        }
    }

    #[test]
    fn prop_display_name_non_empty(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(!model.display_name().is_empty());
    }

    #[test]
    fn prop_display_name_contains_accuforce(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(
            model.display_name().contains("AccuForce"),
            "display_name {:?} must contain 'AccuForce'",
            model.display_name()
        );
    }

    #[test]
    fn prop_max_torque_positive(pid: u16) {
        let model = AccuForceModel::from_product_id(pid);
        prop_assert!(model.max_torque_nm() > 0.0);
        prop_assert!(model.max_torque_nm().is_finite());
    }

    #[test]
    fn prop_device_info_preserves_vid_pid(vid: u16, pid: u16) {
        let info = DeviceInfo::from_vid_pid(vid, pid);
        prop_assert_eq!(info.vendor_id, vid);
        prop_assert_eq!(info.product_id, pid);
    }

    #[test]
    fn prop_device_info_model_consistent(vid: u16, pid: u16) {
        let info = DeviceInfo::from_vid_pid(vid, pid);
        let expected = AccuForceModel::from_product_id(pid);
        prop_assert_eq!(info.model, expected);
    }
}
