//! Cube Controls protocol handler.
//!
//! Cube Controls (Cube Controls S.r.l., Italy) makes premium steering wheels
//! including the GT Pro, Formula Pro, and CSX3. These products expose a standard
//! USB HID PID (force feedback) interface.
//!
//! # VID/PID status — PROVISIONAL
//!
//! The exact USB VID and PIDs for Cube Controls devices have **not** been
//! independently confirmed via official documentation or captured USB descriptors
//! at the time of writing. The values below are provisional best-guesses based on
//! community reports that place Cube Controls hardware on the STMicroelectronics
//! shared VID (0x0483). These PIDs have NOT been verified against real hardware.
//!
//! ACTION REQUIRED: Once confirmed (e.g., from a USB device tree capture on real
//! hardware), update the constants below and remove the PROVISIONAL annotations.
//!
//! Provisional assignments:
//!   VID 0x0483 (STMicroelectronics — shared VID used by many STM32 devices)
//!   GT Pro     PID 0x0C73  (provisional, unconfirmed)
//!   Formula Pro PID 0x0C74 (provisional, unconfirmed)
//!   CSX3       PID 0x0C75  (provisional, unconfirmed)

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info, warn};

/// Cube Controls vendor ID (provisional — STMicroelectronics shared VID).
///
/// **PROVISIONAL**: This VID is unconfirmed. Cube Controls devices may use a
/// different VID. Update once confirmed from real hardware.
pub const CUBE_CONTROLS_VENDOR_ID: u16 = 0x0483;

/// Cube Controls GT Pro product ID (provisional, unconfirmed).
pub const CUBE_CONTROLS_GT_PRO_PID: u16 = 0x0C73;

/// Cube Controls Formula Pro product ID (provisional, unconfirmed).
pub const CUBE_CONTROLS_FORMULA_PRO_PID: u16 = 0x0C74;

/// Cube Controls CSX3 product ID (provisional, unconfirmed).
pub const CUBE_CONTROLS_CSX3_PID: u16 = 0x0C75;

/// Cube Controls model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeControlsModel {
    /// GT Pro — up to ~20 Nm (provisional PID)
    GtPro,
    /// Formula Pro — up to ~20 Nm (provisional PID)
    FormulaPro,
    /// CSX3 — up to ~20 Nm (provisional PID)
    Csx3,
    /// Future or unrecognised Cube Controls product
    Unknown,
}

impl CubeControlsModel {
    /// Resolve model from a product ID.
    pub fn from_product_id(pid: u16) -> Self {
        match pid {
            CUBE_CONTROLS_GT_PRO_PID => Self::GtPro,
            CUBE_CONTROLS_FORMULA_PRO_PID => Self::FormulaPro,
            CUBE_CONTROLS_CSX3_PID => Self::Csx3,
            _ => Self::Unknown,
        }
    }

    /// Human-readable name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::GtPro => "Cube Controls GT Pro",
            Self::FormulaPro => "Cube Controls Formula Pro",
            Self::Csx3 => "Cube Controls CSX3",
            Self::Unknown => "Cube Controls (unknown model)",
        }
    }

    /// Rated peak torque in Nm.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::GtPro | Self::FormulaPro | Self::Csx3 => 20.0,
            Self::Unknown => 20.0, // conservative default
        }
    }

    /// Whether this is a "provisional" (unconfirmed PID) entry.
    pub fn is_provisional(self) -> bool {
        // All current Cube Controls PIDs are provisional until confirmed.
        true
    }
}

/// Return true when `product_id` is a (provisionally) known Cube Controls product.
///
/// **PROVISIONAL**: These PIDs are unconfirmed; the function may need to be
/// updated once real hardware captures are available.
pub fn is_cube_controls_product(product_id: u16) -> bool {
    matches!(
        product_id,
        CUBE_CONTROLS_GT_PRO_PID | CUBE_CONTROLS_FORMULA_PRO_PID | CUBE_CONTROLS_CSX3_PID
    )
}

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
