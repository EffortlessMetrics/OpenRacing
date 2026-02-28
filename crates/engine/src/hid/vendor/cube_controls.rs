//! Cube Controls protocol handler.
//!
//! Cube Controls (Cube Controls S.r.l., Italy) makes premium steering wheels
//! including the GT Pro, Formula Pro, and CSX3. These products expose a standard
//! USB HID PID (force feedback) interface.
//!
//! VID/PID constants and model classification are defined in the
//! `hid-cube-controls-protocol` crate. See that crate's documentation and
//! `docs/protocols/SOURCES.md` for the provisional status of these values.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info, warn};

pub use hid_cube_controls_protocol::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};

/// Protocol handler for Cube Controls steering wheel products.
///
/// Cube Controls wheels are assumed to present a standard HID PID interface.
/// No vendor-specific initialisation sequence has been identified. The handler
/// reports correct capabilities (max torque, brand) and delegates all FFB to
/// the generic HID PID path.
///
/// **PROVISIONAL**: VID and PIDs are unconfirmed — see module-level docs.
pub struct CubeControlsProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: CubeControlsModel,
}

impl CubeControlsProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = CubeControlsModel::from_product_id(product_id);
        debug!(
            "Created CubeControlsProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?} \
             (provisional PIDs — not confirmed from real hardware)",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> CubeControlsModel {
        self.model
    }
}

impl VendorProtocol for CubeControlsProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.model.is_provisional() {
            warn!(
                "Cube Controls device VID=0x{:04X} PID=0x{:04X}: \
                 using PROVISIONAL PID — verify against real hardware",
                self.vendor_id, self.product_id,
            );
        }
        info!(
            "Cube Controls device ready VID=0x{:04X} PID=0x{:04X} model={} \
             max_torque={} Nm (standard HID PID assumed, no proprietary init)",
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
        const MAX_REPORT_BYTES: usize = 64;
        if data.len() + 1 > MAX_REPORT_BYTES {
            return Err(format!(
                "Feature report too large for Cube Controls transport: {} bytes",
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
