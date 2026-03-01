//! Input report parsing for Simucube devices.
//!
//! Two report types are provided:
//!
//! - [`SimucubeHidReport`]: Parses the **documented** standard HID joystick
//!   layout (steering axis, Y axis, 6 additional axes, 128 buttons) as
//!   described in the official Simucube USB interface documentation.
//!
//! - [`SimucubeInputReport`]: A **speculative** extended format carrying
//!   internal diagnostics (encoder angle, motor speed, torque feedback,
//!   temperature, fault/status flags). Its wire encoding is a placeholder and
//!   has **not** been verified against real hardware.
//!
//! ## Sources
//!
//! - Official Simucube developer docs — `Simucube/simucube-docs.github.io`
//!   → `docs/Simucube 2/Developers.md`
//! - Granite Devices wiki USB interface documentation —
//!   <https://granitedevices.com/wiki/Simucube_product_USB_interface_documentation>
//! - USB HID PID 1.01 specification

use super::{
    ANGLE_SENSOR_MAX, HID_ADDITIONAL_AXES, HID_BUTTON_BYTES, HID_JOYSTICK_REPORT_MIN_BYTES,
    SimucubeError, SimucubeResult,
};
use openracing_hid_common::ReportParser;

// ─── Documented HID joystick report ──────────────────────────────────────────

/// Standard HID joystick input report for Simucube wheelbases.
///
/// This struct models the documented USB HID input report layout. All fields
/// come directly from the official Simucube developer documentation:
///
/// | Field | Type | Byte offset (assumed) |
/// |-------|------|-----------------------|
/// | X axis (steering) | `u16` LE | 0–1 |
/// | Y axis | `u16` LE | 2–3 |
/// | Axes 1–6 | `u16` LE each | 4–15 |
/// | 128 buttons | bitfield | 16–31 |
///
/// **Note:** The byte ordering above is inferred from standard HID conventions
/// (axes then buttons). The actual HID report descriptor may differ — this
/// layout has not been verified with a hardware descriptor dump.
///
/// Source: `Simucube/simucube-docs.github.io` → `docs/Simucube 2/Developers.md`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimucubeHidReport {
    /// X axis — steering wheel position (0–65535).
    ///
    /// This is a standard unsigned 16-bit HID axis. The internal 22-bit
    /// encoder resolution is not exposed over USB.
    pub steering: u16,
    /// Y axis — center-idle by default. Users can map this to an external
    /// pedal or handbrake via Simucube True Drive / Tuner.
    pub y_axis: u16,
    /// Additional axes 1–6 (unsigned 16-bit each).
    ///
    /// Can be mapped to Simucube-compatible pedals, handbrakes, or analog
    /// inputs from a SimuCube Wireless Wheel (e.g. clutch paddles).
    pub axes: [u16; HID_ADDITIONAL_AXES],
    /// 128 buttons packed as a 16-byte bitfield (little-endian bit order).
    ///
    /// Buttons originate from the Simucube physical interface and/or an
    /// attached SimuCube Wireless Wheel. The official docs state that all
    /// 128 buttons should be supported for optimal wireless wheel experience.
    pub buttons: [u8; HID_BUTTON_BYTES],
}

impl SimucubeHidReport {
    /// Parse a standard HID joystick report from raw bytes.
    ///
    /// Expects at least [`HID_JOYSTICK_REPORT_MIN_BYTES`] (32) bytes.
    /// Extra trailing bytes are silently ignored.
    pub fn parse(data: &[u8]) -> SimucubeResult<Self> {
        if data.len() < HID_JOYSTICK_REPORT_MIN_BYTES {
            return Err(SimucubeError::InvalidReportSize {
                expected: HID_JOYSTICK_REPORT_MIN_BYTES,
                actual: data.len(),
            });
        }

        let mut parser = ReportParser::from_slice(data);

        let steering = parser.read_u16_le()?;
        let y_axis = parser.read_u16_le()?;

        let mut axes = [0u16; HID_ADDITIONAL_AXES];
        for ax in &mut axes {
            *ax = parser.read_u16_le()?;
        }

        let mut buttons = [0u8; HID_BUTTON_BYTES];
        let btn_bytes = parser.read_bytes(HID_BUTTON_BYTES)?;
        buttons.copy_from_slice(&btn_bytes);

        Ok(Self {
            steering,
            y_axis,
            axes,
            buttons,
        })
    }

    /// Steering position normalised to `0.0..=1.0`.
    pub fn steering_normalized(&self) -> f32 {
        self.steering as f32 / u16::MAX as f32
    }

    /// Steering position as a signed fraction (`-1.0..=1.0`) where `0.0` is
    /// center (0x8000).
    pub fn steering_signed(&self) -> f32 {
        (self.steering as f32 - 32768.0) / 32768.0
    }

    /// Test whether button `n` (0-indexed) is pressed.
    ///
    /// Returns `false` for out-of-range indices.
    pub fn button_pressed(&self, n: usize) -> bool {
        let byte_idx = n / 8;
        let bit_idx = n % 8;
        if byte_idx >= self.buttons.len() {
            return false;
        }
        (self.buttons[byte_idx] >> bit_idx) & 1 != 0
    }

    /// Count of currently pressed buttons.
    pub fn pressed_count(&self) -> u32 {
        self.buttons.iter().map(|b| b.count_ones()).sum()
    }

    /// Normalise an additional axis (0-indexed, 0–5) to `0.0..=1.0`.
    ///
    /// Returns `0.0` for out-of-range indices.
    pub fn axis_normalized(&self, idx: usize) -> f32 {
        if idx >= self.axes.len() {
            return 0.0;
        }
        self.axes[idx] as f32 / u16::MAX as f32
    }
}

impl Default for SimucubeHidReport {
    fn default() -> Self {
        Self {
            steering: 0x8000, // center
            y_axis: 0x8000,   // center-idle
            axes: [0; HID_ADDITIONAL_AXES],
            buttons: [0; HID_BUTTON_BYTES],
        }
    }
}

// ─── Speculative extended input report ───────────────────────────────────────

/// Speculative extended input report with internal diagnostics.
///
/// **Warning:** This struct's wire format is a placeholder — it does not match
/// the standard HID joystick report. It models conceptual fields (encoder
/// angle, motor speed, temperature, fault flags) that may be available through
/// a vendor-specific HID report or True Drive API, but the byte layout has
/// **not** been verified against real hardware.
///
/// For the documented HID joystick format, use [`SimucubeHidReport`] instead.
#[derive(Debug, Clone)]
pub struct SimucubeInputReport {
    pub sequence: u16,
    pub wheel_angle_raw: u32,
    pub wheel_speed_rpm: i16,
    pub torque_nm: i16,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub status_flags: u8,
    /// Button bitmask from an attached SimuCube Wireless Wheel (0 if not present).
    pub wireless_buttons: u16,
    /// Battery level of the wireless wheel in percent (0–100; 0 if no wireless wheel).
    pub wireless_battery_pct: u8,
}

impl SimucubeInputReport {
    pub fn parse(data: &[u8]) -> SimucubeResult<Self> {
        if data.len() < 16 {
            return Err(SimucubeError::InvalidReportSize {
                expected: 16,
                actual: data.len(),
            });
        }

        let mut parser = ReportParser::from_slice(data);

        let sequence = parser.read_u16_le()?;
        let wheel_angle_raw = parser.read_u32_le()?;
        let wheel_speed_rpm = parser.read_i16_le()?;
        let torque_nm = parser.read_i16_le()?;
        let temperature_c = parser.read_u8()?;
        let fault_flags = parser.read_u8()?;
        let _reserved = parser.read_u8()?;
        let status_flags = parser.read_u8()?;

        // Optional wireless wheel extension (bytes 14–16, present on longer reports).
        let (wireless_buttons, wireless_battery_pct) = if data.len() >= 17 {
            let buttons = u16::from_le_bytes([data[14], data[15]]);
            let battery = data[16];
            (buttons, battery)
        } else {
            (0, 0)
        };

        Ok(Self {
            sequence,
            wheel_angle_raw,
            wheel_speed_rpm,
            torque_nm,
            temperature_c,
            fault_flags,
            status_flags,
            wireless_buttons,
            wireless_battery_pct,
        })
    }

    pub fn wheel_angle_degrees(&self) -> f32 {
        let normalized = self.wheel_angle_raw as f32 / ANGLE_SENSOR_MAX as f32;
        normalized * 360.0
    }

    pub fn wheel_angle_radians(&self) -> f32 {
        self.wheel_angle_degrees().to_radians()
    }

    pub fn wheel_speed_rad_s(&self) -> f32 {
        self.wheel_speed_rpm as f32 * 2.0 * std::f32::consts::PI / 60.0
    }

    pub fn applied_torque_nm(&self) -> f32 {
        self.torque_nm as f32 / 100.0
    }

    pub fn has_fault(&self) -> bool {
        self.fault_flags != 0
    }

    pub fn is_connected(&self) -> bool {
        (self.status_flags & 0x01) != 0
    }

    pub fn is_enabled(&self) -> bool {
        (self.status_flags & 0x02) != 0
    }

    /// Return `true` if a SimuCube Wireless Wheel is present (any button or battery data).
    pub fn has_wireless_wheel(&self) -> bool {
        self.wireless_battery_pct > 0 || self.wireless_buttons != 0
    }
}

impl Default for SimucubeInputReport {
    fn default() -> Self {
        Self {
            sequence: 0,
            wheel_angle_raw: 0,
            wheel_speed_rpm: 0,
            torque_nm: 0,
            temperature_c: 25,
            fault_flags: 0,
            status_flags: 0x03,
            wireless_buttons: 0,
            wireless_battery_pct: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_report() -> [u8; 16] {
        let mut data = [0u8; 16];
        data[0] = 0x01;
        data[1] = 0x00;
        data[2] = 0x00;
        data[3] = 0x40;
        data[4] = 0x01;
        data[5] = 0x00;
        data[6] = 0x88;
        data[7] = 0x01;
        data[8] = 0x32;
        data[9] = 0x00;
        data[10] = 0x00;
        data[11] = 0x03;
        data
    }

    #[test]
    fn test_parse_report() {
        let data = make_test_report();
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.sequence, 1);
            assert_eq!(report.wheel_angle_raw, 0x00014000);
            assert_eq!(report.wheel_speed_rpm, 392);
            assert_eq!(report.torque_nm, 50);
            assert_eq!(report.temperature_c, 0);
            assert_eq!(report.fault_flags, 3);
            assert_eq!(report.status_flags, 0);
        }
    }

    #[test]
    fn test_invalid_report_size() {
        let data = vec![0u8; 8];
        let result = SimucubeInputReport::parse(&data);
        assert!(matches!(
            result,
            Err(SimucubeError::InvalidReportSize { .. })
        ));
    }

    #[test]
    fn test_wheel_angle() {
        let report = SimucubeInputReport {
            wheel_angle_raw: ANGLE_SENSOR_MAX / 4,
            ..Default::default()
        };

        let degrees = report.wheel_angle_degrees();
        assert!((degrees - 90.0).abs() < 0.1);
    }

    #[test]
    fn test_wheel_speed() {
        let report = SimucubeInputReport {
            wheel_speed_rpm: 60,
            ..Default::default()
        };

        let rad_s = report.wheel_speed_rad_s();
        assert!((rad_s - 2.0 * std::f32::consts::PI).abs() < 0.01);
    }

    #[test]
    fn test_applied_torque() {
        let report = SimucubeInputReport {
            torque_nm: 1500,
            ..Default::default()
        };

        let torque = report.applied_torque_nm();
        assert!((torque - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_status_flags() {
        let mut report = SimucubeInputReport {
            status_flags: 0x03,
            ..Default::default()
        };
        assert!(report.is_connected());
        assert!(report.is_enabled());

        report.status_flags = 0x02;
        assert!(!report.is_connected());
        assert!(report.is_enabled());

        report.status_flags = 0x01;
        assert!(report.is_connected());
        assert!(!report.is_enabled());

        report.status_flags = 0x00;
        assert!(!report.is_connected());
        assert!(!report.is_enabled());
    }

    #[test]
    fn test_wireless_buttons_in_extended_report() {
        let mut data = [0u8; 17];
        // Minimal valid header (16 core bytes + 1 wireless)
        data[14] = 0b0000_0101; // buttons lo: button 0 and 2 pressed
        data[15] = 0x00; // buttons hi
        data[16] = 85; // battery 85%
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.wireless_buttons, 0b0000_0101);
            assert_eq!(report.wireless_battery_pct, 85);
            assert!(report.has_wireless_wheel());
        }
    }

    #[test]
    fn test_short_report_has_no_wireless_fields() {
        let data = [0u8; 16];
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.wireless_buttons, 0);
            assert_eq!(report.wireless_battery_pct, 0);
            assert!(!report.has_wireless_wheel());
        }
    }

    #[test]
    fn test_wireless_buttons_all_set() {
        let mut data = [0u8; 17];
        data[14] = 0xFF;
        data[15] = 0xFF; // all 16 buttons pressed
        data[16] = 100; // full battery
        let result = SimucubeInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.wireless_buttons, 0xFFFF);
            assert_eq!(report.wireless_battery_pct, 100);
            assert!(report.has_wireless_wheel());
        }
    }

    // ─── SimucubeHidReport tests ─────────────────────────────────────────

    /// Build a minimal 32-byte HID joystick report with known values.
    fn make_hid_report(steering: u16, y: u16, axes: [u16; 6], buttons: [u8; 16]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32);
        buf.extend_from_slice(&steering.to_le_bytes());
        buf.extend_from_slice(&y.to_le_bytes());
        for ax in &axes {
            buf.extend_from_slice(&ax.to_le_bytes());
        }
        buf.extend_from_slice(&buttons);
        buf
    }

    #[test]
    fn test_hid_report_parse_center() -> Result<(), SimucubeError> {
        let data = make_hid_report(0x8000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0x8000);
        assert_eq!(report.y_axis, 0x8000);
        assert_eq!(report.axes, [0u16; 6]);
        assert_eq!(report.buttons, [0u8; 16]);
        Ok(())
    }

    #[test]
    fn test_hid_report_parse_full_left() -> Result<(), SimucubeError> {
        let data = make_hid_report(0x0000, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0);
        let signed = report.steering_signed();
        assert!((signed - (-1.0)).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_hid_report_parse_full_right() -> Result<(), SimucubeError> {
        let data = make_hid_report(0xFFFF, 0x8000, [0; 6], [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0xFFFF);
        let norm = report.steering_normalized();
        assert!((norm - 1.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_hid_report_buttons() -> Result<(), SimucubeError> {
        let mut buttons = [0u8; 16];
        buttons[0] = 0b0000_0101; // buttons 0 and 2
        buttons[15] = 0b1000_0000; // button 127
        let data = make_hid_report(0x8000, 0x8000, [0; 6], buttons);
        let report = SimucubeHidReport::parse(&data)?;
        assert!(report.button_pressed(0));
        assert!(!report.button_pressed(1));
        assert!(report.button_pressed(2));
        assert!(report.button_pressed(127));
        assert!(!report.button_pressed(126));
        assert_eq!(report.pressed_count(), 3);
        Ok(())
    }

    #[test]
    fn test_hid_report_button_out_of_range() {
        let report = SimucubeHidReport::default();
        assert!(!report.button_pressed(128));
        assert!(!report.button_pressed(255));
    }

    #[test]
    fn test_hid_report_axes() -> Result<(), SimucubeError> {
        let axes = [100, 200, 300, 400, 500, 600];
        let data = make_hid_report(0x8000, 0x8000, axes, [0; 16]);
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.axes, axes);
        let norm = report.axis_normalized(0);
        assert!((norm - 100.0 / 65535.0).abs() < 0.001);
        assert_eq!(report.axis_normalized(6), 0.0); // out of range
        Ok(())
    }

    #[test]
    fn test_hid_report_too_short() {
        let data = [0u8; 31]; // one byte short
        let result = SimucubeHidReport::parse(&data);
        assert!(matches!(
            result,
            Err(SimucubeError::InvalidReportSize {
                expected: 32,
                actual: 31
            })
        ));
    }

    #[test]
    fn test_hid_report_extra_bytes_ignored() -> Result<(), SimucubeError> {
        let mut data = vec![0u8; 64]; // padded to 64
        data[0] = 0xFF;
        data[1] = 0x7F; // steering = 0x7FFF
        let report = SimucubeHidReport::parse(&data)?;
        assert_eq!(report.steering, 0x7FFF);
        Ok(())
    }

    #[test]
    fn test_hid_report_default_is_centered() {
        let report = SimucubeHidReport::default();
        assert_eq!(report.steering, 0x8000);
        assert_eq!(report.y_axis, 0x8000);
        let signed = report.steering_signed();
        assert!(
            signed.abs() < 0.001,
            "default steering should be ~0.0, got {signed}"
        );
    }
}
