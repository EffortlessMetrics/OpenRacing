//! OpenFFBoard USB vendor and product ID constants.
//!
//! Sources:
//! - <https://pid.codes/1209/FFB0/> (official pid.codes entry)
//! - <https://github.com/Ultrawipf/OpenFFBoard>

/// OpenFFBoard USB Vendor ID (pid.codes open hardware VID).
pub const OPENFFBOARD_VENDOR_ID: u16 = 0x1209;

/// OpenFFBoard main product ID.
///
/// Used by the primary firmware build for the main board.
pub const OPENFFBOARD_PRODUCT_ID: u16 = 0xFFB0;

/// OpenFFBoard alternate product ID.
///
/// Used by secondary/experimental firmware builds.
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
