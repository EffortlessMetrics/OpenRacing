//! HID protocol implementation for generic button boxes
//!
//! This crate provides protocol implementation for DIY button boxes using
//! standard USB HID Gamepad reports. Compatible with Arduino-based solutions
//! like SimRacingInputs, BangButtons, and similar DIY projects.
//!
//! ## Features
//! - Standard HID Gamepad report format
//! - Up to 32 buttons
//! - 4-axis analog inputs
//! - POV hat switch
//! - Rotary encoder support

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod input;
pub mod types;

pub use input::*;
pub use types::*;

use openracing_hid_common::HidCommonError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ButtonBoxError {
    #[error("Invalid report size: expected {expected}, got {actual}")]
    InvalidReportSize { expected: usize, actual: usize },

    #[error("Invalid button index: {0}")]
    InvalidButtonIndex(usize),

    #[error("Invalid axis index: {0}")]
    InvalidAxisIndex(usize),

    #[error("HID error: {0}")]
    HidError(String),
}

pub type ButtonBoxResult<T> = Result<T, ButtonBoxError>;

impl From<HidCommonError> for ButtonBoxError {
    fn from(e: HidCommonError) -> Self {
        ButtonBoxError::HidError(e.to_string())
    }
}

pub const REPORT_SIZE_GAMEPAD: usize = 8;
pub const MAX_BUTTONS: usize = 32;
pub const MAX_AXES: usize = 4;

pub const VENDOR_ID_GENERIC: u16 = 0x1209;
pub const PRODUCT_ID_BUTTON_BOX: u16 = 0x1BBD;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(REPORT_SIZE_GAMEPAD, 8);
        assert_eq!(MAX_BUTTONS, 32);
    }
}
