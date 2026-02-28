//! HID protocol implementation for Simucube direct drive wheelbases
//!
//! This crate provides the protocol implementation for Simucube wheelbases:
//! - Simucube 2 Sport
//! - Simucube 2 Pro
//! - Simucube 2 Ultimate
//! - Simucube ActivePedal
//!
//! ## Features
//! - 22-bit angle sensor resolution
//! - Up to 25Nm torque (Pro/Ultimate)
//! - 360Hz force feedback
//! - Wireless wheel support (SimuCube Wireless Wheel)
//! - Active pedal support

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::*;
pub use input::*;
pub use output::*;
pub use types::*;

use openracing_hid_common::HidCommonError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SimucubeError {
    #[error("Invalid report size: expected {expected}, got {actual}")]
    InvalidReportSize { expected: usize, actual: usize },

    #[error("Invalid torque value: {0}")]
    InvalidTorque(f32),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Communication error: {0}")]
    Communication(String),
}

pub type SimucubeResult<T> = Result<T, SimucubeError>;

impl From<HidCommonError> for SimucubeError {
    fn from(e: HidCommonError) -> Self {
        SimucubeError::Communication(e.to_string())
    }
}

pub const VENDOR_ID: u16 = 0x16D0;
pub const PRODUCT_ID_SPORT: u16 = 0x0D61;
pub const PRODUCT_ID_PRO: u16 = 0x0D60;
pub const PRODUCT_ID_ULTIMATE: u16 = 0x0D5F;

pub const REPORT_SIZE_INPUT: usize = 64;
pub const REPORT_SIZE_OUTPUT: usize = 64;

pub const MAX_TORQUE_NM: f32 = 25.0;
pub const MAX_TORQUE_SPORT: f32 = 17.0;
pub const MAX_TORQUE_PRO: f32 = 25.0;
pub const MAX_TORQUE_ULTIMATE: f32 = 32.0;

pub const ANGLE_SENSOR_BITS: u32 = 22;
pub const ANGLE_SENSOR_MAX: u32 = (1 << ANGLE_SENSOR_BITS) - 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(VENDOR_ID, 0x16D0);
        assert_eq!(ANGLE_SENSOR_BITS, 22);
        assert_eq!(ANGLE_SENSOR_MAX, 0x3FFFFF);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_max_torque_values() {
        assert!(MAX_TORQUE_NM > 0.0);
        assert!(MAX_TORQUE_SPORT > 0.0);
        assert!(MAX_TORQUE_PRO > 0.0);
        assert!(MAX_TORQUE_ULTIMATE > 0.0);
    }
}
