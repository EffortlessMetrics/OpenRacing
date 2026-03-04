//! Device calibration utilities
//!
//! This crate provides calibration helpers for racing wheel devices.
//!
//! # Calibration Profile Creation
//!
//! ```
//! use openracing_calibration::{DeviceCalibration, AxisCalibration};
//!
//! // Create a device calibration with 3 axes (steering, throttle, brake)
//! let mut device = DeviceCalibration::new("Fanatec CSL DD", 3);
//!
//! // Configure the steering axis with a center point
//! if let Some(steering) = device.axis(0) {
//!     *steering = AxisCalibration::new(0, 65535).with_center(32768);
//! }
//!
//! assert_eq!(device.axes.len(), 3);
//! assert_eq!(device.name, "Fanatec CSL DD");
//! ```
//!
//! # Value Mapping
//!
//! ```
//! use openracing_calibration::AxisCalibration;
//!
//! let calib = AxisCalibration::new(0, 65535);
//!
//! // Raw values map linearly to normalized [0.0, 1.0]
//! assert!((calib.apply(0) - 0.0).abs() < 0.01);
//! assert!((calib.apply(32768) - 0.5).abs() < 0.01);
//! assert!((calib.apply(65535) - 1.0).abs() < 0.01);
//! ```
//!
//! # Calibration Application with Dead-Zone
//!
//! ```
//! use openracing_calibration::AxisCalibration;
//!
//! let calib = AxisCalibration::new(0, 65535)
//!     .with_deadzone(1000, 64535);
//!
//! // Values below the dead-zone minimum map to 0.0
//! assert!((calib.apply(500) - 0.0).abs() < 0.01);
//!
//! // Mid-range produces roughly 0.5
//! assert!((calib.apply(32768) - 0.5).abs() < 0.02);
//! ```

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod joystick;
pub mod pedals;
pub mod types;

pub use joystick::*;
pub use pedals::*;
pub use types::*;

use thiserror::Error;

/// Errors that can occur during device calibration.
///
/// # Examples
///
/// ```
/// use openracing_calibration::{CalibrationError, PedalCalibrator};
///
/// // Calibration fails without sufficient samples
/// let cal = PedalCalibrator::new();
/// let result = cal.calibrate();
/// assert!(result.is_err());
///
/// // Error messages are descriptive
/// let err = CalibrationError::DeviceError("timeout".to_string());
/// assert!(err.to_string().contains("timeout"));
/// ```
#[derive(Error, Debug)]
pub enum CalibrationError {
    /// The supplied calibration data is invalid (e.g., min > max).
    #[error("Invalid calibration data")]
    InvalidData,

    /// Not enough samples have been collected to produce a calibration.
    #[error("Calibration not complete")]
    NotComplete,

    /// An underlying device communication error.
    #[error("Device error: {0}")]
    DeviceError(String),
}

/// Convenience alias for calibration operations.
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
