//! Simagic protocol handler.
//!
//! Legacy devices (VIDs 0x0483, 0x16D0, 0x3670) use capture-first/passive mode.
//! Modern 0x2D5C devices use active FFB initialization via the simagic protocol crate.

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
    /// Legacy Simagic alternate VID.
    pub const SIMAGIC_ALT: u16 = 0x16D0;
    /// Simagic-owned VID used by newer devices.
    pub const SIMAGIC_EVO: u16 = 0x3670;
    /// Modern Simagic VID (2D5C family) with active FFB support.
    pub const SIMAGIC_MODERN: u16 = 0x2D5C;
}

/// Known and candidate Simagic product IDs.
pub mod product_ids {
    // Legacy PIDs (VIDs 0x0483, 0x16D0)
    pub const ALPHA: u16 = 0x0522;
    pub const ALPHA_MINI: u16 = 0x0523;
    pub const ALPHA_ULTIMATE: u16 = 0x0524;
    pub const M10: u16 = 0x0D5A;
    pub const FX: u16 = 0x0D5B;

    // Capture-candidate IDs for Alpha EVO generation (VID 0x3670).
    pub const ALPHA_EVO_SPORT_CANDIDATE: u16 = 0x0001;
    pub const ALPHA_EVO_CANDIDATE: u16 = 0x0002;
    pub const ALPHA_EVO_PRO_CANDIDATE: u16 = 0x0003;

    // Modern PIDs (VID 0x2D5C)
    pub const ALPHA_MODERN: u16 = 0x0101;
    pub const ALPHA_MINI_MODERN: u16 = 0x0102;
    pub const ALPHA_EVO: u16 = 0x0103;
    pub const M10_MODERN: u16 = 0x0201;
    pub const NEO: u16 = 0x0301;
    pub const NEO_MINI: u16 = 0x0302;
}

/// Simagic model classification used for conservative defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimagicModel {
    Alpha,
    AlphaMini,
    AlphaUltimate,
    M10,
    Fx,
    AlphaEvoSportCandidate,
    AlphaEvoCandidate,
    AlphaEvoProCandidate,
    AlphaEvoUnknown,
    // Modern 0x2D5C variants
    AlphaEvo,
    M10New,
    Neo,
    NeoMini,
    Unknown,
}

impl SimagicModel {
    fn from_ids(vendor_id: u16, product_id: u16) -> Self {
        if vendor_id == vendor_ids::SIMAGIC_MODERN {
            return match product_id {
                product_ids::ALPHA_MODERN => Self::Alpha,
                product_ids::ALPHA_MINI_MODERN => Self::AlphaMini,
                product_ids::ALPHA_EVO => Self::AlphaEvo,
                product_ids::M10_MODERN => Self::M10New,
                product_ids::NEO => Self::Neo,
                product_ids::NEO_MINI => Self::NeoMini,
                _ => Self::Unknown,
            };
        }

        if vendor_id == vendor_ids::SIMAGIC_EVO {
            return match product_id {
                product_ids::ALPHA_EVO_SPORT_CANDIDATE => Self::AlphaEvoSportCandidate,
                product_ids::ALPHA_EVO_CANDIDATE => Self::AlphaEvoCandidate,
                product_ids::ALPHA_EVO_PRO_CANDIDATE => Self::AlphaEvoProCandidate,
                _ => Self::AlphaEvoUnknown,
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
            Self::AlphaEvoSportCandidate => 9.0,
            Self::AlphaEvoCandidate => 12.0,
            Self::AlphaEvoProCandidate => 18.0,
            // Conservative default until capture confirms exact hardware tier.
            Self::AlphaEvoUnknown => 9.0,
            Self::AlphaEvo => 15.0,
            Self::M10New => 10.0,
            Self::Neo => 10.0,
            Self::NeoMini => 7.0,
            Self::Unknown => 5.0,
        }
    }

    fn encoder_cpr(self) -> u32 {
        match self {
            Self::AlphaEvoSportCandidate
            | Self::AlphaEvoCandidate
            | Self::AlphaEvoProCandidate
            | Self::AlphaEvoUnknown
            | Self::AlphaEvo
            | Self::M10New
            | Self::Neo
            | Self::NeoMini => 2_097_152,
            _ => 262_144,
        }
    }

    fn is_evo_generation(self) -> bool {
        matches!(
            self,
            Self::AlphaEvoSportCandidate
                | Self::AlphaEvoCandidate
                | Self::AlphaEvoProCandidate
                | Self::AlphaEvoUnknown
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

    fn is_modern_device(&self) -> bool {
        self.vendor_id == vendor_ids::SIMAGIC_MODERN
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

        if self.is_modern_device() {
            debug!("Simagic modern device (0x2D5C): sending gain and rotation range");
            writer.write_feature_report(&build_device_gain(0xFF))?;
            writer.write_feature_report(&build_rotation_range(900))?;
        } else if self.model.is_evo_generation() {
            warn!(
                "Alpha EVO initialization handshake is capture-pending; skipping unverified arming sequence"
            );
        } else {
            debug!(
                "No vendor handshake applied for Simagic model {:?}; continuing in passive mode",
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
        self.model.is_evo_generation() || self.is_modern_device()
    }

    fn output_report_id(&self) -> Option<u8> {
        if self.is_modern_device() {
            Some(report_ids::CONSTANT_FORCE)
        } else {
            None
        }
    }

    fn output_report_len(&self) -> Option<usize> {
        if self.is_modern_device() {
            Some(CONSTANT_FORCE_REPORT_LEN)
        } else {
            None
        }
    }
}
