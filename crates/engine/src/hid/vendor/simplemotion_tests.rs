//! Tests for the SimpleMotion V2 protocol handler.

use super::simplemotion::{
    ARGON_PRODUCT_ID, IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM, SM_VENDOR_ID,
    SimpleMotionProtocolHandler, TORQUE_COMMAND_LEN, TorqueCommandEncoder,
};
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol};

struct MockWriter {
    output_reports: Vec<Vec<u8>>,
    feature_reports: Vec<Vec<u8>>,
}

impl MockWriter {
    fn new() -> Self {
        Self {
            output_reports: Vec::new(),
            feature_reports: Vec::new(),
        }
    }
}

impl DeviceWriter for MockWriter {
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_reports.push(data.to_vec());
        Ok(data.len())
    }

    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.push(data.to_vec());
        Ok(data.len())
    }
}

#[test]
fn test_handler_ioni() {
    let h = SimpleMotionProtocolHandler::new(SM_VENDOR_ID, IONI_PRODUCT_ID);
    assert!(h.supports_ffb());
    let cfg = h.get_ffb_config();
    assert!(cfg.max_torque_nm > 0.0);
}

#[test]
fn test_handler_ioni_premium_is_v2() {
    let h = SimpleMotionProtocolHandler::new(SM_VENDOR_ID, IONI_PRODUCT_ID_PREMIUM);
    assert!(h.supports_ffb());
    assert!(h.is_v2_hardware());
    let cfg = h.get_ffb_config();
    assert!(cfg.max_torque_nm > 0.0);
}

#[test]
fn test_handler_argon_is_v2() {
    let h = SimpleMotionProtocolHandler::new(SM_VENDOR_ID, ARGON_PRODUCT_ID);
    assert!(h.supports_ffb());
    assert!(h.is_v2_hardware());
    let cfg = h.get_ffb_config();
    assert!(cfg.max_torque_nm > 0.0);
}

#[test]
fn test_handler_unknown_product_no_ffb() {
    let h = SimpleMotionProtocolHandler::new(SM_VENDOR_ID, 0xFFFF);
    assert!(!h.supports_ffb());
    assert!(!h.is_v2_hardware());
    let cfg = h.get_ffb_config();
    assert_eq!(cfg.max_torque_nm, 0.0);
    assert_eq!(h.output_report_id(), None);
    assert_eq!(h.output_report_len(), None);
}

#[test]
fn test_initialize_sends_enable_command() -> Result<(), Box<dyn std::error::Error>> {
    let h = SimpleMotionProtocolHandler::new(SM_VENDOR_ID, IONI_PRODUCT_ID);
    let mut writer = MockWriter::new();
    h.initialize_device(&mut writer)?;
    assert_eq!(
        writer.output_reports.len(),
        1,
        "init must send exactly one output report"
    );
    // SimpleMotion V2 output report starts with report ID 0x01
    assert_eq!(writer.output_reports[0][0], 0x01);
    Ok(())
}

#[test]
fn test_torque_encoding_positive() {
    let mut encoder = TorqueCommandEncoder::new(20.0);
    let mut buf = [0u8; TORQUE_COMMAND_LEN];
    let n = SimpleMotionProtocolHandler::write_torque(&mut encoder, 10.0, &mut buf);
    assert!(n > 0, "torque encode must produce bytes");
    assert_eq!(buf[0], 0x01, "report ID must be 0x01");
}

#[test]
fn test_torque_encoding_zero() {
    let mut encoder = TorqueCommandEncoder::new(20.0);
    let mut buf = [0u8; TORQUE_COMMAND_LEN];
    let n = SimpleMotionProtocolHandler::write_torque(&mut encoder, 0.0, &mut buf);
    assert!(n > 0, "zero torque must still produce bytes");
}

#[test]
fn test_feedback_parsing_valid() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = [0u8; 64];
    data[0] = 0x02; // feedback report ID
    data[1] = 0x07;
    let state = SimpleMotionProtocolHandler::read_state(&data)?;
    assert_eq!(state.seq, 0x07);
    Ok(())
}

#[test]
fn test_feedback_parsing_invalid_id() {
    let data = [0u8; 64]; // report ID 0x00 — invalid
    let result = SimpleMotionProtocolHandler::read_state(&data);
    assert!(result.is_err(), "must reject invalid report ID");
}

#[test]
fn test_get_vendor_protocol_dispatches_sm_vid() {
    let proto = get_vendor_protocol(SM_VENDOR_ID, IONI_PRODUCT_ID);
    assert!(proto.is_some(), "SM VID must be dispatched to a handler");
}

#[test]
fn test_output_report_metadata_wheelbase() {
    let h = SimpleMotionProtocolHandler::new(SM_VENDOR_ID, IONI_PRODUCT_ID);
    assert_eq!(h.output_report_id(), Some(0x01));
    assert_eq!(h.output_report_len(), Some(TORQUE_COMMAND_LEN));
}
