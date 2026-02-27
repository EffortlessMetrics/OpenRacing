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
        let report = ButtonBoxInputReport::parse_gamepad(&data).unwrap();

        assert_eq!(report.buttons, 0x0001);
        assert_eq!(report.axis_x, 0x7F80i16 as i16);
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
        let mut report = ButtonBoxInputReport::default();

        report.axis_x = 1000;
        report.axis_y = 2000;
        report.axis_z = 3000;
        report.axis_rz = 4000;

        assert_eq!(report.axis(0), 1000);
        assert_eq!(report.axis(1), 2000);
        assert_eq!(report.axis(2), 3000);
        assert_eq!(report.axis(3), 4000);
        assert_eq!(report.axis(4), 0);
    }

    #[test]
    fn test_axis_normalized() {
        let mut report = ButtonBoxInputReport::default();

        report.axis_x = i16::MAX;
        assert!((report.axis_normalized(0) - 1.0).abs() < 0.001);

        report.axis_x = 0;
        assert!((report.axis_normalized(0)).abs() < 0.001);
    }
}
