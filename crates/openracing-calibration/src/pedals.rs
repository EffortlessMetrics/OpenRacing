//! Pedal calibration

use crate::{AxisCalibration, CalibrationResult};

/// Collects raw pedal samples and produces per-axis calibrations.
///
/// Feed throttle, brake, and clutch readings via the `add_*` methods while
/// the user sweeps each pedal through its full range, then call
/// [`calibrate`](Self::calibrate) to compute the result.
///
/// # Examples
///
/// ```
/// use openracing_calibration::PedalCalibrator;
///
/// let mut cal = PedalCalibrator::new();
/// cal.add_throttle(0);
/// cal.add_throttle(65535);
/// cal.add_brake(0);
/// cal.add_brake(65535);
/// cal.add_clutch(0);
/// cal.add_clutch(65535);
///
/// let axes = cal.calibrate().expect("calibration should succeed");
/// assert_eq!(axes.len(), 3); // throttle, brake, clutch
/// ```
pub struct PedalCalibrator {
    throttle_samples: Vec<u16>,
    brake_samples: Vec<u16>,
    clutch_samples: Vec<u16>,
}

impl PedalCalibrator {
    /// Creates an empty pedal calibrator with no samples.
    pub fn new() -> Self {
        Self {
            throttle_samples: Vec::new(),
            brake_samples: Vec::new(),
            clutch_samples: Vec::new(),
        }
    }

    /// Records a raw throttle reading.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::PedalCalibrator;
    ///
    /// let mut cal = PedalCalibrator::new();
    /// cal.add_throttle(100);
    /// cal.add_throttle(900);
    /// // Samples are collected for later calibration
    /// ```
    pub fn add_throttle(&mut self, raw: u16) {
        self.throttle_samples.push(raw);
    }

    /// Records a raw brake reading.
    pub fn add_brake(&mut self, raw: u16) {
        self.brake_samples.push(raw);
    }

    /// Records a raw clutch reading.
    pub fn add_clutch(&mut self, raw: u16) {
        self.clutch_samples.push(raw);
    }

    fn calibrate_axis(&self, samples: &[u16]) -> CalibrationResult<AxisCalibration> {
        if samples.is_empty() {
            return Err(crate::CalibrationError::NotComplete);
        }

        let min = *samples
            .iter()
            .min()
            .ok_or(crate::CalibrationError::NotComplete)?;
        let max = *samples
            .iter()
            .max()
            .ok_or(crate::CalibrationError::NotComplete)?;

        Ok(AxisCalibration::new(min, max))
    }

    /// Computes calibrations for all three pedal axes.
    ///
    /// Returns `[throttle, brake, clutch]` calibrations. Fails if any axis
    /// has no samples.
    pub fn calibrate(&self) -> CalibrationResult<Vec<AxisCalibration>> {
        let results = vec![
            self.calibrate_axis(&self.throttle_samples)?,
            self.calibrate_axis(&self.brake_samples)?,
            self.calibrate_axis(&self.clutch_samples)?,
        ];

        Ok(results)
    }

    /// Discards all collected samples so calibration can be restarted.
    ///
    /// # Examples
    ///
    /// ```
    /// use openracing_calibration::PedalCalibrator;
    ///
    /// let mut cal = PedalCalibrator::new();
    /// cal.add_throttle(100);
    /// cal.add_brake(200);
    /// cal.add_clutch(300);
    /// cal.reset();
    /// // After reset, calibrate will fail (no samples)
    /// assert!(cal.calibrate().is_err());
    /// ```
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

/// Convenience function to calibrate all three pedal axes from raw sample slices.
///
/// Equivalent to creating a [`PedalCalibrator`], feeding all samples, and calling
/// [`calibrate`](PedalCalibrator::calibrate).
///
/// # Examples
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use openracing_calibration::create_pedal_calibration;
///
/// let axes = create_pedal_calibration(
///     &[0, 32768, 65535],  // throttle samples
///     &[0, 65535],          // brake samples
///     &[0, 65535],          // clutch samples
/// )?;
///
/// assert_eq!(axes.len(), 3);
/// assert_eq!(axes[0].min, 0);
/// assert_eq!(axes[0].max, 65535);
/// # Ok(())
/// # }
/// ```
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
    fn test_pedal_calibrator_throttle_only_fails_brake() {
        let mut calibrator = PedalCalibrator::new();
        calibrator.add_throttle(0);
        calibrator.add_throttle(65535);
        // Missing brake and clutch should fail
        let result = calibrator.calibrate();
        assert!(result.is_err());
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
    fn test_pedal_calibrator_single_sample_per_axis() {
        let mut calibrator = PedalCalibrator::new();
        calibrator.add_throttle(500);
        calibrator.add_brake(600);
        calibrator.add_clutch(700);

        let result = calibrator
            .calibrate()
            .expect("single sample should succeed");
        assert_eq!(result.len(), 3);
        // Single sample means min == max
        assert_eq!(result[0].min, 500);
        assert_eq!(result[0].max, 500);
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
        let result = create_pedal_calibration(&[0, 65535], &[0, 65535], &[0, 65535])
            .expect("create should succeed");

        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_create_pedal_calibration_empty_fails() {
        let result = create_pedal_calibration(&[], &[0, 65535], &[0, 65535]);
        assert!(result.is_err());
    }

    #[test]
    fn test_pedal_calibrator_default_equivalence() {
        let new = PedalCalibrator::new();
        let default = PedalCalibrator::default();
        // Both should fail calibration (empty)
        assert!(new.calibrate().is_err());
        assert!(default.calibrate().is_err());
    }

    #[test]
    fn test_pedal_calibrator_narrow_range() {
        let mut calibrator = PedalCalibrator::new();
        calibrator.add_throttle(100);
        calibrator.add_throttle(105);
        calibrator.add_brake(200);
        calibrator.add_brake(210);
        calibrator.add_clutch(300);
        calibrator.add_clutch(310);

        let result = calibrator.calibrate().expect("narrow range should succeed");
        assert_eq!(result[0].min, 100);
        assert_eq!(result[0].max, 105);
    }
}
