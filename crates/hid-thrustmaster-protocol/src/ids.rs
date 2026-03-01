//! Thrustmaster USB vendor and product ID constants.

#![deny(static_mut_refs)]

/// Thrustmaster USB Vendor ID.
pub const THRUSTMASTER_VENDOR_ID: u16 = 0x044F;

/// Known Thrustmaster product IDs.
///
/// # Verification sources
///
/// PIDs are cross-referenced against:
/// - Linux kernel `hid-thrustmaster.c` / scarburato/hid-tminit (upstream init driver)
/// - Kimplul/hid-tmff2 `src/hid-tmff2.h` (community FFB driver, PID defines)
/// - berarma/oversteer `oversteer/wheel_ids.py` (steering wheel manager)
/// - JacKeTUs/linux-steering-wheels (compatibility table)
/// - linux-hardware.org USB device database
/// - devicehunt.com USB ID repository
///
/// Last verified: 2025-07 against hid-tmff2 commit f004195, oversteer commit 74c7484.
///
/// # Protocol families (from hid-tmff2 probe function)
///
/// The following wheels all share the **T300RS FFB wire protocol** (Report ID
/// 0x60, 63-byte payloads, common gain/range/effect commands):
///   T300 RS (all modes), T248, TX Racing, TS-XW Racer, TS-PC Racer.
///
/// **Not** in the T300RS family:
///   - T500 RS — uses a different, older protocol (hid-tmff2 issue #18)
///   - T150 / TMX — separate protocol, not supported by hid-tmff2
///   - T818 — not in hid-tmff2; reports T248 PID per issue #58
///   - T-GT II — reuses T300 USB PIDs per hid-tmff2 README
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
    /// Verified: scarburato/hid-tminit `thrustmaster_devices[]` (044f:b65d).
    pub const FFB_WHEEL_GENERIC: u16 = 0xB65D;
    /// T150 (entry-level belt drive, post-init PID).
    /// Verified: oversteer `TM_T150 = '044f:b677'`; devicehunt.com (044f:b677 = "T150 Racing Wheel").
    pub const T150: u16 = 0xB677;
    /// T500 RS (high-end belt-drive, post-init PID).
    /// Verified: oversteer `TM_T500RS = '044f:b65e'`; hid-tminit model 0x00 switch_value 0x0002.
    /// Previously misidentified as T150 Pro; the T150 Pro shares the T150
    /// PID (0xB677) since it is the same wheelbase bundled with T3PA pedals.
    pub const T500_RS: u16 = 0xB65E;
    /// T300 RS in PlayStation 4 compatibility mode (same hardware as T300_RS,
    /// different PID reported when the PS4-mode switch is active).
    /// Verified: hid-tmff2 `TMT300RS_PS4_NORM_ID = 0xb66d`;
    /// oversteer `TM_T300RS_GT = '044f:b66d'`; linux-steering-wheels table.
    pub const T300_RS_PS4: u16 = 0xB66D;
    /// TMX (Xbox One variant of the T150/T300 family).
    /// Verified: oversteer `TM_TMX = '044f:b67f'`; linux-steering-wheels table (uses hid-tminit).
    pub const TMX: u16 = 0xB67F;
    /// T300 RS (belt-driven, PS3 normal mode).
    /// Verified: hid-tmff2 `TMT300RS_PS3_NORM_ID = 0xb66e`;
    /// oversteer `TM_T300RS = '044f:b66e'`; linux-steering-wheels table.
    pub const T300_RS: u16 = 0xB66E;
    /// T300 RS in PS3 advanced mode (activated with F1 wheel attachment).
    /// Verified: hid-tmff2 `TMT300RS_PS3_ADV_ID = 0xb66f`;
    /// oversteer `TM_T300RS_FF1 = '044f:b66f'`; linux-steering-wheels "PS3 advanced mode".
    /// Note: The GT Edition shares the normal PS3 PID (0xB66E), not this one.
    pub const T300_RS_GT: u16 = 0xB66F;
    /// TX Racing (Xbox variant, post-init active PID).
    /// Verified: hid-tmff2 `TX_ACTIVE = 0xb669`;
    /// oversteer `TM_TX458 = '044f:b669'`; linux-steering-wheels table.
    pub const TX_RACING: u16 = 0xB669;
    /// T248 (hybrid drive, PC mode).
    /// Verified: hid-tmff2 `TMT248_PC_ID = 0xb696`;
    /// oversteer `TM_T248 = '044f:b696'`; linux-steering-wheels table.
    /// Note: Per hid-tmff2 issue #58, the T818 also reports this PID.
    pub const T248: u16 = 0xB696;
    /// T248X (Xbox variant, GIP protocol).
    /// Verified: linux-hardware.org (044f:b69a = "T248X GIP Racing Wheel").
    pub const T248X: u16 = 0xB69A;
    /// TS-PC Racer (PC-only belt drive).
    /// Verified: hid-tmff2 `TMTS_PC_RACER_ID = 0xb689`;
    /// oversteer `TS_PC = '044f:b689'`.
    pub const TS_PC_RACER: u16 = 0xB689;
    /// TS-XW Racer (USB/HID mode, post-init).
    /// Verified: hid-tmff2 `TSXW_ACTIVE = 0xb692`;
    /// oversteer `TM_TSXW = '044f:b692'`; linux-hardware.org.
    pub const TS_XW: u16 = 0xB692;
    /// TS-XW Racer in GIP/Xbox protocol mode.
    /// Verified: linux-hardware.org (044f:b691 = "TS-XW Racer GIP Wheel").
    pub const TS_XW_GIP: u16 = 0xB691;
    /// T818 (direct drive).
    /// Caution: hid-tmff2 issue #58 reports the T818 enumerates with PID 0xB696
    /// (same as T248). This 0xB69B value is unverified and may be incorrect;
    /// it does not appear in any community driver source (hid-tmff2, oversteer,
    /// linux-steering-wheels). Retained for backward compatibility; callers
    /// should also check `T248` PID when detecting T818 hardware.
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

    /// Peak torque output in Newton-metres for this model.
    ///
    /// These are **approximate marketing/community-consensus values**. Thrustmaster
    /// does not publish official Nm specs for most products. Values are sourced
    /// from community dynamometer measurements and retail spec sheets.
    ///
    /// Note: The hid-tmff2 driver does not contain torque specifications — it
    /// operates in digital force units (signed i16, ≈ [-16384, 16384]). Physical
    /// torque conversion is the caller's responsibility.
    ///
    /// # Protocol note (from hid-tmff2)
    ///
    /// T300RS, T248, TX, TS-XW, and TS-PC share the same digital force range
    /// despite having different physical torque outputs (different motors/gearing).
    /// The T500RS uses a separate, older protocol (not yet in hid-tmff2).
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
