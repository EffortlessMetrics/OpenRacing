//! OpenFFBoard USB HID protocol constants and command encoders.
//!
//! OpenFFBoard is an open-source direct-drive wheel controller from
//! <https://github.com/Ultrawipf/OpenFFBoard>. It uses standard HID PID
//! (Physical Interface Device) force effects with a custom command layer.
//!
//! # VID / PID
//! - Vendor ID: 0x1209 (pid.codes open hardware)
//! - Product IDs: 0xFFB0 (main firmware), 0xFFB1 (alternate)
//!
//! # Protocol Overview
//! OpenFFBoard implements standard USB HID PID, making it compatible with
//! DirectInput on Windows and evdev on Linux. Force effects are sent as
//! standard HID PID reports. Custom configuration uses vendor-defined HID
//! report IDs on the same USB interface.
//!
//! # Torque command
//! Constant force (report ID 0x01) carries a signed 16-bit torque value
//! in the range [-10000, 10000], where ±10000 corresponds to full scale.
//!
//! # Sources
//! - <https://github.com/Ultrawipf/OpenFFBoard>
//! - OpenFFBoard wiki: hardware configuration, firmware protocol docs

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;
pub mod output;

pub use ids::{
    is_openffboard_product, OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT,
    OPENFFBOARD_VENDOR_ID,
};
pub use output::{
    build_enable_ffb, build_set_gain, OpenFFBoardTorqueEncoder, CONSTANT_FORCE_REPORT_ID,
    CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
};
