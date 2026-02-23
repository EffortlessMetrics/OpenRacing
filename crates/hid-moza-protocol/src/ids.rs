//! Moza vendor ID and product ID constants.

#![deny(static_mut_refs)]

/// Moza Racing USB Vendor ID.
pub const MOZA_VENDOR_ID: u16 = 0x346E;

/// Known Moza product IDs.
pub mod product_ids {
    // Wheelbases (V1)
    pub const R16_R21_V1: u16 = 0x0000;
    pub const R9_V1: u16 = 0x0002;
    pub const R5_V1: u16 = 0x0004;
    pub const R3_V1: u16 = 0x0005;
    pub const R12_V1: u16 = 0x0006;

    // Wheelbases (V2)
    pub const R16_R21_V2: u16 = 0x0010;
    pub const R9_V2: u16 = 0x0012;
    pub const R5_V2: u16 = 0x0014;
    pub const R3_V2: u16 = 0x0015;
    pub const R12_V2: u16 = 0x0016;

    // Peripherals
    pub const SR_P_PEDALS: u16 = 0x0003;
    pub const HGP_SHIFTER: u16 = 0x0020;
    pub const SGP_SHIFTER: u16 = 0x0021;
    pub const HBP_HANDBRAKE: u16 = 0x0022;
}

/// Known Moza rim IDs when attached to a compatible wheelbase.
///
/// These are rim identity values reported through the wheelbase transport,
/// not standalone USB product IDs.
pub mod rim_ids {
    pub const CS_V2: u8 = 0x01;
    pub const GS_V2: u8 = 0x02;
    pub const RS_V2: u8 = 0x03;
    pub const FSR: u8 = 0x04;
    pub const KS: u8 = 0x05;
    pub const ES: u8 = 0x06;
}
