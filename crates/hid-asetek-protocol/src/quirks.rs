//! Hardware quirks for Asetek SimSports devices.
//!
//! These flags describe known hardware behaviours that the host driver
//! must handle for a correct plug-and-play experience.

/// Asetek SimSports wheelbases (Forte, Invicta, La Prima, Tony Kanaan Edition)
/// will **infinitely reboot** if the host does not continuously poll them via
/// HID interrupt transfers.
///
/// On Linux the `HID_QUIRK_ALWAYS_POLL` quirk (bitmask `0x0400`) must be set.
/// The easiest way is via a `modprobe.d` configuration file; the file shipped at
/// `packaging/linux/90-racing-wheel-quirks.conf` (and installed by `install.sh`)
/// sets the required `usbhid.quirks` module parameter for all four Asetek PIDs.
/// A reboot (or manual `usbhid` module reload) is required after installation.
///
/// If installing manually, add to `/etc/modprobe.d/`:
/// ```text
/// options usbhid quirks=0x2433:0xF300:0x0400,0x2433:0xF301:0x0400,0x2433:0xF303:0x0400,0x2433:0xF306:0x0400
/// ```
///
/// On Windows the OS polls all HID devices by default; no workaround is needed.
///
/// Source: JacKeTUs/linux-steering-wheels compatibility table (footnote 13).
pub const REQUIRES_ALWAYS_POLL_LINUX: bool = true;
