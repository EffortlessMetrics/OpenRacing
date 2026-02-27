//! HID protocol constants and device classification for SimExperience AccuForce wheelbases.
//!
//! The SimExperience AccuForce Pro is a brushless direct drive wheelbase that
//! exposes a standard USB HID PID (force feedback) interface.
//!
//! Confirmed VID/PID values (source: community USB device captures and
//! RetroBat emulator launcher Wheels.cs, commit 0a54752):
//! - VID `0x1FC9` (NXP Semiconductors — USB chip used internally)
//! - AccuForce Pro PID `0x804C`
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

pub use ids::{VENDOR_ID, PID_ACCUFORCE_PRO, is_accuforce, is_accuforce_pid};
pub use report::{HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, RECOMMENDED_B_INTERVAL_MS};
pub use types::{AccuForceModel, DeviceInfo};

#[cfg(test)]
mod tests;
