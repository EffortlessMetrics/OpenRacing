//! Tests for VRS DirectForce Pro protocol handler.

use super::vrs::{VrsProtocolHandler, is_vrs_product, product_ids};
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol};
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
fn test_new_directforce_pro() {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 20.0).abs() < 0.01);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn test_new_directforce_pro_v2() {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO_V2);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 25.0).abs() < 0.01);
    assert!(handler.is_v2_hardware());
}

#[test]
fn test_initialize_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;
    let reports = writer.feature_reports();
    assert_eq!(
        reports.len(),
        3,
        "wheelbase init must send 3 feature reports"
    );
    Ok(())
}

#[test]
fn test_initialize_pedals() -> Result<(), Box<dyn std::error::Error>> {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::PEDALS_V1);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "pedals send no init reports"
    );
    Ok(())
}

#[test]
fn test_shutdown_wheelbase() -> Result<(), Box<dyn std::error::Error>> {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO);
    let mut writer = MockDeviceWriter::new();
    handler.shutdown_device(&mut writer)?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1, "shutdown must send 1 disable report");
    // DEVICE_CONTROL (0x0B) with disable byte 0x00
    assert_eq!(reports[0][0], 0x0B);
    assert_eq!(reports[0][1], 0x00);
    Ok(())
}

#[test]
fn test_ffb_config() {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO);
    let config = handler.get_ffb_config();
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert_eq!(config.encoder_cpr, 1_048_576);
}

#[test]
fn test_output_report_id() {
    let wheelbase = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO);
    assert_eq!(wheelbase.output_report_id(), Some(0x11));
    assert_eq!(wheelbase.output_report_len(), Some(8));

    let pedals = VrsProtocolHandler::new(0x0483, product_ids::PEDALS_V1);
    assert!(pedals.output_report_id().is_none());
    assert!(pedals.output_report_len().is_none());
}

#[test]
fn test_is_vrs_product() {
    assert!(is_vrs_product(0xA355));
    assert!(is_vrs_product(0xA356));
    assert!(is_vrs_product(0xA357));
    assert!(is_vrs_product(0xA358));
    assert!(is_vrs_product(0xA3BE)); // Pedals (corrected)
    assert!(is_vrs_product(0xA44C)); // R295
    assert!(!is_vrs_product(0x0522));
    assert!(!is_vrs_product(0x1234));
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = VrsProtocolHandler::new(0x0483, product_ids::DIRECTFORCE_PRO);
    let mut writer = MockDeviceWriter::new();
    handler.send_feature_report(&mut writer, 0x11, &[0x01, 0x02])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x11, 0x01, 0x02]);
    Ok(())
}

#[test]
fn test_get_vendor_protocol_vrs_pid() {
    // VRS PIDs (0xA3xx) on VID 0x0483 must route to VRS, not Simagic
    assert!(get_vendor_protocol(0x0483, 0xA355).is_some());
    assert!(get_vendor_protocol(0x0483, 0xA356).is_some());
}
