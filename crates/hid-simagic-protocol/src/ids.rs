//! Simagic USB vendor and product ID constants.

#![deny(static_mut_refs)]

/// Simagic USB Vendor ID.
pub const SIMAGIC_VENDOR_ID: u16 = 0x2D5C;

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

/// Known Simagic product IDs.
pub mod product_ids {
    /// Simagic Alpha wheelbase (15 Nm).
    pub const ALPHA: u16 = 0x0101;
    /// Simagic Alpha Mini wheelbase (10 Nm).
    pub const ALPHA_MINI: u16 = 0x0102;
    /// Simagic Alpha EVO wheelbase (15 Nm).
    pub const ALPHA_EVO: u16 = 0x0103;
    /// Simagic M10 wheelbase (10 Nm).
    pub const M10: u16 = 0x0201;
    /// Simagic Neo wheelbase (10 Nm).
    pub const NEO: u16 = 0x0301;
    /// Simagic Neo Mini wheelbase (7 Nm).
    pub const NEO_MINI: u16 = 0x0302;

    /// Simagic P1000 pedals.
    pub const P1000_PEDALS: u16 = 0x1001;
    /// Simagic P2000 pedals.
    pub const P2000_PEDALS: u16 = 0x1002;
    /// Simagic P1000A pedals.
    pub const P1000A_PEDALS: u16 = 0x1003;

    /// Simagic H-pattern shifter.
    pub const SHIFTER_H: u16 = 0x2001;
    /// Simagic Sequential shifter.
    pub const SHIFTER_SEQ: u16 = 0x2002;

    /// Simagic Handbrake.
    pub const HANDBRAKE: u16 = 0x3001;

    /// Simagic WR1 steering wheel rim.
    pub const RIM_WR1: u16 = 0x4001;
    /// Simagic GT1 steering wheel rim.
    pub const RIM_GT1: u16 = 0x4002;
    /// Simagic GT Neo steering wheel rim.
    pub const RIM_GT_NEO: u16 = 0x4003;
    /// Simagic Formula steering wheel rim.
    pub const RIM_FORMULA: u16 = 0x4004;
}
