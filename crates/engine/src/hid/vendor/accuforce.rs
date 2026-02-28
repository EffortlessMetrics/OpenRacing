//! AccuForce Pro (SimExperience) protocol handler.
//!
//! The SimExperience AccuForce Pro is a brushless direct drive wheelbase that
//! exposes a standard USB HID PID (force feedback) interface. No proprietary
//! torque protocol is used at the USB level.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
pub use racing_wheel_hid_accuforce_protocol::is_accuforce_pid as is_accuforce_product;
use racing_wheel_hid_accuforce_protocol::{AccuForceModel, MAX_REPORT_BYTES};
use tracing::{debug, info};

/// Protocol handler for SimExperience AccuForce Pro wheelbases.
///
/// AccuForce Pro presents a standard HID PID interface; no vendor-specific
/// initialisation procedure is required. The handler reports correct capabilities
/// (max torque, brand) and delegates all FFB to the generic HID PID path.
pub struct AccuForceProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: AccuForceModel,
}

impl AccuForceProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = AccuForceModel::from_product_id(product_id);
        debug!(
            "Created AccuForceProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> AccuForceModel {
        self.model
    }
}

impl VendorProtocol for AccuForceProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // AccuForce Pro is plug-and-play over standard HID PID.
        info!(
            "AccuForce device ready VID=0x{:04X} PID=0x{:04X} model={} \
             max_torque={} Nm (standard HID PID, no proprietary init needed)",
            self.vendor_id,
            self.product_id,
            self.model.display_name(),
            self.model.max_torque_nm(),
        );
        Ok(())
    }

    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "Feature report too large for AccuForce transport: {} bytes",
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
            // AccuForce Pro USB update rate is ~100–200 Hz; 8 ms is a safe interval
            required_b_interval: Some(8),
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 0, // encoder resolution not publicly documented
        }
    }

    fn is_v2_hardware(&self) -> bool {
        false
    }

    fn output_report_id(&self) -> Option<u8> {
        None // standard HID PID; report ID managed by OS driver
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
