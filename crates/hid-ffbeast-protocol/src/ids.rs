//! FFBeast USB vendor and product ID constants.
//!
//! Sources:
//! - Linux kernel `hid-ids.h` (`USB_VENDOR_ID_FFBEAST = 0x045b`)
//! - <https://ffbeast.github.io/ffbeast.github.io/> (official site; ffbeast.com is defunct)
//! - FFBeast wheel C/C++ API reference (VID=0x045B, wheel PID=0x59D7)
//! - JacKeTUs/linux-steering-wheels compatibility table

/// FFBeast USB Vendor ID (`USB_VENDOR_ID_FFBEAST` in the Linux kernel).
pub const FFBEAST_VENDOR_ID: u16 = 0x045B;

/// FFBeast joystick product ID.
pub const FFBEAST_PRODUCT_ID_JOYSTICK: u16 = 0x58F9;

/// FFBeast rudder product ID.
pub const FFBEAST_PRODUCT_ID_RUDDER: u16 = 0x5968;

/// FFBeast wheel product ID.
pub const FFBEAST_PRODUCT_ID_WHEEL: u16 = 0x59D7;

/// Returns `true` if `product_id` is a known FFBeast product.
///
/// # Examples
/// ```
/// use racing_wheel_hid_ffbeast_protocol::ids::is_ffbeast_product;
/// assert!(is_ffbeast_product(0x58f9));
/// assert!(is_ffbeast_product(0x5968));
/// assert!(is_ffbeast_product(0x59d7));
/// assert!(!is_ffbeast_product(0x0001));
/// ```
pub fn is_ffbeast_product(product_id: u16) -> bool {
    matches!(
        product_id,
        FFBEAST_PRODUCT_ID_JOYSTICK | FFBEAST_PRODUCT_ID_RUDDER | FFBEAST_PRODUCT_ID_WHEEL
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_products_recognised() {
        assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_JOYSTICK));
        assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_RUDDER));
        assert!(is_ffbeast_product(FFBEAST_PRODUCT_ID_WHEEL));
    }

    #[test]
    fn unknown_product_not_recognised() {
        assert!(!is_ffbeast_product(0x0000));
        assert!(!is_ffbeast_product(0xFFFF));
    }
}
