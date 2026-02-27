//! Tests for Cammus C5/C12 protocol handler.

use super::cammus::{
    is_cammus_product, CammusModel, CammusProtocolHandler, CAMMUS_C12_PID, CAMMUS_C5_PID,
    CAMMUS_VENDOR_ID,
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
fn test_new_c5() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    assert_eq!(handler.model(), CammusModel::C5);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 5.0).abs() < 0.01);
}

#[test]
fn test_new_c12() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C12_PID);
    assert_eq!(handler.model(), CammusModel::C12);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 12.0).abs() < 0.01);
}

#[test]
fn test_new_unknown_pid() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, 0x0399);
    assert_eq!(handler.model(), CammusModel::Unknown);
    let config = handler.get_ffb_config();
    // conservative default for unknown model
    assert!(config.max_torque_nm > 0.0);
}

#[test]
fn test_initialize_sends_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "Cammus init must send no reports (standard HID PID)"
    );
    Ok(())
}

#[test]
fn test_ffb_config_c5() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    let config = handler.get_ffb_config();
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert_eq!(config.encoder_cpr, 0);
}

#[test]
fn test_ffb_config_c12() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C12_PID);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 12.0).abs() < 0.01);
    assert_eq!(config.required_b_interval, Some(1));
}

#[test]
fn test_is_v2_hardware() {
    let c5 = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    assert!(!c5.is_v2_hardware(), "C5 is not v2 hardware");

    let c12 = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C12_PID);
    assert!(c12.is_v2_hardware(), "C12 treated as v2 hardware");
}

#[test]
fn test_output_report() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    assert!(handler.output_report_id().is_none());
    assert!(handler.output_report_len().is_none());
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    let mut writer = MockDeviceWriter::new();
    handler.send_feature_report(&mut writer, 0x01, &[0xAA, 0xBB])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x01, 0xAA, 0xBB]);
    Ok(())
}

#[test]
fn test_send_feature_report_too_large() {
    let handler = CammusProtocolHandler::new(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    let mut writer = MockDeviceWriter::new();
    let big_payload = [0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x01, &big_payload);
    assert!(result.is_err(), "report exceeding 64 bytes must fail");
}

#[test]
fn test_is_cammus_product() {
    assert!(is_cammus_product(CAMMUS_C5_PID));
    assert!(is_cammus_product(CAMMUS_C12_PID));
    assert!(!is_cammus_product(0x1234));
    assert!(!is_cammus_product(0x0522)); // Simagic
}

#[test]
fn test_get_vendor_protocol_cammus() {
    let proto = get_vendor_protocol(CAMMUS_VENDOR_ID, CAMMUS_C5_PID);
    assert!(proto.is_some(), "C5 must resolve to a vendor protocol");

    let proto = get_vendor_protocol(CAMMUS_VENDOR_ID, CAMMUS_C12_PID);
    assert!(proto.is_some(), "C12 must resolve to a vendor protocol");
}

#[test]
fn test_cammus_model_display_names() {
    assert_eq!(CammusModel::C5.display_name(), "Cammus C5");
    assert_eq!(CammusModel::C12.display_name(), "Cammus C12");
    assert!(!CammusModel::Unknown.display_name().is_empty());
}
