//! Output report generation for Asetek force feedback
//!
//! **Protocol note:** Asetek wheelbases expose a standard USB HID PID
//! force-feedback descriptor, which the Linux kernel (`hid-pidff` /
//! `hid-universal-pidff`) uses for standard FF effects. No Asetek-specific
//! quirk flags are applied in the upstream driver. This output report
//! structure is a simplified direct-motor-control interface (torque in
//! centi-Newton-metres, little-endian i16) for the RT hot path. The exact
//! vendor-specific byte layout has not been independently confirmed by
//! community USB descriptor dumps.

use super::{AsetekResult, MAX_TORQUE_NM, REPORT_SIZE_OUTPUT};
use openracing_hid_common::ReportBuilder;

#[derive(Debug, Clone, Copy)]
#[allow(non_snake_case)]
pub struct AsetekOutputReport {
    pub sequence: u16,
    pub torque_cNm: i16,
    pub led_mode: u8,
    pub led_value: u8,
}

impl AsetekOutputReport {
    pub fn new(sequence: u16) -> Self {
        Self {
            sequence,
            torque_cNm: 0,
            led_mode: 0,
            led_value: 0,
        }
    }

    pub fn with_torque(mut self, torque_nm: f32) -> Self {
        self.torque_cNm = (torque_nm.clamp(-MAX_TORQUE_NM, MAX_TORQUE_NM) * 100.0) as i16;
        self
    }

    pub fn with_led(mut self, mode: u8, value: u8) -> Self {
        self.led_mode = mode;
        self.led_value = value;
        self
    }

    pub fn build(&self) -> AsetekResult<Vec<u8>> {
        let mut builder = ReportBuilder::with_capacity(REPORT_SIZE_OUTPUT);

        builder.write_u16_le(self.sequence);
        builder.write_i16_le(self.torque_cNm);
        builder.write_u8(self.led_mode);
        builder.write_u8(self.led_value);

        let mut data = builder.into_inner();
        while data.len() < REPORT_SIZE_OUTPUT {
            data.push(0);
        }

        Ok(data)
    }
}

impl Default for AsetekOutputReport {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_report_default() {
        let report = AsetekOutputReport::default();
        assert_eq!(report.sequence, 0);
        assert_eq!(report.torque_cNm, 0);
    }

    #[test]
    fn test_output_report_with_torque() {
        let report = AsetekOutputReport::new(1).with_torque(10.5);

        assert_eq!(report.sequence, 1);
        assert_eq!(report.torque_cNm, 1050);
    }

    #[test]
    fn test_output_report_build() {
        let report = AsetekOutputReport::new(42).with_torque(15.0);
        let result = report.build();
        assert!(result.is_ok());
        if let Ok(data) = result {
            assert_eq!(data.len(), REPORT_SIZE_OUTPUT);
        }
    }
}
