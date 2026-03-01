//! HID protocol implementation for Simucube direct drive wheelbases.
//!
//! This crate provides the protocol implementation for Simucube wheelbases:
//! - Simucube 1 (IONI servo drive)
//! - Simucube 2 Sport (17 Nm)
//! - Simucube 2 Pro (25 Nm)
//! - Simucube 2 Ultimate (32 Nm)
//! - Simucube ActivePedal (via SC-Link Hub)
//!
//! ## Protocol Notes
//!
//! **Important:** Simucube wheelbases use the **standard USB HID PID (Physical
//! Interface Device)** protocol for force feedback — *not* a custom binary
//! torque-streaming format. On Windows this maps to DirectInput; on Linux the
//! `hid-pidff` kernel driver handles it.
//!
//! ### Input report (documented)
//!
//! Per the official Simucube USB interface documentation, the HID input report
//! is a standard joystick report containing:
//!
//! | Field | Type | Description |
//! |-------|------|-------------|
//! | X axis (steering) | `u16` (0–65535) | Wheel position |
//! | Y axis | `u16` | Center-idle; user-mappable to pedal/handbrake |
//! | 6 additional axes | `u16` each | Pedals, handbrakes, wireless wheel analogs |
//! | 128 buttons | bitfield | Physical buttons + SimuCube Wireless Wheel |
//!
//! The internal 22-bit encoder resolution is **not** exposed over USB — only
//! a 16-bit unsigned axis is available to applications.
//!
//! ### Output report (FFB)
//!
//! The output (FFB) side is effect-based: applications upload structured PID
//! effect descriptors (Constant, Spring, Damper, Sine, etc.) which the device
//! firmware executes autonomously. There is no direct torque-streaming API.
//!
//! Rotation range is configured via Simucube True Drive / Tuner software and
//! is **not** settable via the USB protocol.
//!
//! ### Implementation status
//!
//! - **`ids`**: Verified from official Simucube developer docs and community
//!   sources (linux-steering-wheels, Granite Devices wiki).
//! - **`input`**: [`SimucubeHidReport`] implements the documented HID joystick
//!   layout. [`SimucubeInputReport`] retains a speculative extended format for
//!   internal diagnostics; its wire encoding is **not verified**.
//! - **`output`**: The builder produces a placeholder wire format. Real FFB
//!   uses USB HID PID effect descriptors per the PID 1.01 specification.
//!
//! ## Sources
//!
//! - Official Simucube developer docs:
//!   <https://github.com/Simucube/simucube-docs.github.io> →
//!   `docs/Simucube 2/Developers.md`
//! - Granite Devices wiki (Linux udev rules, firmware):
//!   <https://granitedevices.com/wiki/Using_Simucube_wheel_base_in_Linux>
//! - JacKeTUs/linux-steering-wheels compatibility table:
//!   <https://github.com/JacKeTUs/linux-steering-wheels>
//! - USB HID PID specification:
//!   <https://www.usb.org/sites/default/files/documents/pid1_01.pdf>
//!
//! ## Features
//! - Up to 32 Nm torque (Ultimate)
//! - Standard USB HID PID force feedback
//! - Wireless wheel support (SimuCube Wireless Wheel)
//! - Active pedal support (via SC-Link Hub)

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]
#![deny(static_mut_refs)]

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

/// Internal angle sensor resolution in bits (not exposed over USB).
/// Over USB the steering axis is a standard 16-bit unsigned value (0–65535).
pub const ANGLE_SENSOR_BITS: u32 = 22;
/// Maximum internal angle sensor value (`2^22 - 1`).
pub const ANGLE_SENSOR_MAX: u32 = (1 << ANGLE_SENSOR_BITS) - 1;

/// Number of axes in the standard HID joystick report beyond X and Y.
///
/// Source: Official Simucube developer docs — `Simucube/simucube-docs.github.io`
/// → `docs/Simucube 2/Developers.md`.
pub const HID_ADDITIONAL_AXES: usize = 6;

/// Total number of buttons exposed by the HID joystick report.
///
/// Source: Official Simucube developer docs.
pub const HID_BUTTON_COUNT: usize = 128;

/// Size of the button bitfield in bytes (`HID_BUTTON_COUNT / 8`).
pub const HID_BUTTON_BYTES: usize = HID_BUTTON_COUNT / 8;

/// Minimum size of the documented HID joystick input report in bytes.
///
/// Layout: 8 axes × 2 bytes + 128 buttons / 8 = 32 bytes.
pub const HID_JOYSTICK_REPORT_MIN_BYTES: usize = 8 * 2 + HID_BUTTON_BYTES;

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
    fn test_hid_constants() {
        assert_eq!(HID_ADDITIONAL_AXES, 6);
        assert_eq!(HID_BUTTON_COUNT, 128);
        assert_eq!(HID_BUTTON_BYTES, 16);
        assert_eq!(HID_JOYSTICK_REPORT_MIN_BYTES, 32);
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
