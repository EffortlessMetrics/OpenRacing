//! Tests for SimExperience AccuForce Pro protocol handler.

use super::accuforce::{
    is_accuforce_product, AccuForceModel, AccuForceProtocolHandler, ACCUFORCE_PRO_PID,
    ACCUFORCE_VENDOR_ID,
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
fn test_new_pro() {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    assert_eq!(handler.model(), AccuForceModel::Pro);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 7.0).abs() < 0.01);
}

#[test]
fn test_new_unknown_pid() {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, 0x804D);
    assert_eq!(handler.model(), AccuForceModel::Unknown);
}

#[test]
fn test_initialize_sends_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "AccuForce init must send no reports (standard HID PID)"
    );
    Ok(())
}

#[test]
fn test_ffb_config() {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    let config = handler.get_ffb_config();
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(8));
    assert_eq!(config.encoder_cpr, 0);
    assert!((config.max_torque_nm - 7.0).abs() < 0.01);
}

#[test]
fn test_is_v2_hardware() {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn test_output_report() {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    assert!(handler.output_report_id().is_none());
    assert!(handler.output_report_len().is_none());
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    let mut writer = MockDeviceWriter::new();
    handler.send_feature_report(&mut writer, 0x02, &[0x11, 0x22, 0x33])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x02, 0x11, 0x22, 0x33]);
    Ok(())
}

#[test]
fn test_send_feature_report_too_large() {
    let handler = AccuForceProtocolHandler::new(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    let mut writer = MockDeviceWriter::new();
    let big_payload = [0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x01, &big_payload);
    assert!(result.is_err(), "report exceeding 64 bytes must fail");
}

#[test]
fn test_is_accuforce_product() {
    assert!(is_accuforce_product(ACCUFORCE_PRO_PID));
    assert!(!is_accuforce_product(0x1234));
    assert!(!is_accuforce_product(0x0522)); // Simagic
}

#[test]
fn test_get_vendor_protocol_accuforce() {
    let proto = get_vendor_protocol(ACCUFORCE_VENDOR_ID, ACCUFORCE_PRO_PID);
    assert!(proto.is_some(), "AccuForce Pro must resolve to a vendor protocol");
}

#[test]
fn test_accuforce_model_display_name() {
    assert_eq!(
        AccuForceModel::Pro.display_name(),
        "SimExperience AccuForce Pro"
    );
    assert!(!AccuForceModel::Unknown.display_name().is_empty());
}
