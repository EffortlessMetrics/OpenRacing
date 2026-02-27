//! Tests for Simagic protocol handler.

use super::simagic::{product_ids, vendor_ids, SimagicModel, SimagicProtocol};
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
fn test_simagic_model_classification() {
    let legacy = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);
    assert_eq!(legacy.model(), SimagicModel::Alpha);

    let evo_unknown = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, 0xFEED);
    assert_eq!(evo_unknown.model(), SimagicModel::EvoUnknown);

    let evo_pro = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO_PRO);
    assert_eq!(evo_pro.model(), SimagicModel::EvoPro);
}

#[test]
fn test_simagic_ffb_config_legacy_alpha() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);
    let config = protocol.get_ffb_config();

    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 15.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 262_144);
    assert!(!protocol.is_v2_hardware());
}

#[test]
fn test_simagic_ffb_config_evo_unknown_is_conservative() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, 0x1234);
    let config = protocol.get_ffb_config();

    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 15.0).abs() < 0.01, "EVO unknown should use conservative 15 Nm default");
    assert_eq!(config.encoder_cpr, 2_097_152);
    assert!(protocol.is_v2_hardware());
}

#[test]
fn test_initialize_device_evo_sends_init_reports()
-> Result<(), Box<dyn std::error::Error>> {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, 0x1234);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;
    // EVO devices now actively send gain + rotation range init reports
    assert_eq!(writer.feature_reports().len(), 2, "EVO device must send 2 init reports");
    Ok(())
}

#[test]
fn test_send_feature_report_prefixes_report_id() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);
    let mut writer = MockDeviceWriter::new();

    protocol.send_feature_report(&mut writer, 0x42, &[0x01, 0x02, 0x03])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x42, 0x01, 0x02, 0x03]);
    Ok(())
}

#[test]
fn test_send_feature_report_rejects_oversize() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);
    let mut writer = MockDeviceWriter::new();
    let data = [0xAAu8; 64];

    let result = protocol.send_feature_report(&mut writer, 0x22, &data);
    assert!(result.is_err());
}

#[test]
fn test_get_vendor_protocol_simagic_vids() {
    assert!(get_vendor_protocol(vendor_ids::SIMAGIC_STM, product_ids::ALPHA).is_some());
    assert!(get_vendor_protocol(vendor_ids::SIMAGIC_ALT, product_ids::M10).is_some());
    assert!(get_vendor_protocol(vendor_ids::SIMAGIC_EVO, 0x1234).is_some());
}

#[test]
fn test_simagic_output_report_metadata_defaults_to_none() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);

    assert!(protocol.output_report_id().is_none());
    assert!(protocol.output_report_len().is_none());
}

#[test]
fn test_evo_sport_init() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO_SPORT);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 2, "EVO device must send 2 init reports");
    // First report: device gain (report ID 0x21)
    assert_eq!(reports[0][0], 0x21);
    // Second report: rotation range (report ID 0x20)
    assert_eq!(reports[1][0], 0x20);
    Ok(())
}

#[test]
fn test_evo_init() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 2, "EVO device must send 2 init reports");
    Ok(())
}

#[test]
fn test_legacy_alpha_passive() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_STM, product_ids::ALPHA);
    let mut writer = MockDeviceWriter::new();
    protocol.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "legacy device must not send init reports"
    );
    Ok(())
}

#[test]
fn test_evo_device_output_report_metadata() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO_SPORT);
    assert_eq!(protocol.output_report_id(), Some(0x11));
    assert_eq!(protocol.output_report_len(), Some(8));
}

#[test]
fn test_evo_device_is_v2_hardware() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO);
    assert!(protocol.is_v2_hardware());
}

#[test]
fn test_evo_pro_ffb_config() {
    let protocol = SimagicProtocol::new(vendor_ids::SIMAGIC_EVO, product_ids::EVO_PRO);
    let config = protocol.get_ffb_config();
    assert!((config.max_torque_nm - 30.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 2_097_152);
}

#[test]
fn test_get_vendor_protocol_simagic_evo_vid() {
    assert!(get_vendor_protocol(vendor_ids::SIMAGIC_EVO, product_ids::EVO_SPORT).is_some());
    assert!(get_vendor_protocol(vendor_ids::SIMAGIC_EVO, product_ids::EVO).is_some());
    assert!(get_vendor_protocol(vendor_ids::SIMAGIC_EVO, product_ids::EVO_PRO).is_some());
}
