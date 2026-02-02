//! Vendor-specific protocol handlers
//!
//! This module provides abstractions for vendor-specific device initialization,
//! configuration, and quirks handling.

#![deny(static_mut_refs)]

pub mod moza;

#[cfg(test)]
mod moza_tests;

/// Trait for vendor-specific protocol handling
pub trait VendorProtocol: Send + Sync {
    /// Initialize the device with vendor-specific handshake
    fn initialize_device(
        &self,
        writer: &mut dyn DeviceWriter,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Send a feature report for configuration
    fn send_feature_report(
        &self,
        writer: &mut dyn DeviceWriter,
        report_id: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Get FFB configuration including quirks
    fn get_ffb_config(&self) -> FfbConfig;

    /// Check if this is a V2 hardware revision
    fn is_v2_hardware(&self) -> bool;
}

/// Device writer abstraction for sending reports
pub trait DeviceWriter: Send {
    fn write_feature_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>>;
    fn write_output_report(&mut self, data: &[u8]) -> Result<usize, Box<dyn std::error::Error>>;
}

/// FFB configuration including quirks
#[derive(Debug, Clone)]
pub struct FfbConfig {
    /// Swap positive/negative coefficients for conditional effects
    pub fix_conditional_direction: bool,
    /// Uses vendor-specific HID usage page
    pub uses_vendor_usage_page: bool,
    /// Required bInterval for USB polling
    pub required_b_interval: Option<u8>,
    /// Maximum torque in Nm
    pub max_torque_nm: f32,
    /// Encoder counts per revolution
    pub encoder_cpr: u32,
}

impl Default for FfbConfig {
    fn default() -> Self {
        Self {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: None,
            max_torque_nm: 10.0,
            encoder_cpr: 4096,
        }
    }
}

/// Get the appropriate vendor protocol handler for a device
pub fn get_vendor_protocol(vendor_id: u16, product_id: u16) -> Option<Box<dyn VendorProtocol>> {
    match vendor_id {
        0x346E => Some(Box::new(moza::MozaProtocol::new(product_id))),
        _ => None,
    }
}
