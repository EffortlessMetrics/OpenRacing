//! Thrustmaster HID protocol: report parsing, initialization, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod ids;
pub mod input;
pub mod output;
pub mod protocol;
pub mod types;

pub use ids::THRUSTMASTER_VENDOR_ID;
pub use ids::{Model, product_ids};
pub use input::{ThrustmasterInputState, parse_input_report};
pub use output::{
    EFFECT_REPORT_LEN, ThrustmasterConstantForceEncoder, ThrustmasterEffectEncoder,
    build_actuator_enable, build_damper_effect, build_device_gain, build_friction_effect,
    build_set_range_report, build_spring_effect,
};
pub use protocol::{ThrustmasterInitState, ThrustmasterProtocol};
pub use types::{
    ThrustmasterDeviceCategory, ThrustmasterDeviceIdentity, ThrustmasterPedalAxes,
    ThrustmasterPedalAxesRaw, identify_device, is_pedal_product, is_wheel_product,
};
