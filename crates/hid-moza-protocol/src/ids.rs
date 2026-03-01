//! Moza vendor ID and product ID constants.
//!
//! # Verified sources
//!
//! V1 wheelbase PIDs are confirmed by multiple independent open-source projects:
//!
//! - **Linux kernel `hid-ids.h`** (mainline):
//!   `USB_VENDOR_ID_MOZA = 0x346e` and all wheelbase device IDs.
//!   <https://github.com/torvalds/linux/blob/master/drivers/hid/hid-ids.h>
//!
//! - **Linux kernel `hid-universal-pidff` driver** (merged Linux 6.15):
//!   `drivers/hid/hid-universal-pidff.c` — lists both V1 and V2 (`_2` suffix)
//!   device IDs with `USB_VENDOR_ID_MOZA` (0x346E). V2 PIDs follow the pattern
//!   V1 | 0x0010 and are registered as separate USB product IDs.
//!   All Moza entries carry `HID_PIDFF_QUIRK_FIX_CONDITIONAL_DIRECTION`.
//!   <https://github.com/torvalds/linux/blob/master/drivers/hid/hid-universal-pidff.c>
//!
//! - **JacKeTUs/linux-steering-wheels** compatibility table (V1 PIDs):
//!   R3=0x0005, R5=0x0004, R9=0x0002, R12=0x0006, R16/R21=0x0000.
//!   <https://github.com/JacKeTUs/linux-steering-wheels>
//!
//! - **USB ID database** (usb-ids.gowdy.us):
//!   VID 0x346E registered as "Gudsen Technology (HK) Co., Ltd (MOZA)".
//!   <https://usb-ids.gowdy.us/read/UD/346E>
//!
//! - **Lawstorant/boxflat** (community Moza configuration tool):
//!   Uses serial-over-USB (CDC ACM) at 115200 baud for device configuration.
//!   Peripherals identified by HID device name patterns, not PIDs.
//!   <https://github.com/Lawstorant/boxflat>
//!
//! Peripheral PIDs (SR-P, HGP, SGP, HBP) are from USB descriptor captures
//! and are not covered by the FFB-focused community sources above.
//!
//! # Known gaps
//!
//! - **CRP pedals**: boxflat matches `(srp|sr-p|crp)` in HID names but we only
//!   have `SR_P_PEDALS`. CRP may share the same PID or use a distinct one.
//! - **E-Stop, Dashboard, Hub, Stalks**: boxflat references these as serial
//!   devices (IDs 28, 20, 18, "stalks") but their standalone USB PIDs are
//!   not yet captured or verified.
//! - **Universal Hub**: appears as a separate HID device ("gudsen universal hub")
//!   in boxflat; USB PID not yet catalogued here.

#![deny(static_mut_refs)]

/// Moza Racing USB Vendor ID (Gudsen Technology Co., Ltd).
///
/// Confirmed by three independent sources:
/// - Linux kernel `hid-ids.h`: `USB_VENDOR_ID_MOZA = 0x346e`
/// - USB ID database: "Gudsen Technology (HK) Co., Ltd (MOZA)"
/// - boxflat udev rule: `ATTRS{idVendor}=="346e"`
pub const MOZA_VENDOR_ID: u16 = 0x346E;

/// Known Moza product IDs.
///
/// V1 PIDs are the primary USB identifiers. V2 PIDs (V1 | 0x0010) are
/// used by newer hardware/firmware revisions and are recognized by the
/// Linux kernel `hid-universal-pidff` driver as separate device entries.
pub mod product_ids {
    // ── Wheelbases (V1) ─────────────────────────────────────────────
    // Verified against Linux kernel hid-ids.h (mainline) and hid-universal-pidff.c:
    //   USB_DEVICE_ID_MOZA_R16_R21 = 0x0000
    //   USB_DEVICE_ID_MOZA_R9      = 0x0002
    //   USB_DEVICE_ID_MOZA_R5      = 0x0004
    //   USB_DEVICE_ID_MOZA_R3      = 0x0005
    //   USB_DEVICE_ID_MOZA_R12     = 0x0006

    /// R16 and R21 share the same USB PID; differentiate by torque query.
    pub const R16_R21_V1: u16 = 0x0000;
    pub const R9_V1: u16 = 0x0002;
    pub const R5_V1: u16 = 0x0004;
    pub const R3_V1: u16 = 0x0005;
    pub const R12_V1: u16 = 0x0006;

    // ── Wheelbases (V2) ─────────────────────────────────────────────
    // Verified against Linux kernel hid-ids.h (mainline):
    //   USB_DEVICE_ID_MOZA_R16_R21_2 = 0x0010
    //   USB_DEVICE_ID_MOZA_R9_2      = 0x0012
    //   USB_DEVICE_ID_MOZA_R5_2      = 0x0014
    //   USB_DEVICE_ID_MOZA_R3_2      = 0x0015
    //   USB_DEVICE_ID_MOZA_R12_2     = 0x0016
    // Pattern: V1 PID | 0x0010. Confirmed by kernel `_2` device entries.
    pub const R16_R21_V2: u16 = 0x0010;
    pub const R9_V2: u16 = 0x0012;
    pub const R5_V2: u16 = 0x0014;
    pub const R3_V2: u16 = 0x0015;
    pub const R12_V2: u16 = 0x0016;

    // ── Peripherals ─────────────────────────────────────────────────
    // From USB descriptor captures; not in FFB-focused community sources.
    // boxflat identifies peripherals by HID name pattern, not PID:
    //   pedals    = "gudsen moza (srp|sr-p|crp)[0-9]? pedals"
    //   hpattern  = "hgp shifter"
    //   sequential= "sgp shifter"
    //   handbrake = "hbp handbrake"
    // GAP: CRP pedals (Moza CRP) may use a different PID than SR-P.
    pub const SR_P_PEDALS: u16 = 0x0003;
    pub const HGP_SHIFTER: u16 = 0x0020;
    pub const SGP_SHIFTER: u16 = 0x0021;
    pub const HBP_HANDBRAKE: u16 = 0x0022;
}

/// Known Moza rim IDs when attached to a compatible wheelbase.
///
/// These are rim identity values reported through the wheelbase transport,
/// not standalone USB product IDs.
pub mod rim_ids {
    pub const CS_V2: u8 = 0x01;
    pub const GS_V2: u8 = 0x02;
    pub const RS_V2: u8 = 0x03;
    pub const FSR: u8 = 0x04;
    pub const KS: u8 = 0x05;
    pub const ES: u8 = 0x06;
}
