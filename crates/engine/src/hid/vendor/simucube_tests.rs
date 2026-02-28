//! Tests for the Simucube protocol handler.

use super::simucube::{
    SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID, SIMUCUBE_ACTIVE_PEDAL_PID,
    SIMUCUBE_VENDOR_ID, SIMUCUBE_WIRELESS_WHEEL_PID, SimucubeModel, SimucubeProtocolHandler,
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
fn test_new_sport() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_SPORT_PID);
    assert_eq!(handler.model(), SimucubeModel::Sport);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 15.0).abs() < 0.01);
}

#[test]
fn test_new_pro() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    assert_eq!(handler.model(), SimucubeModel::Pro);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 25.0).abs() < 0.01);
}

#[test]
fn test_new_ultimate() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_ULTIMATE_PID);
    assert_eq!(handler.model(), SimucubeModel::Ultimate);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 35.0).abs() < 0.01);
    assert!(handler.is_v2_hardware());
}

#[test]
fn test_new_active_pedal() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_ACTIVE_PEDAL_PID);
    assert_eq!(handler.model(), SimucubeModel::ActivePedal);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 0.0).abs() < 0.01);
}

#[test]
fn test_initialize_no_reports() -> Result<(), Box<dyn std::error::Error>> {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    let mut writer = MockDeviceWriter::new();

    handler.initialize_device(&mut writer)?;

    assert!(
        writer.feature_reports.is_empty(),
        "Simucube init must not send any feature reports"
    );
    assert!(
        writer.output_reports.is_empty(),
        "Simucube init must not send any output reports"
    );
    Ok(())
}

#[test]
fn test_ffb_config_sport() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_SPORT_PID);
    let config = handler.get_ffb_config();
    assert_eq!(config.encoder_cpr, 4_194_304);
    assert_eq!(config.required_b_interval, Some(3));
    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
}

#[test]
fn test_ffb_config_pro() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 25.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 4_194_304);
}

#[test]
fn test_output_report_metadata() {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    assert_eq!(handler.output_report_id(), Some(0x01));
    assert_eq!(handler.output_report_len(), Some(64));
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    let mut writer = MockDeviceWriter::new();

    handler.send_feature_report(&mut writer, 0x10, &[0x01, 0x02])?;

    assert_eq!(writer.feature_reports.len(), 1);
    assert_eq!(writer.feature_reports[0], vec![0x10, 0x01, 0x02]);
    Ok(())
}

#[test]
fn test_is_v2_hardware() {
    let sport = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_SPORT_PID);
    assert!(!sport.is_v2_hardware());

    let pro = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID);
    assert!(pro.is_v2_hardware());

    let ultimate = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_ULTIMATE_PID);
    assert!(ultimate.is_v2_hardware());

    let pedal = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_ACTIVE_PEDAL_PID);
    assert!(!pedal.is_v2_hardware());

    let wireless = SimucubeProtocolHandler::new(SIMUCUBE_VENDOR_ID, SIMUCUBE_WIRELESS_WHEEL_PID);
    assert!(!wireless.is_v2_hardware());
}

#[test]
fn test_get_vendor_protocol_simucube() {
    assert!(get_vendor_protocol(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_SPORT_PID).is_some());
    assert!(get_vendor_protocol(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_PRO_PID).is_some());
    assert!(get_vendor_protocol(SIMUCUBE_VENDOR_ID, SIMUCUBE_2_ULTIMATE_PID).is_some());
    assert!(get_vendor_protocol(SIMUCUBE_VENDOR_ID, 0xFFFF).is_some());
}
