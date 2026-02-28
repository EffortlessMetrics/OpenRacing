//! Leo Bodnar USB vendor and product ID constants.
//!
//! VID `0x1DD2` is assigned to Leo Bodnar Electronics Ltd (UK).
//!
//! Sources: USB ID databases (devicehunt.com, the-sz.com, linux-hardware.org),
//! USB device captures, linux-steering-wheels compatibility table,
//! community reports, and the existing OpenRacing engine vendor list.

/// Leo Bodnar USB Vendor ID.
pub const VENDOR_ID: u16 = 0x1DD2;

// ── Confirmed product IDs ────────────────────────────────────────────────────

/// USB Joystick – generic input-only joystick interface (confirmed).
pub const PID_USB_JOYSTICK: u16 = 0x0001;

/// BBI-32 Button Box Interface – 32-button input-only device (confirmed).
pub const PID_BBI32: u16 = 0x000C;

/// USB Sim Racing Wheel Interface – HID PID force feedback wheel (confirmed).
pub const PID_WHEEL_INTERFACE: u16 = 0x000E;

/// Leo Bodnar FFB Joystick – force feedback joystick with direct drive.
pub const PID_FFB_JOYSTICK: u16 = 0x000F;

/// SLI-M Shift Light Indicator – output/display device.
///
/// **WARNING**: PID `0xBEEF` is a well-known hex magic number and is almost
/// certainly a placeholder rather than a real USB product ID.  Leo Bodnar's
/// product catalogue (leobodnar.com, checked 2025-06) lists "SLI-Pro" and
/// "SLI-F1" shift light indicators but **no** product called "SLI-M".
/// The SLI-F1 product page states it enumerates as "SLI-Pro" on USB.
/// devicehunt.com lists zero product IDs for VID `0x1DD2`.
/// Treat this value as **unverified** until confirmed by a real USB device
/// capture.
///
/// TODO(web-verify): Replace PID 0xBEEF with the real PID once a USB
/// capture from an SLI-Pro or SLI-F1 device is obtained. Consider
/// renaming the constant and enum variant to match the actual product.
pub const PID_SLI_M: u16 = 0xBEEF;

// ── Estimated product IDs (from community USB captures) ─────────────────────

/// BU0836A – 12-bit joystick interface (8 analog axes, 32 buttons).
/// PID estimated from community USB device reports; not independently
/// confirmed from an official source.
pub const PID_BU0836A: u16 = 0x000B;

/// BU0836X – 12-bit joystick interface with push-in wire connectors
/// (8 analog axes, 32 buttons). PID estimated from community USB device
/// reports; not independently confirmed from an official source.
pub const PID_BU0836X: u16 = 0x0030;

/// BU0836 16-bit – high-resolution joystick interface (16-bit ADC,
/// 8 analog axes, 32 buttons). PID estimated from community USB device
/// reports; not independently confirmed from an official source.
pub const PID_BU0836_16BIT: u16 = 0x0031;

/// Returns `true` if the VID/PID pair identifies a Leo Bodnar device.
pub fn is_leo_bodnar(vid: u16, pid: u16) -> bool {
    vid == VENDOR_ID
        && matches!(
            pid,
            PID_USB_JOYSTICK
                | PID_BBI32
                | PID_WHEEL_INTERFACE
                | PID_FFB_JOYSTICK
                | PID_SLI_M
                | PID_BU0836A
                | PID_BU0836X
                | PID_BU0836_16BIT
        )
}

/// Returns `true` if `pid` is a known Leo Bodnar product ID (VID not checked).
pub fn is_leo_bodnar_device(pid: u16) -> bool {
    matches!(
        pid,
        PID_USB_JOYSTICK
            | PID_BBI32
            | PID_WHEEL_INTERFACE
            | PID_FFB_JOYSTICK
            | PID_SLI_M
            | PID_BU0836A
            | PID_BU0836X
            | PID_BU0836_16BIT
    )
}

/// Returns `true` if `pid` identifies a Leo Bodnar FFB-capable device.
pub fn is_leo_bodnar_ffb_pid(pid: u16) -> bool {
    matches!(pid, PID_WHEEL_INTERFACE | PID_FFB_JOYSTICK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_pids_recognised() {
        assert!(is_leo_bodnar(VENDOR_ID, PID_USB_JOYSTICK));
        assert!(is_leo_bodnar(VENDOR_ID, PID_BBI32));
        assert!(is_leo_bodnar(VENDOR_ID, PID_WHEEL_INTERFACE));
        assert!(is_leo_bodnar(VENDOR_ID, PID_FFB_JOYSTICK));
        assert!(is_leo_bodnar(VENDOR_ID, PID_SLI_M));
    }

    #[test]
    fn estimated_pids_recognised() {
        assert!(is_leo_bodnar(VENDOR_ID, PID_BU0836A));
        assert!(is_leo_bodnar(VENDOR_ID, PID_BU0836X));
        assert!(is_leo_bodnar(VENDOR_ID, PID_BU0836_16BIT));
    }

    #[test]
    fn wrong_vid_not_recognised() {
        assert!(!is_leo_bodnar(0x0000, PID_WHEEL_INTERFACE));
        assert!(!is_leo_bodnar(0x16D0, PID_WHEEL_INTERFACE)); // Simucube VID
    }

    #[test]
    fn unknown_pid_not_recognised() {
        assert!(!is_leo_bodnar(VENDOR_ID, 0xFFFF));
        assert!(!is_leo_bodnar_device(0xFFFF));
    }

    #[test]
    fn ffb_pids_identified_correctly() {
        assert!(is_leo_bodnar_ffb_pid(PID_WHEEL_INTERFACE));
        assert!(is_leo_bodnar_ffb_pid(PID_FFB_JOYSTICK));
        assert!(!is_leo_bodnar_ffb_pid(PID_BBI32));
        assert!(!is_leo_bodnar_ffb_pid(PID_SLI_M));
        assert!(!is_leo_bodnar_ffb_pid(PID_USB_JOYSTICK));
    }
}
