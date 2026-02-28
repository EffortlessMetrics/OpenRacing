//! Fanatec protocol handler.
//!
//! Implements `VendorProtocol` for Fanatec devices. Pure encoding/parsing
//! is delegated to `racing-wheel-hid-fanatec-protocol`.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info, warn};

pub use racing_wheel_hid_fanatec_protocol::{
    CONSTANT_FORCE_REPORT_LEN, FANATEC_VENDOR_ID, FanatecConstantForceEncoder,
    FanatecExtendedState, FanatecInputState, FanatecModel, FanatecPedalModel, FanatecPedalState,
    FanatecRimId, is_pedal_product, is_wheelbase_product, parse_extended_report,
    parse_pedal_report, parse_standard_report, product_ids, rim_ids,
};

/// Fanatec protocol state.
pub struct FanatecProtocol {
    vendor_id: u16,
    product_id: u16,
    model: FanatecModel,
}

impl FanatecProtocol {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = FanatecModel::from_product_id(product_id);
        debug!(
            "Created FanatecProtocol VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> FanatecModel {
        self.model
    }
}

impl VendorProtocol for FanatecProtocol {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !is_wheelbase_product(self.product_id) {
            debug!(
                "Fanatec PID=0x{:04X} is not a wheelbase; skipping mode-switch handshake",
                self.product_id
            );
            return Ok(());
        }

        info!(
            "Initializing Fanatec device VID=0x{:04X} PID=0x{:04X} model={:?}",
            self.vendor_id, self.product_id, self.model
        );

        let report = racing_wheel_hid_fanatec_protocol::build_mode_switch_report();
        writer.write_feature_report(&report).map_err(|e| {
            warn!(
                "Fanatec mode-switch feature report failed VID=0x{:04X} PID=0x{:04X}: {}",
                self.vendor_id, self.product_id, e
            );
            e
        })?;

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
                "Feature report too large for Fanatec transport: {} bytes",
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
            encoder_cpr: self.model.encoder_cpr(),
        }
    }

    fn is_v2_hardware(&self) -> bool {
        matches!(
            self.model,
            FanatecModel::Dd1 | FanatecModel::Dd2 | FanatecModel::ClubSportDd
        )
    }

    fn output_report_id(&self) -> Option<u8> {
        if is_wheelbase_product(self.product_id) {
            Some(racing_wheel_hid_fanatec_protocol::ids::report_ids::FFB_OUTPUT)
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

    fn shutdown_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !is_wheelbase_product(self.product_id) {
            return Ok(());
        }

        debug!(
            "Sending stop-all to Fanatec VID=0x{:04X} PID=0x{:04X} on shutdown",
            self.vendor_id, self.product_id
        );

        let report = racing_wheel_hid_fanatec_protocol::build_stop_all_report();
        // Write errors are logged at debug — device may already be disconnected.
        if let Err(e) = writer.write_output_report(&report) {
            debug!(
                "Fanatec stop-all failed for VID=0x{:04X} PID=0x{:04X} (device may be disconnected): {}",
                self.vendor_id, self.product_id, e
            );
        }
        Ok(())
    }
}
