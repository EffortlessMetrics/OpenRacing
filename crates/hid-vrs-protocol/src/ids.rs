//! VRS DirectForce Pro USB vendor and product ID constants.
//!
//! ## VID collision: 0x0483 (STMicroelectronics)
//!
//! VRS products use STM32 microcontrollers and inherit the generic
//! STMicroelectronics USB Vendor ID `0x0483`. This VID is **extremely**
//! crowded — hundreds of unrelated STM32-based devices share it. In the
//! sim racing world, at least two other vendors also ship on this VID:
//!
//! * **Simagic (legacy)** — PIDs `0x0522`–`0x0524` (Alpha, Alpha Mini, Alpha Ultimate)
//! * **Cube Controls** (PROVISIONAL) — PIDs `0x0C73`–`0x0C75`
//!
//! Runtime disambiguation **must** be done by product ID, not by vendor ID
//! alone. See `crates/engine/src/hid/vendor/mod.rs` for the dispatch logic
//! and `docs/FRICTION_LOG.md` (F-034) for details.
//!
//! ## Verification status
//!
//! | Field | Status | Source |
//! |-------|--------|--------|
//! | VID 0x0483 | ✅ Confirmed | STMicroelectronics (usb.org), devicehunt.com |
//! | DFP PID 0xA355 | ⚠ Unverified externally | Not in USB-IF DB or linux-hardware.org |
//! | DFP V2 PID 0xA356 | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | Pedals V1 PID 0xA357 | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | Pedals V2 PID 0xA358 | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | Handbrake PID 0xA359 | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | Shifter PID 0xA35A | ⚠ Unverified externally | Provisionally estimated (sequential) |
//! | DFP torque 20 Nm | ✅ Confirmed | simracinggarage.com review ("20nm Mige motors") |
//! | DFP V2 torque 25 Nm | ⚠ Unverified | No authoritative source found |

/// VRS DirectForce Pro USB Vendor ID (STMicroelectronics generic VID).
///
/// **Shared VID** — also used by legacy Simagic and Cube Controls.
/// Dispatch by PID is required at runtime.
pub const VRS_VENDOR_ID: u16 = 0x0483;

/// VRS DirectForce Pro Product ID. ⚠ Unverified in external USB databases.
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
/// All PIDs in the `0xA35x` range are **unverified** in external USB
/// databases. The sequential numbering is provisionally assumed based
/// on the confirmed DFP PID (`0xA355`).
pub mod product_ids {
    /// VRS DirectForce Pro wheelbase (20 Nm, ✅ torque confirmed).
    pub const DIRECTFORCE_PRO: u16 = 0xA355;
    /// VRS DirectForce Pro V2 wheelbase (25 Nm, ⚠ torque unverified).
    pub const DIRECTFORCE_PRO_V2: u16 = 0xA356;
    /// VRS Pedals (analog). ⚠ PID unverified.
    pub const PEDALS_V1: u16 = 0xA357;
    /// VRS Pedals (digital/load cell). ⚠ PID unverified.
    pub const PEDALS_V2: u16 = 0xA358;
    /// VRS Handbrake. ⚠ PID unverified.
    pub const HANDBRAKE: u16 = 0xA359;
    /// VRS Shifter. ⚠ PID unverified.
    pub const SHIFTER: u16 = 0xA35A;
}
