//! Simagic protocol handler.
//!
//! The Alpha EVO transport is intentionally capture-first in this codebase.
//! We identify candidate hardware and apply conservative configuration, but
//! do not send unverified arming/output sequences.

#![deny(static_mut_refs)]

use super::{DeviceWriter, FfbConfig, VendorProtocol};
use tracing::{debug, info, warn};

/// Simagic vendor IDs observed across hardware generations.
pub mod vendor_ids {
    /// Legacy Simagic VID (STMicroelectronics-based USB stack).
    pub const SIMAGIC_STM: u16 = 0x0483;
    /// Legacy Simagic alternate VID.
    pub const SIMAGIC_ALT: u16 = 0x16D0;
    /// Simagic-owned VID used by newer devices.
    pub const SIMAGIC_EVO: u16 = 0x3670;
}

/// Known and candidate Simagic product IDs.
pub mod product_ids {
    pub const ALPHA: u16 = 0x0522;
    pub const ALPHA_MINI: u16 = 0x0523;
    pub const ALPHA_ULTIMATE: u16 = 0x0524;
    pub const M10: u16 = 0x0D5A;
    pub const FX: u16 = 0x0D5B;

    // Capture-candidate IDs for Alpha EVO generation.
    pub const ALPHA_EVO_SPORT_CANDIDATE: u16 = 0x0001;
    pub const ALPHA_EVO_CANDIDATE: u16 = 0x0002;
    pub const ALPHA_EVO_PRO_CANDIDATE: u16 = 0x0003;
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
    Unknown,
}

impl SimagicModel {
    fn from_ids(vendor_id: u16, product_id: u16) -> Self {
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
            Self::Unknown => 5.0,
        }
    }

    fn encoder_cpr(self) -> u32 {
        match self {
            Self::AlphaEvoSportCandidate
            | Self::AlphaEvoCandidate
            | Self::AlphaEvoProCandidate
            | Self::AlphaEvoUnknown => 2_097_152,
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
}

impl VendorProtocol for SimagicProtocol {
    fn initialize_device(
        &self,
        _writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Initializing Simagic device VID=0x{:04X} PID=0x{:04X} model={:?}",
            self.vendor_id, self.product_id, self.model
        );

        if self.model.is_evo_generation() {
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
        self.model.is_evo_generation()
    }
}
