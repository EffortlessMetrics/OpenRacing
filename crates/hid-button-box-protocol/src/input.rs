//! Input report parsing for button boxes

use super::{ButtonBoxError, ButtonBoxResult, MAX_BUTTONS, REPORT_SIZE_GAMEPAD};
use openracing_hid_common::ReportParser;

#[derive(Debug, Clone)]
pub struct ButtonBoxInputReport {
    pub buttons: u32,
    pub axis_x: i16,
    pub axis_y: i16,
    pub axis_z: i16,
    pub axis_rz: i16,
    pub hat: u8,
}

impl ButtonBoxInputReport {
    pub fn parse_gamepad(data: &[u8]) -> ButtonBoxResult<Self> {
        if data.len() < REPORT_SIZE_GAMEPAD {
            return Err(ButtonBoxError::InvalidReportSize {
                expected: REPORT_SIZE_GAMEPAD,
                actual: data.len(),
            });
        }

        let mut parser = ReportParser::from_slice(data);

        let buttons = parser.read_u16_le()? as u32;
        let axis_x = parser.read_i16_le()?;
        let axis_y = parser.read_i16_le()?;
        let axis_z = parser.read_i16_le()?;
        let hat = parser.read_u8()?;

        let _ = parser.read_u8()?;

        Ok(Self {
            buttons,
            axis_x,
            axis_y,
            axis_z,
            axis_rz: 0,
            hat,
        })
    }

    pub fn parse_extended(data: &[u8]) -> ButtonBoxResult<Self> {
        if data.len() < 12 {
            return Err(ButtonBoxError::InvalidReportSize {
                expected: 12,
                actual: data.len(),
            });
        }

        let mut parser = ReportParser::from_slice(data);

        let buttons = parser.read_u32_le()?;
        let axis_x = parser.read_i16_le()?;
        let axis_y = parser.read_i16_le()?;
        let axis_z = parser.read_i16_le()?;
        let axis_rz = parser.read_i16_le()?;
        let hat = parser.read_u8()?;

        Ok(Self {
            buttons,
            axis_x,
            axis_y,
            axis_z,
            axis_rz,
            hat,
        })
    }

    pub fn button(&self, index: usize) -> bool {
        if index >= MAX_BUTTONS {
            return false;
        }
        (self.buttons & (1 << index)) != 0
    }

    pub fn set_button(&mut self, index: usize, value: bool) {
        if index >= MAX_BUTTONS {
            return;
        }
        if value {
            self.buttons |= 1 << index;
        } else {
            self.buttons &= !(1 << index);
        }
    }

    pub fn button_count(&self) -> usize {
        self.buttons.count_ones() as usize
    }

    pub fn hat_direction(&self) -> HatDirection {
        match self.hat {
            0 => HatDirection::Up,
            1 => HatDirection::UpRight,
            2 => HatDirection::Right,
            3 => HatDirection::DownRight,
            4 => HatDirection::Down,
            5 => HatDirection::DownLeft,
            6 => HatDirection::Left,
            7 => HatDirection::UpLeft,
            _ => HatDirection::Neutral,
        }
    }

    pub fn axis(&self, index: usize) -> i16 {
        match index {
            0 => self.axis_x,
            1 => self.axis_y,
            2 => self.axis_z,
            3 => self.axis_rz,
            _ => 0,
        }
    }

    pub fn axis_normalized(&self, index: usize) -> f32 {
        let value = self.axis(index);
        value as f32 / i16::MAX as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HatDirection {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
    #[default]
    Neutral,
}

impl Default for ButtonBoxInputReport {
    fn default() -> Self {
        Self {
            buttons: 0,
            axis_x: 0,
            axis_y: 0,
            axis_z: 0,
            axis_rz: 0,
            hat: 0xFF,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gamepad_report() -> [u8; 10] {
        [0x01, 0x00, 0x80, 0x7F, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00]
    }

    #[test]
    fn test_parse_gamepad() {
        let data = make_gamepad_report();
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.buttons, 0x0001);
            assert_eq!(report.axis_x, 0x7F80_i16);
        }
    }

    #[test]
    fn test_invalid_report_size() {
        let data = vec![0u8; 4];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        assert!(matches!(
            result,
            Err(ButtonBoxError::InvalidReportSize { .. })
        ));
    }

    #[test]
    fn test_button_access() {
        let mut report = ButtonBoxInputReport::default();

        report.set_button(0, true);
        assert!(report.button(0));

        report.set_button(7, true);
        assert!(report.button(7));

        report.set_button(0, false);
        assert!(!report.button(0));
        assert!(report.button(7));
    }

    #[test]
    fn test_button_count() {
        let mut report = ButtonBoxInputReport::default();

        assert_eq!(report.button_count(), 0);

        report.set_button(0, true);
        report.set_button(5, true);
        report.set_button(15, true);

        assert_eq!(report.button_count(), 3);
    }

    #[test]
    fn test_hat_direction() {
        let mut report = ButtonBoxInputReport::default();

        for dir in 0..8 {
            report.hat = dir;
            assert_ne!(report.hat_direction(), HatDirection::Neutral);
        }

        report.hat = 0xFF;
        assert_eq!(report.hat_direction(), HatDirection::Neutral);
    }

    #[test]
    fn test_axis_access() {
        let report = ButtonBoxInputReport {
            axis_x: 1000,
            axis_y: 2000,
            axis_z: 3000,
            axis_rz: 4000,
            ..Default::default()
        };

        assert_eq!(report.axis(0), 1000);
        assert_eq!(report.axis(1), 2000);
        assert_eq!(report.axis(2), 3000);
        assert_eq!(report.axis(3), 4000);
        assert_eq!(report.axis(4), 0);
    }

    #[test]
    fn test_axis_normalized() {
        let mut report = ButtonBoxInputReport {
            axis_x: i16::MAX,
            ..Default::default()
        };
        assert!((report.axis_normalized(0) - 1.0).abs() < 0.001);

        report.axis_x = 0;
        assert!((report.axis_normalized(0)).abs() < 0.001);
    }

    #[test]
    fn test_axis_normalized_negative() {
        let report = ButtonBoxInputReport {
            axis_x: i16::MIN,
            ..Default::default()
        };
        let norm = report.axis_normalized(0);
        assert!(norm < 0.0);
        assert!(norm >= -1.01);
    }

    #[test]
    fn test_axis_normalized_out_of_range() {
        let report = ButtonBoxInputReport::default();
        assert!((report.axis_normalized(5)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_default_hat_is_neutral() {
        let report = ButtonBoxInputReport::default();
        assert_eq!(report.hat, 0xFF);
        assert_eq!(report.hat_direction(), HatDirection::Neutral);
    }

    #[test]
    fn test_default_buttons_zero() {
        let report = ButtonBoxInputReport::default();
        assert_eq!(report.buttons, 0);
        assert_eq!(report.button_count(), 0);
        for i in 0..32 {
            assert!(!report.button(i));
        }
    }

    #[test]
    fn test_default_axes_zero() {
        let report = ButtonBoxInputReport::default();
        assert_eq!(report.axis_x, 0);
        assert_eq!(report.axis_y, 0);
        assert_eq!(report.axis_z, 0);
        assert_eq!(report.axis_rz, 0);
    }

    #[test]
    fn test_parse_gamepad_exact_minimum() {
        // parse_gamepad size check requires >= 8 bytes, but parser reads 10 bytes total
        // 8 bytes passes size check but fails at parser level
        let data = [0u8; 8];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        assert!(result.is_err());

        // 10 bytes is the actual minimum for successful parse
        let data10 = [0u8; 10];
        let result10 = ButtonBoxInputReport::parse_gamepad(&data10);
        assert!(result10.is_ok());
    }

    #[test]
    fn test_parse_gamepad_7_bytes_fails() {
        let data = [0u8; 7];
        let result = ButtonBoxInputReport::parse_gamepad(&data);
        assert!(matches!(
            result,
            Err(ButtonBoxError::InvalidReportSize { .. })
        ));
    }

    #[test]
    fn test_parse_extended_exact_minimum() {
        // parse_extended size check requires >= 12 bytes, but parser reads 13 bytes total
        // 12 bytes passes size check but fails at parser level
        let data = [0u8; 12];
        let result = ButtonBoxInputReport::parse_extended(&data);
        assert!(result.is_err());

        // 13 bytes is the actual minimum for successful parse
        let data13 = [0u8; 13];
        let result13 = ButtonBoxInputReport::parse_extended(&data13);
        assert!(result13.is_ok());
    }

    #[test]
    fn test_parse_extended_13_bytes() {
        let data = [0u8; 13];
        let result = ButtonBoxInputReport::parse_extended(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_extended_11_bytes_fails() {
        let data = [0u8; 11];
        let result = ButtonBoxInputReport::parse_extended(&data);
        assert!(matches!(
            result,
            Err(ButtonBoxError::InvalidReportSize { .. })
        ));
    }

    #[test]
    fn test_button_31_highest() {
        let mut report = ButtonBoxInputReport::default();
        report.set_button(31, true);
        assert!(report.button(31));
        assert_eq!(report.button_count(), 1);

        // Out of range should be no-op
        report.set_button(32, true);
        assert!(!report.button(32));
        assert_eq!(report.button_count(), 1);
    }

    #[test]
    fn test_hat_direction_all_named() {
        let report_up = ButtonBoxInputReport {
            hat: 0,
            ..Default::default()
        };
        assert_eq!(report_up.hat_direction(), HatDirection::Up);

        let report_right = ButtonBoxInputReport {
            hat: 2,
            ..Default::default()
        };
        assert_eq!(report_right.hat_direction(), HatDirection::Right);

        let report_down = ButtonBoxInputReport {
            hat: 4,
            ..Default::default()
        };
        assert_eq!(report_down.hat_direction(), HatDirection::Down);

        let report_left = ButtonBoxInputReport {
            hat: 6,
            ..Default::default()
        };
        assert_eq!(report_left.hat_direction(), HatDirection::Left);
    }

    #[test]
    fn test_hat_direction_diagonals() {
        assert_eq!(
            ButtonBoxInputReport {
                hat: 1,
                ..Default::default()
            }
            .hat_direction(),
            HatDirection::UpRight
        );
        assert_eq!(
            ButtonBoxInputReport {
                hat: 3,
                ..Default::default()
            }
            .hat_direction(),
            HatDirection::DownRight
        );
        assert_eq!(
            ButtonBoxInputReport {
                hat: 5,
                ..Default::default()
            }
            .hat_direction(),
            HatDirection::DownLeft
        );
        assert_eq!(
            ButtonBoxInputReport {
                hat: 7,
                ..Default::default()
            }
            .hat_direction(),
            HatDirection::UpLeft
        );
    }

    #[test]
    fn test_parse_extended_with_axes() -> Result<(), Box<dyn std::error::Error>> {
        let mut data = [0u8; 13];
        // buttons = 0
        // axis_x = 1000 (0x03E8)
        data[4] = 0xE8;
        data[5] = 0x03;
        // axis_y = -1000 (0xFC18)
        data[6] = 0x18;
        data[7] = 0xFC;
        let report = ButtonBoxInputReport::parse_extended(&data).map_err(|e| e.to_string())?;
        assert_eq!(report.axis_x, 1000);
        assert_eq!(report.axis_y, -1000);
        Ok(())
    }

    #[test]
    fn test_hat_direction_default() {
        let dir = HatDirection::default();
        assert_eq!(dir, HatDirection::Neutral);
    }

    #[test]
    fn test_report_clone() {
        let mut report = ButtonBoxInputReport::default();
        report.set_button(5, true);
        report.axis_x = 123;
        let cloned = report.clone();
        assert_eq!(cloned.buttons, report.buttons);
        assert_eq!(cloned.axis_x, report.axis_x);
        assert_eq!(cloned.hat, report.hat);
    }
}
