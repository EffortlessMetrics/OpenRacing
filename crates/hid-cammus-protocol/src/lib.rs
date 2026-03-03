//! Cammus C5/C12 direct drive wheel HID protocol implementation.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested without hardware.
//!
//! ## Wire-format verification status (2025-07)
//!
//! **VID/PID:** ✅ VID `0x3416` and PIDs `0x0301` (C5), `0x0302` (C12) confirmed in
//! the Linux kernel upstream (`torvalds/linux`, `drivers/hid/hid-ids.h`) and the
//! `hid-universal-pidff.c` driver table. Cross-referenced with JacKeTUs/linux-steering-wheels
//! (Platinum rating) and JacKeTUs/simracing-hwdb. Pedal PIDs `0x1018` (CP5) and
//! `0x1019` (LC100) confirmed via simracing-hwdb.
//!
//! **FFB protocol:** Cammus devices use standard **USB HID PID** for force feedback.
//! The firmware omits the `0xa7` (effect delay) HID descriptor field, which required
//! a kernel patch for Linux < 6.15; fixed natively in Linux 6.15 via `hid-universal-pidff`.
//! No vendor-specific quirk flags are applied in the kernel driver.
//!
//! **Input/output wire format:** The report layouts in `report.rs` (64-byte input,
//! report ID 0x01, i16 LE steering ±32767) and `direct.rs` (8-byte output, report ID
//! 0x01, i16 LE torque ±0x7FFF) are **internally estimated**. No community USB
//! descriptor dump or open-source driver documents the exact byte layout.
//!
//! **No open-source FFB wire-format documentation exists** for Cammus at this time.
//! Do not change byte layout without a USB capture from real hardware.

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
