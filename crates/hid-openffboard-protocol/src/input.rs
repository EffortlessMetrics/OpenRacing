//! OpenFFBoard HID input report parser.
//!
//! Parses the standard gamepad input report sent by OpenFFBoard firmware.
//! The report layout is derived from the official firmware source:
//! `Firmware/FFBoard/UserExtensions/Src/usb_hid_gamepad.c` (commit `cbd64db`).
//!
//! # Report layout (Report ID 0x01)
//!
//! ```text
//! Offset  Size  Description
//! ------  ----  -------------------------------------------
//!  0       1    Report ID (0x01)
//!  1       8    Buttons (64 buttons, 1 bit each)
//!  9       2    X axis  (steering) — i16 LE [-32767, 32767]
//! 11       2    Y axis  (throttle) — i16 LE [-32767, 32767]
//! 13       2    Z axis  (brake)    — i16 LE [-32767, 32767]
//! 15       2    Rx axis (clutch)   — i16 LE [-32767, 32767]
//! 17       2    Ry axis            — i16 LE [-32767, 32767]
//! 19       2    Rz axis            — i16 LE [-32767, 32767]
//! 21       2    Dial (Slider 1)    — i16 LE [-32767, 32767]
//! 23       2    Slider (Slider 0)  — i16 LE [-32767, 32767]
//! ```
//!
//! Total: 25 bytes including report ID.
//!
//! # Source
//! <https://github.com/Ultrawipf/OpenFFBoard> — `usb_hid_gamepad.c`
//! HID descriptor confirmed at commit `cbd64dbb678eaf17758a6f0aae8f3655d4ea0954`.

/// HID report ID for the gamepad input report.
pub const INPUT_REPORT_ID: u8 = 0x01;

/// Total length of the input report in bytes (including report ID).
pub const INPUT_REPORT_LEN: usize = 25;

/// Number of button bytes (64 buttons / 8 bits per byte).
pub const BUTTON_BYTES: usize = 8;

/// Maximum number of buttons supported.
pub const MAX_BUTTONS: usize = 64;

/// Number of analog axes.
pub const NUM_AXES: usize = 8;

/// Maximum absolute value for each axis (i16).
pub const AXIS_MAX: i16 = 32767;

// Byte offsets within the report (after report ID at byte 0).
const BUTTONS_OFFSET: usize = 1;
const AXES_OFFSET: usize = 9; // 1 (report ID) + 8 (buttons)

/// Parsed OpenFFBoard input report.
///
/// Contains all axes and buttons from a single HID input report.
/// Axis values are raw i16 in the range `[-32767, 32767]`.
///
/// # Axis mapping
/// OpenFFBoard firmware maps axes as: X (steering), Y, Z, Rx, Ry, Rz,
/// Dial (SL1), Slider (SL0). The exact physical mapping depends on the
/// user's OpenFFBoard configuration (which axes are connected to which
/// physical controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFFBoardInputReport {
    /// 64 buttons as an 8-byte bitmask. Button N is set if bit N is set
    /// (byte `N / 8`, bit `N % 8`).
    pub buttons: [u8; BUTTON_BYTES],
    /// Analog axes in firmware order: X, Y, Z, Rx, Ry, Rz, Dial, Slider.
    /// Each is i16 in `[-32767, 32767]`.
    pub axes: [i16; NUM_AXES],
}

impl OpenFFBoardInputReport {
    /// Parse an input report from raw HID bytes.
    ///
    /// Returns `None` if the report is too short or the report ID does not
    /// match `INPUT_REPORT_ID`.
    ///
    /// # Examples
    /// ```
    /// use racing_wheel_hid_openffboard_protocol::input::OpenFFBoardInputReport;
    ///
    /// let mut report = [0u8; 25];
    /// report[0] = 0x01; // report ID
    /// // Set steering (X axis) to 1000 LE
    /// report[9] = 0xE8;
    /// report[10] = 0x03;
    /// let parsed = OpenFFBoardInputReport::parse(&report).unwrap();
    /// assert_eq!(parsed.axes[0], 1000);
    /// ```
    pub fn parse(report: &[u8]) -> Option<Self> {
        if report.len() < INPUT_REPORT_LEN {
            return None;
        }
        if report[0] != INPUT_REPORT_ID {
            return None;
        }

        let mut buttons = [0u8; BUTTON_BYTES];
        buttons.copy_from_slice(&report[BUTTONS_OFFSET..BUTTONS_OFFSET + BUTTON_BYTES]);

        let mut axes = [0i16; NUM_AXES];
        for (i, axis) in axes.iter_mut().enumerate() {
            let offset = AXES_OFFSET + i * 2;
            *axis = i16::from_le_bytes([report[offset], report[offset + 1]]);
        }

        Some(Self { buttons, axes })
    }

    /// Returns the X axis (typically steering), raw i16.
    pub fn x(&self) -> i16 {
        self.axes[0]
    }

    /// Returns the Y axis, raw i16.
    pub fn y(&self) -> i16 {
        self.axes[1]
    }

    /// Returns the Z axis, raw i16.
    pub fn z(&self) -> i16 {
        self.axes[2]
    }

    /// Returns the Rx axis, raw i16.
    pub fn rx(&self) -> i16 {
        self.axes[3]
    }

    /// Returns the Ry axis, raw i16.
    pub fn ry(&self) -> i16 {
        self.axes[4]
    }

    /// Returns the Rz axis, raw i16.
    pub fn rz(&self) -> i16 {
        self.axes[5]
    }

    /// Returns the Dial axis (Slider 1), raw i16.
    pub fn dial(&self) -> i16 {
        self.axes[6]
    }

    /// Returns the Slider axis (Slider 0), raw i16.
    pub fn slider(&self) -> i16 {
        self.axes[7]
    }

    /// Returns the steering axis normalized to `[-1.0, 1.0]`.
    pub fn steering_normalized(&self) -> f32 {
        self.axes[0] as f32 / AXIS_MAX as f32
    }

    /// Returns `true` if button `n` (0-indexed) is pressed.
    ///
    /// Returns `false` for out-of-range button indices.
    pub fn button(&self, n: usize) -> bool {
        if n >= MAX_BUTTONS {
            return false;
        }
        let byte_idx = n / 8;
        let bit_idx = n % 8;
        (self.buttons[byte_idx] >> bit_idx) & 1 == 1
    }

    /// Returns a count of how many buttons are currently pressed.
    pub fn buttons_pressed(&self) -> u32 {
        self.buttons.iter().map(|b| b.count_ones()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report() -> [u8; INPUT_REPORT_LEN] {
        let mut r = [0u8; INPUT_REPORT_LEN];
        r[0] = INPUT_REPORT_ID;
        r
    }

    #[test]
    fn parse_all_zeros() {
        let r = make_report();
        let parsed = OpenFFBoardInputReport::parse(&r);
        assert!(parsed.is_some());
        let p = parsed.expect("parse should succeed");
        assert_eq!(p.axes, [0i16; NUM_AXES]);
        assert_eq!(p.buttons, [0u8; BUTTON_BYTES]);
    }

    #[test]
    fn parse_rejects_short_report() {
        let r = [INPUT_REPORT_ID; 10];
        assert!(OpenFFBoardInputReport::parse(&r).is_none());
    }

    #[test]
    fn parse_rejects_wrong_report_id() {
        let mut r = make_report();
        r[0] = 0x02;
        assert!(OpenFFBoardInputReport::parse(&r).is_none());
    }

    #[test]
    fn parse_steering_axis() {
        let mut r = make_report();
        // X axis = 5000 (little-endian: 0x88, 0x13)
        let bytes = 5000i16.to_le_bytes();
        r[9] = bytes[0];
        r[10] = bytes[1];
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        assert_eq!(p.x(), 5000);
    }

    #[test]
    fn parse_negative_axis() {
        let mut r = make_report();
        let bytes = (-10000i16).to_le_bytes();
        r[9] = bytes[0];
        r[10] = bytes[1];
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        assert_eq!(p.x(), -10000);
    }

    #[test]
    fn parse_all_axes() {
        let mut r = make_report();
        let values: [i16; NUM_AXES] = [100, -200, 300, -400, 500, -600, 700, -800];
        for (i, &v) in values.iter().enumerate() {
            let bytes = v.to_le_bytes();
            r[AXES_OFFSET + i * 2] = bytes[0];
            r[AXES_OFFSET + i * 2 + 1] = bytes[1];
        }
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        assert_eq!(p.axes, values);
        assert_eq!(p.y(), -200);
        assert_eq!(p.z(), 300);
        assert_eq!(p.rx(), -400);
        assert_eq!(p.ry(), 500);
        assert_eq!(p.rz(), -600);
        assert_eq!(p.dial(), 700);
        assert_eq!(p.slider(), -800);
    }

    #[test]
    fn button_pressed() {
        let mut r = make_report();
        r[1] = 0b0000_0101; // buttons 0 and 2 pressed
        r[2] = 0b1000_0000; // button 15 pressed
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        assert!(p.button(0));
        assert!(!p.button(1));
        assert!(p.button(2));
        assert!(p.button(15));
        assert!(!p.button(16));
        assert_eq!(p.buttons_pressed(), 3);
    }

    #[test]
    fn button_out_of_range_returns_false() {
        let r = make_report();
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        assert!(!p.button(64));
        assert!(!p.button(100));
    }

    #[test]
    fn steering_normalized_full_positive() {
        let mut r = make_report();
        let bytes = AXIS_MAX.to_le_bytes();
        r[9] = bytes[0];
        r[10] = bytes[1];
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        let normalized = p.steering_normalized();
        assert!((normalized - 1.0).abs() < 0.001);
    }

    #[test]
    fn steering_normalized_full_negative() {
        let mut r = make_report();
        let bytes = (-AXIS_MAX).to_le_bytes();
        r[9] = bytes[0];
        r[10] = bytes[1];
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        let normalized = p.steering_normalized();
        assert!((normalized + 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_accepts_longer_report() {
        let mut r = [0u8; 30];
        r[0] = INPUT_REPORT_ID;
        assert!(OpenFFBoardInputReport::parse(&r).is_some());
    }

    #[test]
    fn max_axis_values() {
        let mut r = make_report();
        // Set all axes to max positive
        for i in 0..NUM_AXES {
            let bytes = AXIS_MAX.to_le_bytes();
            r[AXES_OFFSET + i * 2] = bytes[0];
            r[AXES_OFFSET + i * 2 + 1] = bytes[1];
        }
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        for &axis in &p.axes {
            assert_eq!(axis, AXIS_MAX);
        }
    }

    #[test]
    fn min_axis_values() {
        let mut r = make_report();
        // The HID descriptor says LOGICAL_MINIMUM is -32767, not -32768
        let min_val: i16 = -32767;
        for i in 0..NUM_AXES {
            let bytes = min_val.to_le_bytes();
            r[AXES_OFFSET + i * 2] = bytes[0];
            r[AXES_OFFSET + i * 2 + 1] = bytes[1];
        }
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        for &axis in &p.axes {
            assert_eq!(axis, min_val);
        }
    }

    #[test]
    fn all_buttons_pressed() {
        let mut r = make_report();
        for byte in &mut r[1..=8] {
            *byte = 0xFF;
        }
        let p = OpenFFBoardInputReport::parse(&r).expect("parse should succeed");
        assert_eq!(p.buttons_pressed(), 64);
        for n in 0..64 {
            assert!(p.button(n), "button {n} should be pressed");
        }
    }
}
