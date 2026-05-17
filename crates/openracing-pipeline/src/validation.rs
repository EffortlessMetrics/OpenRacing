//! Pipeline validation logic
//!
//! This module provides validation for filter configurations before compilation.

use crate::types::PipelineError;
use openracing_curves::CurveError;
use openracing_curves::CurveType;
use racing_wheel_schemas::entities::FilterConfig;
use racing_wheel_schemas::prelude::CurvePoint;

/// Pipeline validator for filter configurations
///
/// Validates configurations before compilation to ensure they will produce
/// a valid pipeline.
#[derive(Debug, Clone, Default)]
pub struct PipelineValidator;

impl PipelineValidator {
    /// Create a new pipeline validator
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Validate a filter configuration
    ///
    /// Checks all parameters for valid ranges and constraints.
    ///
    /// # Errors
    ///
    /// Returns `PipelineError` if:
    /// - Reconstruction level is > 8
    /// - Gain values are outside [0.0, 1.0]
    /// - Curve points are not monotonic
    /// - Notch filter frequencies are invalid
    /// - Notch filter Q factors are invalid
    pub fn validate_config(&self, config: &FilterConfig) -> Result<(), PipelineError> {
        if config.reconstruction > 8 {
            return Err(PipelineError::InvalidConfig(format!(
                "Reconstruction level must be 0-8, got {}",
                config.reconstruction
            )));
        }

        if !(0.0..=1.0).contains(&config.friction.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Friction must be 0.0-1.0, got {}",
                config.friction.value()
            )));
        }

        if !(0.0..=1.0).contains(&config.damper.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Damper must be 0.0-1.0, got {}",
                config.damper.value()
            )));
        }

        if !(0.0..=1.0).contains(&config.inertia.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Inertia must be 0.0-1.0, got {}",
                config.inertia.value()
            )));
        }

        if !(0.0..=1.0).contains(&config.slew_rate.value()) {
            return Err(PipelineError::InvalidParameters(format!(
                "Slew rate must be 0.0-1.0, got {}",
                config.slew_rate.value()
            )));
        }

        self.validate_curve_monotonic(&config.curve_points)?;

        for (i, filter) in config.notch_filters.iter().enumerate() {
            if !((0.0..=500.0).contains(&filter.frequency.value())
                && filter.frequency.value() > 0.0)
            {
                return Err(PipelineError::InvalidParameters(format!(
                    "Notch filter {} frequency must be 0-500 Hz, got {}",
                    i,
                    filter.frequency.value()
                )));
            }

            if !((0.0..=20.0).contains(&filter.q_factor) && filter.q_factor > 0.0) {
                return Err(PipelineError::InvalidParameters(format!(
                    "Notch filter {} Q factor must be 0-20, got {}",
                    i, filter.q_factor
                )));
            }
        }

        self.validate_bumpstop_config(&config.bumpstop)?;
        self.validate_hands_off_config(&config.hands_off)?;

        Ok(())
    }

    /// Validate a response curve type
    ///
    /// # Errors
    ///
    /// Returns `PipelineError` if the curve is invalid.
    pub fn validate_response_curve(&self, curve: &CurveType) -> Result<(), PipelineError> {
        curve.validate().map_err(|e: CurveError| {
            PipelineError::InvalidConfig(format!("Invalid response curve: {}", e))
        })
    }

    /// Validate that curve points are monotonic
    fn validate_curve_monotonic(&self, curve_points: &[CurvePoint]) -> Result<(), PipelineError> {
        if curve_points.len() < 2 {
            return Err(PipelineError::InvalidConfig(
                "Curve must have at least 2 points".to_string(),
            ));
        }

        for window in curve_points.windows(2) {
            if window[1].input <= window[0].input {
                return Err(PipelineError::NonMonotonicCurve);
            }
        }

        let first = &curve_points[0];
        let last = &curve_points[curve_points.len() - 1];

        if first.input != 0.0 {
            return Err(PipelineError::InvalidConfig(
                "Curve must start at input 0.0".to_string(),
            ));
        }

        if last.input != 1.0 {
            return Err(PipelineError::InvalidConfig(
                "Curve must end at input 1.0".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate bumpstop configuration
    fn validate_bumpstop_config(
        &self,
        config: &racing_wheel_schemas::entities::BumpstopConfig,
    ) -> Result<(), PipelineError> {
        if config.enabled {
            if config.max_angle <= config.start_angle {
                return Err(PipelineError::InvalidParameters(
                    "Bumpstop max_angle must be greater than start_angle".to_string(),
                ));
            }

            if !(0.0..=1.0).contains(&config.stiffness) {
                return Err(PipelineError::InvalidParameters(format!(
                    "Bumpstop stiffness must be 0.0-1.0, got {}",
                    config.stiffness
                )));
            }

            if !(0.0..=1.0).contains(&config.damping) {
                return Err(PipelineError::InvalidParameters(format!(
                    "Bumpstop damping must be 0.0-1.0, got {}",
                    config.damping
                )));
            }
        }

        Ok(())
    }

    /// Validate hands-off detection configuration
    fn validate_hands_off_config(
        &self,
        config: &racing_wheel_schemas::entities::HandsOffConfig,
    ) -> Result<(), PipelineError> {
        if config.enabled {
            if !(0.0..=1.0).contains(&config.threshold) {
                return Err(PipelineError::InvalidParameters(format!(
                    "Hands-off threshold must be 0.0-1.0, got {}",
                    config.threshold
                )));
            }

            if config.timeout_seconds <= 0.0 {
                return Err(PipelineError::InvalidParameters(format!(
                    "Hands-off timeout must be positive, got {}",
                    config.timeout_seconds
                )));
            }
        }

        Ok(())
    }

    /// Check if a configuration would produce an empty pipeline
    ///
    /// Returns true if all effects are disabled or at default values.
    #[must_use]
    pub fn is_empty_config(&self, config: &FilterConfig) -> bool {
        config.reconstruction == 0
            && config.friction.value() == 0.0
            && config.damper.value() == 0.0
            && config.inertia.value() == 0.0
            && config.notch_filters.is_empty()
            && config.slew_rate.value() >= 1.0
            && config.torque_cap.value() >= 1.0
            && !config.bumpstop.enabled
            && !config.hands_off.enabled
            && Self::is_linear_curve(&config.curve_points)
    }

    /// Check if curve points represent a linear (identity) curve
    fn is_linear_curve(curve_points: &[CurvePoint]) -> bool {
        curve_points.len() == 2
            && curve_points[0].input == 0.0
            && curve_points[0].output == 0.0
            && curve_points[1].input == 1.0
            && curve_points[1].output == 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::prelude::{FrequencyHz, Gain, NotchFilter};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn create_valid_config() -> Result<FilterConfig, Box<dyn std::error::Error>> {
        Ok(FilterConfig::new_complete(
            4,
            Gain::new(0.1)?,
            Gain::new(0.15)?,
            Gain::new(0.05)?,
            vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
            Gain::new(0.8)?,
            vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(0.5, 0.6)?,
                CurvePoint::new(1.0, 1.0)?,
            ],
            Gain::new(0.9)?,
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )?)
    }

    fn assert_invalid_parameters(
        result: Result<(), PipelineError>,
        expected_message: &str,
    ) -> TestResult {
        match result {
            Err(PipelineError::InvalidParameters(message)) => {
                assert!(
                    message.contains(expected_message),
                    "expected error message to contain '{expected_message}', got '{message}'"
                );
                Ok(())
            }
            other => Err(format!("expected InvalidParameters, got {other:?}").into()),
        }
    }

    #[test]
    fn test_validate_valid_config() -> TestResult {
        let validator = PipelineValidator::new();
        let config = create_valid_config()?;
        validator.validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn test_validate_invalid_reconstruction() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.reconstruction = 10;

        let result = validator.validate_config(&config);
        match result {
            Err(PipelineError::InvalidConfig(message)) => {
                assert!(message.contains("Reconstruction level"));
                Ok(())
            }
            other => Err(format!("expected InvalidConfig error, got {other:?}").into()),
        }
    }

    #[test]
    fn test_validate_boundary_gains_accepts_zero_and_one() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.friction = Gain::ZERO;
        config.damper = Gain::FULL;
        config.inertia = Gain::ZERO;
        config.slew_rate = Gain::FULL;
        config.torque_cap = Gain::FULL;

        validator.validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn test_validate_non_monotonic_curve_is_rejected_by_schema() -> TestResult {
        let config_result = FilterConfig::new_complete(
            4,
            Gain::new(0.1)?,
            Gain::new(0.15)?,
            Gain::new(0.05)?,
            vec![],
            Gain::new(0.8)?,
            vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(0.7, 0.6)?,
                CurvePoint::new(0.5, 0.8)?,
                CurvePoint::new(1.0, 1.0)?,
            ],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        );

        assert!(config_result.is_err());
        Ok(())
    }

    #[test]
    fn test_validate_notch_frequency_above_pipeline_limit() -> TestResult {
        let validator = PipelineValidator::new();
        let config = FilterConfig::new_complete(
            4,
            Gain::new(0.1)?,
            Gain::new(0.15)?,
            Gain::new(0.05)?,
            vec![NotchFilter::new(FrequencyHz::new(600.0)?, 2.0, -12.0)?],
            Gain::new(0.8)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )?;

        assert_invalid_parameters(validator.validate_config(&config), "frequency")
    }

    #[test]
    fn test_validate_notch_q_factor_above_pipeline_limit() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.notch_filters = vec![NotchFilter::new(FrequencyHz::new(60.0)?, 20.1, -12.0)?];

        assert_invalid_parameters(validator.validate_config(&config), "Q factor")
    }

    #[test]
    fn test_validate_enabled_bumpstop_requires_ordered_angles() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.bumpstop = racing_wheel_schemas::entities::BumpstopConfig {
            enabled: true,
            start_angle: 540.0,
            max_angle: 540.0,
            stiffness: 0.5,
            damping: 0.5,
        };

        assert_invalid_parameters(validator.validate_config(&config), "max_angle")
    }

    #[test]
    fn test_validate_enabled_bumpstop_rejects_stiffness_out_of_range() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.bumpstop = racing_wheel_schemas::entities::BumpstopConfig {
            enabled: true,
            start_angle: 450.0,
            max_angle: 540.0,
            stiffness: 1.1,
            damping: 0.5,
        };

        assert_invalid_parameters(validator.validate_config(&config), "stiffness")
    }

    #[test]
    fn test_validate_disabled_bumpstop_ignores_inert_parameters() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.bumpstop = racing_wheel_schemas::entities::BumpstopConfig {
            enabled: false,
            start_angle: 540.0,
            max_angle: 450.0,
            stiffness: f32::NAN,
            damping: f32::INFINITY,
        };

        validator.validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn test_validate_enabled_hands_off_rejects_threshold_out_of_range() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.hands_off = racing_wheel_schemas::entities::HandsOffConfig {
            enabled: true,
            threshold: 1.1,
            timeout_seconds: 5.0,
        };

        assert_invalid_parameters(validator.validate_config(&config), "threshold")
    }

    #[test]
    fn test_validate_enabled_hands_off_requires_positive_timeout() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.hands_off = racing_wheel_schemas::entities::HandsOffConfig {
            enabled: true,
            threshold: 0.05,
            timeout_seconds: 0.0,
        };

        assert_invalid_parameters(validator.validate_config(&config), "timeout")
    }

    #[test]
    fn test_validate_disabled_hands_off_ignores_inert_parameters() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.hands_off = racing_wheel_schemas::entities::HandsOffConfig {
            enabled: false,
            threshold: f32::NAN,
            timeout_seconds: -1.0,
        };

        validator.validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn test_is_empty_config() -> TestResult {
        let validator = PipelineValidator::new();

        let mut empty_config = FilterConfig::default();
        // Disable bumpstop and hands-off to get a truly empty config
        empty_config.bumpstop.enabled = false;
        empty_config.hands_off.enabled = false;
        assert!(validator.is_empty_config(&empty_config));

        let non_empty_config = create_valid_config()?;
        assert!(!validator.is_empty_config(&non_empty_config));
        Ok(())
    }

    #[test]
    fn test_is_empty_config_requires_exact_linear_identity_curve() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = FilterConfig::default();
        config.bumpstop.enabled = false;
        config.hands_off.enabled = false;
        config.curve_points = vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.5)?,
            CurvePoint::new(1.0, 1.0)?,
        ];

        assert!(!validator.is_empty_config(&config));
        Ok(())
    }

    #[test]
    fn test_validate_response_curve() -> TestResult {
        let validator = PipelineValidator::new();

        validator.validate_response_curve(&CurveType::Linear)?;

        let exp_curve = CurveType::exponential(2.0)?;
        validator.validate_response_curve(&exp_curve)?;
        Ok(())
    }
}
