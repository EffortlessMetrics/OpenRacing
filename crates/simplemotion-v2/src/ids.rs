//! SimpleMotion V2 USB vendor and product ID constants.

/// SimpleMotion V2 USB Vendor ID (Granite Devices).
pub const SM_VENDOR_ID: u16 = 0x1D50;

/// IONI USB Vendor ID.
pub const IONI_VENDOR_ID: u16 = 0x1D50;

/// IONI Product ID (Standard).
pub const IONI_PRODUCT_ID: u16 = 0x6050;

/// IONI Product ID (Premium).
pub const IONI_PRODUCT_ID_PREMIUM: u16 = 0x6051;

/// ARGON USB Vendor ID.
pub const ARGON_VENDOR_ID: u16 = 0x1D50;

/// ARGON Product ID.
pub const ARGON_PRODUCT_ID: u16 = 0x6052;

/// Open Sim Wheel (OSW) Vendor ID.
pub const OSW_VENDOR_ID: u16 = 0x1D50;

/// Known SimpleMotion V2 product IDs.
pub mod product_ids {
    use super::*;

    /// IONI servo drive (standard).
    pub const IONI: u16 = IONI_PRODUCT_ID;
    /// IONI servo drive (premium).
    pub const IONI_PREMIUM: u16 = IONI_PRODUCT_ID_PREMIUM;
    /// ARGON servo drive.
    pub const ARGON: u16 = ARGON_PRODUCT_ID;
    /// Simucube 1 (using IONI).
    pub const SIMUCUBE_1: u16 = IONI_PRODUCT_ID;
    /// Simucube 2 (using IONI Premium).
    pub const SIMUCUBE_2: u16 = IONI_PRODUCT_ID_PREMIUM;
    /// Simucube Sport (using ARGON).
    pub const SIMUCUBE_SPORT: u16 = ARGON_PRODUCT_ID;
    /// Simucube Pro (using IONI Premium).
    pub const SIMUCUBE_PRO: u16 = IONI_PRODUCT_ID_PREMIUM;
}
