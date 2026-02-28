//! Output report generation for Simucube force feedback

use super::{MAX_TORQUE_NM, REPORT_SIZE_OUTPUT, SimucubeError, SimucubeResult};
use openracing_hid_common::ReportBuilder;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(non_snake_case)]
pub struct SimucubeOutputReport {
    pub sequence: u16,
    pub torque_cNm: i16,
    pub led_r: u8,
    pub led_g: u8,
    pub led_b: u8,
    pub effect_type: EffectType,
    pub effect_parameter: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectType {
    #[default]
    None = 0,
    Constant = 1,
    Ramp = 2,
    Square = 3,
    Sine = 4,
    Triangle = 5,
    SawtoothUp = 6,
    SawtoothDown = 7,
    Spring = 8,
    Damper = 9,
    Friction = 10,
}

impl SimucubeOutputReport {
    pub fn new(sequence: u16) -> Self {
        Self {
            sequence,
            torque_cNm: 0,
            led_r: 0,
            led_g: 0,
            led_b: 0,
            effect_type: EffectType::None,
            effect_parameter: 0,
        }
    }

    pub fn with_torque(mut self, torque_nm: f32) -> Self {
        self.torque_cNm = (torque_nm.clamp(-MAX_TORQUE_NM, MAX_TORQUE_NM) * 100.0) as i16;
        self
    }

    pub fn with_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.led_r = r;
        self.led_g = g;
        self.led_b = b;
        self
    }

    pub fn with_effect(mut self, effect_type: EffectType, parameter: u16) -> Self {
        self.effect_type = effect_type;
        self.effect_parameter = parameter;
        self
    }

    pub fn build(&self) -> SimucubeResult<Vec<u8>> {
        let mut builder = ReportBuilder::new(REPORT_SIZE_OUTPUT);

        builder.write_u8(0x01);
        builder.write_u16_le(self.sequence);
        builder.write_i16_le(self.torque_cNm);
        builder.write_u8(self.led_r);
        builder.write_u8(self.led_g);
        builder.write_u8(self.led_b);
        builder.write_u8(self.effect_type as u8);
        builder.write_u16_le(self.effect_parameter);

        let mut data = builder.into_inner();
        while data.len() < REPORT_SIZE_OUTPUT {
            data.push(0);
        }

        Ok(data)
    }

    pub fn validate_torque(&self) -> SimucubeResult<()> {
        let torque_nm = self.torque_cNm as f32 / 100.0;
        if torque_nm.abs() > MAX_TORQUE_NM {
            return Err(SimucubeError::InvalidTorque(torque_nm));
        }
        Ok(())
    }
}

impl Default for SimucubeOutputReport {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_report_default() {
        let report = SimucubeOutputReport::default();
        assert_eq!(report.sequence, 0);
        assert_eq!(report.torque_cNm, 0);
    }

    #[test]
    fn test_output_report_with_torque() {
        let report = SimucubeOutputReport::new(1).with_torque(10.5);

        assert_eq!(report.sequence, 1);
        assert_eq!(report.torque_cNm, 1050);
    }

    #[test]
    fn test_output_report_torque_clamping() {
        let report_max = SimucubeOutputReport::new(0).with_torque(100.0);
        assert_eq!(report_max.torque_cNm, (MAX_TORQUE_NM * 100.0) as i16);

        let report_min = SimucubeOutputReport::new(0).with_torque(-100.0);
        assert_eq!(report_min.torque_cNm, (-MAX_TORQUE_NM * 100.0) as i16);
    }

    #[test]
    fn test_output_report_with_rgb() {
        let report = SimucubeOutputReport::new(0).with_rgb(255, 128, 64);

        assert_eq!(report.led_r, 255);
        assert_eq!(report.led_g, 128);
        assert_eq!(report.led_b, 64);
    }

    #[test]
    fn test_output_report_build() {
        let report = SimucubeOutputReport::new(42).with_torque(15.0);
        let result = report.build();
        assert!(result.is_ok());
        if let Ok(data) = result {
            assert!(data.len() >= REPORT_SIZE_OUTPUT);
        }
    }

    #[test]
    fn test_output_report_with_effect() {
        let report = SimucubeOutputReport::new(0).with_effect(EffectType::Spring, 500);

        assert_eq!(report.effect_type, EffectType::Spring);
        assert_eq!(report.effect_parameter, 500);
    }

    #[test]
    fn test_validate_torque_valid() {
        let report = SimucubeOutputReport::new(0).with_torque(10.0);
        assert!(report.validate_torque().is_ok());
    }

    #[test]
    fn test_validate_torque_invalid() {
        let report = SimucubeOutputReport {
            torque_cNm: (MAX_TORQUE_NM * 200.0) as i16,
            ..Default::default()
        };
        assert!(matches!(
            report.validate_torque(),
            Err(SimucubeError::InvalidTorque(_))
        ));
    }

    #[test]
    fn test_effect_types() {
        assert_eq!(EffectType::None as u8, 0);
        assert_eq!(EffectType::Constant as u8, 1);
        assert_eq!(EffectType::Spring as u8, 8);
        assert_eq!(EffectType::Damper as u8, 9);
    }
}
