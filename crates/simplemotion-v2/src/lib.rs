//! SimpleMotion V2 protocol implementation for Simucube and Open Sim Wheel (OSW) bases.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.
//!
//! Supports IONI and ARGON servo drives using the SimpleMotion V2 protocol over
//! USB HID and RS485 transports.
//!
//! # Key Features
//! - SimpleMotion V2 command encoding/decoding
//! - Torque command for FFB at up to 20kHz
//! - Position and velocity feedback reading
//! - Device enumeration and identification
//! - RS485 and USB HID transport support

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

pub mod commands;
pub mod error;
pub mod ids;
pub mod input;
pub mod output;
pub mod types;

pub use commands::{
    MOTOR_POLE_PAIRS, SmCommand, SmCommandType, SmStatus, build_set_torque_command,
    build_set_torque_command_with_velocity,
};
pub use error::{SmError, SmResult};
pub use ids::{
    ARGON_PRODUCT_ID, ARGON_VENDOR_ID, IONI_PRODUCT_ID, IONI_PRODUCT_ID_PREMIUM, IONI_VENDOR_ID,
    OSW_VENDOR_ID, product_ids as sm_product_ids,
};
pub use input::{SmFeedbackState, SmMotorFeedback, parse_feedback_report};
pub use output::{
    FEEDBACK_REPORT_LEN, SETPARAM_REPORT_LEN, STATUS_REPORT_LEN, TORQUE_COMMAND_LEN,
    TorqueCommandEncoder, build_device_enable, build_get_parameter, build_get_status,
    build_set_parameter, build_set_zero_position,
};
pub use types::{
    SmDeviceCategory, SmDeviceIdentity, identify_device, is_wheelbase_product, sm_device_identity,
};
