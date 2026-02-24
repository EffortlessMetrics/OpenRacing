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

pub use ids::led_commands;
pub use ids::{FANATEC_VENDOR_ID, product_ids, rim_ids};
pub use input::{
    FanatecExtendedState, FanatecInputState, FanatecPedalState, parse_extended_report,
    parse_pedal_report, parse_standard_report,
};
pub use output::{
    CONSTANT_FORCE_REPORT_LEN, FanatecConstantForceEncoder, LED_REPORT_LEN, build_display_report,
    build_led_report, build_mode_switch_report, build_rumble_report, build_set_gain_report,
    build_stop_all_report,
};
pub use types::{
    FanatecModel, FanatecPedalModel, FanatecRimId, is_pedal_product, is_wheelbase_product,
};
