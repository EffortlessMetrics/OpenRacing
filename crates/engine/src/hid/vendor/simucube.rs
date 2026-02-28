//! Simucube 2 protocol handler.
//!
//! Implements `VendorProtocol` for Simucube 2 direct drive wheelbases.
//! Pure encoding/parsing is delegated to `hid-simucube-protocol`.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use hid_simucube_protocol::{
    REPORT_SIZE_OUTPUT, SIMUCUBE_2_PRO_PID, SIMUCUBE_2_SPORT_PID, SIMUCUBE_2_ULTIMATE_PID,
    SIMUCUBE_ACTIVE_PEDAL_PID, SIMUCUBE_VENDOR_ID, SIMUCUBE_WIRELESS_WHEEL_PID, SimucubeModel,
};

/// Simucube 2 protocol state.
pub struct SimucubeProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: SimucubeModel,
}

impl SimucubeProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = SimucubeModel::from_product_id(product_id);
        debug!(
            "Created SimucubeProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> SimucubeModel {
        self.model
    }
}

impl VendorProtocol for SimucubeProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Simucube 2 devices are FFB-ready on USB plug-in; no handshake required.
        info!(
            "Simucube device ready VID=0x{:04X} PID=0x{:04X} model={} (no initialization steps needed)",
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
        const MAX_REPORT_BYTES: usize = 64;
        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "Feature report too large for Simucube transport: {} bytes",
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
            required_b_interval: Some(3), // ~360 Hz
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 4_194_304, // 22-bit angle sensor
        }
    }

    fn is_v2_hardware(&self) -> bool {
        matches!(self.model, SimucubeModel::Pro | SimucubeModel::Ultimate)
    }

    fn output_report_id(&self) -> Option<u8> {
        // First byte of every SimucubeOutputReport is the report ID 0x01.
        Some(0x01)
    }

    fn output_report_len(&self) -> Option<usize> {
        Some(REPORT_SIZE_OUTPUT)
    }
}

/// Return `true` when `product_id` belongs to a known Simucube 2 wheelbase.
pub fn is_simucube_product(product_id: u16) -> bool {
    matches!(
        product_id,
        SIMUCUBE_2_SPORT_PID
            | SIMUCUBE_2_PRO_PID
            | SIMUCUBE_2_ULTIMATE_PID
            | SIMUCUBE_ACTIVE_PEDAL_PID
            | SIMUCUBE_WIRELESS_WHEEL_PID
    )
}
