//! Tests for the Asetek protocol handler.

use super::asetek::{
    ASETEK_FORTE_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PID, ASETEK_VENDOR_ID, AsetekModel,
    AsetekProtocolHandler,
};
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol};

struct MockDeviceWriter {
    feature_reports: Vec<Vec<u8>>,
    output_reports: Vec<Vec<u8>>,
}

impl MockDeviceWriter {
    fn new() -> Self {
        Self {
            feature_reports: Vec::new(),
            output_reports: Vec::new(),
        }
    }
}

impl DeviceWriter for MockDeviceWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.push(data.to_vec());
        Ok(data.len())
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_reports.push(data.to_vec());
        Ok(data.len())
    }
}

#[test]
fn test_new_forte() {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    assert_eq!(handler.model(), AsetekModel::Forte);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 20.0).abs() < 0.01);
    assert!(handler.is_v2_hardware());
}

#[test]
fn test_new_invicta() {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_INVICTA_PID);
    assert_eq!(handler.model(), AsetekModel::Invicta);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 15.0).abs() < 0.01);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn test_new_laprima() {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_LAPRIMA_PID);
    assert_eq!(handler.model(), AsetekModel::LaPrima);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 10.0).abs() < 0.01);
    assert!(!handler.is_v2_hardware());
}

#[test]
fn test_initialize_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    let mut writer = MockDeviceWriter::new();

    handler.initialize_device(&mut writer)?;

    assert!(
        writer.feature_reports.is_empty(),
        "Asetek init must not send any feature reports"
    );
    assert!(
        writer.output_reports.is_empty(),
        "Asetek init must not send any output reports"
    );
    Ok(())
}

#[test]
fn test_ffb_config() {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    let config = handler.get_ffb_config();
    assert_eq!(config.encoder_cpr, 1_048_576);
    assert_eq!(config.required_b_interval, Some(1));
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
}

#[test]
fn test_output_report_metadata() {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    assert!(handler.output_report_id().is_none());
    assert!(handler.output_report_len().is_none());
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = AsetekProtocolHandler::new(ASETEK_VENDOR_ID, ASETEK_FORTE_PID);
    let mut writer = MockDeviceWriter::new();

    handler.send_feature_report(&mut writer, 0x20, &[0xAA, 0xBB])?;

    assert_eq!(writer.feature_reports.len(), 1);
    assert_eq!(writer.feature_reports[0], vec![0x20, 0xAA, 0xBB]);
    Ok(())
}

#[test]
fn test_get_vendor_protocol_asetek() {
    assert!(get_vendor_protocol(ASETEK_VENDOR_ID, ASETEK_FORTE_PID).is_some());
    assert!(get_vendor_protocol(ASETEK_VENDOR_ID, ASETEK_INVICTA_PID).is_some());
    assert!(get_vendor_protocol(ASETEK_VENDOR_ID, ASETEK_LAPRIMA_PID).is_some());
    assert!(get_vendor_protocol(ASETEK_VENDOR_ID, 0xFFFF).is_some());
}
