//! OpenFFBoard USB HID protocol constants and command encoders.
//!
//! OpenFFBoard is an open-source direct-drive wheel controller from
//! <https://github.com/Ultrawipf/OpenFFBoard>. It uses standard HID PID
//! (Physical Interface Device) force effects with a custom command layer.
//!
//! # VID / PID
//! - Vendor ID: 0x1209 (pid.codes open hardware)
//! - Product IDs: 0xFFB0 (main firmware, confirmed), 0xFFB1 (alternate, unverified)
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

pub mod commands;
pub mod effects;
pub mod ids;
pub mod input;
pub mod output;

pub use commands::{
    build_request, build_request_device_id, build_request_fw_version, build_request_hw_type,
    build_reset_device, build_save_config, build_write, CmdType, VendorCommand,
    VENDOR_CMD_REPORT_ID, VENDOR_CMD_REPORT_LEN,
};
pub use effects::{
    encode_block_free, encode_device_control, encode_device_gain, encode_effect_operation,
    encode_set_condition, encode_set_constant_force, encode_set_effect, encode_set_envelope,
    encode_set_periodic, encode_set_ramp_force, parse_block_load, parse_pid_pool, BlockLoadStatus,
    EffectOp, EffectType, DURATION_INFINITE, MAX_EFFECTS,
};
pub use ids::{
    is_openffboard_product, OpenFFBoardVariant, OPENFFBOARD_PRODUCT_ID, OPENFFBOARD_PRODUCT_ID_ALT,
    OPENFFBOARD_VENDOR_ID,
};
pub use input::{OpenFFBoardInputReport, INPUT_REPORT_ID, INPUT_REPORT_LEN};
pub use output::{
    build_enable_ffb, build_set_gain, OpenFFBoardTorqueEncoder, CONSTANT_FORCE_REPORT_ID,
    CONSTANT_FORCE_REPORT_LEN, GAIN_REPORT_ID,
};
