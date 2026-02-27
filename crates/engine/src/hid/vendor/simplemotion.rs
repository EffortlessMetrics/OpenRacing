//! SimpleMotion V2 vendor protocol handler.
//!
//! Supports Granite Devices IONI Pro, IONI Drive, ARGON servo drives,
//! and community Open Sim Wheel (OSW) direct-drive bases.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use racing_wheel_simplemotion_v2::{
    ARGON_PRODUCT_ID, IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM, IONI_VENDOR_ID,
    SmDeviceIdentity, SmFeedbackState, TORQUE_COMMAND_LEN, TorqueCommandEncoder,
    build_device_enable, identify_device, is_wheelbase_product, parse_feedback_report,
};

/// Granite Devices / SimpleMotion V2 USB Vendor ID.
pub const SM_VENDOR_ID: u16 = 0x1D50;

/// SimpleMotion V2 protocol handler for Granite Devices IONI/ARGON drives and OSW bases.
pub struct SimpleMotionProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    identity: SmDeviceIdentity,
}

impl SimpleMotionProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let identity = identify_device(product_id);
        debug!(
            "Created SimpleMotionProtocolHandler VID=0x{:04X} PID=0x{:04X} device={}",
            vendor_id, product_id, identity.name
        );
        Self {
            vendor_id,
            product_id,
            identity,
        }
    }

    /// Device identity classification used by tests and diagnostics.
    pub fn identity(&self) -> &SmDeviceIdentity {
        &self.identity
    }

    /// Returns true if this device supports force feedback.
    pub fn supports_ffb(&self) -> bool {
        self.identity.supports_ffb
    }

    /// Encode a torque command into a pre-allocated buffer for the RT path.
    ///
    /// Returns the number of bytes written.
    pub fn write_torque(
        encoder: &mut TorqueCommandEncoder,
        torque_nm: f32,
        out: &mut [u8; TORQUE_COMMAND_LEN],
    ) -> usize {
        encoder.encode(torque_nm, out)
    }

    /// Parse a feedback report from the device.
    pub fn read_state(data: &[u8]) -> Result<SmFeedbackState, Box<dyn std::error::Error>> {
        parse_feedback_report(data).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl VendorProtocol for SimpleMotionProtocolHandler {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Enable motor drive (set control mode to torque via parameter 0x1001)
        let enable_cmd = build_device_enable(true, 0);
        writer.write_output_report(&enable_cmd)?;

        info!(
            "Initialized SimpleMotion V2 device: {} (VID=0x{:04X} PID=0x{:04X}, FFB={})",
            self.identity.name, self.vendor_id, self.product_id, self.identity.supports_ffb,
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
                "Feature report too large for SimpleMotion transport: {} bytes",
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
            uses_vendor_usage_page: true,
            required_b_interval: Some(1), // 1kHz
            max_torque_nm: self.identity.max_torque_nm.unwrap_or(0.0),
            encoder_cpr: 131_072, // 17-bit default for SimpleMotion V2
        }
    }

    fn is_v2_hardware(&self) -> bool {
        // IONI Premium and ARGON are V2 hardware
        matches!(self.product_id, IONI_PRODUCT_ID_PREMIUM | ARGON_PRODUCT_ID)
    }

    fn output_report_id(&self) -> Option<u8> {
        if self.identity.supports_ffb {
            Some(0x01)
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if self.identity.supports_ffb {
            Some(TORQUE_COMMAND_LEN)
        } else {
            None
        }
    }
}
