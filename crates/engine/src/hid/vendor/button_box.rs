//! HID button box vendor protocol handler.
//!
//! Supports generic HID button boxes (input-only, no force feedback).
//! Compatible with Arduino DIY button boxes, BangButtons, SimRacingInputs, etc.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use hid_button_box_protocol::{
    ButtonBoxInputReport, MAX_BUTTONS, PRODUCT_ID_BUTTON_BOX, VENDOR_ID_GENERIC,
};

/// Returns true when the product ID is a known button box product.
pub fn is_button_box_product(product_id: u16) -> bool {
    product_id == PRODUCT_ID_BUTTON_BOX
}

/// Button box protocol handler (input-only, no FFB output).
pub struct ButtonBoxProtocolHandler {
    vendor_id: u16,
    product_id: u16,
}

impl ButtonBoxProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        debug!(
            "Created ButtonBoxProtocolHandler VID=0x{:04X} PID=0x{:04X}",
            vendor_id, product_id
        );
        Self {
            vendor_id,
            product_id,
        }
    }

    /// Parse button state from a raw HID gamepad input report.
    pub fn parse_input(data: &[u8]) -> Result<ButtonBoxInputReport, Box<dyn std::error::Error>> {
        ButtonBoxInputReport::parse_gamepad(data)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl VendorProtocol for ButtonBoxProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Button box ready VID=0x{:04X} PID=0x{:04X} (input-only, no init required)",
            self.vendor_id, self.product_id,
        );
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        const MAX_REPORT_BYTES: usize = 64;
        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "Feature report too large for button box transport: {} bytes",
                data.len() + 1
            )
            .into());
        }

        let mut report = [0u8; MAX_REPORT_BYTES];
        report[0] = report_id;
        report[1..(data.len() + 1)].copy_from_slice(data);
        writer.write_feature_report(&report[..(data.len() + 1)])?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: None,
            max_torque_nm: 0.0,
            encoder_cpr: 0,
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        None
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
