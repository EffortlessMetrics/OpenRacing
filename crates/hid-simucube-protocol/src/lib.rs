//! HID protocol implementation for Simucube direct drive wheelbases
//!
//! This crate provides the protocol implementation for Simucube wheelbases:
//! - Simucube 2 Sport
//! - Simucube 2 Pro
//! - Simucube 2 Ultimate
//! - Simucube ActivePedal
//!
//! ## Protocol Notes
//!
//! **Important:** Simucube wheelbases use the **standard USB HID PID (Physical
//! Interface Device)** protocol for force feedback — *not* a custom binary
//! torque-streaming format. On Windows this maps to DirectInput; on Linux the
//! `hid-pidff` kernel driver handles it.
//!
//! The input report is a standard HID joystick report with a 16-bit unsigned
//! X axis (steering), Y axis, 6 additional axes, and up to 128 buttons.
//! The internal 22-bit encoder resolution is **not** exposed over USB.
//!
//! The output (FFB) side is effect-based: applications upload structured PID
//! effect descriptors (Constant, Spring, Damper, Sine, etc.) which the device
//! firmware executes autonomously. There is no direct torque-streaming API.
//!
//! Rotation range is configured via Simucube True Drive / Tuner software and
//! is **not** settable via the USB protocol.
//!
//! ### Current implementation status
//!
//! The `input` and `output` modules currently use a **placeholder** binary
//! layout that does not match the actual HID PID wire format. The data
//! structures capture the correct conceptual fields (torque, angle, effects)
//! but the byte-level encoding is speculative. PIDs, VID, torque specs, and
//! model classification are verified from Simucube developer documentation.
//!
//! Source: <https://github.com/Simucube/simucube-docs.github.io>
//!
//! ## Features
//! - Up to 32Nm torque (Ultimate)
//! - Standard USB HID PID force feedback
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

/// Errors returned by Simucube protocol operations.
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

/// Convenience result alias for Simucube operations.
pub type SimucubeResult<T> = Result<T, SimucubeError>;

impl From<HidCommonError> for SimucubeError {
    fn from(e: HidCommonError) -> Self {
        SimucubeError::Communication(e.to_string())
    }
}

/// Simucube / Granite Devices USB Vendor ID (`0x16D0`).
pub const VENDOR_ID: u16 = 0x16D0;
/// Product ID for Simucube 2 Sport.
pub const PRODUCT_ID_SPORT: u16 = 0x0D61;
/// Product ID for Simucube 2 Pro.
pub const PRODUCT_ID_PRO: u16 = 0x0D60;
/// Product ID for Simucube 2 Ultimate.
pub const PRODUCT_ID_ULTIMATE: u16 = 0x0D5F;

/// HID input report size in bytes.
pub const REPORT_SIZE_INPUT: usize = 64;
/// HID output report size in bytes.
pub const REPORT_SIZE_OUTPUT: usize = 64;

/// Default maximum torque (Nm) used when model is unknown.
pub const MAX_TORQUE_NM: f32 = 25.0;
/// Maximum torque (Nm) for Simucube 2 Sport.
pub const MAX_TORQUE_SPORT: f32 = 17.0;
/// Maximum torque (Nm) for Simucube 2 Pro.
pub const MAX_TORQUE_PRO: f32 = 25.0;
/// Maximum torque (Nm) for Simucube 2 Ultimate.
pub const MAX_TORQUE_ULTIMATE: f32 = 32.0;

/// Angle sensor resolution in bits.
pub const ANGLE_SENSOR_BITS: u32 = 22;
/// Maximum angle sensor value (`2^22 - 1`).
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
