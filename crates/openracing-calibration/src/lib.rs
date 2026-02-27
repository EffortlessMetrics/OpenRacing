//! Device calibration utilities
//!
//! This crate provides calibration helpers for racing wheel devices.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod types;
pub mod joystick;
pub mod pedals;

pub use types::*;
pub use joystick::*;
pub use pedals::*;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CalibrationError {
    #[error("Invalid calibration data")]
    InvalidData,
    
    #[error("Calibration not complete")]
    NotComplete,
    
    #[error("Device error: {0}")]
    DeviceError(String),
}

pub type CalibrationResult<T> = Result<T, CalibrationError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_types() {
        let err = CalibrationError::InvalidData;
        assert_eq!(format!("{}", err), "Invalid calibration data");
        
        let err = CalibrationError::NotComplete;
        assert_eq!(format!("{}", err), "Calibration not complete");
    }
}
