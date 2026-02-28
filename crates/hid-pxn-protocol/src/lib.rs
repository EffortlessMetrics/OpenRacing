//! PXN V10/V12 direct drive wheel HID PIDFF protocol implementation.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested without hardware.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::{
    PRODUCT_GT987_FF, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_SE, VENDOR_ID,
    is_pxn_device, product_name,
};
pub use input::{ParseError, PxnInputReport, REPORT_ID, REPORT_LEN, STEERING_RANGE_DEG, parse};
pub use output::{FFB_REPORT_ID, FFB_REPORT_LEN, encode_stop, encode_torque};
pub use types::PxnModel;
