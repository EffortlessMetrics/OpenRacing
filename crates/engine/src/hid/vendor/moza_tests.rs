//! Tests for Moza protocol handler

use super::moza::{
    FfbMode, MozaDeviceCategory, MozaModel, MozaProtocol, MozaTopologyHint, identify_device,
    is_wheelbase_product, product_ids,
};
use super::{DeviceWriter, FfbConfig, VendorProtocol, get_vendor_protocol};
use std::cell::RefCell;

/// Mock device writer for testing
struct MockDeviceWriter {
    feature_reports: RefCell<Vec<Vec<u8>>>,
    output_reports: RefCell<Vec<Vec<u8>>>,
    fail_on_write: bool,
}

impl MockDeviceWriter {
    fn new() -> Self {
        Self {
            feature_reports: RefCell::new(Vec::new()),
            output_reports: RefCell::new(Vec::new()),
            fail_on_write: false,
        }
    }

    fn with_failure() -> Self {
        Self {
            feature_reports: RefCell::new(Vec::new()),
            output_reports: RefCell::new(Vec::new()),
            fail_on_write: true,
        }
    }

    fn get_feature_reports(&self) -> Vec<Vec<u8>> {
        self.feature_reports.borrow().clone()
    }
}

impl DeviceWriter for MockDeviceWriter {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_on_write {
            return Err("Mock write failure".into());
        }
        let len = data.len();
        self.feature_reports.borrow_mut().push(data.to_vec());
        Ok(len)
    }

    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if self.fail_on_write {
            return Err("Mock write failure".into());
        }
        let len = data.len();
        self.output_reports.borrow_mut().push(data.to_vec());
        Ok(len)
    }
}

#[test]
fn test_moza_protocol_creation() {
    let protocol = MozaProtocol::new(0x0002);
    assert_eq!(protocol.model(), MozaModel::R9);
    assert!(!protocol.is_v2_hardware());

    let protocol_v2 = MozaProtocol::new(0x0012);
    assert_eq!(protocol_v2.model(), MozaModel::R9);
    assert!(protocol_v2.is_v2_hardware());
}

#[test]
fn test_moza_model_from_pid() {
    // V1 PIDs
    assert_eq!(MozaModel::from_pid(0x0005), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(0x0004), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(0x0002), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(0x0006), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(0x0000), MozaModel::R16);
    assert_eq!(MozaModel::from_pid(0x0003), MozaModel::SrpPedals);

    // V2 PIDs
    assert_eq!(MozaModel::from_pid(0x0015), MozaModel::R3);
    assert_eq!(MozaModel::from_pid(0x0014), MozaModel::R5);
    assert_eq!(MozaModel::from_pid(0x0012), MozaModel::R9);
    assert_eq!(MozaModel::from_pid(0x0016), MozaModel::R12);
    assert_eq!(MozaModel::from_pid(0x0010), MozaModel::R16);

    // Unknown
    assert_eq!(MozaModel::from_pid(0xFFFF), MozaModel::Unknown);
}

#[test]
fn test_moza_identity_wheelbase_topology() {
    let identity = identify_device(product_ids::R9_V2);
    assert_eq!(identity.category, MozaDeviceCategory::Wheelbase);
    assert_eq!(
        identity.topology_hint,
        MozaTopologyHint::WheelbaseAggregated
    );
    assert!(identity.supports_ffb);
    assert!(is_wheelbase_product(product_ids::R9_V2));
}

#[test]
fn test_moza_identity_peripherals() {
    let pedals = identify_device(product_ids::SR_P_PEDALS);
    assert_eq!(pedals.category, MozaDeviceCategory::Pedals);
    assert_eq!(pedals.topology_hint, MozaTopologyHint::StandaloneUsb);
    assert!(!pedals.supports_ffb);

    let shifter = identify_device(product_ids::HGP_SHIFTER);
    assert_eq!(shifter.category, MozaDeviceCategory::Shifter);
    assert_eq!(shifter.topology_hint, MozaTopologyHint::StandaloneUsb);
    assert!(!shifter.supports_ffb);

    let unknown = identify_device(0xFEED);
    assert_eq!(unknown.category, MozaDeviceCategory::Unknown);
    assert_eq!(unknown.topology_hint, MozaTopologyHint::Unknown);
    assert!(!unknown.supports_ffb);
    assert!(!is_wheelbase_product(0xFEED));
}

#[test]
fn test_moza_max_torque() {
    assert!((MozaModel::R3.max_torque_nm() - 3.9).abs() < 0.01);
    assert!((MozaModel::R5.max_torque_nm() - 5.5).abs() < 0.01);
    assert!((MozaModel::R9.max_torque_nm() - 9.0).abs() < 0.01);
    assert!((MozaModel::R12.max_torque_nm() - 12.0).abs() < 0.01);
    assert!((MozaModel::R16.max_torque_nm() - 16.0).abs() < 0.01);
    assert!((MozaModel::R21.max_torque_nm() - 21.0).abs() < 0.01);
    assert!((MozaModel::SrpPedals.max_torque_nm() - 0.0).abs() < 0.01);
    assert!((MozaModel::Unknown.max_torque_nm() - 10.0).abs() < 0.01);
}

#[test]
fn test_moza_encoder_cpr() {
    // V1 devices use 15-bit encoder
    let v1_protocol = MozaProtocol::new(0x0002); // R9 V1
    let v1_config = v1_protocol.get_ffb_config();
    assert_eq!(v1_config.encoder_cpr, 32768);

    // V2 standard devices use 18-bit encoder
    let v2_r9 = MozaProtocol::new(0x0012); // R9 V2
    let v2_config = v2_r9.get_ffb_config();
    assert_eq!(v2_config.encoder_cpr, 262144);

    // V2 R16/R21 use 21-bit encoder
    let v2_r16 = MozaProtocol::new(0x0010); // R16 V2
    let r16_config = v2_r16.get_ffb_config();
    assert_eq!(r16_config.encoder_cpr, 2097152);
}

#[test]
fn test_moza_ffb_config() {
    let protocol = MozaProtocol::new(0x0002); // R9 V1
    let config = protocol.get_ffb_config();

    assert!(config.fix_conditional_direction);
    assert!(config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 9.0).abs() < 0.01);
}

#[test]
fn test_moza_initialize_device() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002); // R9
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 3); // high torque, start reports, ffb mode

    // Check high torque report
    assert_eq!(reports[0][0], super::moza::report_ids::HIGH_TORQUE);
    assert_eq!(reports[0][1], 0x01); // Enable command
    assert_eq!(reports[0][2], 0x01); // Enable flag

    // Check start reports
    assert_eq!(reports[1][0], super::moza::report_ids::START_REPORTS);
    assert_eq!(reports[1][1], 0x01); // Start command

    // Check FFB mode
    assert_eq!(reports[2][0], super::moza::report_ids::FFB_MODE);
    assert_eq!(reports[2][1], FfbMode::Standard as u8);

    Ok(())
}

#[test]
fn test_moza_pedals_skip_initialization() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0003); // SR-P Pedals
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.get_feature_reports();
    assert!(reports.is_empty()); // No reports sent for pedals

    Ok(())
}

#[test]
fn test_moza_initialization_continues_on_failure() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002); // R9
    let mut writer = MockDeviceWriter::with_failure();

    // Should not return error, just warn
    let result = protocol.initialize_device(&mut writer);
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_moza_set_rotation_range() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    protocol.set_rotation_range(&mut writer, 900)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], super::moza::report_ids::ROTATION_RANGE);
    assert_eq!(reports[0][1], 0x01); // Set Range command

    // Check degrees (900 in little-endian)
    let degrees_bytes = 900u16.to_le_bytes();
    assert_eq!(reports[0][2], degrees_bytes[0]);
    assert_eq!(reports[0][3], degrees_bytes[1]);

    Ok(())
}

#[test]
fn test_moza_set_ffb_mode() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    protocol.set_ffb_mode(&mut writer, FfbMode::Direct)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0][0], super::moza::report_ids::FFB_MODE);
    assert_eq!(reports[0][1], FfbMode::Direct as u8);

    Ok(())
}

#[test]
fn test_get_vendor_protocol_moza() {
    let protocol = get_vendor_protocol(0x346E, 0x0002);
    assert!(protocol.is_some());

    let proto = protocol.as_ref();
    assert!(proto.is_some());
    let p = proto.map(|p| p.as_ref());
    assert!(p.is_some());
}

#[test]
fn test_get_vendor_protocol_unknown() {
    let protocol = get_vendor_protocol(0x1234, 0x5678);
    assert!(protocol.is_none());
}

#[test]
fn test_ffb_config_default() {
    let config = FfbConfig::default();

    assert!(!config.fix_conditional_direction);
    assert!(!config.uses_vendor_usage_page);
    assert_eq!(config.required_b_interval, None);
    assert!((config.max_torque_nm - 10.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 4096);
}

#[test]
fn test_moza_send_feature_report() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = MozaProtocol::new(0x0002);
    let mut writer = MockDeviceWriter::new();

    let data = [0x01, 0x02, 0x03];
    protocol.send_feature_report(&mut writer, 0xAB, &data)?;

    let reports = writer.get_feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0xAB, 0x01, 0x02, 0x03]);

    Ok(())
}

#[test]
fn test_v1_vs_v2_detection() {
    // V1 PIDs (0x000x)
    assert!(!MozaProtocol::new(0x0000).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0002).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0004).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0005).is_v2_hardware());
    assert!(!MozaProtocol::new(0x0006).is_v2_hardware());

    // V2 PIDs (0x001x)
    assert!(MozaProtocol::new(0x0010).is_v2_hardware());
    assert!(MozaProtocol::new(0x0012).is_v2_hardware());
    assert!(MozaProtocol::new(0x0014).is_v2_hardware());
    assert!(MozaProtocol::new(0x0015).is_v2_hardware());
    assert!(MozaProtocol::new(0x0016).is_v2_hardware());
}
