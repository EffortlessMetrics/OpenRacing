//! Handbrake input parsing

use super::{HandbrakeResult, MAX_ANALOG_VALUE};

pub struct HandbrakeInput {
    pub raw_value: u16,
    pub is_engaged: bool,
    pub calibration_min: u16,
    pub calibration_max: u16,
}

impl HandbrakeInput {
    pub fn parse_gamepad(data: &[u8]) -> HandbrakeResult<Self> {
        if data.len() < 4 {
            return Err(super::HandbrakeError::Disconnected);
        }

        let raw_value = u16::from(data[2]) | (u16::from(data[3]) << 8);

        Ok(Self {
            raw_value,
            is_engaged: raw_value > 100,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        })
    }

    pub fn normalized(&self) -> f32 {
        let range = (self.calibration_max - self.calibration_min) as f32;
        if range > 0.0 {
            ((self.raw_value - self.calibration_min) as f32 / range).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn with_calibration(mut self, min: u16, max: u16) -> Self {
        self.calibration_min = min;
        self.calibration_max = max;
        self
    }

    pub fn calibrate(&mut self, min: u16, max: u16) {
        self.calibration_min = min;
        self.calibration_max = max;
    }
}

impl Default for HandbrakeInput {
    fn default() -> Self {
        Self {
            raw_value: 0,
            is_engaged: false,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        }
    }
}

pub struct HandbrakeCalibration {
    pub min: u16,
    pub max: u16,
    pub center: Option<u16>,
}

impl HandbrakeCalibration {
    pub fn new() -> Self {
        Self {
            min: 0,
            max: MAX_ANALOG_VALUE,
            center: None,
        }
    }

    pub fn sample(&mut self, value: u16) {
        if value < self.min || self.min == 0 {
            self.min = value;
        }
        if value > self.max || self.max == MAX_ANALOG_VALUE {
            self.max = value;
        }
    }

    pub fn apply(&self, input: &mut HandbrakeInput) {
        input.calibrate(self.min, self.max);
    }
}

impl Default for HandbrakeCalibration {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gamepad() {
        let data = vec![0x00, 0x00, 0xFF, 0xFF];
        let input = HandbrakeInput::parse_gamepad(&data).expect("parse should succeed");

        assert_eq!(input.raw_value, 0xFFFF);
        assert!(input.is_engaged);
    }

    #[test]
    fn test_normalized_full() {
        let input = HandbrakeInput {
            raw_value: MAX_ANALOG_VALUE,
            is_engaged: true,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };

        assert!((input.normalized() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_normalized_half() {
        let input = HandbrakeInput {
            raw_value: MAX_ANALOG_VALUE / 2,
            is_engaged: false,
            calibration_min: 0,
            calibration_max: MAX_ANALOG_VALUE,
        };

        assert!((input.normalized() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_with_calibration() {
        let input = HandbrakeInput::default().with_calibration(1000, 9000);

        assert_eq!(input.calibration_min, 1000);
        assert_eq!(input.calibration_max, 9000);
    }

    #[test]
    fn test_calibration() {
        let mut calibration = HandbrakeCalibration::new();

        calibration.sample(100);
        calibration.sample(50);
        calibration.sample(200);

        assert_eq!(calibration.min, 50);
        assert_eq!(calibration.max, 200);
    }

    #[test]
    fn test_calibration_apply() {
        let mut calibration = HandbrakeCalibration::new();
        calibration.sample(100);
        calibration.sample(9000);

        let mut input = HandbrakeInput::default();
        calibration.apply(&mut input);

        assert_eq!(input.calibration_min, 100);
        assert_eq!(input.calibration_max, 9000);
    }

    #[test]
    fn test_disconnected() {
        let data = vec![0x00];
        let result = HandbrakeInput::parse_gamepad(&data);
        assert!(result.is_err());
    }
}
