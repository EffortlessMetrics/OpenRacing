//! Tests for the button box protocol handler.

use super::button_box::{
    ButtonBoxProtocolHandler, PRODUCT_ID_BUTTON_BOX, VENDOR_ID_GENERIC, is_button_box_product,
};
use super::{DeviceWriter, VendorProtocol, get_vendor_protocol};

struct MockWriter;

impl DeviceWriter for MockWriter {
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(data.len())
    }

    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        Ok(data.len())
    }
}

#[test]
fn test_handler_creation_no_ffb() {
    let h = ButtonBoxProtocolHandler::new(VENDOR_ID_GENERIC, PRODUCT_ID_BUTTON_BOX);
    let cfg = h.get_ffb_config();
    assert_eq!(cfg.max_torque_nm, 0.0);
    assert_eq!(cfg.encoder_cpr, 0);
    assert!(!h.is_v2_hardware());
    assert_eq!(h.output_report_id(), None);
    assert_eq!(h.output_report_len(), None);
}

#[test]
fn test_initialize_no_writes() -> Result<(), Box<dyn std::error::Error>> {
    let h = ButtonBoxProtocolHandler::new(VENDOR_ID_GENERIC, PRODUCT_ID_BUTTON_BOX);
    let mut writer = MockWriter;
    h.initialize_device(&mut writer)?;
    Ok(())
}

#[test]
fn test_parse_input_report() -> Result<(), Box<dyn std::error::Error>> {
    // 10-byte gamepad report: buttons(2) + axis_x(2) + axis_y(2) + axis_z(2) + hat(1) + pad(1)
    // button 0 pressed (LSB of first byte)
    let data = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00];
    let report = ButtonBoxProtocolHandler::parse_input(&data)?;
    assert!(report.button(0), "button 0 should be pressed");
    assert!(!report.button(1), "button 1 should not be pressed");
    Ok(())
}

#[test]
fn test_parse_input_too_short() {
    let data = [0u8; 4];
    let result = ButtonBoxProtocolHandler::parse_input(&data);
    assert!(result.is_err(), "short report must be rejected");
}

#[test]
fn test_is_button_box_product() {
    assert!(is_button_box_product(PRODUCT_ID_BUTTON_BOX));
    assert!(!is_button_box_product(0xFFFF));
    assert!(!is_button_box_product(0x0000));
}

#[test]
fn test_get_vendor_protocol_button_box() {
    let proto = get_vendor_protocol(VENDOR_ID_GENERIC, PRODUCT_ID_BUTTON_BOX);
    assert!(proto.is_some(), "known button box must be dispatched to a handler");
}
