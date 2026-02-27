//! Tests for the Leo Bodnar vendor protocol handler.

use super::leo_bodnar::{
    LEO_BODNAR_PID_BBI32, LEO_BODNAR_PID_FFB_JOYSTICK, LEO_BODNAR_PID_JOYSTICK,
    LEO_BODNAR_PID_SLIM, LEO_BODNAR_PID_WHEEL, LEO_BODNAR_VENDOR_ID, LeoBodnarHandler,
    is_leo_bodnar_ffb_product,
};
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol};
use std::cell::RefCell;

struct MockWriter {
    feature_reports: RefCell<Vec<Vec<u8>>>,
    output_reports: RefCell<Vec<Vec<u8>>>,
}

impl MockWriter {
    fn new() -> Self {
        Self {
            feature_reports: RefCell::new(vec![]),
            output_reports: RefCell::new(vec![]),
        }
    }

    fn feature_reports(&self) -> Vec<Vec<u8>> {
        self.feature_reports.borrow().clone()
    }
}

impl DeviceWriter for MockWriter {
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_reports.borrow_mut().push(data.to_vec());
        Ok(data.len())
    }

    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.borrow_mut().push(data.to_vec());
        Ok(data.len())
    }
}

#[test]
fn handler_creates_for_ffb_wheel_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    let config = handler.get_ffb_config();
    assert!(
        config.max_torque_nm > 0.0,
        "wheel interface must have positive max torque"
    );
    assert!(
        config.encoder_cpr > 0,
        "wheel interface must have positive encoder CPR"
    );
}

#[test]
fn handler_creates_for_bbi32_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_BBI32);
    let config = handler.get_ffb_config();
    assert_eq!(
        config.max_torque_nm, 0.0,
        "BBI-32 is input-only, torque must be zero"
    );
}

#[test]
fn handler_creates_for_slim_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_SLIM);
    let config = handler.get_ffb_config();
    assert_eq!(
        config.max_torque_nm, 0.0,
        "SLI-M is input-only, torque must be zero"
    );
}

#[test]
fn handler_creates_for_joystick_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_JOYSTICK);
    let config = handler.get_ffb_config();
    assert_eq!(
        config.max_torque_nm, 0.0,
        "USB joystick is input-only, torque must be zero"
    );
}

#[test]
fn ffb_wheel_supports_pid_ffb() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    assert!(
        handler.supports_pid_ffb(),
        "PID 0x000E must support HID PID FFB"
    );
}

#[test]
fn input_only_devices_do_not_support_pid_ffb() {
    for &pid in &[
        LEO_BODNAR_PID_BBI32,
        LEO_BODNAR_PID_SLIM,
        LEO_BODNAR_PID_JOYSTICK,
    ] {
        let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, pid);
        assert!(
            !handler.supports_pid_ffb(),
            "PID 0x{pid:04X} must not support HID PID FFB"
        );
    }
}

#[test]
fn initialize_ffb_wheel_sends_no_vendor_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    let mut writer = MockWriter::new();
    handler.initialize_device(&mut writer)?;
    // Standard HID PID devices require no vendor-specific init reports.
    assert_eq!(
        writer.feature_reports().len(),
        0,
        "standard HID PID init must send no vendor feature reports"
    );
    Ok(())
}

#[test]
fn initialize_input_only_sends_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_BBI32);
    let mut writer = MockWriter::new();
    handler.initialize_device(&mut writer)?;
    assert_eq!(
        writer.feature_reports().len(),
        0,
        "input-only init must send no reports"
    );
    Ok(())
}

#[test]
fn ffb_config_for_wheel_has_valid_ranges() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    let config = handler.get_ffb_config();
    assert!(
        config.max_torque_nm >= 1.0,
        "max torque must be at least 1 Nm"
    );
    assert!(
        config.max_torque_nm <= 100.0,
        "max torque must be within safe range"
    );
    assert!(
        config.encoder_cpr >= 100,
        "encoder CPR must be a reasonable value"
    );
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
}

#[test]
fn ffb_config_for_input_only_has_zero_torque() {
    for &pid in &[
        LEO_BODNAR_PID_BBI32,
        LEO_BODNAR_PID_SLIM,
        LEO_BODNAR_PID_JOYSTICK,
    ] {
        let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, pid);
        let config = handler.get_ffb_config();
        assert_eq!(
            config.max_torque_nm, 0.0,
            "input-only PID 0x{pid:04X} must report zero torque"
        );
        assert_eq!(
            config.encoder_cpr, 0,
            "input-only must report zero encoder CPR"
        );
    }
}

#[test]
fn send_feature_report_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    let mut writer = MockWriter::new();
    let payload = [0x01u8, 0x02, 0x03];
    handler.send_feature_report(&mut writer, 0x10, &payload)?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], 0x10, "first byte must be report ID");
    assert_eq!(&reports[0][1..4], &payload, "payload must follow report ID");
    Ok(())
}

#[test]
fn send_feature_report_too_large_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    let mut writer = MockWriter::new();
    // 64 bytes of data + 1 report ID byte = 65 bytes, exceeds the 64-byte USB HID limit.
    let oversized = vec![0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x01, &oversized);
    assert!(result.is_err(), "oversized feature report must return Err");
    Ok(())
}

#[test]
fn not_v2_hardware() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn output_report_id_is_none_for_standard_hid_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    assert!(
        handler.output_report_id().is_none(),
        "standard HID PID handler must not pin a fixed output report ID"
    );
    assert!(handler.output_report_len().is_none());
}

#[test]
fn is_leo_bodnar_ffb_product_only_matches_wheel_pid() {
    assert!(is_leo_bodnar_ffb_product(LEO_BODNAR_PID_WHEEL));
    assert!(!is_leo_bodnar_ffb_product(LEO_BODNAR_PID_BBI32));
    assert!(!is_leo_bodnar_ffb_product(LEO_BODNAR_PID_SLIM));
    assert!(!is_leo_bodnar_ffb_product(LEO_BODNAR_PID_JOYSTICK));
    assert!(!is_leo_bodnar_ffb_product(0xFFFF));
}

#[test]
fn get_vendor_protocol_returns_handler_for_leo_bodnar_vid() {
    let handler = get_vendor_protocol(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    assert!(
        handler.is_some(),
        "must return a handler for Leo Bodnar VID"
    );
}

#[test]
fn get_vendor_protocol_returns_handler_for_bbi32() {
    let handler = get_vendor_protocol(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_BBI32);
    assert!(
        handler.is_some(),
        "must return a handler for BBI-32 button box"
    );
}

// ── Insta snapshot tests ──────────────────────────────────────────────────────

#[test]
fn snapshot_ffb_config_wheel_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_WHEEL);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[test]
fn snapshot_ffb_config_bbi32_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_BBI32);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[test]
fn snapshot_ffb_config_slim_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_SLIM);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[test]
fn snapshot_ffb_config_joystick_pid() {
    let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, LEO_BODNAR_PID_JOYSTICK);
    insta::assert_debug_snapshot!(handler.get_ffb_config());
}

#[cfg(windows)]
#[test]
fn snapshot_device_caps_wheel_pid() {
    let caps = super::super::windows::determine_device_capabilities(
        LEO_BODNAR_VENDOR_ID,
        LEO_BODNAR_PID_WHEEL,
    );
    insta::assert_debug_snapshot!(caps);
}

#[cfg(windows)]
#[test]
fn snapshot_device_caps_bbi32_pid() {
    let caps = super::super::windows::determine_device_capabilities(
        LEO_BODNAR_VENDOR_ID,
        LEO_BODNAR_PID_BBI32,
    );
    insta::assert_debug_snapshot!(caps);
}

// ── Proptest property tests ───────────────────────────────────────────────────

use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    /// is_leo_bodnar_ffb_product must be true if and only if the PID is an
    /// FFB-capable Leo Bodnar device (Wheel Interface 0x000E or FFB Joystick 0x000F).
    #[test]
    fn prop_is_leo_bodnar_ffb_product_exact_pid(pid in any::<u16>()) {
        prop_assert_eq!(
            is_leo_bodnar_ffb_product(pid),
            pid == LEO_BODNAR_PID_WHEEL || pid == LEO_BODNAR_PID_FFB_JOYSTICK,
            "is_leo_bodnar_ffb_product must match only FFB-capable PIDs (0x{:04X}, 0x{:04X})",
            LEO_BODNAR_PID_WHEEL, LEO_BODNAR_PID_FFB_JOYSTICK
        );
    }

    /// max_torque_nm is positive iff the PID is the FFB wheel interface;
    /// all other Leo Bodnar PIDs are input-only and must report zero torque.
    #[test]
    fn prop_torque_positive_iff_ffb_product(pid in any::<u16>()) {
        let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, pid);
        let config = handler.get_ffb_config();
        if is_leo_bodnar_ffb_product(pid) {
            prop_assert!(
                config.max_torque_nm > 0.0,
                "FFB wheel PID 0x{:04X} must have positive torque", pid
            );
        } else {
            prop_assert_eq!(
                config.max_torque_nm, 0.0,
                "Input-only PID 0x{:04X} must report zero torque", pid
            );
        }
    }

    /// max_torque_nm is always within the physically safe range [0, 100] Nm.
    #[test]
    fn prop_ffb_config_torque_in_safe_range(pid in any::<u16>()) {
        let handler = LeoBodnarHandler::new(LEO_BODNAR_VENDOR_ID, pid);
        let config = handler.get_ffb_config();
        prop_assert!(
            config.max_torque_nm >= 0.0 && config.max_torque_nm <= 100.0,
            "max_torque_nm {} is outside safe range [0, 100]", config.max_torque_nm
        );
    }
}
