//! VRS DirectForce Pro protocol handler (PIDFF).

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use racing_wheel_hid_vrs_protocol::{
    CONSTANT_FORCE_REPORT_LEN, VRS_VENDOR_ID, VrsConstantForceEncoder, build_device_gain,
    build_ffb_enable, build_rotation_range, is_wheelbase_product, product_ids,
};

/// VRS protocol state.
pub struct VrsProtocolHandler {
    vendor_id: u16,
    product_id: u16,
}

impl VrsProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        debug!(
            "Created VrsProtocolHandler VID=0x{:04X} PID=0x{:04X}",
            vendor_id, product_id
        );
        Self {
            vendor_id,
            product_id,
        }
    }

    fn max_torque_nm(&self) -> f32 {
        match self.product_id {
            product_ids::DIRECTFORCE_PRO => 20.0,
            product_ids::DIRECTFORCE_PRO_V2 => 25.0,
            _ => 20.0,
        }
    }
}

/// Return true when the product ID is a known VRS product.
pub fn is_vrs_product(product_id: u16) -> bool {
    matches!(product_id, 0xA355..=0xA35A)
}

impl VendorProtocol for VrsProtocolHandler {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initializing VRS device VID=0x{:04X} PID=0x{:04X}",
            self.vendor_id, self.product_id
        );

        if is_wheelbase_product(self.product_id) {
            debug!("VRS wheelbase: enabling FFB and setting gain/rotation");
            writer.write_feature_report(&build_ffb_enable(true))?;
            writer.write_feature_report(&build_device_gain(0xFF))?;
            writer.write_feature_report(&build_rotation_range(1080))?;
        } else {
            debug!("VRS non-wheelbase device: no init commands needed");
        }

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
                "Feature report too large for VRS transport: {} bytes",
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
        if is_wheelbase_product(self.product_id) {
            debug!("VRS wheelbase: disabling FFB on shutdown");
            writer.write_feature_report(&build_ffb_enable(false))?;
        }
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: Some(1),
            max_torque_nm: self.max_torque_nm(),
            encoder_cpr: 1_048_576,
        }
    }

    fn is_v2_hardware(&self) -> bool {
        self.product_id == product_ids::DIRECTFORCE_PRO_V2
    }

    fn output_report_id(&self) -> Option<u8> {
        if is_wheelbase_product(self.product_id) {
            Some(0x11) // CONSTANT_FORCE
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if is_wheelbase_product(self.product_id) {
            Some(CONSTANT_FORCE_REPORT_LEN)
        } else {
            None
        }
    }
}
