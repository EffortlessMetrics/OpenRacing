//! Tests for PXN / Lite Star protocol handler.

use super::pxn::{
    PXN_VENDOR_ID, PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2,
    PxnModel, PxnProtocolHandler, is_pxn_product,
};
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
fn test_new_v10() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    assert_eq!(handler.model(), PxnModel::V10);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 10.0).abs() < 0.01);
}

#[test]
fn test_new_v12() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V12);
    assert_eq!(handler.model(), PxnModel::V12);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 12.0).abs() < 0.01);
}

#[test]
fn test_new_v12_lite() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V12_LITE);
    assert_eq!(handler.model(), PxnModel::V12Lite);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 6.0).abs() < 0.01);
}

#[test]
fn test_new_v12_lite_se() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V12_LITE_2);
    assert_eq!(handler.model(), PxnModel::V12LiteSe);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 6.0).abs() < 0.01);
}

#[test]
fn test_new_gt987() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_GT987);
    assert_eq!(handler.model(), PxnModel::Gt987);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 5.0).abs() < 0.01);
}

#[test]
fn test_new_unknown_pid() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, 0x0001);
    assert_eq!(handler.model(), PxnModel::Unknown);
    let config = handler.get_ffb_config();
    assert!(config.max_torque_nm > 0.0);
}

#[test]
fn test_initialize_sends_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "PXN init must send no reports (standard HID PID)"
    );
    Ok(())
}

#[test]
fn test_ffb_config_v10() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    let config = handler.get_ffb_config();
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert_eq!(config.encoder_cpr, 0);
}

#[test]
fn test_is_v2_hardware() {
    let v10 = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    assert!(!v10.is_v2_hardware(), "V10 is not v2 hardware");

    let v12 = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V12);
    assert!(v12.is_v2_hardware(), "V12 treated as v2 hardware");
}

#[test]
fn test_output_report() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    assert!(handler.output_report_id().is_none());
    assert!(handler.output_report_len().is_none());
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    let mut writer = MockDeviceWriter::new();
    handler.send_feature_report(&mut writer, 0x01, &[0xAA, 0xBB])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x01, 0xAA, 0xBB]);
    Ok(())
}

#[test]
fn test_send_feature_report_too_large() {
    let handler = PxnProtocolHandler::new(PXN_VENDOR_ID, PRODUCT_V10);
    let mut writer = MockDeviceWriter::new();
    let big_payload = [0u8; 64];
    let result = handler.send_feature_report(&mut writer, 0x01, &big_payload);
    assert!(result.is_err(), "report exceeding 64 bytes must fail");
}

#[test]
fn test_is_pxn_product() {
    assert!(is_pxn_product(PRODUCT_V10));
    assert!(is_pxn_product(PRODUCT_V12));
    assert!(is_pxn_product(PRODUCT_V12_LITE));
    assert!(is_pxn_product(PRODUCT_V12_LITE_2));
    assert!(is_pxn_product(PRODUCT_GT987));
    assert!(!is_pxn_product(0x0001));
    assert!(!is_pxn_product(0xFFFF));
}

#[test]
fn test_get_vendor_protocol_pxn() {
    let proto = get_vendor_protocol(PXN_VENDOR_ID, PRODUCT_V10);
    assert!(proto.is_some(), "PXN V10 must resolve to a vendor protocol");

    let proto = get_vendor_protocol(PXN_VENDOR_ID, PRODUCT_V12);
    assert!(proto.is_some(), "PXN V12 must resolve to a vendor protocol");

    let proto = get_vendor_protocol(PXN_VENDOR_ID, PRODUCT_GT987);
    assert!(
        proto.is_some(),
        "Lite Star GT987 must resolve to a vendor protocol"
    );
}

#[test]
fn test_pxn_model_display_names() {
    assert_eq!(PxnModel::V10.display_name(), "PXN V10");
    assert_eq!(PxnModel::V12.display_name(), "PXN V12");
    assert_eq!(PxnModel::V12Lite.display_name(), "PXN V12 Lite");
    assert_eq!(PxnModel::V12LiteSe.display_name(), "PXN V12 Lite (SE)");
    assert_eq!(PxnModel::Gt987.display_name(), "Lite Star GT987 FF");
    assert!(!PxnModel::Unknown.display_name().is_empty());
}
