//! Cammus C5/C12 direct drive wheel HID protocol implementation.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested without hardware.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod direct;
pub mod ids;
pub mod report;
pub mod types;

pub use direct::{
    FFB_REPORT_ID, FFB_REPORT_LEN, MODE_CONFIG, MODE_GAME, encode_stop, encode_torque,
};
pub use ids::{
    PRODUCT_C5, PRODUCT_C12, PRODUCT_CP5_PEDALS, PRODUCT_LC100_PEDALS, VENDOR_ID, is_cammus,
    product_name,
};
pub use report::{CammusInputReport, ParseError, REPORT_ID, REPORT_LEN, STEERING_RANGE_DEG, parse};
pub use types::CammusModel;
