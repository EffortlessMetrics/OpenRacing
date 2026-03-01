//! Cammus USB vendor and product ID constants.
//!
//! ## Verification status
//!
//! | Field | Status | Source |
//! |-------|--------|--------|
//! | VID 0x3416 | ✅ Confirmed | Linux kernel `hid-ids.h` (`USB_VENDOR_ID_CAMMUS`), linux-steering-wheels |
//! | C5 PID 0x0301 | ✅ Confirmed | Linux kernel `hid-ids.h` (`USB_DEVICE_ID_CAMMUS_C5`), linux-steering-wheels (Platinum), simracing-hwdb |
//! | C12 PID 0x0302 | ✅ Confirmed | Linux kernel `hid-ids.h` (`USB_DEVICE_ID_CAMMUS_C12`), linux-steering-wheels (Platinum), simracing-hwdb |
//! | CP5 Pedals PID 0x1018 | ✅ Confirmed (community) | JacKeTUs/simracing-hwdb `90-cammus.hwdb` |
//! | LC100 Pedals PID 0x1019 | ✅ Confirmed (community) | JacKeTUs/simracing-hwdb `90-cammus.hwdb` |
//!
//! ## HID protocol notes
//!
//! Cammus devices use USB HID PID (Physical Interface Device) for force
//! feedback. The firmware omits the `0xa7` (effect delay) HID descriptor,
//! which required a kernel patch for Linux < 6.15. Fixed natively in
//! Linux 6.15 via `hid-universal-pidff`.

/// Cammus USB Vendor ID (Shenzhen Cammus Electronic Technology Co., Ltd.).
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_VENDOR_ID_CAMMUS = 0x3416`),
/// JacKeTUs/linux-steering-wheels compatibility table.
pub const VENDOR_ID: u16 = 0x3416;

/// Cammus C5 (5 Nm desktop direct drive wheel) product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_CAMMUS_C5 = 0x0301`),
/// linux-steering-wheels (Platinum rating), simracing-hwdb.
pub const PRODUCT_C5: u16 = 0x0301;

/// Cammus C12 (12 Nm desktop direct drive wheel) product ID.
///
/// ✅ Confirmed: Linux kernel `hid-ids.h` (`USB_DEVICE_ID_CAMMUS_C12 = 0x0302`),
/// linux-steering-wheels (Platinum rating), simracing-hwdb.
pub const PRODUCT_C12: u16 = 0x0302;

/// Cammus CP5 Pedals product ID.
///
/// ✅ Confirmed (community): JacKeTUs/simracing-hwdb `90-cammus.hwdb`
/// (`v3416p1018`, labeled "Cammus CP5 Pedals").
pub const PRODUCT_CP5_PEDALS: u16 = 0x1018;

/// Cammus LC100 Pedals product ID.
///
/// ✅ Confirmed (community): JacKeTUs/simracing-hwdb `90-cammus.hwdb`
/// (`v3416p1019`, labeled "Cammus LC100 Pedals").
pub const PRODUCT_LC100_PEDALS: u16 = 0x1019;

/// Returns `true` if the VID/PID pair identifies a known Cammus device.
pub fn is_cammus(vid: u16, pid: u16) -> bool {
    vid == VENDOR_ID
        && matches!(
            pid,
            PRODUCT_C5 | PRODUCT_C12 | PRODUCT_CP5_PEDALS | PRODUCT_LC100_PEDALS
        )
}

/// Returns the product name for a known Cammus PID, or `None` for unknown PIDs.
pub fn product_name(pid: u16) -> Option<&'static str> {
    match pid {
        PRODUCT_C5 => Some("Cammus C5"),
        PRODUCT_C12 => Some("Cammus C12"),
        PRODUCT_CP5_PEDALS => Some("Cammus CP5 Pedals"),
        PRODUCT_LC100_PEDALS => Some("Cammus LC100 Pedals"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_products_recognised() {
        assert!(is_cammus(VENDOR_ID, PRODUCT_C5));
        assert!(is_cammus(VENDOR_ID, PRODUCT_C12));
        assert!(is_cammus(VENDOR_ID, PRODUCT_CP5_PEDALS));
        assert!(is_cammus(VENDOR_ID, PRODUCT_LC100_PEDALS));
    }

    #[test]
    fn unknown_product_not_recognised() {
        assert!(!is_cammus(VENDOR_ID, 0x0001));
        assert!(!is_cammus(0x0000, PRODUCT_C5));
    }

    #[test]
    fn product_names() {
        assert_eq!(product_name(PRODUCT_C5), Some("Cammus C5"));
        assert_eq!(product_name(PRODUCT_C12), Some("Cammus C12"));
        assert_eq!(product_name(PRODUCT_CP5_PEDALS), Some("Cammus CP5 Pedals"));
        assert_eq!(
            product_name(PRODUCT_LC100_PEDALS),
            Some("Cammus LC100 Pedals")
        );
        assert_eq!(product_name(0xFFFF), None);
    }
}
