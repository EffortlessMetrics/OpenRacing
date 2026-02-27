//! Tests for the FFBeast vendor protocol handler.

use super::ffbeast::FFBeastHandler;
use super::VendorProtocol;
use racing_wheel_hid_ffbeast_protocol::{
    FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER, FFBEAST_PRODUCT_ID_WHEEL,
    FFBEAST_VENDOR_ID,
};
use std::cell::RefCell;

struct MockWriter {
    feature_reports: RefCell<Vec<Vec<u8>>>,
}

impl MockWriter {
    fn new() -> Self {
        Self {
            feature_reports: RefCell::new(vec![]),
        }
    }

    fn reports(&self) -> Vec<Vec<u8>> {
        self.feature_reports.borrow().clone()
    }
}

impl super::DeviceWriter for MockWriter {
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(data.len())
    }

    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.borrow_mut().push(data.to_vec());
        Ok(data.len())
    }
}

#[test]
fn handler_creates_for_wheel_pid() {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    let config = handler.get_ffb_config();
    assert!(config.max_torque_nm > 0.0, "max torque must be positive");
    assert!(config.encoder_cpr > 0, "encoder CPR must be positive");
}

#[test]
fn handler_creates_for_joystick_pid() {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_JOYSTICK);
    let config = handler.get_ffb_config();
    assert!(config.max_torque_nm > 0.0);
}

#[test]
fn handler_creates_for_rudder_pid() {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_RUDDER);
    let config = handler.get_ffb_config();
    assert!(config.max_torque_nm > 0.0);
}

#[test]
fn initialize_sends_enable_and_gain() -> Result<(), Box<dyn std::error::Error>> {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    let mut writer = MockWriter::new();
    handler.initialize_device(&mut writer)?;
    let reports = writer.reports();
    assert_eq!(reports.len(), 2, "expected enable + gain reports");
    // First report should be FFB enable (0x60)
    assert_eq!(reports[0][0], 0x60, "first report should be FFB enable");
    assert_eq!(reports[0][1], 0x01, "FFB should be enabled");
    // Second report should be gain (0x61)
    assert_eq!(reports[1][0], 0x61, "second report should be gain");
    assert_eq!(reports[1][1], 0xFF, "gain should be maximum");
    Ok(())
}

#[test]
fn shutdown_sends_disable() -> Result<(), Box<dyn std::error::Error>> {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    let mut writer = MockWriter::new();
    handler.shutdown_device(&mut writer)?;
    let reports = writer.reports();
    assert_eq!(reports.len(), 1, "expected one shutdown report");
    assert_eq!(reports[0][0], 0x60, "should be FFB enable report");
    assert_eq!(reports[0][1], 0x00, "FFB should be disabled on shutdown");
    Ok(())
}

#[test]
fn output_report_id_and_len_are_set() {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    assert!(handler.output_report_id().is_some(), "should have output report ID");
    assert!(handler.output_report_len().is_some(), "should have output report len");
    assert_eq!(handler.output_report_id(), Some(0x01));
}

#[test]
fn ffb_config_valid_ranges() {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    let config = handler.get_ffb_config();
    assert!(config.max_torque_nm >= 1.0, "max torque should be at least 1 Nm");
    assert!(config.max_torque_nm <= 100.0, "max torque should be <= 100 Nm");
    assert!(config.encoder_cpr >= 100, "encoder CPR should be reasonable");
}

#[test]
fn not_v2_hardware() {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn send_feature_report_too_large_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let handler = FFBeastHandler::new(FFBEAST_VENDOR_ID, FFBEAST_PRODUCT_ID_WHEEL);
    let mut writer = MockWriter::new();
    // 64 bytes of data + 1 report ID = 65 bytes, which exceeds the 64-byte max.
    let oversized = vec![0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x70, &oversized);
    assert!(result.is_err(), "oversized report should return Err");
    Ok(())
}
