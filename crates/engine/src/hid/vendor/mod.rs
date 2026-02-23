//! Vendor-specific protocol handlers
//!
//! This module provides abstractions for vendor-specific device initialization,
//! configuration, and quirks handling.

#![deny(static_mut_refs)]

pub mod moza;
pub mod moza_direct;
pub mod simagic;

#[cfg(test)]
mod moza_tests;
#[cfg(test)]
mod simagic_tests;

pub use racing_wheel_hid_moza_protocol::{DeviceWriter, FfbConfig, VendorProtocol};

/// Get the appropriate vendor protocol handler for a device
pub fn get_vendor_protocol(vendor_id: u16, product_id: u16) -> Option<Box<dyn VendorProtocol>> {
    match vendor_id {
        0x346E => Some(Box::new(moza::MozaProtocol::new(product_id))),
        0x0483 | 0x16D0 | 0x3670 => Some(Box::new(simagic::SimagicProtocol::new(
            vendor_id, product_id,
        ))),
        _ => None,
    }
}
