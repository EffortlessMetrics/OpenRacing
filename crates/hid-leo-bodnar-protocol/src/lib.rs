//! HID protocol constants and device classification for Leo Bodnar USB interfaces.
//!
//! Leo Bodnar Electronics Ltd is a UK manufacturer of high-quality sim racing and
//! DIY USB interfaces. VID `0x1DD2`.
//!
//! ## Products covered
//! - BBI-32 Button Box (32-button input device)
//! - BU0836A / BU0836X / BU0836 16-bit joystick interfaces (8 axes, 32 buttons)
//! - USB Joystick (generic HID joystick)
//! - USB Sim Racing Wheel Interface (HID PID force feedback)
//! - FFB Joystick (direct drive force feedback joystick)
//! - SLI-M Shift Light Indicator (RPM / gear display)
//!
//! ## Design
//! This crate is intentionally I/O-free and allocation-free. It provides only
//! constants, pure functions, and enums that can be used and tested without
//! hardware access.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod report;
pub mod types;

pub use ids::{
    VENDOR_ID, PID_BBI32, PID_BU0836A, PID_BU0836X, PID_BU0836_16BIT, PID_FFB_JOYSTICK,
    PID_SLI_M, PID_USB_JOYSTICK, PID_WHEEL_INTERFACE, is_leo_bodnar, is_leo_bodnar_device,
    is_leo_bodnar_ffb_pid,
};
pub use report::{
    HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, WHEEL_DEFAULT_MAX_TORQUE_NM, WHEEL_ENCODER_CPR,
};
pub use types::LeoBodnarDevice;

#[cfg(test)]
mod tests;
