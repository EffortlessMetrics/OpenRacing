//! Logitech protocol handler.
//!
//! Implements `VendorProtocol` for Logitech wheels. Pure encoding/parsing
//! is delegated to `racing-wheel-hid-logitech-protocol`.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use racing_wheel_hid_logitech_protocol::{
    CONSTANT_FORCE_REPORT_LEN, LOGITECH_VENDOR_ID, LogitechConstantForceEncoder,
    LogitechInputState, LogitechModel, build_native_mode_report, build_set_range_report,
    ids::report_ids, is_wheel_product, parse_input_report, product_ids,
};

/// Logitech protocol state.
pub struct LogitechProtocol {
    vendor_id: u16,
    product_id: u16,
    model: LogitechModel,
}

impl LogitechProtocol {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = LogitechModel::from_product_id(product_id);
        debug!(
            "Created LogitechProtocol VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> LogitechModel {
        self.model
    }
}

impl VendorProtocol for LogitechProtocol {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !is_wheel_product(self.product_id) {
            debug!(
                "PID 0x{:04X} is not a recognized Logitech wheel; skipping init",
                self.product_id
            );
            return Ok(());
        }

        info!(
            "Initializing Logitech {:?} (VID=0x{:04X} PID=0x{:04X}) into native mode",
            self.model, self.vendor_id, self.product_id
        );

        // Step 1: Switch to native mode (full rotation + FFB).
        // Per Logitech protocol, hardware requires ~100ms after this command;
        // the caller is responsible for inserting that delay before further I/O.
        let native_mode = build_native_mode_report();
        writer.write_feature_report(&native_mode)?;

        // Step 2: Set rotation range to the model's maximum.
        let range_deg = self.model.max_rotation_deg();
        let set_range = build_set_range_report(range_deg);
        writer.write_feature_report(&set_range)?;

        info!(
            "Logitech {:?}: native mode set, range={}°",
            self.model, range_deg
        );
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        _report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        writer.write_feature_report(data)?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: None,
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 4096,
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        Some(report_ids::CONSTANT_FORCE)
    }

    fn output_report_len(&self) -> Option<usize> {
        Some(CONSTANT_FORCE_REPORT_LEN)
    }
}
