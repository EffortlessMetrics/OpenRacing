//! PXN USB vendor and product ID constants.
//!
//! VID `0x11FF` is assigned to PXN (Shenzhen Jinyu Technology Co., Ltd.).
//! Source: JacKeTUs/linux-steering-wheels compatibility table.

/// PXN USB Vendor ID (Shenzhen Jinyu Technology Co., Ltd.).
pub const VENDOR_ID: u16 = 0x11FF;

/// PXN V10 – 10 Nm direct drive wheel.
pub const PRODUCT_V10: u16 = 0x3245;

/// PXN V12 – 12 Nm direct drive wheel.
pub const PRODUCT_V12: u16 = 0x1212;

/// PXN V12 Lite – 12 Nm compact direct drive wheel.
pub const PRODUCT_V12_LITE: u16 = 0x1112;

/// PXN V12 Lite SE – 12 Nm compact SE variant.
pub const PRODUCT_V12_LITE_SE: u16 = 0x1211;

/// GT987 FF (Lite Star OEM variant sharing VID 0x11FF).
pub const PRODUCT_GT987_FF: u16 = 0x2141;

// Unconfirmed PIDs for VD-series products — not yet verified against hardware:
//   VD4:   unknown
//   VD6:   unknown
//   VD10+: unknown

/// Returns `true` if the VID/PID pair identifies a known PXN device.
pub fn is_pxn_device(vid: u16, pid: u16) -> bool {
    vid == VENDOR_ID
        && matches!(
            pid,
            PRODUCT_V10 | PRODUCT_V12 | PRODUCT_V12_LITE | PRODUCT_V12_LITE_SE | PRODUCT_GT987_FF
        )
}

/// Returns the product name for a known PXN PID, or `None` for unknown PIDs.
pub fn product_name(pid: u16) -> Option<&'static str> {
    match pid {
        PRODUCT_V10 => Some("PXN V10"),
        PRODUCT_V12 => Some("PXN V12"),
        PRODUCT_V12_LITE => Some("PXN V12 Lite"),
        PRODUCT_V12_LITE_SE => Some("PXN V12 Lite SE"),
        PRODUCT_GT987_FF => Some("GT987 FF (Lite Star OEM)"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_products_recognised() {
        assert!(is_pxn_device(VENDOR_ID, PRODUCT_V10));
        assert!(is_pxn_device(VENDOR_ID, PRODUCT_V12));
        assert!(is_pxn_device(VENDOR_ID, PRODUCT_V12_LITE));
        assert!(is_pxn_device(VENDOR_ID, PRODUCT_V12_LITE_SE));
        assert!(is_pxn_device(VENDOR_ID, PRODUCT_GT987_FF));
    }

    #[test]
    fn unknown_product_not_recognised() {
        assert!(!is_pxn_device(VENDOR_ID, 0x0001));
        assert!(!is_pxn_device(0x0000, PRODUCT_V10));
    }

    #[test]
    fn product_names() {
        assert_eq!(product_name(PRODUCT_V10), Some("PXN V10"));
        assert_eq!(product_name(PRODUCT_V12), Some("PXN V12"));
        assert_eq!(product_name(PRODUCT_V12_LITE), Some("PXN V12 Lite"));
        assert_eq!(product_name(PRODUCT_V12_LITE_SE), Some("PXN V12 Lite SE"));
        assert_eq!(
            product_name(PRODUCT_GT987_FF),
            Some("GT987 FF (Lite Star OEM)")
        );
        assert_eq!(product_name(0xFFFF), None);
    }
}
