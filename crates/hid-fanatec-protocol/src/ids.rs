//! Fanatec USB vendor and product ID constants.
//!
//! Verified against the community Linux kernel driver
//! [`gotzl/hid-fanatecff`](https://github.com/gotzl/hid-fanatecff) (`hid-ftec.h`)
//! and the USB device table in `hid-ftec.c`.

#![deny(static_mut_refs)]

/// Fanatec USB vendor ID (Endor AG).
///
/// Verified: `gotzl/hid-fanatecff` `hid-ftec.h` — `FANATEC_VENDOR_ID 0x0eb7`.
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
    /// Set steering rotation range (degrees, u16 LE in bytes 2–3).
    pub const SET_ROTATION_RANGE: u8 = 0x12;
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
///
/// Verified against `gotzl/hid-fanatecff` `hid-ftec.h` device ID defines
/// and the `hid_device_id` table in `hid-ftec.c`.
pub mod product_ids {
    /// ClubSport Wheel Base V2 (8 Nm belt-drive).
    /// Verified: `CLUBSPORT_V2_WHEELBASE_DEVICE_ID 0x0001`.
    pub const CLUBSPORT_V2: u16 = 0x0001;
    /// ClubSport Wheel Base V2.5 (8 Nm belt-drive).
    /// Verified: `CLUBSPORT_V25_WHEELBASE_DEVICE_ID 0x0004`.
    pub const CLUBSPORT_V2_5: u16 = 0x0004;
    /// CSL Elite Wheel Base PS4 (6 Nm belt-drive, PlayStation variant).
    /// Verified: `CSL_ELITE_PS4_WHEELBASE_DEVICE_ID 0x0005`.
    pub const CSL_ELITE_PS4: u16 = 0x0005;
    /// Podium Wheel Base DD1 (20 Nm direct-drive).
    /// Verified: `PODIUM_WHEELBASE_DD1_DEVICE_ID 0x0006`.
    pub const DD1: u16 = 0x0006;
    /// Podium Wheel Base DD2 (25 Nm direct-drive).
    /// Verified: `PODIUM_WHEELBASE_DD2_DEVICE_ID 0x0007`.
    pub const DD2: u16 = 0x0007;
    /// CSR Elite / Forza Motorsport Wheel Base (legacy belt-drive).
    /// Verified: `CSR_ELITE_WHEELBASE_DEVICE_ID 0x0011`.
    pub const CSR_ELITE: u16 = 0x0011;
    /// CSL DD (8 Nm direct-drive). Also covers DD Pro and ClubSport DD in PC mode.
    /// Verified: `CSL_DD_WHEELBASE_DEVICE_ID 0x0020`.
    pub const CSL_DD: u16 = 0x0020;
    /// Gran Turismo DD Pro (8 Nm direct-drive, PlayStation-specific PID).
    /// **Unverified** — not present in community Linux driver (`gotzl/hid-fanatecff`).
    /// Believed correct from USB captures.
    pub const GT_DD_PRO: u16 = 0x0024;
    /// CSL Elite Wheel Base — PC mode (6 Nm belt-drive).
    /// Verified: `CSL_ELITE_WHEELBASE_DEVICE_ID 0x0E03`.
    pub const CSL_ELITE: u16 = 0x0E03;
    /// ClubSport DD+ (12 Nm direct-drive, 2022 premium base).
    /// **Unverified** — not present in community Linux driver (`gotzl/hid-fanatecff`).
    /// Believed correct from USB captures.
    pub const CLUBSPORT_DD: u16 = 0x01E9;

    // ── Standalone pedal devices ───────────────────────────────────────────

    /// ClubSport Pedals V1 / V2 (USB, 2-axis or 3-axis set).
    /// **Unverified** — not present in community Linux driver (`gotzl/hid-fanatecff`).
    pub const CLUBSPORT_PEDALS_V1_V2: u16 = 0x1839;
    /// ClubSport Pedals V3 (USB, 3-axis with load cell brake).
    /// Verified: `CLUBSPORT_PEDALS_V3_DEVICE_ID 0x183b`.
    pub const CLUBSPORT_PEDALS_V3: u16 = 0x183B;
    /// CSL Elite Pedals (USB).
    /// Verified: `CSL_ELITE_PEDALS_DEVICE_ID 0x6204`.
    pub const CSL_ELITE_PEDALS: u16 = 0x6204;
    /// CSL Pedals with Load Cell Kit (USB adapter).
    /// Verified: `CSL_LC_PEDALS_DEVICE_ID 0x6205`.
    pub const CSL_PEDALS_LC: u16 = 0x6205;
    /// CSL Pedals V2 (USB adapter, updated Hall sensors).
    /// Verified: `CSL_LC_V2_PEDALS_DEVICE_ID 0x6206`.
    pub const CSL_PEDALS_V2: u16 = 0x6206;
}

/// Rim IDs reported in byte 0x1F of the standard input report (report ID 0x01).
///
/// These identify which steering wheel rim is attached to the wheelbase.
/// The community Linux driver reads this field via `data[0x1f]` in `ftecff_raw_event`.
///
/// Verified IDs are cross-referenced against `gotzl/hid-fanatecff` `hid-ftec.h`.
pub mod rim_ids {
    /// BMW GT2 steering wheel rim. **Unverified** — not present in community driver.
    pub const BMW_GT2: u8 = 0x01;
    /// ClubSport Steering Wheel Formula V2 — dual-clutch paddles, funky switch.
    /// Verified: `CLUBSPORT_STEERING_WHEEL_FORMULA_V2_ID 0x0a` in `hid-ftec.h`.
    pub const FORMULA_V2: u8 = 0x0A;
    /// ClubSport Steering Wheel Formula V2.5 / V2.5 X.
    /// **Unverified** — not present in community driver.
    pub const FORMULA_V2_5: u8 = 0x03;
    /// CSL Elite P1 V2 steering wheel rim.
    /// Verified: `CSL_STEERING_WHEEL_P1_V2 0x08` in `hid-ftec.h`.
    pub const CSL_ELITE_P1: u8 = 0x08;
    /// McLaren GT3 V2 — has funky switch, rotary encoders, dual clutch paddles.
    /// Verified: `CSL_ELITE_STEERING_WHEEL_MCLAREN_GT3_V2_ID 0x0b` in `hid-ftec.h`.
    pub const MCLAREN_GT3_V2: u8 = 0x0B;
    /// Podium Steering Wheel Porsche 911 GT3 R.
    /// Verified: `PODIUM_STEERING_WHEEL_PORSCHE_911_GT3_R_ID 0x0c` in `hid-ftec.h`.
    pub const PORSCHE_911_GT3_R: u8 = 0x0C;
    /// Porsche 918 RSR steering wheel rim.
    /// **Unverified** — not present in community driver; may overlap with a different rim.
    pub const PORSCHE_918_RSR: u8 = 0x05;
    /// ClubSport RS steering wheel rim. **Unverified** — not present in community driver.
    pub const CLUBSPORT_RS: u8 = 0x06;
    /// CSL Elite Steering Wheel WRC.
    /// Verified: `CSL_ELITE_STEERING_WHEEL_WRC_ID 0x12` in `hid-ftec.h`.
    pub const WRC: u8 = 0x12;
    /// Podium Hub. **Unverified** — not present in community driver.
    pub const PODIUM_HUB: u8 = 0x09;
}
