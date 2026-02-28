//! Thrustmaster USB vendor and product ID constants.

#![deny(static_mut_refs)]

/// Thrustmaster USB Vendor ID.
pub const THRUSTMASTER_VENDOR_ID: u16 = 0x044F;

/// Known Thrustmaster product IDs.
///
/// # Verification sources
///
/// PIDs are cross-referenced against:
/// - Linux kernel `hid-thrustmaster.c` (upstream init driver)
/// - Kimplul/hid-tmff2 (community FFB driver)
/// - berarma/oversteer (steering wheel manager)
/// - JacKeTUs/linux-steering-wheels (compatibility table)
/// - linux-hardware.org USB device database
/// - devicehunt.com USB ID repository
///
/// # Removed PIDs (previously incorrect)
///
/// - **T-GT** — was 0xB68E, but linux-hardware.org identifies that as "TPR Rudder
///   Bulk" (flight sim rudder pedals). Real T-GT PID is unknown.
/// - **T-GT II** — was 0xB692, but hid-tmff2 confirms that as `TSXW_ACTIVE`
///   (TS-XW Racer). Per hid-tmff2 README, the T-GT II reuses T300 USB PIDs.
/// - **T-LCM** — was 0xB68D, but linux-hardware.org identifies that as
///   "T.Flight Hotas One" (flight controller). Real T-LCM PID is unverified.
/// - **T-LCM Pro** — was 0xB69A, but linux-hardware.org identifies that as
///   "T248X GIP Racing Wheel". Real T-LCM Pro PID is unverified.
/// - **T3PA** — was 0xB678, but devicehunt.com identifies that as "T.Flight
///   Rudder Pedals" (flight sim). T3PA typically connects via RJ12 to the
///   wheelbase; its standalone USB PID (if any) is unverified.
/// - **T3PA Pro** — was 0xB679, but devicehunt.com identifies that as
///   "T-Rudder" (flight sim). Same RJ12 caveat as T3PA.
pub mod product_ids {
    /// Generic pre-init "FFB Wheel" PID used by all Thrustmaster wheels before
    /// mode switching. After init, the wheel re-enumerates with a model-specific PID.
    /// Verified: Linux kernel hid-thrustmaster.c device table (044f:b65d).
    pub const FFB_WHEEL_GENERIC: u16 = 0xB65D;
    /// T150 (entry-level belt drive, post-init PID).
    /// Verified: devicehunt.com + linux-hardware.org (044f:b677 = "T150 Racing Wheel").
    pub const T150: u16 = 0xB677;
    /// T500 RS (high-end belt-drive, post-init PID).
    /// Verified: oversteer `TM_T500RS = '044f:b65e'`.
    /// Previously misidentified as T150 Pro; the T150 Pro shares the T150
    /// PID (0xB677) since it is the same wheelbase bundled with T3PA pedals.
    pub const T500_RS: u16 = 0xB65E;
    /// T300 RS in PlayStation 4 compatibility mode (same hardware as T300_RS,
    /// different PID reported when the PS4-mode switch is active).
    /// Verified: hid-tmff2 TMT300RS_PS4_NORM_ID; linux-steering-wheels table.
    pub const T300_RS_PS4: u16 = 0xB66D;
    /// TMX (Xbox One variant of the T150/T300 family).
    /// Verified: linux-steering-wheels table (044f:b67f = "TMX", uses hid-tminit).
    pub const TMX: u16 = 0xB67F;
    /// T300 RS (belt-driven, PS3 normal mode).
    /// Verified: hid-tmff2 TMT300RS_PS3_NORM_ID.
    pub const T300_RS: u16 = 0xB66E;
    /// T300 RS GT (GT Edition / PS3 advanced mode).
    /// Verified: hid-tmff2 TMT300RS_PS3_ADV_ID.
    pub const T300_RS_GT: u16 = 0xB66F;
    /// TX Racing (Xbox variant).
    /// Verified: hid-tmff2 TX_ACTIVE.
    pub const TX_RACING: u16 = 0xB669;
    /// T248 (hybrid drive).
    /// Verified: hid-tmff2 TMT248_PC_ID.
    pub const T248: u16 = 0xB696;
    /// T248X (Xbox variant, GIP protocol).
    /// Verified: linux-hardware.org (044f:b69a = "T248X GIP Racing Wheel").
    pub const T248X: u16 = 0xB69A;
    /// TS-PC Racer (PC-only belt drive).
    /// Verified: hid-tmff2 TMTS_PC_RACER_ID.
    pub const TS_PC_RACER: u16 = 0xB689;
    /// TS-XW Racer (USB/HID mode, post-init).
    /// Verified: hid-tmff2 TSXW_ACTIVE (0xb692); linux-hardware.org.
    pub const TS_XW: u16 = 0xB692;
    /// TS-XW Racer in GIP/Xbox protocol mode.
    /// Verified: linux-hardware.org (044f:b691 = "TS-XW Racer GIP Wheel").
    pub const TS_XW_GIP: u16 = 0xB691;
    /// T818 (direct drive).
    /// Unverified: listed as open request in hid-tmff2 issue #58.
    pub const T818: u16 = 0xB69B;
}

/// Model identification shorthand.
///
/// Note: `TGT`, `TGTII`, `T3PA`, `T3PAPro`, `TLCM`, and `TLCMPro` are real
/// products but their USB PIDs could not be verified against community driver
/// sources (the previously-assigned PIDs belonged to other devices). They are
/// retained in the enum for metadata (torque, rotation) but cannot be returned
/// by [`Model::from_product_id`]. See `product_ids` docs for details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    T150,
    TMX,
    T300RS,
    T300RSPS4,
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
            product_ids::TMX => Self::TMX,
            product_ids::T300_RS => Self::T300RS,
            product_ids::T300_RS_PS4 => Self::T300RSPS4,
            product_ids::T300_RS_GT => Self::T300RSGT,
            product_ids::TX_RACING => Self::TXRacing,
            product_ids::T500_RS => Self::T500RS,
            product_ids::T248 => Self::T248,
            product_ids::T248X => Self::T248X,
            product_ids::TS_PC_RACER => Self::TSPCRacer,
            product_ids::TS_XW | product_ids::TS_XW_GIP => Self::TSXW,
            product_ids::T818 => Self::T818,
            _ => Self::Unknown,
        }
    }

    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::T150 | Self::TMX => 2.5,
            Self::T300RS
            | Self::T300RSPS4
            | Self::T300RSGT
            | Self::TXRacing
            | Self::T248
            | Self::T248X
            | Self::T500RS => 4.0,
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
            Self::T150
            | Self::T300RS
            | Self::T300RSPS4
            | Self::T300RSGT
            | Self::TSPCRacer
            | Self::TSXW => 1080,
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
            Self::TMX => "Thrustmaster TMX",
            Self::T300RS => "Thrustmaster T300 RS",
            Self::T300RSPS4 => "Thrustmaster T300 RS (PS4 mode)",
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
