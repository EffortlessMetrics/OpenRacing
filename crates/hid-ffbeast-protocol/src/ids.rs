//! FFBeast USB vendor and product ID constants.
//!
//! ## Verification status: ✅ Fully verified (web-verified 2025-07)
//!
//! All three PIDs are confirmed by multiple independent sources:
//!
//! Sources:
//! - **Linux kernel `hid-ids.h`** (mainline, merged for 6.15):
//!   `USB_VENDOR_ID_FFBEAST = 0x045b`, `USB_DEVICE_ID_FFBEAST_JOYSTICK = 0x58f9`,
//!   `USB_DEVICE_ID_FFBEAST_RUDDER = 0x5968`, `USB_DEVICE_ID_FFBEAST_WHEEL = 0x59d7`
//! - **Linux kernel `hid-universal-pidff.c`** (mainline, 6.15):
//!   all three PIDs in the device table; joystick has a special `input_configured`
//!   handler to remove fuzz/deadzone on `ABS_Y`
//! - **JacKeTUs/linux-steering-wheels** compatibility table:
//!   FFBeast Wheel = 045b:59d7, Platinum rating, driver `hid-universal-pidff`
//! - <https://ffbeast.github.io/ffbeast.github.io/> (official site; ffbeast.com is defunct)
//! - FFBeast wheel C/C++ API reference (VID=0x045B, wheel PID=0x59D7)
//!
//! ### USB-ID database cross-check (2025-07)
//!
//! VID `0x045B` is registered to **Renesas Electronics Corp.** (formerly Hitachi, Ltd)
//! in the USB-IF vendor list. FFBeast reuses this VID via Renesas microcontrollers.
//! - usb-ids.gowdy.us: "Renesas Electronics" (no FFBeast-specific products listed)
//! - the-sz.com/products/usbid: "Renesas Electronics Corp." / "Hitachi, Ltd"
//! - devicehunt.com: "Hitachi, Ltd" (only RX610 RX-Stick listed)
//!
//! The FFBeast-specific PIDs (`0x58F9`, `0x5968`, `0x59D7`) are **not** in public
//! USB-ID databases, which is expected — they are registered only in the Linux kernel.
//! Confidence: **High** — kernel mainline + community driver + official FFBeast docs.

/// FFBeast USB Vendor ID (`USB_VENDOR_ID_FFBEAST` in mainline Linux kernel ≥6.15).
///
/// Registered to Renesas Electronics Corp. (the-sz.com, usb-ids.gowdy.us);
/// FFBeast reuses this VID via Renesas MCUs.
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
