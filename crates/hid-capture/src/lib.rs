//! Shared library for hid-capture.
//!
//! Exposes data types and parsing helpers for integration testing.

use serde::{Deserialize, Serialize};

pub fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| format!("invalid hex value '{s}': {e}"))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureReport {
    pub timestamp_us: u64,
    pub report_id: u8,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureFile {
    pub vendor_id: String,
    pub product_id: String,
    pub captures: Vec<CaptureReport>,
}
