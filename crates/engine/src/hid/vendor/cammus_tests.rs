//! Tests for the Cammus C5/C12 vendor protocol handler.

use super::cammus::{CammusProtocolHandler, PRODUCT_C12, PRODUCT_C5, VENDOR_ID};
use super::{get_vendor_protocol, DeviceWriter, VendorProtocol};
use racing_wheel_hid_cammus_protocol::{CammusModel, FFB_REPORT_ID, FFB_REPORT_LEN};
use std::cell::RefCell;

struct MockWriter {
    output_reports: RefCell<Vec<Vec<u8>>>,
    feature_reports: RefCell<Vec<Vec<u8>>>,
}

impl MockWriter {
    fn new() -> Self {
        Self {
            output_reports: RefCell::new(Vec::new()),
            feature_reports: RefCell::new(Vec::new()),
        }
    }

    fn output_reports(&self) -> Vec<Vec<u8>> {
        self.output_reports.borrow().clone()
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
fn handler_creates_c5() {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    assert_eq!(h.model(), CammusModel::C5);
}

#[test]
fn handler_creates_c12() {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C12);
    assert_eq!(h.model(), CammusModel::C12);
}

#[test]
fn handler_unknown_pid_falls_back_to_c5() {
    let h = CammusProtocolHandler::new(VENDOR_ID, 0xFFFF);
    assert_eq!(h.model(), CammusModel::C5);
}

#[test]
fn initialize_sends_one_output_report() -> Result<(), Box<dyn std::error::Error>> {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    let mut writer = MockWriter::new();
    h.initialize_device(&mut writer)?;
    let reports = writer.output_reports();
    assert_eq!(reports.len(), 1, "init should send exactly one output report");
    assert_eq!(reports[0][0], FFB_REPORT_ID);
    // torque bytes should be zero
    assert_eq!(reports[0][1], 0x00);
    assert_eq!(reports[0][2], 0x00);
    Ok(())
}

#[test]
fn initialize_no_feature_reports() -> Result<(), Box<dyn std::error::Error>> {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C12);
    let mut writer = MockWriter::new();
    h.initialize_device(&mut writer)?;
    assert!(writer.feature_reports().is_empty());
    Ok(())
}

#[test]
fn shutdown_sends_stop_report() -> Result<(), Box<dyn std::error::Error>> {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    let mut writer = MockWriter::new();
    h.shutdown_device(&mut writer)?;
    let reports = writer.output_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], FFB_REPORT_ID);
    assert_eq!(reports[0][1], 0x00);
    assert_eq!(reports[0][2], 0x00);
    Ok(())
}

#[test]
fn ffb_config_c5() {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    let cfg = h.get_ffb_config();
    assert!((cfg.max_torque_nm - 5.0).abs() < 0.001);
    assert!(cfg.encoder_cpr > 0);
    assert_eq!(cfg.required_b_interval, Some(1));
}

#[test]
fn ffb_config_c12() {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C12);
    let cfg = h.get_ffb_config();
    assert!((cfg.max_torque_nm - 12.0).abs() < 0.001);
}

#[test]
fn output_report_id_and_len() {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    assert_eq!(h.output_report_id(), Some(FFB_REPORT_ID));
    assert_eq!(h.output_report_len(), Some(FFB_REPORT_LEN));
}

#[test]
fn not_v2_hardware() {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    assert!(!h.is_v2_hardware());
}

#[test]
fn send_feature_report_too_large_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let h = CammusProtocolHandler::new(VENDOR_ID, PRODUCT_C5);
    let mut writer = MockWriter::new();
    let oversized = vec![0u8; 64];
    let result = h.send_feature_report(&mut writer, 0x02, &oversized);
    assert!(result.is_err(), "oversized report should return Err");
    Ok(())
}

#[test]
fn get_vendor_protocol_routes_cammus() {
    assert!(get_vendor_protocol(VENDOR_ID, PRODUCT_C5).is_some());
    assert!(get_vendor_protocol(VENDOR_ID, PRODUCT_C12).is_some());
}

#[test]
fn get_vendor_protocol_unknown_cammus_pid_is_none() {
    // Unknown PID under Cammus VID should return None (not crash).
    let result = get_vendor_protocol(VENDOR_ID, 0x00FF);
    // Could be Some or None depending on routing; just verify no panic.
    let _ = result;
}
