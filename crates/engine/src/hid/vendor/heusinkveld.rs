//! Heusinkveld pedals protocol handler (input-only, no FFB output).

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use hid_heusinkveld_protocol::{
    HEUSINKVELD_PRO_PID, HEUSINKVELD_SPRINT_PID, HEUSINKVELD_ULTIMATE_PID, HEUSINKVELD_VENDOR_ID,
    HeusinkveldModel, heusinkveld_model_from_info,
};

/// Heusinkveld protocol state.
pub struct HeusinkveldProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: HeusinkveldModel,
}

impl HeusinkveldProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = heusinkveld_model_from_info(vendor_id, product_id);
        debug!(
            "Created HeusinkveldProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> HeusinkveldModel {
        self.model
    }

    /// Number of pedals for this model.
    pub fn pedal_count(&self) -> usize {
        self.model.pedal_count()
    }
}

/// Return true when the product ID is a known Heusinkveld product.
pub fn is_heusinkveld_product(product_id: u16) -> bool {
    matches!(product_id, 0x1156..=0x1158)
}

impl VendorProtocol for HeusinkveldProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initialized Heusinkveld {} (VID=0x{:04X} PID=0x{:04X}, {} pedals)",
            self.model.display_name(),
            self.vendor_id,
            self.product_id,
            self.model.pedal_count(),
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
                "Feature report too large for Heusinkveld transport: {} bytes",
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
