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

/// Known Fanatec wheelbase product IDs.
pub mod product_ids {
    /// DD1 / Podium DD1 (20 Nm).
    pub const DD1: u16 = 0x0001;
    /// DD2 / Podium DD2 (25 Nm).
    pub const DD2: u16 = 0x0004;
    /// CSL Elite PS4 edition (6 Nm).
    pub const CSL_ELITE_PS4: u16 = 0x0005;
    /// ClubSport V2.5 (8 Nm).
    pub const CLUBSPORT_V2_5: u16 = 0x0006;
    /// DD2 variant observed in firmware captures (25 Nm).
    pub const DD2_VARIANT: u16 = 0x0007;
    /// CSL DD legacy PID / alternate firmware variant (8 Nm).
    pub const CSL_DD_LEGACY: u16 = 0x0011;
    /// CSL DD (8 Nm).
    pub const CSL_DD: u16 = 0x0020;
    /// Gran Turismo DD Pro (8 Nm).
    pub const GT_DD_PRO: u16 = 0x0024;
    /// CSL Elite (6 Nm).
    pub const CSL_ELITE: u16 = 0x0E03;
    /// ClubSport V2 — legacy USB stack (8 Nm).
    pub const CLUBSPORT_V2_LEGACY: u16 = 0x6204;
}
