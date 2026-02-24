//! Fanatec USB vendor and product ID constants.

#![deny(static_mut_refs)]

/// Fanatec USB vendor ID.
pub const FANATEC_VENDOR_ID: u16 = 0x0EB7;

/// Report IDs used in the Fanatec HID protocol.
pub mod report_ids {
    /// Standard input report (steering, pedals, buttons).
    pub const STANDARD_INPUT: u8 = 0x01;
    /// Extended input / telemetry report (high-res steering, temperatures, fault flags).
    pub const EXTENDED_INPUT: u8 = 0x02;
    /// Mode-switch feature report ID.
    pub const MODE_SWITCH: u8 = 0x01;
    /// Force feedback output report ID.
    pub const FFB_OUTPUT: u8 = 0x01;
    /// LED / display / rumble output report ID.
    pub const LED_DISPLAY: u8 = 0x08;
}

/// FFB command bytes carried in output report 0x01.
pub mod ffb_commands {
    /// Constant force effect.
    pub const CONSTANT_FORCE: u8 = 0x01;
    /// Set overall device gain (0–100 %).
    pub const SET_GAIN: u8 = 0x10;
    /// Stop all active effects.
    pub const STOP_ALL: u8 = 0x0F;
}

/// LED / display / rumble command bytes carried in output report 0x08.
pub mod led_commands {
    /// Set rev-light LEDs on the attached steering-wheel rim.
    pub const REV_LIGHTS: u8 = 0x80;
    /// Set the numeric display (3-digit / OLED) on the attached rim.
    pub const DISPLAY: u8 = 0x81;
    /// Activate rumble motors on the attached rim.
    pub const RUMBLE: u8 = 0x82;
}

/// Known Fanatec wheelbase product IDs.
pub mod product_ids {
    /// ClubSport Wheel Base V2 (8 Nm belt-drive).
    pub const CLUBSPORT_V2: u16 = 0x0001;
    /// CSL Elite Wheel Base (6 Nm belt-drive).
    pub const CSL_ELITE_BASE: u16 = 0x0004;
    /// ClubSport Wheel Base V2.5 (8 Nm belt-drive).
    pub const CLUBSPORT_V2_5: u16 = 0x0005;
    /// Podium Wheel Base DD1 (20 Nm direct-drive).
    pub const DD1: u16 = 0x0006;
    /// Podium Wheel Base DD2 (25 Nm direct-drive).
    pub const DD2: u16 = 0x0007;
    /// CSL DD legacy PID / alternate USB stack (8 Nm).
    pub const CSL_DD_LEGACY: u16 = 0x0011;
    /// CSL DD (8 Nm direct-drive).
    pub const CSL_DD: u16 = 0x0020;
    /// Gran Turismo DD Pro (8 Nm direct-drive).
    pub const GT_DD_PRO: u16 = 0x0024;
    /// CSL Elite V1 / alternate SKU (6 Nm).
    pub const CSL_ELITE: u16 = 0x0E03;
    /// ClubSport V2 — legacy USB HID stack (8 Nm).
    pub const CLUBSPORT_V2_LEGACY: u16 = 0x6204;
}
