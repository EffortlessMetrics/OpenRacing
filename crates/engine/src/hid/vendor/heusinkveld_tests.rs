//! Tests for Heusinkveld pedals protocol handler.

use super::heusinkveld::{
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
    HeusinkveldProtocolHandler, is_heusinkveld_product,
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
fn test_new_sprint() {
    let handler = HeusinkveldProtocolHandler::new(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    assert_eq!(handler.pedal_count(), 2);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 0.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 0);
}

#[test]
fn test_new_ultimate() {
    let handler = HeusinkveldProtocolHandler::new(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID);
    assert_eq!(handler.pedal_count(), 3);
}

#[test]
fn test_new_pro() {
    let handler = HeusinkveldProtocolHandler::new(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_PRO_PID);
    assert_eq!(handler.pedal_count(), 3);
}

#[test]
fn test_initialize_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = HeusinkveldProtocolHandler::new(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "Heusinkveld init sends no reports"
    );
    Ok(())
}

#[test]
fn test_ffb_config() {
    let handler = HeusinkveldProtocolHandler::new(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_ULTIMATE_PID);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 0.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 0);
    assert!(config.required_b_interval.is_none());
    assert!(!config.fix_conditional_direction);
}

#[test]
fn test_no_output_report() {
    let handler = HeusinkveldProtocolHandler::new(HEUSINKVELD_VENDOR_ID, HEUSINKVELD_SPRINT_PID);
    assert!(handler.output_report_id().is_none());
    assert!(handler.output_report_len().is_none());
    assert!(!handler.is_v2_hardware());
}

#[test]
fn test_is_heusinkveld_product() {
    assert!(is_heusinkveld_product(0xF6D0));
    assert!(is_heusinkveld_product(0xF6D2));
    assert!(is_heusinkveld_product(0xF6D3));
    assert!(!is_heusinkveld_product(0x1234));
    assert!(!is_heusinkveld_product(0x0522));
}

#[test]
fn test_get_vendor_protocol_heusinkveld_pids() {
    // Heusinkveld PIDs (0xF6Dx) on VID 0x04D8 must route to Heusinkveld
    assert!(get_vendor_protocol(0x04D8, 0xF6D0).is_some());
    assert!(get_vendor_protocol(0x04D8, 0xF6D2).is_some());
    assert!(get_vendor_protocol(0x04D8, 0xF6D3).is_some());
}
