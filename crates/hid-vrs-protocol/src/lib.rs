//! VRS DirectForce Pro HID protocol: input parsing, device identification, and FFB encoding.
//!
//! This crate is intentionally I/O-free and allocation-free on hot paths.
//! It provides pure functions and types that can be tested and fuzzed without
//! hardware or OS-level HID plumbing.
//!
//! Supports VRS DirectForce Pro wheelbases using standard HID PIDFF protocol.
//!
//! ## Wire-format verification status (2025-07)
//!
//! **VID/PID:** ✅ VID `0x0483` (STMicroelectronics) and PID `0xA355` (DFP) are
//! confirmed in the Linux kernel upstream (`drivers/hid/hid-ids.h` as
//! `USB_VENDOR_ID_VRS` / `USB_DEVICE_ID_VRS_DFP`). PID `0xA44C` (R295) also
//! kernel-confirmed (`USB_DEVICE_ID_VRS_R295`, referenced in `hid-quirks.c`).
//! Pedals PID `0xA3BE` confirmed via JacKeTUs/simracing-hwdb (`v0483pA3BE`).
//! Cross-referenced with linux-steering-wheels (Platinum rating for DFP).
//!
//! **FFB protocol:** VRS uses standard **USB HID PID** (PIDFF). The Linux kernel
//! driver (`hid-universal-pidff.c`) applies `HID_PIDFF_QUIRK_PERMISSIVE_CONTROL`
//! for the DFP. The PIDFF report IDs in `ids.rs::report_ids` (constant force 0x11,
//! spring 0x19, damper 0x1A, friction 0x1B) are consistent with the USB HID PID
//! specification ([pid1_01.pdf](https://www.usb.org/sites/default/files/documents/pid1_01.pdf)).
//!
//! **Input report layout:** ⚠ The 64-byte input report structure in `input.rs`
//! (i16 steering, u16 throttle/brake/clutch, u16 buttons, hat, encoders) is an
//! **internal estimate**. No community USB descriptor dump confirms the exact byte
//! offsets. The kernel driver uses standard HID descriptors for axis mapping.
//!
//! **Output report layout:** The PIDFF output encoders in `output.rs` use standard
//! HID PID magnitude scaling (±10000) which is consistent with the specification,
//! but the vendor-specific set/get report layouts (rotation range, device gain) are
//! **unverified** against USB captures.

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
