//! Joystick/steering calibration

use crate::{AxisCalibration, CalibrationPoint, CalibrationResult};

pub struct JoystickCalibrator {
    points: Vec<CalibrationPoint>,
    axis_index: usize,
}

impl JoystickCalibrator {
    pub fn new(axis_index: usize) -> Self {
        Self {
            points: Vec::new(),
            axis_index,
        }
    }

    pub fn add_sample(&mut self, raw: u16, expected_normalized: f32) {
        self.points
            .push(CalibrationPoint::new(raw, expected_normalized));
    }

    pub fn calibrate(&self) -> CalibrationResult<AxisCalibration> {
        if self.points.is_empty() {
            return Err(crate::CalibrationError::NotComplete);
        }

        // Find min/max
        let min_raw = self.points.iter().map(|p| p.raw).min().unwrap_or(0);
        let max_raw = self.points.iter().map(|p| p.raw).max().unwrap_or(0xFFFF);

        // Check if there's a center point
        let center = self
            .points
            .iter()
            .find(|p| (p.normalized - 0.5).abs() < 0.1)
            .map(|p| p.raw);

        Ok(AxisCalibration {
            min: min_raw,
            center,
            max: max_raw,
            deadzone_min: min_raw,
            deadzone_max: max_raw,
        })
    }

    pub fn reset(&mut self) {
        self.points.clear();
    }
}

pub fn calibrate_joystick_axis(samples: &[(u16, f32)]) -> CalibrationResult<AxisCalibration> {
    let mut calibrator = JoystickCalibrator::new(0);
    for (raw, normalized) in samples {
        calibrator.add_sample(*raw, *normalized);
    }
    calibrator.calibrate()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_joystick_calibrator_empty() {
        let calibrator = JoystickCalibrator::new(0);
        let result = calibrator.calibrate();
        assert!(result.is_err());
    }

    #[test]
    fn test_joystick_calibrator_samples() {
        let mut calibrator = JoystickCalibrator::new(0);

        calibrator.add_sample(0, 0.0);
        calibrator.add_sample(32768, 0.5);
        calibrator.add_sample(65535, 1.0);

        let result = calibrator.calibrate().unwrap();
        assert_eq!(result.min, 0);
        assert_eq!(result.max, 65535);
    }

    #[test]
    fn test_calibrate_joystick_axis() {
        let samples = vec![(0, 0.0), (32768, 0.5), (65535, 1.0)];

        let result = calibrate_joystick_axis(&samples).unwrap();
        assert_eq!(result.min, 0);
        assert_eq!(result.max, 65535);
    }

    #[test]
    fn test_reset() {
        let mut calibrator = JoystickCalibrator::new(0);

        calibrator.add_sample(0, 0.0);
        calibrator.add_sample(65535, 1.0);

        calibrator.reset();

        let result = calibrator.calibrate();
        assert!(result.is_err());
    }
}
