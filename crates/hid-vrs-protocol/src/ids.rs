//! VRS DirectForce Pro USB vendor and product ID constants.
//!
//! ## VID collision: 0x0483 (STMicroelectronics)
//!
//! VRS products use STM32 microcontrollers and inherit the generic
//! STMicroelectronics USB Vendor ID `0x0483`. This VID is **extremely**
//! crowded — hundreds of unrelated STM32-based devices share it. In the
//! sim racing world, at least two other vendors also ship on this VID:
//!
//! * **Simagic (legacy)** — PID `0x0522` (M10, Alpha Mini, Alpha, Alpha Ultimate — all share same PID)
//! * **Cube Controls** (PROVISIONAL) — PIDs `0x0C73`–`0x0C75`
//!
//! Runtime disambiguation **must** be done by product ID, not by vendor ID
//! alone. See `crates/engine/src/hid/vendor/mod.rs` for the dispatch logic
//! and `docs/FRICTION_LOG.md` (F-034) for details.
//!
//! ## Verification status (web-verified 2025-07)
//!
//! Checked against: Linux kernel `hid-ids.h` (mainline), `hid-universal-pidff.c` (mainline),
//! JacKeTUs/linux-steering-wheels, JacKeTUs/simracing-hwdb, the-sz.com, usb-ids.gowdy.us.
//!
//! **Branding note:** VRS is now listed as "Turtle Beach VRS" in linux-steering-wheels
//! (Turtle Beach acquired VRS). The USB VID/PID remains unchanged.
//!
//! | Field | Status | Source |
//! |-------|--------|--------|
//! | VID 0x0483 | ✅ Confirmed | STMicroelectronics (usb.org), the-sz.com, usb-ids.gowdy.us, Linux kernel `hid-ids.h` (mainline) |
//! | DFP PID 0xA355 | ✅ Confirmed | Linux kernel `hid-ids.h` (`USB_DEVICE_ID_VRS_DFP`), `hid-universal-pidff.c` device table (with `HID_PIDFF_QUIRK_PERMISSIVE_CONTROL`), linux-steering-wheels (Platinum), simracing-hwdb |
//! | R295 PID 0xA44C | ✅ Confirmed | Linux kernel `hid-ids.h` (`USB_DEVICE_ID_VRS_R295`), `hid-quirks.c` |
//! | Pedals PID 0xA3BE | ✅ Confirmed (community) | JacKeTUs/simracing-hwdb `90-vrs.hwdb` (`v0483pA3BE`, "VRS DirectForce Pro Pedals") |
//! | DFP V2 PID 0xA356 | ⚠ Unverified externally | Not in kernel `hid-ids.h`, not in linux-steering-wheels, not in simracing-hwdb (2025-07). Sequential assumption only. |
//! | Pedals V2 PID 0xA358 | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | Handbrake PID 0xA359 | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | Shifter PID 0xA35A | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | DFP torque 20 Nm | ✅ Confirmed | simracinggarage.com review ("20nm Mige motors") |
//! | DFP V2 torque 25 Nm | ⚠ Unverified | No authoritative source found |
//!
//! ### USB-ID database cross-check (2025-07)
//!
//! - the-sz.com: VID `0x0483` = "STMicroelectronics" (no VRS products listed — expected,
//!   as VRS uses the generic STM VID and these databases don't track STM32 end-products)
//! - usb-ids.gowdy.us: same — only generic STM devices listed under `0x0483`
//!
//! Confidence: DFP + R295 = **High** (kernel mainline). Pedals = **High** (simracing-hwdb).
//! V2/Handbrake/Shifter = **Low** (sequential estimates only).

/// VRS DirectForce Pro USB Vendor ID (STMicroelectronics generic VID).
///
/// **Shared VID** — also used by legacy Simagic (PID `0x0522`) and Cube Controls.
/// Dispatch by PID is required at runtime.
///
/// Source: Linux kernel `hid-ids.h` (`USB_VENDOR_ID_VRS`), usb.org,
/// the-sz.com ("STMicroelectronics"), usb-ids.gowdy.us ("STMicroelectronics").
pub const VRS_VENDOR_ID: u16 = 0x0483;

/// VRS DirectForce Pro Product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (mainline, `USB_DEVICE_ID_VRS_DFP = 0xa355`),
/// `hid-universal-pidff.c` device table (with `HID_PIDFF_QUIRK_PERMISSIVE_CONTROL`),
/// JacKeTUs/linux-steering-wheels (Platinum rating), and JacKeTUs/simracing-hwdb
/// `90-vrs.hwdb`. Covers all DFP variants: uDFP20, DFP15, DFP20 (same PID per
/// linux-steering-wheels).
pub const VRS_PRODUCT_ID: u16 = 0xA355;

/// HID Report IDs used in the VRS DirectForce Pro HID protocol (PIDFF).
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// FFB Set Effect (PIDFF block load).
    pub const SET_EFFECT: u8 = 0x02;
    /// FFB Effect Operation (PIDFF play/stop).
    pub const EFFECT_OPERATION: u8 = 0x0A;
    /// FFB Device Control (enable/disable FFB).
    pub const DEVICE_CONTROL: u8 = 0x0B;
    /// FFB Constant force report.
    pub const CONSTANT_FORCE: u8 = 0x11;
    /// FFB Ramp force report.
    pub const RAMP_FORCE: u8 = 0x13;
    /// FFB Square wave effect.
    pub const SQUARE_EFFECT: u8 = 0x14;
    /// FFB Sine wave effect.
    pub const SINE_EFFECT: u8 = 0x15;
    /// FFB Triangle wave effect.
    pub const TRIANGLE_EFFECT: u8 = 0x16;
    /// FFB Sawtooth up effect.
    pub const SAWTOOTH_UP_EFFECT: u8 = 0x17;
    /// FFB Sawtooth down effect.
    pub const SAWTOOTH_DOWN_EFFECT: u8 = 0x18;
    /// FFB Spring effect.
    pub const SPRING_EFFECT: u8 = 0x19;
    /// FFB Damper effect.
    pub const DAMPER_EFFECT: u8 = 0x1A;
    /// FFB Friction effect.
    pub const FRICTION_EFFECT: u8 = 0x1B;
    /// FFB Custom force effect.
    pub const CUSTOM_FORCE_EFFECT: u8 = 0x1C;
    /// FFB Download force sample.
    pub const DOWNLOAD_FORCE_SAMPLE: u8 = 0x22;
    /// FFB Set Report.
    pub const SET_REPORT: u8 = 0x0C;
    /// FFB Get Report.
    pub const GET_REPORT: u8 = 0x0D;
}

/// Known VRS product IDs.
///
/// The DFP PID (`0xA355`) and R295 PID (`0xA44C`) are confirmed in the
/// Linux kernel `hid-ids.h`. The Pedals PID (`0xA3BE`) is confirmed in
/// JacKeTUs/simracing-hwdb. Other PIDs remain unverified.
pub mod product_ids {
    /// VRS DirectForce Pro wheelbase (20 Nm, ✅ torque confirmed).
    ///
    /// ✅ PID confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_VRS_DFP`),
    /// `hid-universal-pidff.c`, linux-steering-wheels (Platinum).
    /// Covers uDFP20, DFP15, and DFP20 variants (all share the same PID).
    pub const DIRECTFORCE_PRO: u16 = 0xA355;

    /// VRS DirectForce Pro V2 wheelbase (25 Nm, ⚠ torque unverified).
    ///
    /// ⚠ PID unverified externally (re-checked 2025-07). Not present in Linux
    /// kernel `hid-ids.h`, JacKeTUs/linux-steering-wheels, or simracing-hwdb.
    /// Originally estimated by sequential numbering, but the Pedals PID
    /// (`0xA3BE`) breaking the sequence weakens this assumption.
    pub const DIRECTFORCE_PRO_V2: u16 = 0xA356;

    /// VRS R295 wheelbase.
    ///
    /// ✅ PID confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_VRS_R295 = 0xa44c`),
    /// also referenced in `hid-quirks.c`.
    pub const R295: u16 = 0xA44C;

    /// VRS DirectForce Pro Pedals.
    ///
    /// ✅ PID confirmed (community): JacKeTUs/simracing-hwdb `90-vrs.hwdb`
    /// (`v0483pA3BE`, labeled "VRS DirectForce Pro Pedals").
    /// Replaces previous unverified estimate of `0xA357`.
    pub const PEDALS: u16 = 0xA3BE;

    /// Backward-compat alias for the old unverified pedals PID.
    /// The old PID `0xA357` was an unverified sequential estimate;
    /// the community-confirmed PID is `PEDALS` (`0xA3BE`).
    /// TODO: Migrate callers from `PEDALS_V1` (0xA357) to `PEDALS` (0xA3BE).
    pub const PEDALS_V1: u16 = 0xA357;

    /// VRS Pedals (digital/load cell). ⚠ PID unverified.
    pub const PEDALS_V2: u16 = 0xA358;
    /// VRS Handbrake. ⚠ PID unverified.
    pub const HANDBRAKE: u16 = 0xA359;
    /// VRS Shifter. ⚠ PID unverified.
    pub const SHIFTER: u16 = 0xA35A;
}
