//! Input report parsing for Heusinkveld pedals

use super::{HeusinkveldError, HeusinkveldResult, MAX_LOAD_CELL_VALUE, REPORT_SIZE_INPUT};
use openracing_hid_common::ReportParser;

#[derive(Debug, Clone)]
pub struct HeusinkveldInputReport {
    pub throttle: u16,
    pub brake: u16,
    pub clutch: u16,
    pub status: u8,
}

impl HeusinkveldInputReport {
    pub fn parse(data: &[u8]) -> HeusinkveldResult<Self> {
        if data.len() < REPORT_SIZE_INPUT {
            return Err(HeusinkveldError::InvalidReportSize {
                expected: REPORT_SIZE_INPUT,
                actual: data.len(),
            });
        }

        let mut parser = ReportParser::from_slice(data);

        let throttle = parser.read_u16_le()?;
        let brake = parser.read_u16_le()?;
        let clutch = parser.read_u16_le()?;
        let status = parser.read_u8()?;

        Ok(Self {
            throttle,
            brake,
            clutch,
            status,
        })
    }

    pub fn throttle_normalized(&self) -> f32 {
        self.throttle as f32 / MAX_LOAD_CELL_VALUE as f32
    }

    pub fn brake_normalized(&self) -> f32 {
        self.brake as f32 / MAX_LOAD_CELL_VALUE as f32
    }

    pub fn clutch_normalized(&self) -> f32 {
        self.clutch as f32 / MAX_LOAD_CELL_VALUE as f32
    }

    pub fn is_connected(&self) -> bool {
        (self.status & 0x01) != 0
    }

    pub fn is_calibrated(&self) -> bool {
        (self.status & 0x02) != 0
    }

    pub fn has_fault(&self) -> bool {
        (self.status & 0x04) != 0
    }
}

impl Default for HeusinkveldInputReport {
    fn default() -> Self {
        Self {
            throttle: 0,
            brake: 0,
            clutch: 0,
            status: 0x03,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_report() -> [u8; 8] {
        let mut data = [0u8; 8];
        data[0] = 0x00;
        data[1] = 0x10;
        data[2] = 0x00;
        data[3] = 0x20;
        data[4] = 0x00;
        data[5] = 0x30;
        data[6] = 0x03;
        data[7] = 0x00;
        data
    }

    #[test]
    fn test_parse_report() {
        let data = make_test_report();
        let report = HeusinkveldInputReport::parse(&data).unwrap();

        assert_eq!(report.throttle, 0x1000);
        assert_eq!(report.brake, 0x2000);
        assert_eq!(report.clutch, 0x3000);
        assert_eq!(report.status, 0x03);
    }

    #[test]
    fn test_invalid_report_size() {
        let data = vec![0u8; 4];
        let result = HeusinkveldInputReport::parse(&data);
        assert!(matches!(
            result,
            Err(HeusinkveldError::InvalidReportSize { .. })
        ));
    }

    #[test]
    fn test_normalized_values() {
        let mut report = HeusinkveldInputReport::default();

        report.throttle = MAX_LOAD_CELL_VALUE / 2;
        assert!((report.throttle_normalized() - 0.5).abs() < 0.001);

        report.brake = MAX_LOAD_CELL_VALUE;
        assert!((report.brake_normalized() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_status_flags() {
        let mut report = HeusinkveldInputReport::default();

        report.status = 0x03;
        assert!(report.is_connected());
        assert!(report.is_calibrated());
        assert!(!report.has_fault());

        report.status = 0x01;
        assert!(report.is_connected());
        assert!(!report.is_calibrated());

        report.status = 0x04;
        assert!(!report.is_connected());
        assert!(!report.is_calibrated());
        assert!(report.has_fault());
    }
}
