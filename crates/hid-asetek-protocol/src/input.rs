//! Input report parsing for Asetek wheelbases

use super::{AsetekError, AsetekResult};
use openracing_hid_common::ReportParser;

#[derive(Debug, Clone)]
pub struct AsetekInputReport {
    pub sequence: u16,
    pub wheel_angle: i32,
    pub wheel_speed: i16,
    pub torque: i16,
    pub temperature: u8,
    pub status: u8,
}

impl AsetekInputReport {
    pub fn parse(data: &[u8]) -> AsetekResult<Self> {
        if data.len() < 16 {
            return Err(AsetekError::InvalidReportSize {
                expected: 16,
                actual: data.len(),
            });
        }

        let mut parser = ReportParser::from_slice(data);

        let sequence = parser.read_u16_le()?;
        let wheel_angle = parser.read_i32_le()?;
        let wheel_speed = parser.read_i16_le()?;
        let torque = parser.read_i16_le()?;
        let temperature = parser.read_u8()?;
        let status = parser.read_u8()?;

        Ok(Self {
            sequence,
            wheel_angle,
            wheel_speed,
            torque,
            temperature,
            status,
        })
    }

    pub fn wheel_angle_degrees(&self) -> f32 {
        self.wheel_angle as f32 / 1000.0
    }

    pub fn wheel_speed_rad_s(&self) -> f32 {
        self.wheel_speed as f32 * std::f32::consts::PI / 1800.0
    }

    pub fn applied_torque_nm(&self) -> f32 {
        self.torque as f32 / 100.0
    }

    pub fn is_connected(&self) -> bool {
        (self.status & 0x01) != 0
    }

    pub fn is_enabled(&self) -> bool {
        (self.status & 0x02) != 0
    }
}

impl Default for AsetekInputReport {
    fn default() -> Self {
        Self {
            sequence: 0,
            wheel_angle: 0,
            wheel_speed: 0,
            torque: 0,
            temperature: 25,
            status: 0x03,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_report() {
        let data = vec![
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x88, 0x01,
            0x00, 0x00, 0x32, 0x03, 0x00, 0x00,
        ];
        let result = AsetekInputReport::parse(&data);
        assert!(result.is_ok());
        if let Ok(report) = result {
            assert_eq!(report.sequence, 1);
            assert_eq!(report.wheel_angle, 0);
        }
    }

    #[test]
    fn test_wheel_angle_degrees() {
        let report = AsetekInputReport {
            wheel_angle: 90000,
            ..Default::default()
        };

        let degrees = report.wheel_angle_degrees();
        assert!((degrees - 90.0).abs() < 0.1);
    }

    #[test]
    fn test_applied_torque() {
        let report = AsetekInputReport {
            torque: 1500,
            ..Default::default()
        };

        let torque = report.applied_torque_nm();
        assert!((torque - 15.0).abs() < 0.01);
    }
}
