//! Tests for Cube Controls protocol handler.
//!
//! Note: All Cube Controls PIDs are PROVISIONAL (unconfirmed). Tests are
//! structured to verify handler behaviour; the PID values must be updated once
//! confirmed from real hardware.

use super::cube_controls::{
    is_cube_controls_product, CubeControlsModel, CubeControlsProtocolHandler,
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID,
};
use super::{get_vendor_protocol, DeviceWriter, VendorProtocol};
use std::cell::RefCell;

struct MockDeviceWriter {
    feature_reports: RefCell<Vec<Vec<u8>>>,
    output_reports: RefCell<Vec<Vec<u8>>>,
}

impl MockDeviceWriter {
    fn new() -> Self {
        Self {
            feature_reports: RefCell::new(Vec::new()),
            output_reports: RefCell::new(Vec::new()),
        }
    }

    fn feature_reports(&self) -> Vec<Vec<u8>> {
        self.feature_reports.borrow().clone()
    }
}

impl DeviceWriter for MockDeviceWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.borrow_mut().push(data.to_vec());
        Ok(data.len())
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_reports.borrow_mut().push(data.to_vec());
        Ok(data.len())
    }
}

#[test]
fn test_new_gt_pro() {
    let handler = CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    assert_eq!(handler.model(), CubeControlsModel::GtPro);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 20.0).abs() < 0.01);
}

#[test]
fn test_new_formula_pro() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_FORMULA_PRO_PID);
    assert_eq!(handler.model(), CubeControlsModel::FormulaPro);
}

#[test]
fn test_new_csx3() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_CSX3_PID);
    assert_eq!(handler.model(), CubeControlsModel::Csx3);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 20.0).abs() < 0.01);
}

#[test]
fn test_new_unknown_pid() {
    let handler = CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, 0x0C99);
    assert_eq!(handler.model(), CubeControlsModel::Unknown);
}

#[test]
fn test_initialize_sends_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    let mut writer = MockDeviceWriter::new();
    // Should not fail even with provisional PIDs
    handler.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "Cube Controls init must send no reports (standard HID PID assumed)"
    );
    Ok(())
}

#[test]
fn test_ffb_config() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    let config = handler.get_ffb_config();
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert_eq!(config.encoder_cpr, 0);
}

#[test]
fn test_is_v2_hardware() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn test_output_report() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    assert!(handler.output_report_id().is_none());
    assert!(handler.output_report_len().is_none());
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    let mut writer = MockDeviceWriter::new();
    handler.send_feature_report(&mut writer, 0x03, &[0xCA, 0xFE])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x03, 0xCA, 0xFE]);
    Ok(())
}

#[test]
fn test_send_feature_report_too_large() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    let mut writer = MockDeviceWriter::new();
    let big_payload = [0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x01, &big_payload);
    assert!(result.is_err(), "report exceeding 64 bytes must fail");
}

#[test]
fn test_is_cube_controls_product() {
    assert!(is_cube_controls_product(CUBE_CONTROLS_GT_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_FORMULA_PRO_PID));
    assert!(is_cube_controls_product(CUBE_CONTROLS_CSX3_PID));
    assert!(!is_cube_controls_product(0x1234));
    assert!(!is_cube_controls_product(0x0522)); // Simagic
}

#[test]
fn test_get_vendor_protocol_cube_controls() {
    let proto = get_vendor_protocol(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    assert!(
        proto.is_some(),
        "GT Pro must resolve to a vendor protocol (provisional PID)"
    );
    let proto = get_vendor_protocol(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_FORMULA_PRO_PID);
    assert!(proto.is_some(), "Formula Pro must resolve to a vendor protocol");
    let proto = get_vendor_protocol(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_CSX3_PID);
    assert!(proto.is_some(), "CSX3 must resolve to a vendor protocol");
}

#[test]
fn test_cube_controls_model_display_names() {
    assert_eq!(CubeControlsModel::GtPro.display_name(), "Cube Controls GT Pro");
    assert_eq!(
        CubeControlsModel::FormulaPro.display_name(),
        "Cube Controls Formula Pro"
    );
    assert_eq!(CubeControlsModel::Csx3.display_name(), "Cube Controls CSX3");
    assert!(!CubeControlsModel::Unknown.display_name().is_empty());
}

#[test]
fn test_all_models_are_provisional() {
    assert!(CubeControlsModel::GtPro.is_provisional());
    assert!(CubeControlsModel::FormulaPro.is_provisional());
    assert!(CubeControlsModel::Csx3.is_provisional());
    assert!(CubeControlsModel::Unknown.is_provisional());
}

// ── Insta snapshot tests ──────────────────────────────────────────────────────

#[test]
fn snapshot_ffb_config_gt_pro() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_GT_PRO_PID);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[test]
fn snapshot_ffb_config_formula_pro() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_FORMULA_PRO_PID);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[test]
fn snapshot_ffb_config_csx3() {
    let handler =
        CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, CUBE_CONTROLS_CSX3_PID);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[cfg(windows)]
#[test]
fn snapshot_device_caps_gt_pro() {
    let caps = super::super::windows::determine_device_capabilities(
        CUBE_CONTROLS_VENDOR_ID,
        CUBE_CONTROLS_GT_PRO_PID,
    );
    insta::assert_debug_snapshot!(caps);
}

#[cfg(windows)]
#[test]
fn snapshot_device_caps_formula_pro() {
    let caps = super::super::windows::determine_device_capabilities(
        CUBE_CONTROLS_VENDOR_ID,
        CUBE_CONTROLS_FORMULA_PRO_PID,
    );
    insta::assert_debug_snapshot!(caps);
}

#[cfg(windows)]
#[test]
fn snapshot_device_caps_csx3() {
    let caps = super::super::windows::determine_device_capabilities(
        CUBE_CONTROLS_VENDOR_ID,
        CUBE_CONTROLS_CSX3_PID,
    );
    insta::assert_debug_snapshot!(caps);
}

// ── Proptest property tests ───────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    /// is_cube_controls_product must be true if and only if the PID is one of the
    /// three provisional Cube Controls product IDs.
    #[test]
    fn prop_is_cube_controls_product_matches_known_pids(pid in any::<u16>()) {
        let known = matches!(
            pid,
            CUBE_CONTROLS_GT_PRO_PID | CUBE_CONTROLS_FORMULA_PRO_PID | CUBE_CONTROLS_CSX3_PID
        );
        prop_assert_eq!(
            is_cube_controls_product(pid),
            known,
            "is_cube_controls_product mismatch for PID 0x{:04X}", pid
        );
    }

    /// max_torque_nm is always positive for any Cube Controls PID: all known
    /// models and the Unknown fallback report 20 Nm.
    #[test]
    fn prop_cube_controls_torque_always_positive(pid in any::<u16>()) {
        let handler = CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, pid);
        let config = handler.get_ffb_config();
        prop_assert!(
            config.max_torque_nm > 0.0,
            "Cube Controls PID 0x{:04X} must always report positive torque", pid
        );
    }

    /// max_torque_nm is within the physically safe range (0, 100] Nm for any PID.
    #[test]
    fn prop_ffb_config_torque_in_safe_range(pid in any::<u16>()) {
        let handler = CubeControlsProtocolHandler::new(CUBE_CONTROLS_VENDOR_ID, pid);
        let config = handler.get_ffb_config();
        prop_assert!(
            config.max_torque_nm > 0.0 && config.max_torque_nm <= 100.0,
            "max_torque_nm {} is outside safe range (0, 100]", config.max_torque_nm
        );
    }

    /// CubeControlsModel::from_product_id must be deterministic for any PID.
    #[test]
    fn prop_model_from_pid_deterministic(pid in any::<u16>()) {
        let m1 = CubeControlsModel::from_product_id(pid);
        let m2 = CubeControlsModel::from_product_id(pid);
        prop_assert_eq!(
            m1, m2,
            "model resolution must be deterministic for PID 0x{:04X}", pid
        );
    }
}
