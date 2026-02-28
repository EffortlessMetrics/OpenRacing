//! OpenFFBoard USB vendor and product ID constants.
//!
//! Sources:
//! - <https://pid.codes/1209/FFB0/> (official pid.codes entry)
//! - <https://github.com/Ultrawipf/OpenFFBoard>

/// OpenFFBoard USB Vendor ID (pid.codes open hardware VID).
pub const OPENFFBOARD_VENDOR_ID: u16 = 0x1209;

/// OpenFFBoard main product ID.
///
/// ✅ Confirmed via multiple independent sources:
/// - pid.codes registry: <https://pid.codes/1209/FFB0/>
/// - OpenFFBoard firmware `usb_descriptors.cpp`: `#define USBD_PID 0xFFB0`
/// - OpenFFBoard-configurator `serial_ui.py`: `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`
/// - JacKeTUs/linux-steering-wheels compatibility table (VID `1209`, PID `ffb0`, Platinum)
pub const OPENFFBOARD_PRODUCT_ID: u16 = 0xFFB0;

/// OpenFFBoard alternate product ID.
///
/// **Unverified**: PID `0xFFB1` is *not* registered on pid.codes (returns 404
/// as of 2025-06), does not appear in the official OpenFFBoard firmware
/// (`usb_descriptors.cpp` only defines `USBD_PID 0xFFB0`), is absent from
/// the configurator (`serial_ui.py` only lists `0xFFB0`), and is not in the
/// JacKeTUs/linux-steering-wheels compatibility table.
/// Retained for possible future / community firmware builds, but should not be
/// treated as confirmed.
///
/// TODO(web-verify): Remove or gate behind a feature flag if no evidence of
/// 0xFFB1 usage surfaces. Check OpenFFBoard community forums/issues.
pub const OPENFFBOARD_PRODUCT_ID_ALT: u16 = 0xFFB1;

/// Returns `true` if `product_id` is a known OpenFFBoard product.
///
/// # Examples
/// ```
/// use racing_wheel_hid_openffboard_protocol::ids::is_openffboard_product;
/// assert!(is_openffboard_product(0xFFB0));
/// assert!(is_openffboard_product(0xFFB1));
/// assert!(!is_openffboard_product(0x0001));
/// ```
pub fn is_openffboard_product(product_id: u16) -> bool {
    matches!(product_id, 0xFFB0 | 0xFFB1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_products_recognised() {
        assert!(is_openffboard_product(OPENFFBOARD_PRODUCT_ID));
        assert!(is_openffboard_product(OPENFFBOARD_PRODUCT_ID_ALT));
    }

    #[test]
    fn unknown_product_not_recognised() {
        assert!(!is_openffboard_product(0x0000));
        assert!(!is_openffboard_product(0xFFFF));
    }
}
