//! Tests for the Logitech protocol handler.

use super::logitech::{is_wheel_product, product_ids, LogitechModel, LogitechProtocol};
use super::{get_vendor_protocol, DeviceWriter, VendorProtocol};
use racing_wheel_hid_logitech_protocol::LOGITECH_VENDOR_ID;
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
fn test_logitech_model_classification() {
    let g920 = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::G920);
    assert_eq!(g920.model(), LogitechModel::G920);

    let g923 = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::G923_XBOX);
    assert_eq!(g923.model(), LogitechModel::G923);

    let pro = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::PRO_RACING);
    assert_eq!(pro.model(), LogitechModel::ProRacing);

    let unknown = LogitechProtocol::new(LOGITECH_VENDOR_ID, 0xBEEF);
    assert_eq!(unknown.model(), LogitechModel::Unknown);
}

#[test]
fn test_logitech_ffb_config_g920() {
    let protocol = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::G920);
    let config = protocol.get_ffb_config();
    assert!((config.max_torque_nm - 2.2).abs() < 0.05);
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
}

#[test]
fn test_logitech_ffb_config_pro_racing() {
    let protocol = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::PRO_RACING);
    let config = protocol.get_ffb_config();
    assert!((config.max_torque_nm - 11.0).abs() < 0.1);
}

#[test]
fn test_logitech_initialize_sends_two_feature_reports() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::G920);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;

    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 2, "should send native mode + set range");
    // First report: native mode [0xF8, 0x0A, ...]
    assert_eq!(reports[0][0], 0xF8, "native mode report ID");
    assert_eq!(reports[0][1], 0x0A, "native mode command");
    // Second report: set range [0xF8, 0x81, 0x84, 0x03, ...]
    assert_eq!(reports[1][0], 0xF8, "set range report ID");
    assert_eq!(reports[1][1], 0x81, "set range command");
    assert_eq!(reports[1][2], 0x84, "LSB of 900°");
    assert_eq!(reports[1][3], 0x03, "MSB of 900°");
    Ok(())
}

#[test]
fn test_logitech_unknown_product_skips_init() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = LogitechProtocol::new(LOGITECH_VENDOR_ID, 0xBEEF);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    assert_eq!(
        writer.feature_reports().len(),
        0,
        "unknown PIDs must not send init"
    );
    Ok(())
}

#[test]
fn test_logitech_initialize_pro_racing_1080deg() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::PRO_RACING);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;

    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 2);
    // Range = 1080° = 0x0438; little-endian [0x38, 0x04]
    assert_eq!(reports[1][2], 0x38, "LSB of 1080°");
    assert_eq!(reports[1][3], 0x04, "MSB of 1080°");
    Ok(())
}

#[test]
fn test_get_vendor_protocol_returns_logitech() {
    let protocol = get_vendor_protocol(LOGITECH_VENDOR_ID, product_ids::G920);
    assert!(
        protocol.is_some(),
        "must return a vendor protocol for Logitech VID"
    );
}

#[test]
fn test_logitech_output_report_metadata() {
    let protocol = LogitechProtocol::new(LOGITECH_VENDOR_ID, product_ids::G920);
    assert_eq!(
        protocol.output_report_id(),
        Some(0x12),
        "constant force report ID"
    );
    assert_eq!(
        protocol.output_report_len(),
        Some(4),
        "constant force report length"
    );
}

#[test]
fn test_is_wheel_product_known_ids() {
    assert!(is_wheel_product(product_ids::G920));
    assert!(is_wheel_product(product_ids::G923_XBOX));
    assert!(is_wheel_product(product_ids::G923_PS));
    assert!(is_wheel_product(product_ids::PRO_RACING));
    assert!(!is_wheel_product(0xFFFF));
}
