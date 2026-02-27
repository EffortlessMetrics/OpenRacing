//! Cammus C5/C12 direct drive wheel vendor protocol handler.
//!
//! ## Device IDs
//! - Vendor ID: `0x3285`
//! - Product ID `0x0002`: Cammus C5 (5 Nm)
//! - Product ID `0x0003`: Cammus C12 (12 Nm)
//!
//! ## Protocol
//! Uses a simple 8-byte USB HID output report for real-time torque output.
//! Initialization sends a zero-torque game-mode report to enable FFB.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use racing_wheel_hid_cammus_protocol::{
    CammusModel, FFB_REPORT_ID, FFB_REPORT_LEN, PRODUCT_C12, PRODUCT_C5, VENDOR_ID, encode_stop,
    encode_torque, is_cammus,
};

/// Encoder CPR used for RT steering position calculations.
///
/// Cammus uses a 16-bit signed steering value (±32767 over ±540°), which
/// corresponds to ~21 845 counts per revolution. 16 384 is used as a
/// conservative power-of-two approximation.
const CAMMUS_ENCODER_CPR: u32 = 16_384;

/// Cammus vendor protocol handler.
pub struct CammusProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: CammusModel,
}

impl CammusProtocolHandler {
    /// Create a handler from a USB VID/PID pair.
    ///
    /// Falls back to `CammusModel::C5` for unrecognised PIDs so that newly
    /// released hardware still works with conservative defaults.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = CammusModel::from_pid(product_id).unwrap_or(CammusModel::C5);
        debug!(
            "Created CammusProtocolHandler VID=0x{:04X} PID=0x{:04X} model={}",
            vendor_id,
            product_id,
            model.name()
        );
        Self { vendor_id, product_id, model }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> CammusModel {
        self.model
    }
}

impl VendorProtocol for CammusProtocolHandler {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initialising {} VID=0x{:04X} PID=0x{:04X} max_torque={:.1} Nm",
            self.model.name(),
            self.vendor_id,
            self.product_id,
            self.model.max_torque_nm(),
        );
        // Enter game mode with zero torque so the device is ready for FFB output.
        writer.write_output_report(&encode_stop())?;
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
                "Feature report too large for Cammus transport: {} bytes",
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

    fn shutdown_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!(
            "Shutting down {} VID=0x{:04X} PID=0x{:04X}",
            self.model.name(),
            self.vendor_id,
            self.product_id
        );
        writer.write_output_report(&encode_stop())?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: Some(1),
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: CAMMUS_ENCODER_CPR,
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        Some(FFB_REPORT_ID)
    }

    fn output_report_len(&self) -> Option<usize> {
        Some(FFB_REPORT_LEN)
    }
}
