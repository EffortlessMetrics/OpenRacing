//! Thrustmaster USB vendor and product ID constants.

#![deny(static_mut_refs)]

/// Thrustmaster USB Vendor ID.
pub const THRUSTMASTER_VENDOR_ID: u16 = 0x044F;

/// Known Thrustmaster product IDs.
pub mod product_ids {
    /// T150 (entry-level belt drive).
    pub const T150: u16 = 0xB65D;
    /// T150 Pro (with T3PA pedals).
    pub const T150_PRO: u16 = 0xB65E;
    /// TMX (Xbox variant of T150).
    pub const TMX: u16 = 0xB66D;
    /// T300 RS (belt-driven).
    pub const T300_RS: u16 = 0xB66E;
    /// T300 RS GT (GT Edition).
    pub const T300_RS_GT: u16 = 0xB66F;
    /// TX Racing (Xbox variant).
    pub const TX_RACING: u16 = 0xB669;
    /// T500 RS (older belt drive).
    pub const T500_RS: u16 = 0xB677;
    /// T248 (hybrid drive).
    pub const T248: u16 = 0xB696;
    /// T248X (Xbox variant).
    pub const T248X: u16 = 0xB697;
    /// T-GT (Gran Turismo, 6 Nm).
    pub const T_GT: u16 = 0xB68E;
    /// T-GT II (updated T-GT).
    pub const T_GT_II: u16 = 0xB692;
    /// TS-PC Racer (PC-only belt drive).
    pub const TS_PC_RACER: u16 = 0xB689;
    /// TS-XW (Xbox variant).
    pub const TS_XW: u16 = 0xB691;
    /// T818 (direct drive).
    pub const T818: u16 = 0xB69B;

    /// T3PA pedal set.
    pub const T3PA: u16 = 0xB678;
    /// T3PA Pro (with inverted option).
    pub const T3PA_PRO: u16 = 0xB679;
    /// T-LCM (load cell brake).
    pub const T_LCM: u16 = 0xB68D;
    /// T-LCM Pro (updated load cell).
    pub const T_LCM_PRO: u16 = 0xB69A;
}

/// Model identification shorthand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    T150,
    T150Pro,
    TMX,
    T300RS,
    T300RSGT,
    TXRacing,
    T500RS,
    T248,
    T248X,
    TGT,
    TGTII,
    TSPCRacer,
    TSXW,
    T818,
    T3PA,
    T3PAPro,
    TLCM,
    TLCMPro,
    Unknown,
}

impl Model {
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            product_ids::T150 => Self::T150,
            product_ids::T150_PRO => Self::T150Pro,
            product_ids::TMX => Self::TMX,
            product_ids::T300_RS => Self::T300RS,
            product_ids::T300_RS_GT => Self::T300RSGT,
            product_ids::TX_RACING => Self::TXRacing,
            product_ids::T500_RS => Self::T500RS,
            product_ids::T248 => Self::T248,
            product_ids::T248X => Self::T248X,
            product_ids::T_GT => Self::TGT,
            product_ids::T_GT_II => Self::TGTII,
            product_ids::TS_PC_RACER => Self::TSPCRacer,
            product_ids::TS_XW => Self::TSXW,
            product_ids::T818 => Self::T818,
            product_ids::T3PA => Self::T3PA,
            product_ids::T3PA_PRO => Self::T3PAPro,
            product_ids::T_LCM => Self::TLCM,
            product_ids::T_LCM_PRO => Self::TLCMPro,
            _ => Self::Unknown,
        }
    }

    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::T150 | Self::T150Pro | Self::TMX | Self::T500RS => 2.5,
            Self::T300RS | Self::T300RSGT | Self::TXRacing | Self::T248 | Self::T248X => 4.0,
            Self::TGT | Self::TGTII | Self::TSPCRacer | Self::TSXW => 6.0,
            Self::T818 => 10.0,
            Self::T3PA | Self::T3PAPro | Self::TLCM | Self::TLCMPro => 0.0,
            Self::Unknown => 4.0,
        }
    }

    pub fn max_rotation_deg(self) -> u16 {
        match self {
            Self::T500RS => 1080,
            Self::TGT | Self::TGTII | Self::T818 => 1080,
            Self::TSPCRacer | Self::TSXW => 1070,
            _ => 900,
        }
    }

    pub fn supports_ffb(self) -> bool {
        !matches!(
            self,
            Self::T3PA | Self::T3PAPro | Self::TLCM | Self::TLCMPro | Self::Unknown
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::T150 => "Thrustmaster T150",
            Self::T150Pro => "Thrustmaster T150 Pro",
            Self::TMX => "Thrustmaster TMX",
            Self::T300RS => "Thrustmaster T300 RS",
            Self::T300RSGT => "Thrustmaster T300 RS GT",
            Self::TXRacing => "Thrustmaster TX Racing",
            Self::T500RS => "Thrustmaster T500 RS",
            Self::T248 => "Thrustmaster T248",
            Self::T248X => "Thrustmaster T248X",
            Self::TGT => "Thrustmaster T-GT",
            Self::TGTII => "Thrustmaster T-GT II",
            Self::TSPCRacer => "Thrustmaster TS-PC Racer",
            Self::TSXW => "Thrustmaster TS-XW",
            Self::T818 => "Thrustmaster T818",
            Self::T3PA => "Thrustmaster T3PA",
            Self::T3PAPro => "Thrustmaster T3PA Pro",
            Self::TLCM => "Thrustmaster T-LCM",
            Self::TLCMPro => "Thrustmaster T-LCM Pro",
            Self::Unknown => "Thrustmaster Unknown",
        }
    }
}
