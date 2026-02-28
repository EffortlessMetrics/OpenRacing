//! Asetek protocol handler.
//!
//! Implements `VendorProtocol` for Asetek direct drive wheelbases.
//! Pure encoding/parsing is delegated to `hid-asetek-protocol`.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use hid_asetek_protocol::{
    ASETEK_FORTE_PID, ASETEK_INVICTA_PID, ASETEK_LAPRIMA_PID, ASETEK_VENDOR_ID, AsetekModel,
    REPORT_SIZE_OUTPUT,
};

/// Asetek protocol state.
pub struct AsetekProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: AsetekModel,
}

impl AsetekProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = AsetekModel::from_product_id(product_id);
        debug!(
            "Created AsetekProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> AsetekModel {
        self.model
    }
}

impl VendorProtocol for AsetekProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Asetek direct drive wheels are plug-and-play; no init sequence needed.
        info!(
            "Asetek device ready VID=0x{:04X} PID=0x{:04X} model={} (no init sequence needed)",
            self.vendor_id,
            self.product_id,
            self.model.display_name()
        );
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        const MAX_REPORT_BYTES: usize = 32;
        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "Feature report too large for Asetek transport: {} bytes",
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
            required_b_interval: Some(1),
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 1_048_576, // 20-bit encoder estimate
        }
    }

    fn is_v2_hardware(&self) -> bool {
        matches!(self.model, AsetekModel::Forte)
    }

    fn output_report_id(&self) -> Option<u8> {
        // Asetek output report starts with a sequence number, not a report ID byte.
        None
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
