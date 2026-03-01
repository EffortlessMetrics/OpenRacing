//! PXN racing wheel USB HID protocol constants.
//!
//! PXN is a Chinese gaming peripheral manufacturer whose devices enumerate
//! under VID `0x11FF` (registered as **Lite Star** in the Linux kernel).
//! Products include the V10 and V12 direct-drive wheelbases, the V12 Lite,
//! and the Lite Star GT987 FF.
//!
//! # VID / PID
//! - Vendor ID: `0x11FF` (`USB_VENDOR_ID_LITE_STAR` in the Linux kernel)
//! - V10: `0x3245`, V12: `0x1212`, V12 Lite: `0x1112` / `0x1211`, GT987: `0x2141`
//!
//! # Protocol Overview
//! PXN devices implement standard **USB HID PID** (Physical Interface Device)
//! for force feedback. The Linux kernel applies
//! `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY`, restricting periodic effects to
//! sine-only waveform.
//!
//! Supported natively on Linux ≥6.15 via `hid-universal-pidff`.
//! On Windows, PXN devices use standard DirectInput HID PID.
//!
//! # Sources
//! - Linux kernel `hid-ids.h` (`USB_VENDOR_ID_LITE_STAR`, PXN PIDs)
//! - Linux kernel `hid-universal-pidff.c` (device table + quirks)
//! - JacKeTUs/linux-steering-wheels compatibility table (Gold rating)

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;

pub use ids::{
    is_pxn, product_name, PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE,
    PRODUCT_V12_LITE_2, VENDOR_ID,
};
