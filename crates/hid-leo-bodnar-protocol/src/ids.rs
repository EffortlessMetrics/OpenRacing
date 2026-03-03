//! Leo Bodnar USB vendor and product ID constants.
//!
//! VID `0x1DD2` is assigned to Leo Bodnar Electronics Ltd (UK).
//!
//! ## Web-verification status (2025-07, updated 2026-03)
//!
//! ### VID confirmation
//! - the-sz.com/products/usbid: VID `0x1DD2` = "LEO BODNAR" ✅
//! - devicehunt.com: VID `0x1DD2` = "Leo Bodnar Electronics Ltd" (no PIDs listed) ✅
//! - usb-ids.gowdy.us: VID listed, minimal content (no product IDs)
//!
//! ### PID confirmation
//! - **Not in mainline Linux kernel `hid-ids.h`** — Leo Bodnar has no
//!   dedicated HID driver in the kernel (devices use generic `hid-pidff`).
//! - **Not in JacKeTUs/linux-steering-wheels** compatibility table.
//! - JacKeTUs/simracing-hwdb `90-leo-bodnar.hwdb`:
//!   - Pedals controller: `v1DD2p100C` (PID `0x100C`) — ✅ **now tracked**
//!   - LC Pedals controller: `v1DD2p22D0` (PID `0x22D0`) — ✅ **now tracked**
//! - No public USB-IF product ID database lists Leo Bodnar PIDs.
//!
//! Confidence: VID = **High** (USB-IF registered). Confirmed PIDs (`0x0001`,
//! `0x000C`, `0x000E`, `0x000F`) = **Medium** (community captures, no official
//! registry). simracing-hwdb PIDs (`0x100C`, `0x22D0`) = **Medium** (1 community
//! source). Estimated PIDs (`0x000B`, `0x0030`, `0x0031`, `0x1301`) = **Low**.
//!
//! Sources: USB ID databases (devicehunt.com, the-sz.com), USB device captures,
//! JacKeTUs/simracing-hwdb, community reports, and the existing OpenRacing
//! engine vendor list.

/// Leo Bodnar USB Vendor ID.
///
/// ✅ Confirmed: the-sz.com ("LEO BODNAR"), devicehunt.com ("Leo Bodnar Electronics Ltd").
/// Web-verified 2025-07.
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

/// SLI-Pro Shift Light Indicator – output/display device with button inputs.
///
/// PID `0x1301` is a community estimate from USB device reports and the
/// OpenFlight compatibility database.  The previous value (`0xBEEF`) was a
/// well-known hex magic-number placeholder.
///
/// Leo Bodnar's product catalogue (leobodnar.com, checked 2025-06) lists
/// "SLI-Pro" and "SLI-F1" shift light indicators — **no** product called
/// "SLI-M" exists.  The SLI-F1 enumerates as "SLI-Pro" on USB.
/// devicehunt.com lists zero product IDs for VID `0x1DD2`, so no
/// authoritative USB-IF confirmation is available.
///
/// Treat this value as **estimated** until confirmed by a real USB device
/// capture.  The constant name `PID_SLI_M` is retained for backward
/// compatibility; the actual product is the SLI-Pro.
///
/// Source: OpenFlight compat/devices/leo-bodnar/sli-pro.yaml (community).
pub const PID_SLI_M: u16 = 0x1301;

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

// ── Community-confirmed product IDs (simracing-hwdb) ────────────────────────

/// Leo Bodnar Pedals Controller.
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb `90-leo-bodnar.hwdb` — VID 0x1DD2, PID 0x100C.
pub const PID_PEDALS: u16 = 0x100C;

/// Leo Bodnar LC (Load Cell) Pedals Controller.
///
/// 🔶 Community-sourced: JacKeTUs/simracing-hwdb `90-leo-bodnar.hwdb` — VID 0x1DD2, PID 0x22D0.
pub const PID_LC_PEDALS: u16 = 0x22D0;

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
                | PID_PEDALS
                | PID_LC_PEDALS
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
            | PID_PEDALS
            | PID_LC_PEDALS
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
    fn simracing_hwdb_pids_recognised() {
        assert!(is_leo_bodnar(VENDOR_ID, PID_PEDALS));
        assert!(is_leo_bodnar(VENDOR_ID, PID_LC_PEDALS));
        assert!(is_leo_bodnar_device(PID_PEDALS));
        assert!(is_leo_bodnar_device(PID_LC_PEDALS));
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
