//! Hardware quirks for Asetek SimSports devices.
//!
//! These flags describe known hardware behaviours that the host driver
//! must handle for a correct plug-and-play experience.

/// Asetek SimSports wheelbases (Forte, Invicta, La Prima, Tony Kanaan Edition)
/// will **infinitely reboot** if the host does not continuously poll them via
/// HID interrupt transfers.
///
/// On Linux the `HID_QUIRK_ALWAYS_POLL` quirk (bitmask `0x0400`) must be set.
/// Add the following to the kernel command line for each PID (substituting the
/// hex PID of the connected device):
///
/// ```text
/// usbhid.quirks=0x2433:0xF300:0x0400   # Invicta
/// usbhid.quirks=0x2433:0xF301:0x0400   # Forte
/// usbhid.quirks=0x2433:0xF303:0x0400   # La Prima
/// usbhid.quirks=0x2433:0xF306:0x0400   # Tony Kanaan Edition
/// ```
///
/// A udev/hwdb rule that applies this quirk automatically is shipped in
/// `packaging/linux/99-racing-wheel-suite.rules`.
///
/// On Windows the OS polls all HID devices by default; no workaround is needed.
///
/// Source: JacKeTUs/linux-steering-wheels compatibility table, footnote [^13].
pub const REQUIRES_ALWAYS_POLL_LINUX: bool = true;
