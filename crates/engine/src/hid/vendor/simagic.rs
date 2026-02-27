//! Simagic protocol handler.
//!
//! Legacy devices (VIDs 0x0483, 0x16D0) use passive/capture mode.
//! EVO-generation devices (VID 0x3670) use active FFB initialization.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info, warn};

pub use racing_wheel_hid_simagic_protocol::{
    CONSTANT_FORCE_REPORT_LEN, SIMAGIC_VENDOR_ID, build_device_gain, build_rotation_range,
};
use racing_wheel_hid_simagic_protocol::ids::report_ids;

/// Simagic vendor IDs observed across hardware generations.
pub mod vendor_ids {
    /// Legacy Simagic VID (STMicroelectronics-based USB stack).
    pub const SIMAGIC_STM: u16 = 0x0483;
    /// Legacy Simagic alternate VID (MCS/OpenMoko, shared with Heusinkveld).
    pub const SIMAGIC_ALT: u16 = 0x16D0;
    /// Simagic EVO VID (Shen Zhen Simagic Technology Co., Ltd.).
    pub const SIMAGIC_EVO: u16 = 0x3670;
}

/// Known Simagic product IDs.
pub mod product_ids {
    // Legacy PIDs (VIDs 0x0483, 0x16D0)
    pub const ALPHA: u16 = 0x0522;
    pub const ALPHA_MINI: u16 = 0x0523;
    pub const ALPHA_ULTIMATE: u16 = 0x0524;
    pub const M10: u16 = 0x0D5A;
    pub const FX: u16 = 0x0D5B;

    // EVO generation PIDs (VID 0x3670) — verified via linux-steering-wheels
    pub const EVO_SPORT: u16 = 0x0500;
    pub const EVO: u16 = 0x0501;
    pub const EVO_PRO: u16 = 0x0502;
}

/// Simagic model classification used for conservative defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimagicModel {
    Alpha,
    AlphaMini,
    AlphaUltimate,
    M10,
    Fx,
    EvoSport,
    Evo,
    EvoPro,
    EvoUnknown,
    Unknown,
}

impl SimagicModel {
    fn from_ids(vendor_id: u16, product_id: u16) -> Self {
        if vendor_id == vendor_ids::SIMAGIC_EVO {
            return match product_id {
                product_ids::EVO_SPORT => Self::EvoSport,
                product_ids::EVO => Self::Evo,
                product_ids::EVO_PRO => Self::EvoPro,
                _ => Self::EvoUnknown,
            };
        }

        match product_id {
            product_ids::ALPHA => Self::Alpha,
            product_ids::ALPHA_MINI => Self::AlphaMini,
            product_ids::ALPHA_ULTIMATE => Self::AlphaUltimate,
            product_ids::M10 => Self::M10,
            product_ids::FX => Self::Fx,
            _ => Self::Unknown,
        }
    }

    fn max_torque_nm(self) -> f32 {
        match self {
            Self::Alpha => 15.0,
            Self::AlphaMini => 10.0,
            Self::AlphaUltimate => 23.0,
            Self::M10 => 10.0,
            Self::Fx => 6.0,
            Self::EvoSport => 15.0,
            Self::Evo => 20.0,
            Self::EvoPro => 30.0,
            Self::EvoUnknown => 15.0,
            Self::Unknown => 5.0,
        }
    }

    fn encoder_cpr(self) -> u32 {
        match self {
            Self::EvoSport | Self::Evo | Self::EvoPro | Self::EvoUnknown => 2_097_152,
            _ => 262_144,
        }
    }

    fn is_evo_generation(self) -> bool {
        matches!(
            self,
            Self::EvoSport | Self::Evo | Self::EvoPro | Self::EvoUnknown
        )
    }
}

/// Simagic protocol state.
pub struct SimagicProtocol {
    vendor_id: u16,
    product_id: u16,
    model: SimagicModel,
}

impl SimagicProtocol {
    /// Create a protocol handler from a USB identity pair.
    pub fn new(vendor_id: u16, product_id: u16) -> Self {
        let model = SimagicModel::from_ids(vendor_id, product_id);
        debug!(
            "Created SimagicProtocol VID=0x{:04X} PID=0x{:04X} model={:?}",
            vendor_id, product_id, model
        );
        Self {
            vendor_id,
            product_id,
            model,
        }
    }

    /// Model classification used by tests/diagnostics.
    pub fn model(&self) -> SimagicModel {
        self.model
    }

    fn is_evo_device(&self) -> bool {
        self.vendor_id == vendor_ids::SIMAGIC_EVO
    }
}

impl VendorProtocol for SimagicProtocol {
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initializing Simagic device VID=0x{:04X} PID=0x{:04X} model={:?}",
            self.vendor_id, self.product_id, self.model
        );

        if self.is_evo_device() {
            if matches!(self.model, SimagicModel::EvoUnknown) {
                warn!(
                    "Unknown EVO device PID=0x{:04X}; sending conservative gain and rotation range",
                    self.product_id
                );
            } else {
                debug!("Simagic EVO device (0x3670): sending gain and rotation range");
            }
            writer.write_feature_report(&build_device_gain(0xFF))?;
            writer.write_feature_report(&build_rotation_range(900))?;
        } else {
            debug!(
                "No vendor handshake applied for legacy Simagic model {:?}; continuing in passive mode",
                self.model
            );
        }

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
                "Feature report too large for Simagic transport: {} bytes",
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
        self.model.is_evo_generation()
    }

    fn output_report_id(&self) -> Option<u8> {
        if self.is_evo_device() {
            Some(report_ids::CONSTANT_FORCE)
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if self.is_evo_device() {
            Some(CONSTANT_FORCE_REPORT_LEN)
        } else {
            None
        }
    }
}
