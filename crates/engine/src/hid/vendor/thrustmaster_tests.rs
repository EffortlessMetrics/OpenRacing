//! Tests for the Thrustmaster protocol handler.

use super::thrustmaster::{
    EFFECT_REPORT_LEN, Model, THRUSTMASTER_VENDOR_ID, ThrustmasterProtocolHandler,
    is_pedal_product, is_wheel_product, product_ids,
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
fn test_new_tgt() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T_GT);
    assert_eq!(handler.model(), Model::TGT);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 6.0).abs() < 0.01);
    Ok(())
}

#[test]
fn test_new_t818() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T818);
    assert_eq!(handler.model(), Model::T818);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 10.0).abs() < 0.01);
    Ok(())
}

#[test]
fn test_new_t150() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T150);
    assert_eq!(handler.model(), Model::T150);
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 2.5).abs() < 0.01);
    Ok(())
}

#[test]
fn test_pedal_handler() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T3PA);
    assert_eq!(handler.model(), Model::T3PA);
    assert!(!handler.model().supports_ffb());
    let config = handler.get_ffb_config();
    assert!((config.max_torque_nm - 0.0).abs() < 0.01);
    Ok(())
}

#[test]
fn test_initialize_wheel() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T_GT);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;

    assert_eq!(
        writer.feature_reports.len(),
        4,
        "wheel init must send 4 feature reports"
    );
    // Report 0: reset gain [0x81, 0x00]
    assert_eq!(writer.feature_reports[0][0], 0x81);
    assert_eq!(writer.feature_reports[0][1], 0x00);
    // Report 1: full gain [0x81, 0xFF]
    assert_eq!(writer.feature_reports[1][0], 0x81);
    assert_eq!(writer.feature_reports[1][1], 0xFF);
    // Report 2: actuator enable [0x82, 0x01]
    assert_eq!(writer.feature_reports[2][0], 0x82);
    assert_eq!(writer.feature_reports[2][1], 0x01);
    // Report 3: set range [0x80, 0x01, ...]
    assert_eq!(writer.feature_reports[3][0], 0x80);
    assert_eq!(writer.feature_reports[3][1], 0x01);
    Ok(())
}

#[test]
fn test_initialize_pedal() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T3PA);
    let mut writer = MockDeviceWriter::new();
    handler.initialize_device(&mut writer)?;

    assert_eq!(
        writer.feature_reports.len(),
        0,
        "pedal init must send no reports"
    );
    Ok(())
}

#[test]
fn test_shutdown_wheel() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T300_RS);
    let mut writer = MockDeviceWriter::new();
    handler.shutdown_device(&mut writer)?;

    assert_eq!(writer.feature_reports.len(), 1, "shutdown sends one report");
    // Actuator disable [0x82, 0x00]
    assert_eq!(writer.feature_reports[0][0], 0x82);
    assert_eq!(writer.feature_reports[0][1], 0x00);
    Ok(())
}

#[test]
fn test_ffb_config() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T300_RS);
    let config = handler.get_ffb_config();

    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert_eq!(config.encoder_cpr, 4096);
    assert!((config.max_torque_nm - 4.0).abs() < 0.01);
    Ok(())
}

#[test]
fn test_output_report_id() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T_GT);
    assert_eq!(handler.output_report_id(), Some(0x23));
    Ok(())
}

#[test]
fn test_output_report_id_pedal() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T_LCM);
    assert_eq!(handler.output_report_id(), None);
    Ok(())
}

#[test]
fn test_output_report_len() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T818);
    assert_eq!(handler.output_report_len(), Some(EFFECT_REPORT_LEN));
    Ok(())
}

#[test]
fn test_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T300_RS);
    let mut writer = MockDeviceWriter::new();
    let payload = [0x42u8, 0xDE, 0xAD];
    handler.send_feature_report(&mut writer, 0x01, &payload)?;

    assert_eq!(writer.feature_reports.len(), 1);
    assert_eq!(writer.feature_reports[0], payload);
    Ok(())
}

#[test]
fn test_is_v2_hardware() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, product_ids::T818);
    assert!(!handler.is_v2_hardware());
    Ok(())
}

#[test]
fn test_unknown_product() -> Result<(), Box<dyn std::error::Error>> {
    let handler = ThrustmasterProtocolHandler::new(THRUSTMASTER_VENDOR_ID, 0xFFFF);
    assert_eq!(handler.model(), Model::Unknown);
    let mut writer = MockDeviceWriter::new();
    // Must not crash or send any reports for unknown product.
    handler.initialize_device(&mut writer)?;
    assert_eq!(writer.feature_reports.len(), 0);
    Ok(())
}

#[test]
fn test_get_vendor_protocol_returns_thrustmaster() {
    let protocol = get_vendor_protocol(THRUSTMASTER_VENDOR_ID, product_ids::T_GT);
    assert!(
        protocol.is_some(),
        "must return a vendor protocol for Thrustmaster VID"
    );
}

#[test]
fn test_is_wheel_product_known_ids() {
    assert!(is_wheel_product(product_ids::T_GT));
    assert!(is_wheel_product(product_ids::T300_RS));
    assert!(is_wheel_product(product_ids::T818));
    assert!(!is_wheel_product(product_ids::T_LCM));
    assert!(!is_wheel_product(0xFFFF));
}

#[test]
fn test_is_pedal_product_known_ids() {
    assert!(is_pedal_product(product_ids::T_LCM));
    assert!(is_pedal_product(product_ids::T3PA));
    assert!(!is_pedal_product(product_ids::T_GT));
}
