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
/// Web-verified 2025-07 against:
/// - the-sz.com USB ID database (VID 0x044F product listing)
/// - usb-ids.gowdy.us/read/UD/044F
/// - linux-hardware.org device database (per-PID lookup)
/// - Linux kernel `hid-thrustmaster.c` (torvalds/linux master)
///
/// Result: **No discrepancies found.** All PIDs with external web sources
/// match our constants. T818 (0xB69B) and F430 FF (0xB65A) remain without
/// web database confirmation (see per-PID notes).
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
///   - T-GT II — uses T300RS protocol in PC/"other" mode (reuses T300 PIDs);
///     has its own PID 0xB681 in GT mode where FFB is unverified
///     (hid-tmff2 README + issue #184 lsusb evidence)
///
/// # Removed PIDs (previously incorrect)
///
/// - **T-GT** — was 0xB68E, but linux-hardware.org identifies that as "TPR Rudder
///   Bulk" (flight sim rudder pedals). The T-GT may share PID 0xB681 with the
///   T-GT II in GT mode (the USB product string reports "T-GT" not "T-GT II"),
///   but this is unconfirmed on original T-GT hardware.
/// - **T-GT II** — was 0xB692, but hid-tmff2 confirms that as `TSXW_ACTIVE`
///   (TS-XW Racer). The T-GT II has PID 0xB681 in GT mode (verified via lsusb
///   in hid-tmff2 issue #184). In PC/"other" mode it reuses T300RS PIDs.
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
    /// Web-verified: kernel `hid-thrustmaster.c` `thrustmaster_devices[] = {0x044f, 0xb65d}`.
    pub const FFB_WHEEL_GENERIC: u16 = 0xB65D;
    /// T150 (entry-level belt drive, post-init PID).
    /// Verified: oversteer `TM_T150 = '044f:b677'`; devicehunt.com (044f:b677 = "T150 Racing Wheel").
    /// Web-verified: the-sz.com (044f:b677 = "T150 Racing Wheel").
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
    /// TX Racing Wheel (original PID, possibly pre-mode-switch).
    /// Source: oversteer `TM_TX = '044f:b664'`.
    pub const TX_RACING_ORIG: u16 = 0xB664;
    /// T248 (hybrid drive, PC mode).
    /// Verified: hid-tmff2 `TMT248_PC_ID = 0xb696`;
    /// oversteer `TM_T248 = '044f:b696'`; linux-steering-wheels table.
    /// Note: Per hid-tmff2 issue #58, the T818 also reports this PID.
    pub const T248: u16 = 0xB696;
    /// T248X (Xbox variant, GIP protocol).
    /// Verified: linux-hardware.org (044f:b69a = "T248X GIP Racing Wheel").
    /// Web-verified: linux-hardware.org confirms 044f:b69a = "T248X GIP Racing Wheel".
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
    /// Web-verified: linux-hardware.org confirms 044f:b691 = "TS-XW Racer GIP Wheel".
    pub const TS_XW_GIP: u16 = 0xB691;
    /// T-GT II in GT mode (PS4/PS5 "GT" switch position).
    ///
    /// Verified: hid-tmff2 issue #184 — user `lsusb` output:
    ///   `Bus 001 Device 024: ID 044f:b681 ThrustMaster, Inc. Thrustmaster Racing Wheel FFB T-GT`
    ///
    /// The USB product string reads "T-GT" (not "T-GT II"), so this PID may also
    /// apply to the original T-GT, but that is unconfirmed on original hardware.
    ///
    /// In PC/"other" mode, the T-GT II reuses T300RS PIDs (0xB66E / 0xB66D /
    /// 0xB66F depending on the mode switch and attachment). FFB is only verified
    /// via hid-tmff2 in that configuration (T300RS protocol family).
    ///
    /// Web-unverified: not found in the-sz.com, linux-hardware.org, or
    /// devicehunt.com databases. Single primary-source lsusb confirmation.
    pub const T_GT_II_GT: u16 = 0xB681;
    /// T818 (direct drive).
    /// Caution: hid-tmff2 issue #58 reports the T818 enumerates with PID 0xB696
    /// (same as T248). This 0xB69B value is unverified and may be incorrect;
    /// it does not appear in any community driver source (hid-tmff2, oversteer,
    /// linux-steering-wheels). Retained for backward compatibility; callers
    /// should also check `T248` PID when detecting T818 hardware.
    pub const T818: u16 = 0xB69B;

    // ── Legacy hid-tmff wheels ───────────────────────────────────────────

    /// T80 Racing Wheel (entry-level, no FFB — only rumble).
    /// Verified: oversteer `TM_T80 = '044f:b668'`.
    /// Web-verified: the-sz.com (044f:b668 = "Thrustmaster T80").
    pub const T80: u16 = 0xB668;
    /// T80 Ferrari 488 GTB Edition (entry-level, no FFB — only rumble).
    /// Source: oversteer `TM_T80H = '044f:b66a'`.
    pub const T80_FERRARI_488: u16 = 0xB66A;
    /// NASCAR Pro Force Feedback 2 (legacy gear-driven wheel).
    /// Verified: linux-steering-wheels (hid-tmff), PID 0xb605.
    /// Web-verified: the-sz.com (044f:b605 = "force feedback Racing Wheel").
    pub const NASCAR_PRO_FF2: u16 = 0xB605;
    /// Ferrari GT Rumble Force (legacy entry-level wheel).
    /// Verified: linux-steering-wheels (hid-tmff), PID 0xb651.
    /// Web-verified: the-sz.com (044f:b651 = "Ferrari GT Rumble Force Wheel").
    pub const FGT_RUMBLE_FORCE: u16 = 0xB651;
    /// Rally GT Force Feedback Clutch Edition (legacy wheel).
    /// Verified: linux-steering-wheels (hid-tmff), PID 0xb653.
    /// Web-verified: the-sz.com (044f:b653 = "RGT Force Feedback Clutch Racing Wheel").
    pub const RGT_FF_CLUTCH: u16 = 0xB653;
    /// Ferrari GT Force Feedback (legacy wheel).
    /// Verified: linux-steering-wheels (hid-tmff), PID 0xb654.
    /// Web-verified: the-sz.com (044f:b654 = "Ferrari GT Force Feedback Wheel").
    pub const FGT_FORCE_FEEDBACK: u16 = 0xB654;
    /// Ferrari 430 Force Feedback (legacy belt-driven wheel).
    /// Verified: linux-steering-wheels (hid-tmff), PID 0xb65a.
    /// Web-unverified: not found in the-sz.com, usb-ids.gowdy.us, or linux-hardware.org.
    pub const F430_FORCE_FEEDBACK: u16 = 0xB65A;
}

/// Model identification shorthand.
///
/// Note: `TGT`, `T3PA`, `T3PAPro`, `TLCM`, and `TLCMPro` are real
/// products but their USB PIDs could not be verified against community driver
/// sources (the previously-assigned PIDs belonged to other devices). They are
/// retained in the enum for metadata (torque, rotation) but cannot be returned
/// by [`Model::from_product_id`]. See `product_ids` docs for details.
///
/// `TGTII` is now matchable via PID 0xB681 (GT mode). In PC/"other" mode the
/// T-GT II reuses T300RS PIDs and will be identified as `T300RS` / `T300RSPS4`
/// / `T300RSGT` depending on the mode switch position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// T150: entry-level belt-drive wheel (PS3/PS4/PC).
    ///
    /// **Protocol family: T150** — uses a proprietary FFB protocol that is
    /// **different from the T300RS family**. Documented by the community
    /// `scarburato/t150_driver` Linux kernel module.
    ///
    /// Key protocol differences from T300RS:
    /// - Output via USB interrupt OUT endpoint (not HID Report ID 0x60).
    /// - Range: `[0x40, 0x11, <u16_le>]` where 0xFFFF = max (1080° on T150).
    /// - Gain: `[0x43, <u8>]` (single byte, not the T300RS gain command).
    /// - FFB effects use a 3-packet upload pattern (`ff_first` → `ff_update` → `ff_commit`).
    /// - Effect play/stop: `[0x41, <id>, <mode>, <times>]`.
    /// - Effect type codes: 0x4000 (constant), 0x4022 (sine), 0x4023 (saw up),
    ///   0x4024 (saw down), 0x4040 (spring), 0x4041 (damper).
    ///
    /// **Not supported by hid-tmff2.** Uses `scarburato/t150_driver` on Linux.
    /// Init switch value: 0x0006.
    T150,
    /// TMX: Xbox One variant of the T150 family.
    ///
    /// **Protocol family: T150** — shares the same FFB protocol as the T150
    /// (same command bytes 0x40/0x41/0x43, same 3-packet effect upload pattern).
    /// The only known difference is max range: 900° on TMX vs 1080° on T150.
    ///
    /// Source: `scarburato/t150_driver` supports both T150 (0xB677) and TMX (0xB67F).
    /// **Not supported by hid-tmff2.** Init switch value: 0x0006 (same as T150).
    TMX,
    /// T300RS: mid-range belt-drive wheel (PS3 normal mode, PID 0xB66E).
    ///
    /// **Protocol family: T300** — uses Report ID 0x60, 63-byte output payloads.
    /// Kernel-verified via `Kimplul/hid-tmff2`. Range command: `degrees * 0x3C`.
    T300RS,
    /// T300RS in PS4 compatibility mode (PID 0xB66D).
    ///
    /// **Protocol family: T300** — same as T300RS but with 31-byte payloads in PS4 mode.
    T300RSPS4,
    /// T300RS GT / PS3 advanced mode (activated with F1 attachment, PID 0xB66F).
    ///
    /// **Protocol family: T300** — same protocol as T300RS.
    T300RSGT,
    /// TX Racing: Xbox variant sharing the T300RS protocol.
    ///
    /// **Protocol family: T300** — uses `tx_populate_api()` in hid-tmff2,
    /// which delegates to T300RS wire format. Max range clamped to 900°.
    TXRacing,
    /// T500RS: older high-end belt-drive wheel (PID 0xB65E).
    ///
    /// **Protocol family: T500** — uses an older, **undocumented** FFB protocol
    /// that is different from both T300RS and T150 families.
    ///
    /// What is known:
    /// - Init switch value: 0x0002 (unique among TM wheels).
    /// - hid-tminit model response bytes: 0x0200 at offset +6,+7.
    /// - **No community FFB driver exists.** hid-tmff2 issue #18 is an open
    ///   request. `her001/tmdrv` handles init only (no FFB).
    /// - Not listed in linux-steering-wheels compatibility table.
    /// - Max rotation: 1080° (official Thrustmaster spec).
    ///
    /// The T500RS FFB wire format is unknown. Do not assume T300RS or T150
    /// commands will work.
    T500RS,
    T248,
    T248X,
    TGT,
    /// T-GT II: belt-drive wheelbase with T300RS-family internals.
    ///
    /// **Dual-PID behaviour:** The T-GT II has a mode switch:
    /// - **GT mode** (PS4/PS5): PID 0xB681 (`product_ids::T_GT_II_GT`).
    ///   USB product string: "Thrustmaster Racing Wheel FFB T-GT".
    ///   Verified: hid-tmff2 issue #184 (lsusb evidence).
    /// - **PC / "other" mode**: Reuses T300RS PIDs (0xB66E / 0xB66D / 0xB66F
    ///   depending on sub-mode). Identified as T300RS by `from_product_id`.
    ///
    /// **Protocol family: T300** — in PC mode, the T-GT II uses the T300RS FFB
    /// wire protocol and is supported by hid-tmff2 (README confirms).
    /// FFB in GT mode (0xB681) is unverified with T300RS commands.
    TGTII,
    TSPCRacer,
    TSXW,
    T818,
    T80,
    NascarProFF2,
    FGTRumbleForce,
    RGTFF,
    FGTForceFeedback,
    F430ForceFeedback,
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
            product_ids::TX_RACING | product_ids::TX_RACING_ORIG => Self::TXRacing,
            product_ids::T500_RS => Self::T500RS,
            product_ids::T248 => Self::T248,
            product_ids::T248X => Self::T248X,
            product_ids::TS_PC_RACER => Self::TSPCRacer,
            product_ids::TS_XW | product_ids::TS_XW_GIP => Self::TSXW,
            product_ids::T_GT_II_GT => Self::TGTII,
            product_ids::T818 => Self::T818,
            product_ids::T80 | product_ids::T80_FERRARI_488 => Self::T80,
            product_ids::NASCAR_PRO_FF2 => Self::NascarProFF2,
            product_ids::FGT_RUMBLE_FORCE => Self::FGTRumbleForce,
            product_ids::RGT_FF_CLUTCH => Self::RGTFF,
            product_ids::FGT_FORCE_FEEDBACK => Self::FGTForceFeedback,
            product_ids::F430_FORCE_FEEDBACK => Self::F430ForceFeedback,
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
            | Self::T248X => 4.0,
            // T500RS uses a larger brushless motor than the T300RS belt-drive.
            // Community dynamometer measurements consistently place it above
            // the T300RS (~4.0 Nm) and below the TS-PC Racer (~6.0 Nm).
            // No official Thrustmaster Nm spec exists; 5.0 is a conservative
            // community-consensus estimate.
            Self::T500RS => 5.0,
            Self::TGT | Self::TGTII | Self::TSPCRacer | Self::TSXW => 6.0,
            Self::T818 => 10.0,
            Self::T80 => 0.0,
            Self::NascarProFF2
            | Self::FGTRumbleForce
            | Self::RGTFF
            | Self::FGTForceFeedback
            | Self::F430ForceFeedback => 1.5,
            Self::T3PA | Self::T3PAPro | Self::TLCM | Self::TLCMPro => 0.0,
            Self::Unknown => 4.0,
        }
    }

    /// Maximum wheel rotation in degrees.
    ///
    /// Sources:
    /// - `Kimplul/hid-tmff2` per-model `*_set_range()`: T300RS clamps to 1080°,
    ///   T248 clamps to 900°, TX clamps to 900° (same as T300RS API, different limit).
    /// - T500RS: 1080° (official Thrustmaster spec).
    /// - TS-PC, TS-XW: 1080° (official Thrustmaster spec, T300RS FFB API family).
    /// - T-GT, T-GT II, T818: 1080° (official Thrustmaster spec).
    /// - T80, NASCAR Pro FF2, older FFB wheels: 270° (physical lock).
    /// - Default 900°: T248, TX Racing, TMX, T150 (via protocol clamp or official spec).
    ///
    /// Note: T150 uses a separate protocol (not T300RS family in hid-tmff2) and
    /// official spec lists 1080°, so we keep it at 1080° per manufacturer data.
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
            Self::T80
            | Self::NascarProFF2
            | Self::FGTRumbleForce
            | Self::RGTFF
            | Self::FGTForceFeedback
            | Self::F430ForceFeedback => 270,
            // T248, TX, TMX, T248X, T80Ferrari488, TXRacingOrig: 900° per hid-tmff2/spec
            _ => 900,
        }
    }

    pub fn supports_ffb(self) -> bool {
        !matches!(
            self,
            Self::T80 | Self::T3PA | Self::T3PAPro | Self::TLCM | Self::TLCMPro | Self::Unknown
        )
    }

    /// FFB protocol family for this wheel model.
    ///
    /// Source: Kimplul/hid-tmff2 probe function and per-wheel `*_populate_api()`
    /// functions; Linux kernel `hid-thrustmaster.c` for init switch values.
    ///
    /// - `T300Family`: T300RS (all modes), T248, TX Racing, TS-XW, TS-PC, T-GT II.
    ///   Uses Report ID 0x60, 63-byte payloads (31 in PS4 mode).
    /// - `T150Family`: T150 and TMX. Separate protocol, not in hid-tmff2.
    /// - `T500Family`: T500RS. Older protocol, not supported by hid-tmff2 (issue #18).
    /// - `Unknown`: T818, T-GT, pedals, or unrecognized.
    ///
    /// **T-GT II note:** Classified as T300 family because in PC/"other" mode it
    /// uses T300RS PIDs and the T300RS FFB protocol (confirmed by hid-tmff2).
    /// When detected via PID 0xB681 (GT mode), T300RS commands are unverified;
    /// callers should be aware that FFB may require switching the wheel to PC mode.
    pub fn protocol_family(self) -> ProtocolFamily {
        match self {
            Self::T300RS
            | Self::T300RSPS4
            | Self::T300RSGT
            | Self::TXRacing
            | Self::T248
            | Self::T248X
            | Self::TSPCRacer
            | Self::TSXW
            | Self::TGTII => ProtocolFamily::T300,
            Self::T150 | Self::TMX => ProtocolFamily::T150,
            Self::T500RS => ProtocolFamily::T500,
            _ => ProtocolFamily::Unknown,
        }
    }

    /// USB mode-switch value sent via USB control request (bRequest 83) during
    /// initialization on Linux. The wheel starts in generic mode (PID 0xB65D)
    /// and must be switched to its full-capability mode.
    ///
    /// Source: Linux kernel `hid-thrustmaster.c` `tm_wheels_infos[]`.
    /// Returns `None` for models without known init switch data.
    pub fn init_switch_value(self) -> Option<u16> {
        match self {
            Self::T150 | Self::TMX => Some(0x0006),
            Self::T300RS
            | Self::T300RSPS4
            | Self::T300RSGT
            | Self::TXRacing
            | Self::T248
            | Self::T248X
            | Self::TSPCRacer
            | Self::TSXW
            | Self::TGTII => Some(0x0005),
            Self::T500RS => Some(0x0002),
            _ => None,
        }
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
            Self::T80 => "Thrustmaster T80 Racing Wheel",
            Self::NascarProFF2 => "Thrustmaster NASCAR Pro FF2",
            Self::FGTRumbleForce => "Thrustmaster FGT Rumble Force",
            Self::RGTFF => "Thrustmaster Rally GT FF Clutch",
            Self::FGTForceFeedback => "Thrustmaster FGT Force Feedback",
            Self::F430ForceFeedback => "Thrustmaster Ferrari 430 FF",
            Self::T3PA => "Thrustmaster T3PA",
            Self::T3PAPro => "Thrustmaster T3PA Pro",
            Self::TLCM => "Thrustmaster T-LCM",
            Self::TLCMPro => "Thrustmaster T-LCM Pro",
            Self::Unknown => "Thrustmaster Unknown",
        }
    }
}

/// FFB wire protocol family classification for Thrustmaster wheels.
///
/// Different Thrustmaster wheels use different FFB protocols. Wheels within the
/// same family share identical output report formats, effect encoding, and
/// gain/range commands. The engine must select the correct FFB codec based on
/// the protocol family.
///
/// Source: Kimplul/hid-tmff2 probe function (`tmff2_probe()`) and per-wheel
/// `*_populate_api()` functions, plus Linux kernel `hid-thrustmaster.c`
/// for model identification during initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolFamily {
    /// T300RS family: Report ID 0x60, 63-byte payloads (31 in PS4 mode).
    /// Shared by T300RS, T248, TX, TS-XW, TS-PC, T-GT II.
    /// Full FFB support via hid-tmff2.
    T300,
    /// T150/TMX family: Proprietary FFB protocol using USB interrupt OUT.
    ///
    /// **Not the same as T300RS.** Uses different command bytes and packet structure:
    /// - Range: `[0x40, 0x11, <u16_le>]` (0xFFFF = max range)
    /// - Gain: `[0x43, <u8>]`
    /// - Effects: 3-packet upload (ff_first → ff_update → ff_commit)
    /// - Play/stop: `[0x41, <id>, <mode>, <times>]`
    ///
    /// Source: `scarburato/t150_driver` kernel module (supports T150 + TMX).
    /// **Not supported by hid-tmff2.**
    T150,
    /// T500RS family: Older protocol, **not supported by any community FFB driver**.
    ///
    /// The T500RS uses init switch value 0x0002 and hid-tminit model bytes 0x0200.
    /// No FFB wire format documentation exists. hid-tmff2 issue #18 is an open
    /// request for support. `her001/tmdrv` handles init/mode-switch only.
    ///
    /// Do not assume T300RS or T150 commands will work on T500RS hardware.
    T500,
    /// Unknown or no FFB protocol (pedals, unrecognized models, T818, T-GT).
    Unknown,
}

/// USB mode-switch initialization constants.
///
/// Thrustmaster wheels present as a generic "FFB Wheel" (VID 0x044F, PID 0xB65D)
/// when first connected. The host must send a USB vendor control request
/// (bRequest 83, `change_request`) with a model-specific `wValue` to switch
/// the wheel to its full-capability mode. After switching, the wheel
/// re-enumerates with its model-specific PID.
///
/// Source: Linux kernel `drivers/hid/hid-thrustmaster.c`, `tm_wheels_infos[]`.
pub mod init_protocol {
    /// USB bRequest code to query wheel model type.
    pub const MODEL_QUERY_REQUEST: u8 = 73;
    /// USB bRequest code to switch wheel mode.
    pub const MODE_SWITCH_REQUEST: u8 = 83;
    /// USB bRequestType for model query (vendor, device-to-host).
    pub const MODEL_QUERY_REQUEST_TYPE: u8 = 0xC1;
    /// USB bRequestType for mode switch (vendor, host-to-device).
    pub const MODE_SWITCH_REQUEST_TYPE: u8 = 0x41;
    /// Expected wLength for model query response.
    pub const MODEL_RESPONSE_LEN: u16 = 0x0010;

    /// Interrupt setup packets sent before model query to prevent T300RS crash.
    /// Source: `thrustmaster_interrupts()` in hid-thrustmaster.c.
    pub const SETUP_INTERRUPTS: &[&[u8]] = &[
        &[0x42, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        &[0x0a, 0x04, 0x90, 0x03, 0x00, 0x00, 0x00, 0x00],
        &[0x0a, 0x04, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00],
        &[0x0a, 0x04, 0x12, 0x10, 0x00, 0x00, 0x00, 0x00],
        &[0x0a, 0x04, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00],
    ];

    /// Wheel model code to switch value mapping.
    /// Model code is extracted from the response to `MODEL_QUERY_REQUEST`.
    /// Switch value is sent as `wValue` in `MODE_SWITCH_REQUEST`.
    ///
    /// Format: (model_code, switch_value, name)
    pub const KNOWN_MODELS: &[(u16, u16, &str)] = &[
        (0x0306, 0x0006, "T150 RS"),
        (0x0200, 0x0005, "T300 RS (No Attachment)"),
        (0x0206, 0x0005, "T300 RS"),
        (0x0209, 0x0005, "T300 RS (Open Wheel)"),
        (0x020A, 0x0005, "T300 RS (Sparco R383)"),
        (0x0204, 0x0005, "T300 Ferrari Alcantara"),
        (0x0002, 0x0002, "T500 RS"),
    ];
}
