//! Thrustmaster protocol handler.
//!
//! Implements `VendorProtocol` for Thrustmaster wheels and pedals. Pure encoding
//! is delegated to `racing-wheel-hid-thrustmaster-protocol`.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info, warn};

pub use racing_wheel_hid_thrustmaster_protocol::{
    EFFECT_REPORT_LEN, THRUSTMASTER_VENDOR_ID, ThrustmasterConstantForceEncoder, Model,
    build_actuator_enable, build_device_gain, build_set_range_report,
    output::report_ids, is_wheel_product, is_pedal_product, product_ids,
};

/// Thrustmaster protocol handler.
pub struct ThrustmasterProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: Model,
}

impl ThrustmasterProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = Model::from_product_id(product_id);
        debug!(
            "Created ThrustmasterProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> Model {
        self.model
    }
}

impl VendorProtocol for ThrustmasterProtocolHandler {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !is_wheel_product(self.product_id) {
            debug!(
                "PID 0x{:04X} is not a recognized Thrustmaster wheel; skipping init",
                self.product_id
            );
            return Ok(());
        }

        if !self.model.supports_ffb() {
            warn!(
                "Thrustmaster {:?} (PID=0x{:04X}) does not support FFB; skipping init",
                self.model, self.product_id
            );
            return Ok(());
        }

        info!(
            "Initializing Thrustmaster {:?} (VID=0x{:04X} PID=0x{:04X})",
            self.model, self.vendor_id, self.product_id
        );

        // Step 1: Reset gain to zero.
        let reset_gain = build_device_gain(0);
        writer.write_feature_report(&reset_gain)?;

        // Step 2: Set full gain.
        let full_gain = build_device_gain(0xFF);
        writer.write_feature_report(&full_gain)?;

        // Step 3: Enable actuators.
        let enable = build_actuator_enable(true);
        writer.write_feature_report(&enable)?;

        // Step 4: Set rotation range.
        let range_deg = self.model.max_rotation_deg();
        let set_range = build_set_range_report(range_deg);
        writer.write_feature_report(&set_range)?;

        info!(
            "Thrustmaster {:?}: initialized, range={}°",
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

    fn shutdown_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !is_wheel_product(self.product_id) || !self.model.supports_ffb() {
            return Ok(());
        }

        debug!("Shutting down Thrustmaster {:?}: disabling actuators", self.model);
        let disable = build_actuator_enable(false);
        writer.write_feature_report(&disable)?;
        Ok(())
    }

    fn get_ffb_config(&self) -> FfbConfig {
        FfbConfig {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: Some(1),
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 4096,
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        if self.model.supports_ffb() {
            Some(report_ids::CONSTANT_FORCE)
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if self.model.supports_ffb() {
            Some(EFFECT_REPORT_LEN)
        } else {
            None
        }
    }
}
