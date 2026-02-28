//! PXN protocol handler.
//!
//! PXN V10/V12 direct drive wheels (Shenzhen Jinyu Technology Co., Ltd.)
//! expose a standard USB HID PID (force feedback) interface. They do NOT
//! implement a proprietary 1 kHz torque protocol; all FFB is dispatched
//! through the standard HID PID effect pipeline.
//!
//! Confirmed VID/PID values (source: JacKeTUs/linux-steering-wheels):
//!   VID 0x11FF (Shenzhen Jinyu Technology Co., Ltd.)
//!   V10       PID 0x3245
//!   V12       PID 0x1212
//!   V12 Lite  PID 0x1112
//!   V12 Lite SE PID 0x1211
//!   GT987 FF  PID 0x2141 (Lite Star OEM, shared VID)

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use racing_wheel_hid_pxn_protocol::{PxnModel, VENDOR_ID, is_pxn_device};
use tracing::{debug, info};

pub use racing_wheel_hid_pxn_protocol::VENDOR_ID as PXN_VENDOR_ID;

/// Protocol handler for PXN direct drive wheelbases.
///
/// PXN wheels present a standard HID PID interface; no vendor-specific
/// initialisation sequence is required. The handler reports correct capabilities
/// (max torque, model) and delegates all FFB to the generic HID PID path.
pub struct PxnProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: Option<PxnModel>,
}

impl PxnProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = PxnModel::from_pid(product_id);
        debug!(
            "Created PxnProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> Option<PxnModel> {
        self.model
    }
}

/// Returns true when `product_id` is a known PXN product.
pub fn is_pxn_product(product_id: u16) -> bool {
    is_pxn_device(VENDOR_ID, product_id)
}

impl VendorProtocol for PxnProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // PXN wheels are plug-and-play over standard HID PID.
        let name = self
            .model
            .map(|m| m.name())
            .unwrap_or("PXN (unknown model)");
        let torque = self
            .model
            .map(|m| m.max_torque_nm())
            .unwrap_or(10.0); // conservative default for unknown PXN
        info!(
            "PXN device ready VID=0x{:04X} PID=0x{:04X} model={} \
             max_torque={} Nm (standard HID PID, no proprietary init needed)",
            self.vendor_id, self.product_id, name, torque,
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
                "Feature report too large for PXN transport: {} bytes",
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
            // Standard HID PID — 1 ms bInterval is typical for these devices
            required_b_interval: Some(1),
            max_torque_nm: self.model.map(|m| m.max_torque_nm()).unwrap_or(10.0),
            encoder_cpr: 0, // encoder resolution not publicly documented
        }
    }

    fn is_v2_hardware(&self) -> bool {
        // V12 and V12 Lite use a higher-performance motor; treat as V2 for feature gating
        matches!(
            self.model,
            Some(PxnModel::V12) | Some(PxnModel::V12Lite) | Some(PxnModel::V12LiteSe)
        )
    }

    fn output_report_id(&self) -> Option<u8> {
        None // standard HID PID; report ID managed by OS driver
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
