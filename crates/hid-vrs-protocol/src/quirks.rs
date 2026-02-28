//! Hardware quirks for VRS DirectForce Pro devices.
//!
//! These flags describe known hardware behaviours that the host driver
//! may need to work around for a correct plug-and-play experience.

/// Some VRS DirectForce Pro units enter a "power save" state immediately after
/// connecting.  In this state force feedback is silently disabled until the
/// wheel is physically rotated at least a few degrees.  No software workaround
/// is required — FFB will activate automatically once the wheel moves.
///
/// Source: JacKeTUs/linux-steering-wheels compatibility table, footnote [^12].
pub const POWER_SAVE_ON_FIRST_CONNECT: bool = true;

/// VRS devices use standard PIDFF and do **not** require `HID_QUIRK_ALWAYS_POLL`
/// on Linux.  Normal HID enumeration is sufficient.
pub const REQUIRES_ALWAYS_POLL_LINUX: bool = false;
