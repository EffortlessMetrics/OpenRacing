//! HID protocol implementation for Asetek wheelbases
//!
//! This crate provides protocol implementation for Asetek wheelbases:
//! - Asetek Forte (18 Nm)
//! - Asetek Invicta (27 Nm)
//! - Asetek La Prima (12 Nm)
//! - Asetek Tony Kanaan Edition (27 Nm, Invicta-based)
//!
//! ## Features
//! - Direct drive force feedback
//! - Quick release system
//! - USB HID interface

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod output;
pub mod quirks;
pub mod types;

pub use ids::*;
pub use input::*;
pub use output::*;
pub use types::*;

use openracing_hid_common::HidCommonError;
use thiserror::Error;

/// Errors returned by Asetek protocol operations.
#[derive(Error, Debug)]
pub enum AsetekError {
    #[error("Invalid report size: expected {expected}, got {actual}")]
    InvalidReportSize { expected: usize, actual: usize },

    #[error("Invalid torque value: {0}")]
    InvalidTorque(f32),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),
}

/// Convenience result alias for Asetek operations.
pub type AsetekResult<T> = Result<T, AsetekError>;

impl From<HidCommonError> for AsetekError {
    fn from(e: HidCommonError) -> Self {
        AsetekError::DeviceNotFound(e.to_string())
    }
}

/// Asetek SimSports USB Vendor ID (`0x2433`).
pub const VENDOR_ID: u16 = 0x2433;
/// Product ID for Asetek Forte (18 Nm).
pub const PRODUCT_ID_FORTE: u16 = 0xF301;
/// Product ID for Asetek Invicta (27 Nm).
pub const PRODUCT_ID_INVICTA: u16 = 0xF300;
/// Product ID for Asetek La Prima (12 Nm).
pub const PRODUCT_ID_LAPRIMA: u16 = 0xF303;

/// HID input report size in bytes.
pub const REPORT_SIZE_INPUT: usize = 32;
/// HID output report size in bytes.
pub const REPORT_SIZE_OUTPUT: usize = 32;

/// Maximum torque (Nm) across all Asetek models (Invicta).
pub const MAX_TORQUE_NM: f32 = 27.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(VENDOR_ID, 0x2433);
    }
}
