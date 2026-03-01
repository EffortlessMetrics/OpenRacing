//! OpenFFBoard USB vendor and product ID constants.
//!
//! Sources:
//! - <https://pid.codes/1209/FFB0/> (official pid.codes entry)
//! - <https://github.com/Ultrawipf/OpenFFBoard>
//!
//! Last verified: 2025-07 against OpenFFBoard firmware commit `cbd64db`,
//! OpenFFBoard-configurator `serial_ui.py`, pid.codes registry, and
//! JacKeTUs/linux-steering-wheels compatibility table.
//!
//! FFB protocol: standard HID PID (PIDFF). Linux driver is `hid-pidff`
//! (Platinum rating in linux-steering-wheels).

/// OpenFFBoard USB Vendor ID (pid.codes open hardware VID).
///
/// Source: `Firmware/FFBoard/UserExtensions/Src/usb_descriptors.cpp`
/// `#define USBD_VID 0x1209`
pub const OPENFFBOARD_VENDOR_ID: u16 = 0x1209;

/// OpenFFBoard main product ID.
///
/// ✅ Confirmed via multiple independent sources (2025-07):
/// - pid.codes registry: <https://pid.codes/1209/FFB0/>
/// - OpenFFBoard firmware `Firmware/FFBoard/UserExtensions/Src/usb_descriptors.cpp`:
///   `#define USBD_PID 0xFFB0`
/// - OpenFFBoard-configurator `serial_ui.py`:
///   `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`
/// - JacKeTUs/linux-steering-wheels compatibility table (VID `1209`, PID `ffb0`)
pub const OPENFFBOARD_PRODUCT_ID: u16 = 0xFFB0;

/// OpenFFBoard alternate product ID.
///
/// **SPECULATIVE — no external evidence found** (re-checked 2025-07): PID `0xFFB1` is *not* registered on
/// pid.codes (returns HTTP 404), does not appear in the official OpenFFBoard
/// firmware (`usb_descriptors.cpp` only defines `USBD_PID 0xFFB0`), is absent
/// from the configurator (`serial_ui.py`: `OFFICIAL_VID_PID = [(0x1209, 0xFFB0)]`),
/// is not found anywhere in the `Ultrawipf/OpenFFBoard` repository (GitHub code
/// search returns zero results for "FFB1"), and is not listed in
/// JacKeTUs/linux-steering-wheels (only VID `1209` / PID `ffb0` is present).
///
/// Retained for possible future / community firmware builds, but should not
/// be treated as confirmed.
///
/// TODO(web-verify): Remove or gate behind a feature flag if no evidence of
/// 0xFFB1 usage surfaces. Check OpenFFBoard community forums/Discord.
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
