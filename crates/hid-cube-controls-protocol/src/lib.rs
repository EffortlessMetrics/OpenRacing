//! Cube Controls steering wheel USB HID protocol constants.
//!
//! Cube Controls S.r.l. (Italy) makes premium sim-racing steering wheels
//! including the GT Pro, Formula Pro, and CSX3. These present a standard
//! USB HID PID (force feedback) interface.
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
