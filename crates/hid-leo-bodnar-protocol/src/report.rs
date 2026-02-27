//! HID report format constants for Leo Bodnar devices.
//!
//! Leo Bodnar devices use standard USB HID interfaces. The Sim Racing Wheel
//! Interface (PID `0x000E`) and FFB Joystick (PID `0x000F`) expose standard
//! HID PID (Usage Page `0x000F`) for force feedback. No proprietary report
//! format extensions are required.

/// Maximum USB HID feature/output report size for Leo Bodnar devices (bytes).
///
/// Full-speed USB HID limits interrupt transfer payloads to 64 bytes. All Leo
/// Bodnar devices operate at full-speed (12 Mbit/s).
pub const MAX_REPORT_BYTES: usize = 64;

/// HID Usage Page for force feedback (USB HID PID).
///
/// All Leo Bodnar FFB devices use the standard HID PID usage page rather
/// than a proprietary vendor usage page.
pub const HID_PID_USAGE_PAGE: u16 = 0x000F;

/// Default encoder counts per revolution for the USB Sim Racing Wheel Interface.
///
/// The wheel interface reports position over a 16-bit range via standard HID
/// PID. `65_535` (0xFFFF) represents the full 16-bit range.
pub const WHEEL_ENCODER_CPR: u32 = 65_535;

/// Conservative default maximum torque for the USB Sim Racing Wheel Interface
/// in Newton-metres.
///
/// The actual torque depends on the connected motor and power supply. Users can
/// override this in their force feedback profile.
pub const WHEEL_DEFAULT_MAX_TORQUE_NM: f32 = 10.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_size_within_usb_full_speed_limit() {
        assert!(MAX_REPORT_BYTES <= 64);
    }

    #[test]
    fn encoder_cpr_is_positive() {
        assert!(WHEEL_ENCODER_CPR > 0);
    }

    #[test]
    fn default_torque_is_positive() {
        assert!(WHEEL_DEFAULT_MAX_TORQUE_NM > 0.0);
    }
}
