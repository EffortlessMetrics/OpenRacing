//! Input validation error types.
//!
//! This module provides error types for input validation failures
//! including range checks, format validation, and constraint violations.

use core::fmt;

use crate::common::ErrorSeverity;

/// Validation error types.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ValidationError {
    /// Value out of range
    #[error("{field} value {value} is out of range [{min}, {max}]")]
    OutOfRange {
        /// Field name
        field: String,
        /// The invalid value
        value: String,
        /// Minimum allowed value
        min: String,
        /// Maximum allowed value
        max: String,
    },

    /// Value is required but missing
    #[error("Required field '{0}' is missing")]
    Required(String),

    /// Invalid format
    #[error("Invalid format for field '{field}': {reason}")]
    InvalidFormat {
        /// Field name
        field: String,
        /// Reason for the format error
        reason: String,
    },

    /// Value too long
    #[error("Field '{field}' value is too long: {actual} characters (max: {max})")]
    TooLong {
        /// Field name
        field: String,
        /// Actual length
        actual: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Value too short
    #[error("Field '{field}' value is too short: {actual} characters (min: {min})")]
    TooShort {
        /// Field name
        field: String,
        /// Actual length
        actual: usize,
        /// Minimum required length
        min: usize,
    },

    /// Invalid enum value
    #[error("Invalid value '{value}' for field '{field}', expected one of: {expected}")]
    InvalidEnumValue {
        /// Field name
        field: String,
        /// The invalid value
        value: String,
        /// Expected values
        expected: String,
    },

    /// Constraint violation
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    /// Invalid characters
    #[error("Field '{field}' contains invalid characters: {reason}")]
    InvalidCharacters {
        /// Field name
        field: String,
        /// Reason for the error
        reason: String,
    },

    /// Value not unique
    #[error("Value for field '{field}' must be unique but '{value}' already exists")]
    NotUnique {
        /// Field name
        field: String,
        /// The duplicate value
        value: String,
    },

    /// Dependency not met
    #[error("Dependency not met: field '{field}' requires '{dependency}' to be set")]
    DependencyNotMet {
        /// Field name
        field: String,
        /// Required dependency
        dependency: String,
    },

    /// Invalid type
    #[error("Invalid type for field '{field}': expected {expected}, got {actual}")]
    InvalidType {
        /// Field name
        field: String,
        /// Expected type
        expected: String,
        /// Actual type
        actual: String,
    },

    /// Numeric overflow
    #[error("Numeric overflow in field '{field}'")]
    NumericOverflow {
        /// Field name
        field: String,
    },

    /// Custom validation error
    #[error("Validation error: {0}")]
    Custom(String),
}

impl ValidationError {
    /// Get the error severity.
    pub fn severity(&self) -> ErrorSeverity {
        ErrorSeverity::Error
    }

    /// Create an out of range error for a numeric value.
    pub fn out_of_range<T: fmt::Debug>(field: impl Into<String>, value: T, min: T, max: T) -> Self {
        ValidationError::OutOfRange {
            field: field.into(),
            value: format!("{:?}", value),
            min: format!("{:?}", min),
            max: format!("{:?}", max),
        }
    }

    /// Create a required field error.
    pub fn required(field: impl Into<String>) -> Self {
        ValidationError::Required(field.into())
    }

    /// Create an invalid format error.
    pub fn invalid_format(field: impl Into<String>, reason: impl Into<String>) -> Self {
        ValidationError::InvalidFormat {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Create a too long error.
    pub fn too_long(field: impl Into<String>, actual: usize, max: usize) -> Self {
        ValidationError::TooLong {
            field: field.into(),
            actual,
            max,
        }
    }

    /// Create a too short error.
    pub fn too_short(field: impl Into<String>, actual: usize, min: usize) -> Self {
        ValidationError::TooShort {
            field: field.into(),
            actual,
            min,
        }
    }

    /// Create an invalid enum value error.
    pub fn invalid_enum(
        field: impl Into<String>,
        value: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        ValidationError::InvalidEnumValue {
            field: field.into(),
            value: value.into(),
            expected: expected.into(),
        }
    }

    /// Create a constraint violation error.
    pub fn constraint(msg: impl Into<String>) -> Self {
        ValidationError::ConstraintViolation(msg.into())
    }

    /// Create a custom validation error.
    pub fn custom(msg: impl Into<String>) -> Self {
        ValidationError::Custom(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_out_of_range() {
        let err = ValidationError::out_of_range("torque", 1.5_f32, -1.0_f32, 1.0_f32);
        let msg = err.to_string();
        assert!(msg.contains("torque"));
        assert!(msg.contains("1.5"));
    }

    #[test]
    fn test_validation_error_required() {
        let err = ValidationError::required("profile_id");
        assert_eq!(err.to_string(), "Required field 'profile_id' is missing");
    }

    #[test]
    fn test_validation_error_too_long() {
        let err = ValidationError::too_long("name", 100, 50);
        let msg = err.to_string();
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));
    }

    #[test]
    fn test_validation_error_invalid_enum() {
        let err = ValidationError::invalid_enum("mode", "invalid", "pid, raw, telemetry");
        let msg = err.to_string();
        assert!(msg.contains("invalid"));
        assert!(msg.contains("pid, raw, telemetry"));
    }

    #[test]
    fn test_validation_error_severity() {
        assert_eq!(
            ValidationError::required("test").severity(),
            ErrorSeverity::Error
        );
    }

    #[test]
    fn test_validation_error_is_std_error() {
        let err = ValidationError::required("test");
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_validation_error_equality() {
        let err1 = ValidationError::required("field");
        let err2 = ValidationError::required("field");
        assert_eq!(err1, err2);
    }
}
