//! Tests for the Fanatec protocol handler.

use super::fanatec::{
    FanatecModel, FanatecPedalModel, FanatecProtocol, FanatecRimId, is_pedal_product,
    is_wheelbase_product, product_ids, rim_ids,
};
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol};
use racing_wheel_hid_fanatec_protocol::FANATEC_VENDOR_ID;
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

    fn output_reports(&self) -> Vec<Vec<u8>> {
        self.output_reports.borrow().clone()
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
fn test_fanatec_model_classification() {
    let dd1 = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::DD1);
    assert_eq!(dd1.model(), FanatecModel::Dd1);

    let csl_dd = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_DD);
    assert_eq!(csl_dd.model(), FanatecModel::CslDd);

    let gt_pro = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::GT_DD_PRO);
    assert_eq!(gt_pro.model(), FanatecModel::GtDdPro);

    let clubsport_dd = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CLUBSPORT_DD);
    assert_eq!(clubsport_dd.model(), FanatecModel::ClubSportDd);

    let unknown = FanatecProtocol::new(FANATEC_VENDOR_ID, 0xBEEF);
    assert_eq!(unknown.model(), FanatecModel::Unknown);
}

#[test]
fn test_fanatec_ffb_config_dd1() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::DD1);
    let config = protocol.get_ffb_config();

    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 20.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 16_384);
    assert!(protocol.is_v2_hardware());
}

#[test]
fn test_fanatec_ffb_config_csl_dd() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_DD);
    let config = protocol.get_ffb_config();

    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 8.0).abs() < 0.01);
    assert!(!protocol.is_v2_hardware());
}

#[test]
fn test_fanatec_ffb_config_clubsport_dd() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CLUBSPORT_DD);
    let config = protocol.get_ffb_config();

    assert_eq!(config.required_b_interval, Some(1));
    assert!((config.max_torque_nm - 12.0).abs() < 0.01);
    assert_eq!(config.encoder_cpr, 16_384);
    assert!(protocol.is_v2_hardware());
    assert!(protocol.model().supports_1000hz());
}

#[test]
fn test_initialize_wheelbase_sends_mode_switch() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_DD);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1, "expected exactly one feature report");
    // Mode-switch payload: [0x01, 0x01, 0x03, 0x00, ...]
    assert_eq!(reports[0][0], 0x01);
    assert_eq!(reports[0][1], 0x01);
    assert_eq!(reports[0][2], 0x03);
    Ok(())
}

#[test]
fn test_initialize_non_wheelbase_skips_handshake() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, 0xFFFF); // unknown = not wheelbase
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;
    assert!(
        writer.feature_reports().is_empty(),
        "no reports for non-wheelbase"
    );
    Ok(())
}

#[test]
fn test_send_feature_report_prefixes_report_id() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_DD);
    let mut writer = MockDeviceWriter::new();

    protocol.send_feature_report(&mut writer, 0x10, &[0x01, 0x50])?;
    let reports = writer.feature_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0], vec![0x10, 0x01, 0x50]);
    Ok(())
}

#[test]
fn test_send_feature_report_rejects_oversize() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_DD);
    let mut writer = MockDeviceWriter::new();
    let data = [0xAAu8; 64];

    let result = protocol.send_feature_report(&mut writer, 0x10, &data);
    assert!(result.is_err());
}

#[test]
fn test_output_report_metadata_wheelbase() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_DD);
    assert_eq!(protocol.output_report_id(), Some(0x01));
    assert_eq!(protocol.output_report_len(), Some(8));
}

#[test]
fn test_output_report_metadata_unknown() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, 0xFFFF);
    assert!(protocol.output_report_id().is_none());
    assert!(protocol.output_report_len().is_none());
}

#[test]
fn test_get_vendor_protocol_fanatec() {
    assert!(get_vendor_protocol(FANATEC_VENDOR_ID, product_ids::CSL_DD).is_some());
    assert!(get_vendor_protocol(FANATEC_VENDOR_ID, product_ids::GT_DD_PRO).is_some());
    assert!(get_vendor_protocol(FANATEC_VENDOR_ID, 0xFFFF).is_some());
}

#[test]
fn test_is_wheelbase_product_consistency() {
    assert!(is_wheelbase_product(product_ids::DD1));
    assert!(is_wheelbase_product(product_ids::DD2));
    assert!(is_wheelbase_product(product_ids::CSL_DD));
    assert!(is_wheelbase_product(product_ids::GT_DD_PRO));
    assert!(!is_wheelbase_product(0xFFFF));
}

#[test]
fn test_shutdown_wheelbase_sends_stop_all() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::GT_DD_PRO);
    let mut writer = MockDeviceWriter::new();

    protocol.shutdown_device(&mut writer)?;

    let reports = writer.output_reports();
    assert_eq!(
        reports.len(),
        1,
        "expected exactly one output report on shutdown"
    );
    // stop-all payload: [FFB_OUTPUT=0x01, STOP_ALL=0x0F, 0x00, ...]
    assert_eq!(reports[0][0], 0x01, "byte 0 must be FFB_OUTPUT report ID");
    assert_eq!(reports[0][1], 0x0F, "byte 1 must be STOP_ALL command");
    Ok(())
}

#[test]
fn test_shutdown_non_wheelbase_is_noop() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, 0xFFFF);
    let mut writer = MockDeviceWriter::new();

    protocol.shutdown_device(&mut writer)?;

    assert!(
        writer.output_reports().is_empty(),
        "non-wheelbase shutdown must not write any reports"
    );
    Ok(())
}

#[test]
fn test_is_pedal_product_consistency() {
    // Known pedal PIDs
    assert!(is_pedal_product(product_ids::CLUBSPORT_PEDALS_V3));
    assert!(is_pedal_product(product_ids::CLUBSPORT_PEDALS_V1_V2));
    assert!(is_pedal_product(product_ids::CSL_PEDALS_LC));
    assert!(is_pedal_product(product_ids::CSL_PEDALS_V2));
    // Wheelbase PIDs must not match
    assert!(!is_pedal_product(product_ids::DD1));
    assert!(!is_pedal_product(product_ids::DD2));
    assert!(!is_pedal_product(product_ids::CSL_DD));
    assert!(!is_pedal_product(product_ids::GT_DD_PRO));
    assert!(!is_pedal_product(0xFFFF));
}

#[test]
fn test_pedal_device_skips_handshake() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CLUBSPORT_PEDALS_V3);
    let mut writer = MockDeviceWriter::new();

    protocol.initialize_device(&mut writer)?;

    assert!(
        writer.feature_reports().is_empty(),
        "pedal device must not send mode-switch handshake"
    );
    Ok(())
}

#[test]
fn test_pedal_device_no_output_report_metadata() {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CLUBSPORT_PEDALS_V3);
    assert!(
        protocol.output_report_id().is_none(),
        "pedal device must have no output report ID"
    );
    assert!(
        protocol.output_report_len().is_none(),
        "pedal device must have no output report length"
    );
}

#[test]
fn test_pedal_device_shutdown_is_noop() -> Result<(), Box<dyn std::error::Error>> {
    let protocol = FanatecProtocol::new(FANATEC_VENDOR_ID, product_ids::CSL_PEDALS_LC);
    let mut writer = MockDeviceWriter::new();

    protocol.shutdown_device(&mut writer)?;

    assert!(
        writer.output_reports().is_empty(),
        "pedal device shutdown must not write any output reports"
    );
    Ok(())
}

#[test]
fn test_rim_id_mclaren_from_byte() {
    let rim = FanatecRimId::from_byte(rim_ids::MCLAREN_GT3_V2);
    assert_eq!(rim, FanatecRimId::McLarenGt3V2);
    assert!(rim.has_funky_switch());
    assert!(rim.has_dual_clutch());
    assert!(rim.has_rotary_encoders());
}

#[test]
fn test_pedal_model_clubsport_v3_axis_count() {
    let model = FanatecPedalModel::from_product_id(product_ids::CLUBSPORT_PEDALS_V3);
    assert_eq!(model, FanatecPedalModel::ClubSportV3);
    assert_eq!(model.axis_count(), 3);
}

#[test]
fn test_is_wheelbase_product_does_not_include_pedals() {
    assert!(!is_wheelbase_product(product_ids::CLUBSPORT_PEDALS_V3));
    assert!(!is_wheelbase_product(product_ids::CLUBSPORT_PEDALS_V1_V2));
    assert!(!is_wheelbase_product(product_ids::CSL_PEDALS_LC));
    assert!(!is_wheelbase_product(product_ids::CSL_PEDALS_V2));
}
