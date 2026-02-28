//! VRS DirectForce Pro USB vendor and product ID constants.

/// VRS DirectForce Pro USB Vendor ID.
pub const VRS_VENDOR_ID: u16 = 0x0483;

/// VRS DirectForce Pro Product ID.
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
pub mod product_ids {
    /// VRS DirectForce Pro wheelbase.
    pub const DIRECTFORCE_PRO: u16 = 0xA355;
    /// VRS DirectForce Pro V2 wheelbase.
    pub const DIRECTFORCE_PRO_V2: u16 = 0xA356;
    /// VRS Pedals (analog).
    pub const PEDALS_V1: u16 = 0xA357;
    /// VRS Pedals (digital/load cell).
    pub const PEDALS_V2: u16 = 0xA358;
    /// VRS Handbrake.
    pub const HANDBRAKE: u16 = 0xA359;
    /// VRS Shifter.
    pub const SHIFTER: u16 = 0xA35A;
}
