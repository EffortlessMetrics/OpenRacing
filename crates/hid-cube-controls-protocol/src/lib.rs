//! Cube Controls steering wheel USB HID protocol constants.
//!
//! Cube Controls S.r.l. (Italy) produces premium sim-racing **steering wheels**
//! (button boxes / rims) including the GT Pro, Formula CSX-3, and F-CORE. These
//! are **input-only** USB/Bluetooth HID devices (buttons, rotary encoders,
//! paddles). They do **not** produce force feedback — FFB comes from the
//! wheelbase (a separate device by another vendor such as Simucube).
//!
//! # VID/PID status — PROVISIONAL
//!
//! The USB VID and PIDs for Cube Controls devices have **not** been confirmed
//! from official documentation or independent USB descriptor captures at the
//! time of writing. The values below are provisional best-guesses based on
//! community reports that place Cube Controls hardware on the STMicroelectronics
//! shared VID (0x0483). The JacKeTUs/linux-steering-wheels compatibility table
//! (the primary community reference) has no Cube Controls entries.
//!
//! See `docs/protocols/SOURCES.md` — "Devices Under Investigation".
//!
//! Once confirmed (e.g. from a USB device tree capture on real hardware),
//! update the constants in [`ids`] and remove the provisional annotations.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(static_mut_refs)]
#![deny(clippy::unwrap_used)]

pub mod ids;

pub use ids::{
    CUBE_CONTROLS_CSX3_PID, CUBE_CONTROLS_FORMULA_PRO_PID, CUBE_CONTROLS_GT_PRO_PID,
    CUBE_CONTROLS_VENDOR_ID, CubeControlsModel, is_cube_controls_product,
};
