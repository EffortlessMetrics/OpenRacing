//! Cammus USB vendor and product ID constants.

/// Cammus USB Vendor ID.
pub const VENDOR_ID: u16 = 0x3285;

/// Cammus C5 (5 Nm desktop direct drive wheel) product ID.
pub const PRODUCT_C5: u16 = 0x0002;

/// Cammus C12 (12 Nm desktop direct drive wheel) product ID.
pub const PRODUCT_C12: u16 = 0x0003;

/// Returns `true` if the VID/PID pair identifies a known Cammus device.
pub fn is_cammus(vid: u16, pid: u16) -> bool {
    vid == VENDOR_ID && matches!(pid, PRODUCT_C5 | PRODUCT_C12)
}

/// Returns the product name for a known Cammus PID, or `None` for unknown PIDs.
pub fn product_name(pid: u16) -> Option<&'static str> {
    match pid {
        PRODUCT_C5 => Some("Cammus C5"),
        PRODUCT_C12 => Some("Cammus C12"),
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
        assert_eq!(product_name(0xFFFF), None);
    }
}
