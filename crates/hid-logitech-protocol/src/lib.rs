//! Logitech HID protocol: native mode init, input parsing, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested without hardware.

#![deny(static_mut_refs)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::{product_ids, LOGITECH_VENDOR_ID};
pub use input::{parse_input_report, LogitechInputState};
pub use output::{
    build_gain_report, build_native_mode_report, build_set_autocenter_report,
    build_set_leds_report, build_set_range_report, LogitechConstantForceEncoder,
    CONSTANT_FORCE_REPORT_LEN, VENDOR_REPORT_LEN,
};
pub use types::{is_wheel_product, LogitechModel};
