//! Profile validation logic

use anyhow::{Result, anyhow};
use racing_wheel_schemas::config::{ProfileSchema, SchemaError};
use racing_wheel_schemas::prelude::{Profile, ProfileScope};

/// Profile validation context for comprehensive validation
#[derive(Debug, Default)]
pub struct ProfileValidationContext {
    /// Whether to validate schema version
    pub validate_schema_version: bool,
    /// Whether to validate curve monotonicity
    pub validate_curves: bool,
    /// Whether to validate RPM bands
    pub validate_rpm_bands: bool,
    /// Whether to validate scope
    pub validate_scope: bool,
}

impl ProfileValidationContext {
    /// Create a new validation context with all checks enabled
    pub fn new() -> Self {
        Self {
            validate_schema_version: true,
            validate_curves: true,
            validate_rpm_bands: true,
            validate_scope: true,
        }
    }

    /// Create a minimal validation context (schema only)
    pub fn minimal() -> Self {
        Self {
            validate_schema_version: true,
            validate_curves: false,
            validate_rpm_bands: false,
            validate_scope: false,
        }
    }

    /// Disable curve validation
    pub fn without_curves(mut self) -> Self {
        self.validate_curves = false;
        self
    }

    /// Disable RPM band validation
    pub fn without_rpm_bands(mut self) -> Self {
        self.validate_rpm_bands = false;
        self
    }
}

/// Profile validator wrapper with additional validation capabilities
pub struct ProfileValidator {
    schema_validator: racing_wheel_schemas::config::ProfileValidator,
}

impl ProfileValidator {
    /// Create a new profile validator
    pub fn new() -> Result<Self> {
        let schema_validator = racing_wheel_schemas::config::ProfileValidator::new()
            .map_err(|e| anyhow!("Failed to create schema validator: {}", e))?;

        Ok(Self { schema_validator })
    }

    /// Validate a profile JSON string
    pub fn validate_json(&self, json: &str) -> Result<ProfileSchema> {
        self.schema_validator
            .validate_json(json)
            .map_err(|e| match e {
                SchemaError::ValidationError { path, message } => {
                    anyhow!("Validation error at {}: {}", path, message)
                }
                SchemaError::NonMonotonicCurve => anyhow!("Curve points are not monotonic"),
                SchemaError::UnsupportedSchemaVersion(v) => {
                    anyhow!("Unsupported schema version: {}", v)
                }
                SchemaError::JsonError(e) => anyhow!("JSON parsing error: {}", e),
                SchemaError::SchemaCompilationError(s) => {
                    anyhow!("Schema compilation error: {}", s)
                }
                SchemaError::MultipleValidationErrors(errors) => {
                    let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                    anyhow!("Multiple validation errors: {}", messages.join("; "))
                }
            })
    }

    /// Validate a profile struct
    pub fn validate_profile(&self, profile: &Profile) -> Result<()> {
        self.validate_scope(&profile.scope)?;

        if let Some(ref led_config) = profile.led_config {
            self.validate_rpm_bands(&led_config.rpm_bands)?;
        }

        Ok(())
    }

    /// Validate with a specific context
    pub fn validate_with_context(
        &self,
        json: &str,
        context: &ProfileValidationContext,
    ) -> Result<ProfileSchema> {
        let profile = self.validate_json(json)?;

        if context.validate_curves {
            self.validate_curve_points(&profile.base.filters.curve_points)?;
        }

        if context.validate_rpm_bands
            && let Some(ref leds) = profile.leds
        {
            self.validate_rpm_bands(&leds.rpm_bands)?;
        }

        Ok(profile)
    }

    /// Validate profile scope
    pub fn validate_scope(&self, scope: &ProfileScope) -> Result<()> {
        if let Some(ref game) = scope.car
            && game.trim().is_empty()
        {
            return Err(anyhow!("scope.game: Game name cannot be empty"));
        }

        if let Some(ref car) = scope.car
            && car.trim().is_empty()
        {
            return Err(anyhow!("scope.car: Car name cannot be empty"));
        }

        if let Some(ref track) = scope.track
            && track.trim().is_empty()
        {
            return Err(anyhow!("scope.track: Track name cannot be empty"));
        }

        Ok(())
    }

    /// Validate RPM bands are sorted
    pub fn validate_rpm_bands(&self, bands: &[f32]) -> Result<()> {
        for window in bands.windows(2) {
            if window[1] <= window[0] {
                return Err(anyhow!("RPM bands must be in ascending order"));
            }
        }
        Ok(())
    }

    /// Validate curve points are monotonic
    pub fn validate_curve_points(
        &self,
        points: &[racing_wheel_schemas::config::CurvePoint],
    ) -> Result<()> {
        for window in points.windows(2) {
            if window[1].input <= window[0].input {
                return Err(anyhow!("Curve points are not monotonic"));
            }
        }
        Ok(())
    }
}

impl Default for ProfileValidator {
    fn default() -> Self {
        Self::new().expect("validator creation should succeed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_profile_json() -> &'static str {
        r#"{
            "schema": "wheel.profile/1",
            "scope": { "game": "iracing" },
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 540,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4,
                    "friction": 0.1,
                    "damper": 0.15,
                    "inertia": 0.05,
                    "notchFilters": [],
                    "slewRate": 0.8,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        }"#
    }

    #[test]
    fn test_validator_creation() {
        let validator = ProfileValidator::new().expect("should create validator");
        assert!(validator.validate_json(valid_profile_json()).is_ok());
    }

    #[test]
    fn test_validate_valid_json() {
        let validator = ProfileValidator::new().expect("should create validator");
        let result = validator.validate_json(valid_profile_json());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_invalid_json() {
        let validator = ProfileValidator::new().expect("should create validator");
        let invalid_json = r#"{"invalid": "json"}"#;
        let result = validator.validate_json(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_non_monotonic_curve() {
        let validator = ProfileValidator::new().expect("should create validator");
        let non_monotonic = r#"{
            "schema": "wheel.profile/1",
            "scope": {},
            "base": {
                "ffbGain": 0.8,
                "dorDeg": 540,
                "torqueCapNm": 15.0,
                "filters": {
                    "reconstruction": 4,
                    "friction": 0.1,
                    "damper": 0.15,
                    "inertia": 0.05,
                    "notchFilters": [],
                    "slewRate": 0.8,
                    "curvePoints": [
                        {"input": 0.0, "output": 0.0},
                        {"input": 0.8, "output": 0.8},
                        {"input": 0.5, "output": 0.5},
                        {"input": 1.0, "output": 1.0}
                    ]
                }
            }
        }"#;

        let result = validator.validate_json(non_monotonic);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_rpm_bands_sorted() {
        let validator = ProfileValidator::new().expect("should create validator");
        let sorted_bands = vec![5000.0, 7000.0, 9000.0];
        assert!(validator.validate_rpm_bands(&sorted_bands).is_ok());
    }

    #[test]
    fn test_validate_rpm_bands_unsorted() {
        let validator = ProfileValidator::new().expect("should create validator");
        let unsorted_bands = vec![7000.0, 5000.0, 9000.0];
        assert!(validator.validate_rpm_bands(&unsorted_bands).is_err());
    }

    #[test]
    fn test_validation_context() {
        let validator = ProfileValidator::new().expect("should create validator");
        let context = ProfileValidationContext::minimal();

        let result = validator.validate_with_context(valid_profile_json(), &context);
        assert!(result.is_ok());
    }
}
