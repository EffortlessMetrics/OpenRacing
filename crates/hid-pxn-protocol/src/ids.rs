//! PXN USB vendor and product ID constants.
//!
//! PXN is a Chinese gaming peripheral manufacturer producing direct-drive
//! and belt-driven force feedback racing wheels.  Their USB devices enumerate
//! under VID `0x11FF`, registered to **Lite Star** in the Linux kernel
//! (`USB_VENDOR_ID_LITE_STAR`).
//!
//! ## Verification status: ✅ Fully verified (web-verified 2025-07)
//!
//! All VID/PID values are confirmed by multiple independent sources:
//!
//! Sources:
//! - **Linux kernel `hid-ids.h`** (mainline, merged for ≥6.15):
//!   `USB_VENDOR_ID_LITE_STAR = 0x11ff`,
//!   `USB_DEVICE_ID_PXN_V10 = 0x3245`,
//!   `USB_DEVICE_ID_PXN_V12 = 0x1212`,
//!   `USB_DEVICE_ID_PXN_V12_LITE = 0x1112`,
//!   `USB_DEVICE_ID_PXN_V12_LITE_2 = 0x1211`,
//!   `USB_DEVICE_ID_LITE_STAR_GT987 = 0x2141`
//! - **Linux kernel `hid-universal-pidff.c`** (mainline, ≥6.15):
//!   all PXN PIDs in the device table with `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY`
//! - **JacKeTUs/linux-steering-wheels** compatibility table:
//!   PXN V10 = 11ff:3245, V12 = 11ff:1212, V12 Lite = 11ff:1112,
//!   V12 Lite (SE?) = 11ff:1211; all Gold rating, driver `hid-universal-pidff`
//!
//! ### USB-ID database cross-check (2025-07)
//!
//! VID `0x11FF` is not listed in public USB-ID databases (usb-ids.gowdy.us,
//! the-sz.com, devicehunt.com).  The kernel registers it as `LITE_STAR`.
//!
//! Confidence: **High** — kernel mainline defines + `hid-universal-pidff.c`
//! device table + community Gold rating + linux-steering-wheels confirms all PIDs.
//!
//! ## HID protocol notes
//!
//! PXN devices use standard **USB HID PID** (Physical Interface Device) for
//! force feedback, with the `HID_PIDFF_QUIRK_PERIODIC_SINE_ONLY` quirk applied
//! in the kernel driver.  This quirk limits periodic effects to sine waveform
//! only.  PXN devices are plug-and-play on Linux ≥6.15 via `hid-universal-pidff`.

#![deny(static_mut_refs)]

/// PXN / Lite Star USB Vendor ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (mainline ≥6.15,
/// `USB_VENDOR_ID_LITE_STAR = 0x11ff`),
/// JacKeTUs/linux-steering-wheels compatibility table.
pub const VENDOR_ID: u16 = 0x11FF;

/// PXN V10 direct-drive racing wheel product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V10 = 0x3245`),
/// `hid-universal-pidff.c` device table,
/// linux-steering-wheels (Gold rating, `11ff:3245`).
pub const PRODUCT_V10: u16 = 0x3245;

/// PXN V12 direct-drive racing wheel product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V12 = 0x1212`),
/// `hid-universal-pidff.c` device table,
/// linux-steering-wheels (Gold rating, `11ff:1212`).
pub const PRODUCT_V12: u16 = 0x1212;

/// PXN V12 Lite racing wheel product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V12_LITE = 0x1112`),
/// `hid-universal-pidff.c` device table,
/// linux-steering-wheels (Gold rating, `11ff:1112`).
pub const PRODUCT_V12_LITE: u16 = 0x1112;

/// PXN V12 Lite variant (possibly SE edition) product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_PXN_V12_LITE_2 = 0x1211`),
/// `hid-universal-pidff.c` device table,
/// linux-steering-wheels (Gold rating, `11ff:1211`).
pub const PRODUCT_V12_LITE_2: u16 = 0x1211;

/// Lite Star GT987 FF racing wheel product ID.
///
/// Shares the same VID (`0x11FF`) as PXN devices.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_LITE_STAR_GT987 = 0x2141`),
/// `hid-universal-pidff.c` device table,
/// linux-steering-wheels (Gold rating, `11ff:2141`).
pub const PRODUCT_GT987: u16 = 0x2141;

/// Returns `true` if the VID/PID pair identifies a known PXN / Lite Star device.
pub fn is_pxn(vid: u16, pid: u16) -> bool {
    vid == VENDOR_ID
        && matches!(
            pid,
            PRODUCT_V10
                | PRODUCT_V12
                | PRODUCT_V12_LITE
                | PRODUCT_V12_LITE_2
                | PRODUCT_GT987
        )
}

/// Returns the product name for a known PXN / Lite Star PID, or `None`.
pub fn product_name(pid: u16) -> Option<&'static str> {
    match pid {
        PRODUCT_V10 => Some("PXN V10"),
        PRODUCT_V12 => Some("PXN V12"),
        PRODUCT_V12_LITE => Some("PXN V12 Lite"),
        PRODUCT_V12_LITE_2 => Some("PXN V12 Lite (SE)"),
        PRODUCT_GT987 => Some("Lite Star GT987 FF"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_products_recognised() {
        assert!(is_pxn(VENDOR_ID, PRODUCT_V10));
        assert!(is_pxn(VENDOR_ID, PRODUCT_V12));
        assert!(is_pxn(VENDOR_ID, PRODUCT_V12_LITE));
        assert!(is_pxn(VENDOR_ID, PRODUCT_V12_LITE_2));
        assert!(is_pxn(VENDOR_ID, PRODUCT_GT987));
    }

    #[test]
    fn unknown_product_not_recognised() {
        assert!(!is_pxn(VENDOR_ID, 0x0001));
        assert!(!is_pxn(0x0000, PRODUCT_V10));
    }

    #[test]
    fn product_names() {
        assert_eq!(product_name(PRODUCT_V10), Some("PXN V10"));
        assert_eq!(product_name(PRODUCT_V12), Some("PXN V12"));
        assert_eq!(product_name(PRODUCT_V12_LITE), Some("PXN V12 Lite"));
        assert_eq!(product_name(PRODUCT_V12_LITE_2), Some("PXN V12 Lite (SE)"));
        assert_eq!(product_name(PRODUCT_GT987), Some("Lite Star GT987 FF"));
        assert_eq!(product_name(0xFFFF), None);
    }
}
