//! Logitech USB vendor and product ID constants.

#![deny(static_mut_refs)]

/// Logitech USB vendor ID.
pub const LOGITECH_VENDOR_ID: u16 = 0x046D;

/// Report IDs used in the Logitech HID protocol.
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// Vendor-specific feature/output report (init, range, LEDs, autocenter).
    pub const VENDOR: u8 = 0xF8;
    /// Set Constant Force effect output report.
    pub const CONSTANT_FORCE: u8 = 0x12;
    /// Device Gain output report.
    pub const DEVICE_GAIN: u8 = 0x16;
}

/// Command bytes carried in vendor report 0xF8.
pub mod commands {
    /// Enter native mode (full rotation + FFB).
    pub const NATIVE_MODE: u8 = 0x0A;
    /// Set wheel rotation range.
    pub const SET_RANGE: u8 = 0x81;
    /// Set autocenter force.
    pub const SET_AUTOCENTER: u8 = 0x14;
    /// Set rev-light LEDs.
    pub const SET_LEDS: u8 = 0x12;
}

/// Known Logitech wheel product IDs.
pub mod product_ids {
    /// G25 racing wheel (900°, 2.5 Nm belt-drive).
    pub const G25: u16 = 0xC299;
    /// G27 racing wheel — revision A.
    pub const G27_A: u16 = 0xC294;
    /// G27 racing wheel — revision B.
    pub const G27: u16 = 0xC29B;
    /// G29 racing wheel (PlayStation/PC, 900°, 2.2 Nm).
    pub const G29_PS: u16 = 0xC24F;
    /// G29 racing wheel (Xbox variant, 900°, 2.2 Nm).
    pub const G29_XBOX: u16 = 0xC260;
    /// G920 racing wheel — revision 1.
    pub const G920_V1: u16 = 0xC261;
    /// G920 racing wheel (Xbox/PC, 900°, 2.2 Nm).
    pub const G920: u16 = 0xC262;
    /// G923 racing wheel (Xbox, 900°, 2.2 Nm, TrueForce).
    pub const G923_XBOX: u16 = 0xC26D;
    /// G923 racing wheel (PlayStation, 900°, 2.2 Nm, TrueForce).
    pub const G923_PS: u16 = 0xC26E;
    /// G PRO racing wheel.
    pub const G_PRO: u16 = 0xC266;
    /// Pro Racing Wheel (direct drive, 1080°, 11 Nm, TrueForce).
    pub const PRO_RACING: u16 = 0xC272;
}
