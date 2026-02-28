//! Supported curve types for FFB response mapping.

use serde::{Deserialize, Serialize};

use crate::bezier::BezierCurve;
use crate::error::CurveError;
use crate::lut::CurveLut;

/// Supported curve types for FFB response mapping.
///
/// All curve types map input values in `[0,1]` to output values in `[0,1]`.
/// Each type supports LUT pre-computation for RT-safe evaluation.
///
/// # RT Safety
///
/// - `evaluate()`: NOT RT-safe for Bezier curves (uses Newton-Raphson iteration)
/// - `to_lut()`: NOT RT-safe (allocates, should be called at profile load)
/// - `CurveLut::lookup()`: RT-safe (O(1), no allocations)
///
/// # Example
///
/// ```
/// use openracing_curves::CurveType;
///
/// // Create an exponential curve for progressive response
/// let curve = CurveType::exponential(2.0)?;
///
/// // Evaluate directly (NOT RT-safe)
/// let output = curve.evaluate(0.5);
/// assert!((output - 0.25).abs() < 0.01); // 0.5^2 = 0.25
///
/// // Convert to LUT for RT-safe evaluation
/// let lut = curve.to_lut();
/// let rt_output = lut.lookup(0.5);
/// assert!((rt_output - 0.25).abs() < 0.02);
/// # Ok::<(), openracing_curves::CurveError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum CurveType {
    /// Linear curve: f(x) = x (identity mapping).
    #[default]
    Linear,

    /// Exponential curve: f(x) = x^exponent (where exponent > 0).
    ///
    /// - exponent < 1: compresses high values (fast response)
    /// - exponent = 1: linear
    /// - exponent > 1: expands high values (progressive response)
    Exponential {
        /// The exponent value (must be > 0 and finite).
        exponent: f32,
    },

    /// Logarithmic curve: f(x) = log_base(1 + x*(base-1)) / log_base(base).
    ///
    /// Normalized to map `[0,1]` → `[0,1]`.
    /// Provides compressed response for fine control.
    Logarithmic {
        /// The logarithm base (must be > 1 and finite).
        base: f32,
    },

    /// Cubic Bezier curve with custom control points.
    Bezier(BezierCurve),

    /// Custom pre-computed lookup table.
    Custom(Box<CurveLut>),
}

impl CurveType {
    /// Create a new exponential curve with the given exponent.
    ///
    /// # Arguments
    ///
    /// * `exponent` - The exponent value (must be > 0 and finite)
    ///
    /// # Returns
    ///
    /// * `Ok(CurveType::Exponential)` if exponent is valid
    /// * `Err(CurveError)` if exponent is invalid
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_curves::CurveType;
    ///
    /// // Progressive response (harder to push at high forces)
    /// let progressive = CurveType::exponential(2.0)?;
    ///
    /// // Quick response (easier to push at low forces)
    /// let quick = CurveType::exponential(0.5)?;
    /// # Ok::<(), openracing_curves::CurveError>(())
    /// ```
    pub fn exponential(exponent: f32) -> Result<Self, CurveError> {
        if !exponent.is_finite() {
            return Err(CurveError::InvalidConfiguration(
                "Exponential exponent must be finite".to_string(),
            ));
        }
        if exponent <= 0.0 {
            return Err(CurveError::InvalidConfiguration(format!(
                "Exponential exponent must be > 0, got {}",
                exponent
            )));
        }
        Ok(CurveType::Exponential { exponent })
    }

    /// Create a new logarithmic curve with the given base.
    ///
    /// # Arguments
    ///
    /// * `base` - The logarithm base (must be > 1 and finite)
    ///
    /// # Returns
    ///
    /// * `Ok(CurveType::Logarithmic)` if base is valid
    /// * `Err(CurveError)` if base is invalid
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_curves::CurveType;
    ///
    /// // Base 10 logarithmic curve
    /// let curve = CurveType::logarithmic(10.0)?;
    /// # Ok::<(), openracing_curves::CurveError>(())
    /// ```
    pub fn logarithmic(base: f32) -> Result<Self, CurveError> {
        if !base.is_finite() {
            return Err(CurveError::InvalidConfiguration(
                "Logarithmic base must be finite".to_string(),
            ));
        }
        if base <= 1.0 {
            return Err(CurveError::InvalidConfiguration(format!(
                "Logarithmic base must be > 1, got {}",
                base
            )));
        }
        Ok(CurveType::Logarithmic { base })
    }

    /// Evaluate the curve at the given input value.
    ///
    /// **WARNING**: This method is NOT RT-safe for Bezier curves.
    /// For RT-safe evaluation, use `to_lut()` and then `CurveLut::lookup()`.
    ///
    /// # Arguments
    ///
    /// * `input` - Input value (will be clamped to `[0,1]`)
    ///
    /// # Returns
    ///
    /// Output value in `[0,1]`.
    pub fn evaluate(&self, input: f32) -> f32 {
        let input = input.clamp(0.0, 1.0);

        match self {
            CurveType::Linear => input,
            CurveType::Exponential { exponent } => input.powf(*exponent),
            CurveType::Logarithmic { base } => {
                if input == 0.0 {
                    return 0.0;
                }
                if input == 1.0 {
                    return 1.0;
                }
                let log_base = base.ln();
                let value = 1.0 + input * (base - 1.0);
                (value.ln() / log_base) / 1.0
            }
            CurveType::Bezier(curve) => curve.map(input),
            CurveType::Custom(lut) => lut.lookup(input),
        }
    }

    /// Convert this curve type to a pre-computed LUT for RT-safe evaluation.
    ///
    /// **NOTE**: This should be called at profile load time, not in the RT path.
    ///
    /// # Returns
    ///
    /// A `CurveLut` ready for RT-safe lookups.
    pub fn to_lut(&self) -> CurveLut {
        match self {
            CurveType::Linear => CurveLut::linear(),
            CurveType::Exponential { exponent } => {
                let mut table = [0.0f32; CurveLut::SIZE];
                for (i, entry) in table.iter_mut().enumerate() {
                    let input = i as f32 / (CurveLut::SIZE - 1) as f32;
                    *entry = input.powf(*exponent);
                }
                CurveLut { table }
            }
            CurveType::Logarithmic { base } => {
                let mut table = [0.0f32; CurveLut::SIZE];
                let log_base = base.ln();
                for (i, entry) in table.iter_mut().enumerate() {
                    let input = i as f32 / (CurveLut::SIZE - 1) as f32;
                    if input == 0.0 {
                        *entry = 0.0;
                    } else if input == 1.0 {
                        *entry = 1.0;
                    } else {
                        let value = 1.0 + input * (base - 1.0);
                        *entry = value.ln() / log_base;
                    }
                }
                CurveLut { table }
            }
            CurveType::Bezier(curve) => CurveLut::from_bezier(curve),
            CurveType::Custom(lut) => (**lut).clone(),
        }
    }

    /// Validate the curve parameters.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if parameters are valid
    /// * `Err(CurveError)` if parameters are invalid
    pub fn validate(&self) -> Result<(), CurveError> {
        match self {
            CurveType::Linear => Ok(()),
            CurveType::Exponential { exponent } => {
                if !exponent.is_finite() {
                    return Err(CurveError::InvalidConfiguration(
                        "Exponential exponent must be finite".to_string(),
                    ));
                }
                if *exponent <= 0.0 {
                    return Err(CurveError::InvalidConfiguration(format!(
                        "Exponential exponent must be > 0, got {}",
                        exponent
                    )));
                }
                Ok(())
            }
            CurveType::Logarithmic { base } => {
                if !base.is_finite() {
                    return Err(CurveError::InvalidConfiguration(
                        "Logarithmic base must be finite".to_string(),
                    ));
                }
                if *base <= 1.0 {
                    return Err(CurveError::InvalidConfiguration(format!(
                        "Logarithmic base must be > 1, got {}",
                        base
                    )));
                }
                Ok(())
            }
            CurveType::Bezier(curve) => curve.validate(),
            CurveType::Custom(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(v) => v,
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_curve_type_linear_evaluate() {
        let curve = CurveType::Linear;

        assert!((curve.evaluate(0.0) - 0.0).abs() < 1e-6);
        assert!((curve.evaluate(0.25) - 0.25).abs() < 1e-6);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 1e-6);
        assert!((curve.evaluate(0.75) - 0.75).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_curve_type_linear_endpoints() {
        let curve = CurveType::Linear;

        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_type_exponential_creation_valid() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;
        assert!(
            matches!(curve, CurveType::Exponential { exponent } if (exponent - 2.0).abs() < 1e-6)
        );
        Ok(())
    }

    #[test]
    fn test_curve_type_exponential_creation_invalid_zero() {
        let result = CurveType::exponential(0.0);
        assert!(result.is_err());
        match result {
            Err(CurveError::InvalidConfiguration(msg)) => {
                assert!(msg.contains("must be > 0"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }

    #[test]
    fn test_curve_type_exponential_creation_invalid_negative() {
        let result = CurveType::exponential(-1.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_exponential_creation_invalid_nan() {
        let result = CurveType::exponential(f32::NAN);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_exponential_creation_invalid_inf() {
        let result = CurveType::exponential(f32::INFINITY);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_exponential_evaluate() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;

        assert!((curve.evaluate(0.0) - 0.0).abs() < 1e-6);
        assert!((curve.evaluate(0.5) - 0.25).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_curve_type_exponential_endpoints() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);

        let curve_half = CurveType::exponential(0.5)?;
        assert!((curve_half.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve_half.evaluate(1.0) - 1.0).abs() < 0.01);

        let curve_three = CurveType::exponential(3.0)?;
        assert!((curve_three.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve_three.evaluate(1.0) - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_curve_type_exponential_output_range() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;

        for i in 0..=100 {
            let input = i as f32 / 100.0;
            let output = curve.evaluate(input);
            assert!(
                (0.0..=1.0).contains(&output),
                "Output {} out of range for input {}",
                output,
                input
            );
        }
        Ok(())
    }

    #[test]
    fn test_curve_type_logarithmic_creation_valid() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;
        assert!(matches!(curve, CurveType::Logarithmic { base } if (base - 10.0).abs() < 1e-6));
        Ok(())
    }

    #[test]
    fn test_curve_type_logarithmic_creation_invalid_one() {
        let result = CurveType::logarithmic(1.0);
        assert!(result.is_err());
        match result {
            Err(CurveError::InvalidConfiguration(msg)) => {
                assert!(msg.contains("must be > 1"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }

    #[test]
    fn test_curve_type_logarithmic_creation_invalid_less_than_one() {
        let result = CurveType::logarithmic(0.5);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_logarithmic_creation_invalid_nan() {
        let result = CurveType::logarithmic(f32::NAN);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_logarithmic_evaluate() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;

        assert!((curve.evaluate(0.0) - 0.0).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_curve_type_logarithmic_endpoints() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);

        let curve_e = CurveType::logarithmic(std::f32::consts::E)?;
        assert!((curve_e.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve_e.evaluate(1.0) - 1.0).abs() < 0.01);

        let curve_2 = CurveType::logarithmic(2.0)?;
        assert!((curve_2.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve_2.evaluate(1.0) - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_curve_type_logarithmic_output_range() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;

        for i in 0..=100 {
            let input = i as f32 / 100.0;
            let output = curve.evaluate(input);
            assert!(
                (0.0..=1.0).contains(&output),
                "Output {} out of range for input {}",
                output,
                input
            );
        }
        Ok(())
    }

    #[test]
    fn test_curve_type_bezier_endpoints() {
        let bezier = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));
        let curve = CurveType::Bezier(bezier);

        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_type_to_lut_linear() {
        let curve = CurveType::Linear;
        let lut = curve.to_lut();

        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.5).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_type_to_lut_exponential() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;
        let lut = curve.to_lut();

        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.25).abs() < 0.02);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_curve_type_to_lut_logarithmic() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;
        let lut = curve.to_lut();

        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_curve_type_to_lut_bezier() {
        let bezier = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));
        let curve = CurveType::Bezier(bezier);
        let lut = curve.to_lut();

        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_type_validate_linear() {
        let curve = CurveType::Linear;
        assert!(curve.validate().is_ok());
    }

    #[test]
    fn test_curve_type_validate_exponential_valid() {
        let curve = CurveType::Exponential { exponent: 2.0 };
        assert!(curve.validate().is_ok());
    }

    #[test]
    fn test_curve_type_validate_exponential_invalid() {
        let curve = CurveType::Exponential { exponent: -1.0 };
        assert!(curve.validate().is_err());

        let curve_zero = CurveType::Exponential { exponent: 0.0 };
        assert!(curve_zero.validate().is_err());

        let curve_nan = CurveType::Exponential { exponent: f32::NAN };
        assert!(curve_nan.validate().is_err());

        let curve_inf = CurveType::Exponential {
            exponent: f32::INFINITY,
        };
        assert!(curve_inf.validate().is_err());
    }

    #[test]
    fn test_curve_type_validate_logarithmic_valid() {
        let curve = CurveType::Logarithmic { base: 10.0 };
        assert!(curve.validate().is_ok());
    }

    #[test]
    fn test_curve_type_validate_logarithmic_invalid() {
        let curve = CurveType::Logarithmic { base: 1.0 };
        assert!(curve.validate().is_err());

        let curve_less = CurveType::Logarithmic { base: 0.5 };
        assert!(curve_less.validate().is_err());

        let curve_nan = CurveType::Logarithmic { base: f32::NAN };
        assert!(curve_nan.validate().is_err());

        let curve_inf = CurveType::Logarithmic {
            base: f32::INFINITY,
        };
        assert!(curve_inf.validate().is_err());
    }

    #[test]
    fn test_curve_type_validate_bezier_valid() {
        let bezier = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));
        let curve = CurveType::Bezier(bezier);
        assert!(curve.validate().is_ok());
    }

    #[test]
    fn test_curve_type_validate_bezier_invalid() {
        let bezier = BezierCurve {
            control_points: [(0.0, 0.0), (1.5, 0.5), (0.75, 0.25), (1.0, 1.0)],
        };
        let curve = CurveType::Bezier(bezier);
        assert!(curve.validate().is_err());
    }

    #[test]
    fn test_curve_type_default() {
        let curve = CurveType::default();
        assert!(matches!(curve, CurveType::Linear));
    }

    #[test]
    fn test_curve_type_serialization() -> Result<(), CurveError> {
        let linear = CurveType::Linear;
        let json = serde_json::to_string(&linear)
            .map_err(|e| CurveError::InvalidConfiguration(e.to_string()))?;
        let deserialized: CurveType = serde_json::from_str(&json)
            .map_err(|e| CurveError::InvalidConfiguration(e.to_string()))?;
        assert_eq!(linear, deserialized);

        let exp = CurveType::exponential(2.0)?;
        let json = serde_json::to_string(&exp)
            .map_err(|e| CurveError::InvalidConfiguration(e.to_string()))?;
        let deserialized: CurveType = serde_json::from_str(&json)
            .map_err(|e| CurveError::InvalidConfiguration(e.to_string()))?;
        assert_eq!(exp, deserialized);

        let log = CurveType::logarithmic(10.0)?;
        let json = serde_json::to_string(&log)
            .map_err(|e| CurveError::InvalidConfiguration(e.to_string()))?;
        let deserialized: CurveType = serde_json::from_str(&json)
            .map_err(|e| CurveError::InvalidConfiguration(e.to_string()))?;
        assert_eq!(log, deserialized);

        Ok(())
    }

    #[test]
    fn test_curve_type_clamping() -> Result<(), CurveError> {
        let curves: Vec<CurveType> = vec![
            CurveType::Linear,
            CurveType::exponential(2.0)?,
            CurveType::logarithmic(10.0)?,
        ];

        for curve in curves {
            let below = curve.evaluate(-0.5);
            let above = curve.evaluate(1.5);

            assert!(
                (0.0..=1.0).contains(&below),
                "Output {} out of range for input -0.5",
                below
            );
            assert!(
                (0.0..=1.0).contains(&above),
                "Output {} out of range for input 1.5",
                above
            );
        }
        Ok(())
    }

    #[test]
    fn test_curve_type_custom_lut() {
        let lut = CurveLut::linear();
        let curve = CurveType::Custom(Box::new(lut.clone()));

        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);

        let lut2 = curve.to_lut();
        assert!((lut2.lookup(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_all_curve_types_endpoint_consistency() -> Result<(), CurveError> {
        let tolerance = 0.01;

        let curves: Vec<CurveType> = vec![
            CurveType::Linear,
            CurveType::exponential(0.5)?,
            CurveType::exponential(1.0)?,
            CurveType::exponential(2.0)?,
            CurveType::exponential(3.0)?,
            CurveType::logarithmic(2.0)?,
            CurveType::logarithmic(std::f32::consts::E)?,
            CurveType::logarithmic(10.0)?,
            CurveType::Bezier(BezierCurve::linear()),
            CurveType::Bezier(must(BezierCurve::new([
                (0.0, 0.0),
                (0.25, 0.75),
                (0.75, 0.25),
                (1.0, 1.0),
            ]))),
        ];

        for curve in curves {
            let at_zero = curve.evaluate(0.0);
            let at_one = curve.evaluate(1.0);

            assert!(
                (at_zero - 0.0).abs() < tolerance,
                "Curve {:?} at 0.0 returned {} (expected ~0.0)",
                curve,
                at_zero
            );
            assert!(
                (at_one - 1.0).abs() < tolerance,
                "Curve {:?} at 1.0 returned {} (expected ~1.0)",
                curve,
                at_one
            );
        }
        Ok(())
    }
}
