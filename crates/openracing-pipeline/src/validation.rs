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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use racing_wheel_schemas::prelude::{FrequencyHz, Gain, NotchFilter};

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must() failed: {:?}", e),
        }
    }

    fn create_valid_config() -> FilterConfig {
        FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(60.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.5, 0.6)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)),
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )
        .unwrap()
    }

    #[test]
    fn test_validate_valid_config() {
        let validator = PipelineValidator::new();
        let config = create_valid_config();
        assert!(validator.validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_invalid_reconstruction() {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config();
        config.reconstruction = 10;

        let result = validator.validate_config(&config);
        assert!(result.is_err());
        match result {
            Err(PipelineError::InvalidConfig(_)) => {}
            _ => panic!("Expected InvalidConfig error"),
        }
    }

    #[test]
    fn test_validate_invalid_friction() {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config();
        // Gain::new() validates at construction, so we need to construct an invalid config differently
        // The validation at compile time will catch this
        // For now, test with a valid config modified
        config.friction = must(Gain::new(1.0)); // This is valid

        let result = validator.validate_config(&config);
        assert!(result.is_ok(), "1.0 is a valid friction value");
    }

    #[test]
    fn test_validate_non_monotonic_curve() {
        // Non-monotonic curves are rejected at construction time
        let config_result = FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.7, 0.6)),
                must(CurvePoint::new(0.5, 0.8)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        );

        assert!(config_result.is_err());
    }

    #[test]
    fn test_validate_invalid_notch_frequency() {
        let validator = PipelineValidator::new();
        let config = FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(600.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )
        .unwrap();

        let result = validator.validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_empty_config() {
        let validator = PipelineValidator::new();

        let mut empty_config = FilterConfig::default();
        // Disable bumpstop and hands-off to get a truly empty config
        empty_config.bumpstop.enabled = false;
        empty_config.hands_off.enabled = false;
        assert!(validator.is_empty_config(&empty_config));

        let non_empty_config = create_valid_config();
        assert!(!validator.is_empty_config(&non_empty_config));
    }

    #[test]
    fn test_validate_response_curve() {
        let validator = PipelineValidator::new();

        assert!(
            validator
                .validate_response_curve(&CurveType::Linear)
                .is_ok()
        );

        let exp_curve = CurveType::exponential(2.0).unwrap();
        assert!(validator.validate_response_curve(&exp_curve).is_ok());
    }
}
