//! HID report format constants for AccuForce devices.
//!
//! The SimExperience AccuForce Pro exposes a standard USB HID PID (force
//! feedback) interface. No proprietary report format extensions are required.

/// Maximum USB HID feature/output report size for AccuForce devices (bytes).
///
/// Full-speed USB HID limits interrupt transfer payloads to 64 bytes. The
/// AccuForce Pro operates at full-speed (12 Mbit/s).
pub const MAX_REPORT_BYTES: usize = 64;

/// HID Usage Page for force feedback (USB HID PID).
///
/// AccuForce Pro uses the standard HID PID usage page rather than a
/// proprietary vendor usage page.
pub const HID_PID_USAGE_PAGE: u16 = 0x000F;

/// Approximate USB update rate interval in milliseconds.
///
/// The AccuForce Pro USB update rate is ~100–200 Hz; 8 ms is a safe interval.
pub const RECOMMENDED_B_INTERVAL_MS: u8 = 8;

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn report_size_within_usb_full_speed_limit() {
        assert!(MAX_REPORT_BYTES <= 64);
    }

    #[test]
    fn b_interval_is_positive() {
        assert!(RECOMMENDED_B_INTERVAL_MS > 0);
    }
}
