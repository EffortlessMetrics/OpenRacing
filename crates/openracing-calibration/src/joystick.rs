//! Joystick/steering calibration

use crate::{AxisCalibration, CalibrationPoint, CalibrationResult};

/// Collects raw joystick/steering axis samples and produces a calibration.
///
/// Feed `(raw, expected_normalized)` pairs while the user sweeps the axis,
/// then call [`calibrate`](Self::calibrate) to compute min/max/center.
///
/// # Examples
///
/// ```
/// use openracing_calibration::JoystickCalibrator;
///
/// let mut cal = JoystickCalibrator::new(0);
/// cal.add_sample(0, 0.0);
/// cal.add_sample(32768, 0.5);
/// cal.add_sample(65535, 1.0);
///
/// let axis = cal.calibrate().expect("calibration should succeed");
/// assert_eq!(axis.min, 0);
/// assert_eq!(axis.max, 65535);
/// ```
pub struct JoystickCalibrator {
    points: Vec<CalibrationPoint>,
    #[allow(dead_code)]
    axis_index: usize,
}

impl JoystickCalibrator {
    /// Creates a new calibrator for the given axis index.
    pub fn new(axis_index: usize) -> Self {
        Self {
            points: Vec::new(),
            axis_index,
        }
    }

    /// Records a raw sample paired with its expected normalized value.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::JoystickCalibrator;
    ///
    /// let mut cal = JoystickCalibrator::new(0);
    /// cal.add_sample(0, 0.0);
    /// cal.add_sample(32768, 0.5);
    /// cal.add_sample(65535, 1.0);
    /// // Samples are stored for calibration
    /// ```
    pub fn add_sample(&mut self, raw: u16, expected_normalized: f32) {
        self.points
            .push(CalibrationPoint::new(raw, expected_normalized));
    }

    /// Computes an [`AxisCalibration`] from the collected samples.
    ///
    /// Automatically detects a center point if any sample is near 0.5.
    /// Returns an error if no samples have been added.
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

    /// Discards all collected samples.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::JoystickCalibrator;
    ///
    /// let mut cal = JoystickCalibrator::new(0);
    /// cal.add_sample(100, 0.0);
    /// cal.reset();
    /// assert!(cal.calibrate().is_err());
    /// ```
    pub fn reset(&mut self) {
        self.points.clear();
    }
}

/// Convenience function to calibrate a joystick axis from `(raw, normalized)` pairs.
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use openracing_calibration::calibrate_joystick_axis;
///
/// let samples = &[(0, 0.0), (32768, 0.5), (65535, 1.0)];
/// let axis = calibrate_joystick_axis(samples)?;
///
/// assert_eq!(axis.min, 0);
/// assert_eq!(axis.max, 65535);
/// // Center detected from the 0.5 sample
/// assert!(axis.center.is_some());
/// # Ok(())
/// # }
/// ```
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

        let result = calibrator.calibrate().expect("calibrate should succeed");
        assert_eq!(result.min, 0);
        assert_eq!(result.max, 65535);
    }

    #[test]
    fn test_joystick_center_detected() {
        let mut calibrator = JoystickCalibrator::new(0);
        calibrator.add_sample(0, 0.0);
        calibrator.add_sample(32768, 0.5); // near 0.5 → should be center
        calibrator.add_sample(65535, 1.0);

        let result = calibrator.calibrate().expect("calibrate should succeed");
        assert_eq!(result.center, Some(32768));
    }

    #[test]
    fn test_joystick_no_center_when_far_from_half() {
        let mut calibrator = JoystickCalibrator::new(0);
        calibrator.add_sample(0, 0.0);
        calibrator.add_sample(65535, 1.0);
        // No sample near 0.5, so center should be None

        let result = calibrator.calibrate().expect("calibrate should succeed");
        assert!(result.center.is_none());
    }

    #[test]
    fn test_joystick_single_sample() {
        let mut calibrator = JoystickCalibrator::new(0);
        calibrator.add_sample(500, 0.3);

        let result = calibrator
            .calibrate()
            .expect("single sample should succeed");
        assert_eq!(result.min, 500);
        assert_eq!(result.max, 500);
    }

    #[test]
    fn test_calibrate_joystick_axis() {
        let samples = vec![(0, 0.0), (32768, 0.5), (65535, 1.0)];

        let result = calibrate_joystick_axis(&samples).expect("calibrate should succeed");
        assert_eq!(result.min, 0);
        assert_eq!(result.max, 65535);
    }

    #[test]
    fn test_calibrate_joystick_axis_empty_fails() {
        let result = calibrate_joystick_axis(&[]);
        assert!(result.is_err());
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

    #[test]
    fn test_joystick_deadzone_matches_range() {
        let mut calibrator = JoystickCalibrator::new(0);
        calibrator.add_sample(100, 0.0);
        calibrator.add_sample(900, 1.0);

        let result = calibrator.calibrate().expect("calibrate should succeed");
        // Deadzone should be set to the same as min/max
        assert_eq!(result.deadzone_min, result.min);
        assert_eq!(result.deadzone_max, result.max);
    }
}
