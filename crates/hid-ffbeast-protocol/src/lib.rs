//! FFBeast USB HID protocol constants and command encoders.
//!
//! FFBeast is an open-source direct-drive force feedback controller from
//! <https://github.com/HF-Robotics/FFBeast>. It uses standard HID PID
//! (Physical Interface Device) force effects compatible with DirectInput.
//!
//! # VID / PID
//! - Vendor ID: 0x045B (`USB_VENDOR_ID_FFBEAST` in the Linux kernel)
//! - Product IDs: 0x58F9 (joystick), 0x5968 (rudder), 0x59D7 (wheel)
//!
//! # Protocol Overview
//! FFBeast implements standard USB HID PID, making it compatible with
//! DirectInput on Windows and evdev on Linux. Force effects are sent as
//! standard HID PID reports. Custom configuration uses vendor-defined HID
//! report IDs on the same USB interface.
//!
//! # Torque command
//! Constant force (report ID 0x01) carries a signed 16-bit torque value
//! in the range [-10000, 10000], where ±10000 corresponds to full scale.
//!
//! # Sources
//! - <https://github.com/HF-Robotics/FFBeast>
//! - Linux kernel `hid-ids.h` (`USB_VENDOR_ID_FFBEAST`)

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]

pub mod ids;
pub mod output;

pub use ids::{
    is_ffbeast_product, FFBEAST_PRODUCT_ID_JOYSTICK, FFBEAST_PRODUCT_ID_RUDDER,
    FFBEAST_PRODUCT_ID_WHEEL, FFBEAST_VENDOR_ID,
};
pub use output::{
    build_enable_ffb, build_set_gain, FFBeastTorqueEncoder, CONSTANT_FORCE_REPORT_ID,
    CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
};
