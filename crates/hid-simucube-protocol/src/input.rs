//! Input report parsing for Simucube devices

use super::{ANGLE_SENSOR_MAX, SimucubeError, SimucubeResult};
use openracing_hid_common::ReportParser;

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
}
