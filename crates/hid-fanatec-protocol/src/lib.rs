//! Fanatec HID protocol: report parsing, mode switching, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.

#![deny(static_mut_refs)]

pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use ids::{product_ids, FANATEC_VENDOR_ID};
pub use input::{
    FanatecExtendedState, FanatecInputState, parse_extended_report, parse_standard_report,
};
pub use output::{
    FanatecConstantForceEncoder, CONSTANT_FORCE_REPORT_LEN, build_mode_switch_report,
    build_set_gain_report, build_stop_all_report,
};
pub use types::{FanatecModel, is_wheelbase_product};
