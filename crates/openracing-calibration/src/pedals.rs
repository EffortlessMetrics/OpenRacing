//! Pedal calibration

use crate::{AxisCalibration, CalibrationResult};

pub struct PedalCalibrator {
    throttle_samples: Vec<u16>,
    brake_samples: Vec<u16>,
    clutch_samples: Vec<u16>,
}

impl PedalCalibrator {
    pub fn new() -> Self {
        Self {
            throttle_samples: Vec::new(),
            brake_samples: Vec::new(),
            clutch_samples: Vec::new(),
        }
    }

    pub fn add_throttle(&mut self, raw: u16) {
        self.throttle_samples.push(raw);
    }

    pub fn add_brake(&mut self, raw: u16) {
        self.brake_samples.push(raw);
    }

    pub fn add_clutch(&mut self, raw: u16) {
        self.clutch_samples.push(raw);
    }

    fn calibrate_axis(&self, samples: &[u16]) -> CalibrationResult<AxisCalibration> {
        if samples.is_empty() {
            return Err(crate::CalibrationError::NotComplete);
        }

        let min = *samples.iter().min().expect("samples not empty");
        let max = *samples.iter().max().expect("samples not empty");

        Ok(AxisCalibration::new(min, max))
    }

    pub fn calibrate(&self) -> CalibrationResult<Vec<AxisCalibration>> {
        let results = vec![
            self.calibrate_axis(&self.throttle_samples)?,
            self.calibrate_axis(&self.brake_samples)?,
            self.calibrate_axis(&self.clutch_samples)?,
        ];

        Ok(results)
    }

    pub fn reset(&mut self) {
        self.throttle_samples.clear();
        self.brake_samples.clear();
        self.clutch_samples.clear();
    }
}

impl Default for PedalCalibrator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn create_pedal_calibration(
    throttle: &[u16],
    brake: &[u16],
    clutch: &[u16],
) -> CalibrationResult<Vec<AxisCalibration>> {
    let mut calibrator = PedalCalibrator::new();

    for &raw in throttle {
        calibrator.add_throttle(raw);
    }
    for &raw in brake {
        calibrator.add_brake(raw);
    }
    for &raw in clutch {
        calibrator.add_clutch(raw);
    }

    calibrator.calibrate()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pedal_calibrator_empty() {
        let calibrator = PedalCalibrator::new();
        let result = calibrator.calibrate();
        assert!(result.is_err());
    }

    #[test]
    fn test_pedal_calibrator_throttle_only() {
        let mut calibrator = PedalCalibrator::new();

        calibrator.add_throttle(0);
        calibrator.add_throttle(32000);
        calibrator.add_throttle(65535);

        // Throttle-only should work now since we added brake/clutch too
        calibrator.add_brake(0);
        calibrator.add_brake(65535);
        calibrator.add_clutch(0);
        calibrator.add_clutch(65535);

        let result = calibrator.calibrate().expect("calibrate should succeed");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_pedal_calibrator_full() {
        let mut calibrator = PedalCalibrator::new();

        calibrator.add_throttle(0);
        calibrator.add_throttle(65535);

        calibrator.add_brake(0);
        calibrator.add_brake(65535);

        calibrator.add_clutch(0);
        calibrator.add_clutch(65535);

        let result = calibrator.calibrate().expect("calibrate should succeed");

        assert_eq!(result.len(), 3);

        // Throttle
        assert_eq!(result[0].min, 0);
        assert_eq!(result[0].max, 65535);

        // Brake
        assert_eq!(result[1].min, 0);
        assert_eq!(result[1].max, 65535);

        // Clutch
        assert_eq!(result[2].min, 0);
        assert_eq!(result[2].max, 65535);
    }

    #[test]
    fn test_reset() {
        let mut calibrator = PedalCalibrator::new();

        calibrator.add_throttle(0);
        calibrator.add_throttle(65535);

        calibrator.reset();

        let result = calibrator.calibrate();
        assert!(result.is_err());
    }

    #[test]
    fn test_create_pedal_calibration() {
        let result = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535]).expect("create should succeed");

        assert_eq!(result.len(), 3);
    }
}
