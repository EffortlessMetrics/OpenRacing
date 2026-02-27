//! Tests for the generic HID PID fallback vendor protocol handler.

use super::generic_hid_pid::GenericHidPidHandler;
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol_with_hid_pid_fallback};
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
fn handler_creates_for_arbitrary_vid_pid() {
    let handler = GenericHidPidHandler::new(0xABCD, 0x1234);
    let config = handler.get_ffb_config();
    assert!(
        config.max_torque_nm > 0.0,
        "generic handler must have positive max torque"
    );
    assert!(
        config.encoder_cpr > 0,
        "generic handler must have positive encoder CPR"
    );
}

#[test]
fn initialize_sends_no_vendor_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = GenericHidPidHandler::new(0x1234, 0x5678);
    let mut writer = MockWriter::new();
    handler.initialize_device(&mut writer)?;
    assert_eq!(
        writer.feature_reports().len(),
        0,
        "generic HID PID init must send no vendor feature reports"
    );
    Ok(())
}

#[test]
fn ffb_config_has_conservative_positive_torque() {
    let handler = GenericHidPidHandler::new(0x1234, 0x5678);
    let config = handler.get_ffb_config();
    assert!(
        config.max_torque_nm >= 1.0,
        "generic torque must be at least 1 Nm"
    );
    assert!(
        config.max_torque_nm <= 15.0,
        "generic torque must be conservative (<=15 Nm) to protect unknown hardware"
    );
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, None);
}

#[test]
fn send_feature_report_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let handler = GenericHidPidHandler::new(0x1234, 0x5678);
    let mut writer = MockWriter::new();
    let payload = [0xAAu8, 0xBB, 0xCC];
    handler.send_feature_report(&mut writer, 0x05, &payload)?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], 0x05, "first byte must be the report ID");
    assert_eq!(
        &reports[0][1..4],
        &payload,
        "payload bytes must follow report ID"
    );
    Ok(())
}

#[test]
fn send_feature_report_too_large_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let handler = GenericHidPidHandler::new(0x1234, 0x5678);
    let mut writer = MockWriter::new();
    // 64 bytes of data + 1 report ID byte = 65 bytes, exceeds the 64-byte USB HID limit.
    let oversized = vec![0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x01, &oversized);
    assert!(result.is_err(), "oversized feature report must return Err");
    Ok(())
}

#[test]
fn not_v2_hardware() {
    let handler = GenericHidPidHandler::new(0xDEAD, 0xBEEF);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn output_report_id_and_len_are_none() {
    let handler = GenericHidPidHandler::new(0x1234, 0x5678);
    assert!(
        handler.output_report_id().is_none(),
        "generic handler must not pin a fixed output report ID"
    );
    assert!(handler.output_report_len().is_none());
}

#[test]
fn fallback_returns_generic_handler_when_hid_pid_advertised() {
    // An unknown VID/PID that is not registered with any vendor handler.
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xABCD, 0x1234, true);
    assert!(
        handler.is_some(),
        "must return a generic handler when HID PID capability is advertised"
    );
}

#[test]
fn fallback_returns_none_when_hid_pid_not_advertised() {
    let handler = get_vendor_protocol_with_hid_pid_fallback(0xABCD, 0x1234, false);
    assert!(
        handler.is_none(),
        "must return None when device does not advertise HID PID capability"
    );
}

#[test]
fn fallback_prefers_specific_vendor_handler_over_generic() {
    // 0x346E is the Moza VID — must get the Moza handler, not the generic one.
    let handler = get_vendor_protocol_with_hid_pid_fallback(0x346E, 0x0002, true);
    assert!(
        handler.is_some(),
        "Moza device must have a specific vendor handler"
    );
    // Verify the config resembles a Moza handler (high torque, not 8 Nm generic default).
    let config = handler
        .map(|h| h.get_ffb_config())
        .expect("handler should be some");
    assert!(
        config.max_torque_nm > 8.0,
        "Moza handler torque must exceed the 8 Nm generic default"
    );
}
