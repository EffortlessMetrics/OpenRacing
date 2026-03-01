//! HID protocol implementation for Heusinkveld sim pedals
//!
//! This crate provides protocol implementation for Heusinkveld pedals:
//! - Heusinkveld Sprint
//! - Heusinkveld Ultimate+
//! - Heusinkveld Pro
//!
//! ## Features
//! - Load cell support up to 200kg
//! - USB HID interface
//! - SmartControl software integration
//! - Hydraulic damping support

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod types;

pub use ids::*;
pub use input::*;
pub use types::*;

use openracing_hid_common::HidCommonError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HeusinkveldError {
    #[error("Invalid report size: expected {expected}, got {actual}")]
    InvalidReportSize { expected: usize, actual: usize },

    #[error("Invalid pedal value: {0}")]
    InvalidPedalValue(u16),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),
}

pub type HeusinkveldResult<T> = Result<T, HeusinkveldError>;

impl From<HidCommonError> for HeusinkveldError {
    fn from(e: HidCommonError) -> Self {
        HeusinkveldError::DeviceNotFound(e.to_string())
    }
}

pub const VENDOR_ID: u16 = 0x04D8;
pub const PRODUCT_ID_SPRINT: u16 = 0xF6D0;
pub const PRODUCT_ID_ULTIMATE: u16 = 0xF6D2;
pub const PRODUCT_ID_PRO: u16 = 0xF6D3;

pub const REPORT_SIZE_INPUT: usize = 8;

pub const MAX_LOAD_CELL_VALUE: u16 = 0xFFFF;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(VENDOR_ID, 0x04D8);
        assert_eq!(MAX_LOAD_CELL_VALUE, 0xFFFF);
    }
}
