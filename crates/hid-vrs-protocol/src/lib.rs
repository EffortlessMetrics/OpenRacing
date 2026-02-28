//! VRS DirectForce Pro HID protocol: input parsing, device identification, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.
//!
//! Supports VRS DirectForce Pro wheelbases using standard HID PIDFF protocol.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod input;
pub mod output;
pub mod quirks;
pub mod types;

pub use ids::{VRS_PRODUCT_ID, VRS_VENDOR_ID, product_ids};
pub use input::{VrsInputState, parse_input_report};
pub use output::{
    CONSTANT_FORCE_REPORT_LEN, DAMPER_REPORT_LEN, FRICTION_REPORT_LEN, SPRING_REPORT_LEN,
    VrsConstantForceEncoder, VrsDamperEncoder, VrsFrictionEncoder, VrsSpringEncoder,
    build_device_gain, build_ffb_enable, build_rotation_range,
};
pub use types::{
    VrsDeviceIdentity, VrsFfbEffectType, VrsPedalAxes, VrsPedalAxesRaw, identify_device,
    is_wheelbase_product,
};
