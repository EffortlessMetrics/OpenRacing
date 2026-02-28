//! Simagic USB vendor and product ID constants.
//!
//! VID `0x3670` is assigned to Shen Zhen Simagic Technology Co., Limited and is
//! used by the EVO generation of Simagic wheelbases.
//!
//! Legacy Simagic devices (Alpha, Alpha Mini, M10, Alpha Ultimate) use the
//! STMicroelectronics generic VID `0x0483` with PID `0x0522` and are handled by
//! the `simagic-ff` / `hid-pidff` kernel drivers, not this proprietary crate.
//!
//! Sources: USB VID registry (the-sz.com), JacKeTUs/linux-steering-wheels
//! compatibility table, JacKeTUs/simagic-ff driver.

#![deny(static_mut_refs)]

/// Simagic EVO-generation USB Vendor ID (Shen Zhen Simagic Technology Co., Limited).
pub const SIMAGIC_VENDOR_ID: u16 = 0x3670;

/// Legacy Simagic VID (STMicroelectronics generic, shared with VRS DirectForce).
/// Used by: Alpha, Alpha Mini, M10, Alpha Ultimate (all share PID 0x0522).
pub const SIMAGIC_LEGACY_VENDOR_ID: u16 = 0x0483;

/// Legacy Simagic PID shared by Alpha / Alpha Mini / M10 / Alpha Ultimate.
pub const SIMAGIC_LEGACY_PID: u16 = 0x0522;

/// HID Report IDs used in the Simagic HID protocol.
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// Extended input report with additional button states.
    pub const EXTENDED_INPUT: u8 = 0x02;
    /// Device info report.
    pub const DEVICE_INFO: u8 = 0x03;
    /// FFB constant force report.
    pub const CONSTANT_FORCE: u8 = 0x11;
    /// FFB spring effect report.
    pub const SPRING_EFFECT: u8 = 0x12;
    /// FFB damper effect report.
    pub const DAMPER_EFFECT: u8 = 0x13;
    /// FFB friction effect report.
    pub const FRICTION_EFFECT: u8 = 0x14;
    /// FFB sine effect report.
    pub const SINE_EFFECT: u8 = 0x15;
    /// FFB square effect report.
    pub const SQUARE_EFFECT: u8 = 0x16;
    /// FFB triangle effect report.
    pub const TRIANGLE_EFFECT: u8 = 0x17;
    /// Set rotation range.
    pub const ROTATION_RANGE: u8 = 0x20;
    /// Set device gain.
    pub const DEVICE_GAIN: u8 = 0x21;
    /// LED control report.
    pub const LED_CONTROL: u8 = 0x30;
    /// Quick release status query.
    pub const QUICK_RELEASE_STATUS: u8 = 0x40;
}

/// Known Simagic product IDs (VID `0x3670` EVO generation unless otherwise noted).
pub mod product_ids {
    // ── EVO generation wheelbases (VID 0x3670, verified) ────────────────────
    /// Simagic EVO Sport wheelbase.
    pub const EVO_SPORT: u16 = 0x0500;
    /// Simagic EVO wheelbase.
    pub const EVO: u16 = 0x0501;
    /// Simagic EVO Pro wheelbase.
    pub const EVO_PRO: u16 = 0x0502;

    // ── Newer wheelbases (VID 0x3670 assumed, PIDs estimated) ─────────────
    /// Simagic Alpha EVO wheelbase (estimated PID — not independently verified).
    pub const ALPHA_EVO: u16 = 0x0600;
    /// Simagic Neo wheelbase (estimated PID — not independently verified).
    pub const NEO: u16 = 0x0700;
    /// Simagic Neo Mini wheelbase (estimated PID — not independently verified).
    pub const NEO_MINI: u16 = 0x0701;

    // ── Accessories (VID 0x3670 assumed, PIDs estimated) ─────────────────
    /// Simagic P1000 pedals (estimated PID).
    pub const P1000_PEDALS: u16 = 0x1001;
    /// Simagic P2000 pedals (estimated PID).
    pub const P2000_PEDALS: u16 = 0x1002;
    /// Simagic P1000A pedals (estimated PID).
    pub const P1000A_PEDALS: u16 = 0x1003;
    /// Simagic H-pattern shifter (estimated PID).
    pub const SHIFTER_H: u16 = 0x2001;
    /// Simagic Sequential shifter (estimated PID).
    pub const SHIFTER_SEQ: u16 = 0x2002;
    /// Simagic Handbrake (estimated PID).
    pub const HANDBRAKE: u16 = 0x3001;
    /// Simagic WR1 steering wheel rim (estimated PID).
    pub const RIM_WR1: u16 = 0x4001;
    /// Simagic GT1 steering wheel rim (estimated PID).
    pub const RIM_GT1: u16 = 0x4002;
    /// Simagic GT Neo steering wheel rim (estimated PID).
    pub const RIM_GT_NEO: u16 = 0x4003;
    /// Simagic Formula steering wheel rim (estimated PID).
    pub const RIM_FORMULA: u16 = 0x4004;
}
