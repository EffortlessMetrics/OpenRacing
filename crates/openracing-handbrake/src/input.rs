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
    fn test_parse_gamepad() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0xFF, 0xFF];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;

        assert_eq!(input.raw_value, 0xFFFF);
        assert!(input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_zero() -> Result<(), Box<dyn std::error::Error>> {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 0);
        assert!(!input.is_engaged);
        Ok(())
    }

    #[test]
    fn test_parse_gamepad_engagement_threshold() -> Result<(), Box<dyn std::error::Error>> {
        // Value of 100 should not be engaged (threshold is > 100)
        let data = vec![0x00, 0x00, 100, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 100);
        assert!(!input.is_engaged);

        // Value of 101 should be engaged
        let data = vec![0x00, 0x00, 101, 0x00];
        let input = HandbrakeInput::parse_gamepad(&data).map_err(|e| e.to_string())?;
        assert_eq!(input.raw_value, 101);
        assert!(input.is_engaged);
        Ok(())
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
    fn test_normalized_zero_range() {
        let input = HandbrakeInput {
            raw_value: 5000,
            is_engaged: false,
            calibration_min: 5000,
            calibration_max: 5000,
        };
        assert!((input.normalized()).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalized_clamped_above_max() {
        let input = HandbrakeInput {
            raw_value: 10000,
            is_engaged: true,
            calibration_min: 1000,
            calibration_max: 5000,
        };
        assert!((input.normalized() - 1.0).abs() < f32::EPSILON);
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
    fn test_calibration_default() {
        let calibration = HandbrakeCalibration::default();
        assert_eq!(calibration.min, 0);
        assert_eq!(calibration.max, MAX_ANALOG_VALUE);
        assert_eq!(calibration.center, None);
    }

    #[test]
    fn test_disconnected() {
        let data = vec![0x00];
        let result = HandbrakeInput::parse_gamepad(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_handbrake_input_default() {
        let input = HandbrakeInput::default();
        assert_eq!(input.raw_value, 0);
        assert!(!input.is_engaged);
        assert_eq!(input.calibration_min, 0);
        assert_eq!(input.calibration_max, MAX_ANALOG_VALUE);
    }

    #[test]
    fn test_calibrate_method() {
        let mut input = HandbrakeInput::default();
        input.calibrate(500, 8000);
        assert_eq!(input.calibration_min, 500);
        assert_eq!(input.calibration_max, 8000);
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn prop_normalized_within_unit_range(min in 0u16..=32767u16, max in 32768u16..=65535u16) {
            // Constrain raw_value >= min to avoid u16 subtraction overflow in normalized()
            let raw_value = min.saturating_add((max - min) / 2);
            let input = HandbrakeInput {
                raw_value,
                is_engaged: raw_value > 100,
                calibration_min: min,
                calibration_max: max,
            };
            let norm = input.normalized();
            prop_assert!(norm >= 0.0, "normalized must be >= 0, got {}", norm);
            prop_assert!(norm <= 1.0, "normalized must be <= 1, got {}", norm);
        }

        #[test]
        fn prop_parse_gamepad_succeeds_for_sufficient_data(
            data in proptest::collection::vec(any::<u8>(), 4..=64),
        ) {
            let result = HandbrakeInput::parse_gamepad(&data);
            prop_assert!(result.is_ok());
        }

        #[test]
        fn prop_parse_gamepad_fails_for_short_data(
            data in proptest::collection::vec(any::<u8>(), 0..4usize),
        ) {
            let result = HandbrakeInput::parse_gamepad(&data);
            prop_assert!(result.is_err());
        }

        #[test]
        fn prop_engagement_threshold_consistent(lo in 0u8..=255u8, hi in 0u8..=255u8) {
            let data = vec![0x00, 0x00, lo, hi];
            if let Ok(input) = HandbrakeInput::parse_gamepad(&data) {
                let expected_engaged = input.raw_value > 100;
                prop_assert_eq!(input.is_engaged, expected_engaged);
            }
        }

        #[test]
        fn prop_calibration_sample_tracks_extremes(samples in proptest::collection::vec(1u16..=65534u16, 1..50)) {
            let mut calibration = HandbrakeCalibration::new();
            for &s in &samples {
                calibration.sample(s);
            }
            let expected_min = *samples.iter().min().expect("non-empty");
            let expected_max = *samples.iter().max().expect("non-empty");
            prop_assert_eq!(calibration.min, expected_min);
            prop_assert_eq!(calibration.max, expected_max);
        }
    }
}
