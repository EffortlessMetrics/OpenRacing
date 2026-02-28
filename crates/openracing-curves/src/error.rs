//! Error types for curve operations.

use std::fmt;

/// Error type for curve operations.
///
/// This error type covers all validation and configuration errors
/// that can occur when creating or validating curves.
#[derive(Debug, Clone, PartialEq)]
pub enum CurveError {
    /// Control point is outside the valid `[0,1]` range.
    ///
    /// All Bezier control points must have x and y coordinates
    /// in the range `[0.0, 1.0]` inclusive.
    ControlPointOutOfRange {
        /// Index of the control point (0-3 for cubic Bezier).
        point_index: usize,
        /// Which coordinate is out of range ("x" or "y").
        coordinate: &'static str,
        /// The invalid value.
        value: f32,
    },
    /// Invalid curve configuration.
    ///
    /// This covers errors like:
    /// - Exponential exponent <= 0 or non-finite
    /// - Logarithmic base <= 1 or non-finite
    /// - Invalid LUT size
    InvalidConfiguration(String),
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlPointOutOfRange {
                point_index,
                coordinate,
                value,
            } => {
                write!(
                    f,
                    "Control point {} {} coordinate {} is outside valid range [0,1]",
                    point_index, coordinate, value
                )
            }
            Self::InvalidConfiguration(msg) => {
                write!(f, "Invalid curve configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for CurveError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_control_point() {
        let err = CurveError::ControlPointOutOfRange {
            point_index: 2,
            coordinate: "x",
            value: 1.5,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Control point 2"));
        assert!(msg.contains("x coordinate"));
        assert!(msg.contains("1.5"));
    }

    #[test]
    fn test_error_display_invalid_config() {
        let err = CurveError::InvalidConfiguration("exponent must be positive".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid curve configuration"));
        assert!(msg.contains("exponent must be positive"));
    }

    #[test]
    fn test_error_is_std_error() {
        let err = CurveError::InvalidConfiguration("test".to_string());
        let _: &dyn std::error::Error = &err;
    }
}
