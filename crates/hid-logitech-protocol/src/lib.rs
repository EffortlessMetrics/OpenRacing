//! Logitech HID protocol: native mode init, input parsing, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested without hardware.

#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::{LOGITECH_VENDOR_ID, product_ids};
pub use input::{LogitechInputState, parse_input_report};
pub use output::{
    CONSTANT_FORCE_REPORT_LEN, LogitechConstantForceEncoder, VENDOR_REPORT_LEN, build_gain_report,
    build_mode_switch_report, build_native_mode_report, build_set_autocenter_report,
    build_set_leds_report, build_set_range_dfp_report, build_set_range_dfp_reports,
    build_set_range_report,
};
pub use types::{LogitechModel, is_wheel_product};
