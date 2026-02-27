//! Cammus protocol handler.
//!
//! Cammus C5 and C12 direct drive wheelbases are Chinese DD wheels that expose
//! a standard USB HID PID (force feedback) interface. They do NOT implement a
//! proprietary 1 kHz torque protocol; all FFB is dispatched through the standard
//! HID PID effect pipeline.
//!
//! Confirmed VID/PID values (source: community USB device captures and
//! RetroBat emulator launcher Wheels.cs, commit 0a54752):
//!   VID 0x3416 (Cammus Technology Co., Ltd.)
//!   C5  PID 0x0301
//!   C12 PID 0x0302

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info};

/// Cammus vendor ID (Cammus Technology Co., Ltd.)
pub const CAMMUS_VENDOR_ID: u16 = 0x3416;

/// Cammus C5 product ID (5 Nm direct drive, confirmed)
pub const CAMMUS_C5_PID: u16 = 0x0301;

/// Cammus C12 product ID (12 Nm direct drive, confirmed)
pub const CAMMUS_C12_PID: u16 = 0x0302;

/// Cammus model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CammusModel {
    /// Cammus C5 — 5 Nm
    C5,
    /// Cammus C12 — 12 Nm
    C12,
    /// Future or unrecognised Cammus product
    Unknown,
}

impl CammusModel {
    /// Resolve model from a product ID.
    pub fn from_product_id(pid: u16) -> Self {
        match pid {
            CAMMUS_C5_PID => Self::C5,
            CAMMUS_C12_PID => Self::C12,
            _ => Self::Unknown,
        }
    }

    /// Human-readable name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::C5 => "Cammus C5",
            Self::C12 => "Cammus C12",
            Self::Unknown => "Cammus (unknown model)",
        }
    }

    /// Rated peak torque in Nm.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::C5 => 5.0,
            Self::C12 => 12.0,
            Self::Unknown => 5.0, // conservative default
        }
    }
}

/// Return true when `product_id` is a known Cammus product.
pub fn is_cammus_product(product_id: u16) -> bool {
    matches!(product_id, CAMMUS_C5_PID | CAMMUS_C12_PID)
}

/// Protocol handler for Cammus direct drive wheelbases.
///
/// Cammus wheels present a standard HID PID interface; no vendor-specific
/// initialisation sequence is required. The handler reports correct capabilities
/// (max torque, brand) and delegates all FFB to the generic HID PID path.
pub struct CammusProtocolHandler {
    vendor_id: u16,
    product_id: u16,
    model: CammusModel,
}

impl CammusProtocolHandler {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = CammusModel::from_product_id(product_id);
        debug!(
            "Created CammusProtocolHandler VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests and diagnostics.
    pub fn model(&self) -> CammusModel {
        self.model
    }
}

impl VendorProtocol for CammusProtocolHandler {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Cammus wheels are plug-and-play over standard HID PID.
        info!(
            "Cammus device ready VID=0x{:04X} PID=0x{:04X} model={} \
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
                "Feature report too large for Cammus transport: {} bytes",
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
            max_torque_nm: self.model.max_torque_nm(),
            encoder_cpr: 0, // encoder resolution not publicly documented
        }
    }

    fn is_v2_hardware(&self) -> bool {
        // C12 uses a higher-performance motor; treat as V2 for feature gating
        matches!(self.model, CammusModel::C12)
    }

    fn output_report_id(&self) -> Option<u8> {
        None // standard HID PID; report ID managed by OS driver
    }

    fn output_report_len(&self) -> Option<usize> {
        None
    }
}
