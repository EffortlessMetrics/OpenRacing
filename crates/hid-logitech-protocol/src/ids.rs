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
///
/// VID/PID values verified against the Linux kernel hid-ids.h, the
/// new-lg4ff driver (berarma/new-lg4ff), and the oversteer project
/// (berarma/oversteer).
pub mod product_ids {
    /// G25 racing wheel (900°, 2.5 Nm belt-drive).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G25_WHEEL = 0xc299`.
    pub const G25: u16 = 0xC299;
    /// Driving Force / Formula EX wheel; also appears when a G27 is in
    /// compatibility (emulation) mode.
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_WHEEL = 0xc294`.
    pub const G27_A: u16 = 0xC294;
    /// G27 racing wheel (900°, 2.5 Nm belt-drive).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G27_WHEEL = 0xc29b`.
    pub const G27: u16 = 0xC29B;
    /// G29 racing wheel (PlayStation/PC, 900°, 2.2 Nm).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G29_WHEEL = 0xc24f`.
    pub const G29_PS: u16 = 0xC24F;
    /// G920 racing wheel (Xbox/PC, 900°, 2.2 Nm).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G920_WHEEL = 0xc262`.
    pub const G920: u16 = 0xC262;
    /// G923 racing wheel — native/HID mode (after mode switch from PS compat).
    ///
    /// Verified: new-lg4ff `USB_DEVICE_ID_LOGITECH_G923_WHEEL = 0xc266`,
    /// oversteer `LG_G923P = '046d:c266'`.
    pub const G923: u16 = 0xC266;
    /// G923 racing wheel — PlayStation compatibility mode (initial enumeration).
    ///
    /// Verified: new-lg4ff `USB_DEVICE_ID_LOGITECH_G923_PS_WHEEL = 0xc267`.
    pub const G923_PS: u16 = 0xC267;
    /// G923 racing wheel (Xbox/PC, 900°, 2.2 Nm, TrueForce).
    ///
    /// Verified: kernel `USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL = 0xc26e`,
    /// new-lg4ff `USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL = 0xc26e`.
    pub const G923_XBOX: u16 = 0xC26E;
    /// G PRO racing wheel (PlayStation/PC, direct drive, 11 Nm, 1080°).
    ///
    /// Verified: oversteer `LG_GPRO_PS = '046d:c268'`.
    pub const G_PRO: u16 = 0xC268;
    /// G PRO racing wheel (Xbox/PC, direct drive, 11 Nm, 1080°).
    ///
    /// Verified: oversteer `LG_GPRO_XBOX = '046d:c272'`.
    pub const G_PRO_XBOX: u16 = 0xC272;
}
