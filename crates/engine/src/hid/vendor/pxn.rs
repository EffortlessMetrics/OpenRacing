//! PXN / Lite Star protocol handler (standard HID PID).
//!
//! PXN devices enumerate under VID `0x11FF` (Lite Star) and implement standard
//! USB HID PID for force feedback.  The Linux kernel applies
//! `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY`; no proprietary initialisation is
//! required.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

pub use racing_wheel_hid_pxn_protocol::{
    PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2,
    VENDOR_ID as PXN_VENDOR_ID, is_pxn, product_name,
};

/// PXN model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PxnModel {
    /// PXN V10 direct-drive
    V10,
    /// PXN V12 direct-drive
    V12,
    /// PXN V12 Lite
    V12Lite,
    /// PXN V12 Lite variant (SE)
    V12LiteSe,
    /// Lite Star GT987 FF
    Gt987,
    /// Unrecognised PXN product
    Unknown,
}

impl PxnModel {
    /// Resolve model from a product ID.
    pub fn from_product_id(pid: u16) -> Self {
        match pid {
            PRODUCT_V10 => Self::V10,
            PRODUCT_V12 => Self::V12,
            PRODUCT_V12_LITE => Self::V12Lite,
            PRODUCT_V12_LITE_2 => Self::V12LiteSe,
            PRODUCT_GT987 => Self::Gt987,
            _ => Self::Unknown,
        }
    }

    /// Human-readable name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::V10 => "PXN V10",
            Self::V12 => "PXN V12",
            Self::V12Lite => "PXN V12 Lite",
            Self::V12LiteSe => "PXN V12 Lite (SE)",
            Self::Gt987 => "Lite Star GT987 FF",
            Self::Unknown => "PXN (unknown model)",
        }
    }

    /// Conservative rated peak torque in Nm.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::V10 => 10.0,
            Self::V12 => 12.0,
            Self::V12Lite | Self::V12LiteSe => 6.0,
            Self::Gt987 => 5.0,
            Self::Unknown => 5.0,
        }
    }
}

/// Return true when `product_id` is a known PXN / Lite Star product.
pub fn is_pxn_product(product_id: u16) -> bool {
    matches!(
        product_id,
        PRODUCT_V10 | PRODUCT_V12 | PRODUCT_V12_LITE | PRODUCT_V12_LITE_2 | PRODUCT_GT987
    )
}

/// Protocol handler for PXN / Lite Star wheelbases.
///
/// PXN devices present a standard HID PID interface; no vendor-specific
/// initialisation is required.
pub struct PxnProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: PxnModel,
}

impl PxnProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = PxnModel::from_product_id(product_id);
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
    pub fn model(&self) -> PxnModel {
        self.model
    }
}

impl VendorProtocol for PxnProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // PXN wheels are plug-and-play over standard HID PID.
        info!(
            "PXN device ready VID=0x{:04X} PID=0x{:04X} model={} \
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
            required_b_interval: Some(1),
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 0, // encoder resolution not publicly documented
        }
    }

    fn is_v2_hardware(&self) -> bool {
        // V12 uses a higher-performance motor; treat as V2 for feature gating
        matches!(self.model, PxnModel::V12)
    }

    fn output_report_id(&self) -> Option<u8> {
        None // standard HID PID; report ID managed by OS driver
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
