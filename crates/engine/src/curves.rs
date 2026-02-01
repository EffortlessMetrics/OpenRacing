//! Curve-Based FFB Effects
//!
//! This module implements curve-based force feedback response mapping,
//! including Bezier curves and pre-computed lookup tables for RT-safe evaluation.
//!
//! The curve system supports:
//! - Cubic Bezier curves for custom response mapping
//! - Pre-computed LUTs for zero-allocation RT path evaluation
//! - Linear interpolation for smooth output between LUT entries

use serde::{Deserialize, Serialize};

/// Supported curve types for FFB response mapping
///
/// All curve types map input values in [0,1] to output values in [0,1].
/// Each type supports LUT pre-computation for RT-safe evaluation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum CurveType {
    /// Linear curve: f(x) = x (identity mapping)
    #[default]
    Linear,
    /// Exponential curve: f(x) = x^exponent (where exponent > 0)
    Exponential {
        /// The exponent value (must be > 0)
        exponent: f32,
    },
    /// Logarithmic curve: f(x) = log_base(1 + x*(base-1)) / log_base(base)
    /// Normalized to map [0,1] → [0,1]
    Logarithmic {
        /// The logarithm base (must be > 1)
        base: f32,
    },
    /// Cubic Bezier curve with custom control points
    Bezier(BezierCurve),
    /// Custom pre-computed lookup table (boxed to reduce enum size)
    Custom(Box<CurveLut>),
}

impl CurveType {
    /// Create a new exponential curve with the given exponent.
    ///
    /// # Arguments
    /// * `exponent` - The exponent value (must be > 0 and finite)
    ///
    /// # Returns
    /// * `Ok(CurveType::Exponential)` if exponent is valid
    /// * `Err(CurveError)` if exponent is invalid
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
    /// * `base` - The logarithm base (must be > 1 and finite)
    ///
    /// # Returns
    /// * `Ok(CurveType::Logarithmic)` if base is valid
    /// * `Err(CurveError)` if base is invalid
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
    /// This method is NOT RT-safe for Bezier curves (uses Newton-Raphson iteration).
    /// For RT-safe evaluation, use `to_lut()` and then `CurveLut::lookup()`.
    ///
    /// # Arguments
    /// * `input` - Input value (will be clamped to [0,1])
    ///
    /// # Returns
    /// Output value in [0,1]
    pub fn evaluate(&self, input: f32) -> f32 {
        let input = input.clamp(0.0, 1.0);

        match self {
            CurveType::Linear => input,
            CurveType::Exponential { exponent } => {
                // f(x) = x^exponent
                // This naturally maps 0→0 and 1→1
                input.powf(*exponent)
            }
            CurveType::Logarithmic { base } => {
                // f(x) = log_base(1 + x*(base-1)) / log_base(base)
                // At x=0: log_base(1) / log_base(base) = 0 / 1 = 0
                // At x=1: log_base(base) / log_base(base) = 1 / 1 = 1
                if input == 0.0 {
                    return 0.0;
                }
                if input == 1.0 {
                    return 1.0;
                }
                let log_base = base.ln();
                let value = 1.0 + input * (base - 1.0);
                (value.ln() / log_base) / 1.0 // log_base(base) = 1
            }
            CurveType::Bezier(curve) => curve.map(input),
            CurveType::Custom(lut) => lut.lookup(input),
        }
    }

    /// Convert this curve type to a pre-computed LUT for RT-safe evaluation.
    ///
    /// This should be called at profile load time, not in the RT path.
    ///
    /// # Returns
    /// A CurveLut ready for RT-safe lookups
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
            CurveType::Bezier(curve) => {
                // Validate control points are in [0,1]²
                for (i, (x, y)) in curve.control_points.iter().enumerate() {
                    if !x.is_finite() || *x < 0.0 || *x > 1.0 {
                        return Err(CurveError::ControlPointOutOfRange {
                            point_index: i,
                            coordinate: "x",
                            value: *x,
                        });
                    }
                    if !y.is_finite() || *y < 0.0 || *y > 1.0 {
                        return Err(CurveError::ControlPointOutOfRange {
                            point_index: i,
                            coordinate: "y",
                            value: *y,
                        });
                    }
                }
                Ok(())
            }
            CurveType::Custom(_) => Ok(()), // Custom LUTs are always valid
        }
    }
}

/// Error type for curve operations
#[derive(Debug, Clone, PartialEq)]
pub enum CurveError {
    /// Control point is outside the valid [0,1] range
    ControlPointOutOfRange {
        point_index: usize,
        coordinate: &'static str,
        value: f32,
    },
    /// Invalid curve configuration
    InvalidConfiguration(String),
}

impl std::fmt::Display for CurveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CurveError::ControlPointOutOfRange {
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
            CurveError::InvalidConfiguration(msg) => {
                write!(f, "Invalid curve configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for CurveError {}

/// Bezier curve for FFB response mapping
///
/// A cubic Bezier curve defined by four control points P0, P1, P2, P3.
/// For FFB response mapping, P0 is typically (0,0) and P3 is (1,1),
/// with P1 and P2 controlling the curve shape.
///
/// The curve maps input values in [0,1] to output values in [0,1].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BezierCurve {
    /// Control points P0, P1, P2, P3 as (x, y) tuples
    pub control_points: [(f32, f32); 4],
}

impl BezierCurve {
    /// Create a new Bezier curve with the given control points.
    ///
    /// # Arguments
    /// * `control_points` - Four control points [(x0,y0), (x1,y1), (x2,y2), (x3,y3)]
    ///
    /// # Returns
    /// * `Ok(BezierCurve)` if all control points are valid
    /// * `Err(CurveError)` if any control point is outside [0,1]²
    pub fn new(control_points: [(f32, f32); 4]) -> Result<Self, CurveError> {
        // Validate all control points are in [0,1]²
        for (i, (x, y)) in control_points.iter().enumerate() {
            if !Self::is_valid_coordinate(*x) {
                return Err(CurveError::ControlPointOutOfRange {
                    point_index: i,
                    coordinate: "x",
                    value: *x,
                });
            }
            if !Self::is_valid_coordinate(*y) {
                return Err(CurveError::ControlPointOutOfRange {
                    point_index: i,
                    coordinate: "y",
                    value: *y,
                });
            }
        }

        Ok(Self { control_points })
    }

    /// Create a linear curve (identity mapping)
    pub fn linear() -> Self {
        Self {
            control_points: [(0.0, 0.0), (0.33, 0.33), (0.67, 0.67), (1.0, 1.0)],
        }
    }

    /// Check if a coordinate value is valid (in [0,1] range)
    #[inline]
    fn is_valid_coordinate(value: f32) -> bool {
        value.is_finite() && (0.0..=1.0).contains(&value)
    }

    /// Evaluate the cubic Bezier curve at parameter t.
    ///
    /// Uses the formula: B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
    ///
    /// # Arguments
    /// * `t` - Parameter value in [0,1]
    ///
    /// # Returns
    /// The (x, y) point on the curve at parameter t
    #[inline]
    pub fn evaluate(&self, t: f32) -> (f32, f32) {
        let t = t.clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = self.control_points;

        // B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
        let x = mt3 * x0 + 3.0 * mt2 * t * x1 + 3.0 * mt * t2 * x2 + t3 * x3;
        let y = mt3 * y0 + 3.0 * mt2 * t * y1 + 3.0 * mt * t2 * y2 + t3 * y3;

        (x, y)
    }

    /// Find the parameter t that produces a given x value using Newton-Raphson iteration.
    ///
    /// This is needed because we want to map input x to output y, but the Bezier
    /// curve is parameterized by t, not x.
    ///
    /// # Arguments
    /// * `target_x` - The x value to find (in [0,1])
    ///
    /// # Returns
    /// The parameter t that produces the target x value
    fn find_t_for_x(&self, target_x: f32) -> f32 {
        let target_x = target_x.clamp(0.0, 1.0);

        // Initial guess: t = target_x (works well for curves close to linear)
        let mut t = target_x;

        // Newton-Raphson iteration (typically converges in 3-5 iterations)
        const MAX_ITERATIONS: usize = 8;
        const EPSILON: f32 = 1e-6;

        for _ in 0..MAX_ITERATIONS {
            let (x, _) = self.evaluate(t);
            let error = x - target_x;

            if error.abs() < EPSILON {
                break;
            }

            // Compute derivative dx/dt using the Bezier derivative formula
            let dx_dt = self.evaluate_x_derivative(t);

            // Avoid division by zero
            if dx_dt.abs() < EPSILON {
                break;
            }

            // Newton-Raphson update
            t -= error / dx_dt;
            t = t.clamp(0.0, 1.0);
        }

        t
    }

    /// Evaluate the x-derivative of the Bezier curve at parameter t.
    ///
    /// The derivative of a cubic Bezier is a quadratic Bezier:
    /// B'(t) = 3[(1-t)²(P₁-P₀) + 2(1-t)t(P₂-P₁) + t²(P₃-P₂)]
    #[inline]
    fn evaluate_x_derivative(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let mt = 1.0 - t;

        let [(x0, _), (x1, _), (x2, _), (x3, _)] = self.control_points;

        // Derivative coefficients
        let d0 = x1 - x0;
        let d1 = x2 - x1;
        let d2 = x3 - x2;

        // B'(t) = 3[(1-t)²d₀ + 2(1-t)t·d₁ + t²d₂]
        3.0 * (mt * mt * d0 + 2.0 * mt * t * d1 + t * t * d2)
    }

    /// Map an input x value to an output y value.
    ///
    /// This finds the parameter t where the curve's x equals the input,
    /// then returns the corresponding y value.
    ///
    /// # Arguments
    /// * `input` - Input value in [0,1]
    ///
    /// # Returns
    /// Output value in [0,1]
    pub fn map(&self, input: f32) -> f32 {
        let t = self.find_t_for_x(input);
        let (_, y) = self.evaluate(t);
        y.clamp(0.0, 1.0)
    }
}

impl Default for BezierCurve {
    fn default() -> Self {
        Self::linear()
    }
}

/// Pre-computed lookup table for RT path (no allocation)
///
/// The LUT provides O(1) curve evaluation with linear interpolation
/// between entries. This is essential for the RT path where we cannot
/// perform iterative algorithms like Newton-Raphson.
#[derive(Clone, Debug, PartialEq)]
pub struct CurveLut {
    /// 256-entry lookup table for fast interpolation
    table: [f32; 256],
}

// Custom serialization for CurveLut since serde doesn't support [f32; 256] directly
impl Serialize for CurveLut {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as a Vec<f32>
        self.table.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CurveLut {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<f32> = Vec::deserialize(deserializer)?;
        if vec.len() != 256 {
            return Err(serde::de::Error::custom(format!(
                "Expected 256 entries in CurveLut, got {}",
                vec.len()
            )));
        }
        let mut table = [0.0f32; 256];
        table.copy_from_slice(&vec);
        Ok(CurveLut { table })
    }
}

impl CurveLut {
    /// LUT size (256 entries provides good precision with minimal memory)
    pub const SIZE: usize = 256;

    /// Build LUT from Bezier curve (called at profile load, not RT)
    ///
    /// This pre-computes the curve mapping for 256 evenly-spaced input values,
    /// allowing fast lookup with linear interpolation in the RT path.
    ///
    /// # Arguments
    /// * `curve` - The Bezier curve to pre-compute
    ///
    /// # Returns
    /// A CurveLut ready for RT-safe lookups
    pub fn from_bezier(curve: &BezierCurve) -> Self {
        let mut table = [0.0f32; Self::SIZE];

        for (i, entry) in table.iter_mut().enumerate() {
            let input = i as f32 / (Self::SIZE - 1) as f32;
            *entry = curve.map(input);
        }

        Self { table }
    }

    /// Create a linear (identity) LUT
    pub fn linear() -> Self {
        let mut table = [0.0f32; Self::SIZE];

        for (i, entry) in table.iter_mut().enumerate() {
            *entry = i as f32 / (Self::SIZE - 1) as f32;
        }

        Self { table }
    }

    /// Fast lookup with linear interpolation (RT-safe)
    ///
    /// This method is designed for the RT path:
    /// - No heap allocations
    /// - O(1) time complexity
    /// - Bounded execution time
    ///
    /// # Arguments
    /// * `input` - Input value (will be clamped to [0,1])
    ///
    /// # Returns
    /// Interpolated output value in [0,1]
    #[inline]
    pub fn lookup(&self, input: f32) -> f32 {
        // Clamp input to valid range
        let input = input.clamp(0.0, 1.0);

        // Calculate fractional index
        let scaled = input * (Self::SIZE - 1) as f32;
        let index_low = (scaled as usize).min(Self::SIZE - 2);
        let index_high = index_low + 1;
        let fraction = scaled - index_low as f32;

        // Linear interpolation between adjacent entries
        let low_value = self.table[index_low];
        let high_value = self.table[index_high];

        low_value + fraction * (high_value - low_value)
    }

    /// Get the raw table for inspection (useful for testing)
    #[cfg(test)]
    pub fn table(&self) -> &[f32; 256] {
        &self.table
    }
}

impl Default for CurveLut {
    fn default() -> Self {
        Self::linear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create test results without unwrap
    fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(v) => v,
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_bezier_curve_creation_valid() -> Result<(), CurveError> {
        let curve = BezierCurve::new([(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)])?;
        assert_eq!(curve.control_points[0], (0.0, 0.0));
        assert_eq!(curve.control_points[3], (1.0, 1.0));
        Ok(())
    }

    #[test]
    fn test_bezier_curve_creation_invalid_x() {
        let result = BezierCurve::new([(0.0, 0.0), (1.5, 0.5), (0.75, 0.5), (1.0, 1.0)]);
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "x");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_bezier_curve_creation_invalid_y() {
        let result = BezierCurve::new([(0.0, 0.0), (0.25, -0.1), (0.75, 0.5), (1.0, 1.0)]);
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "y");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_bezier_curve_creation_nan() {
        let result = BezierCurve::new([(0.0, 0.0), (f32::NAN, 0.5), (0.75, 0.5), (1.0, 1.0)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bezier_curve_creation_infinity() {
        let result = BezierCurve::new([(0.0, 0.0), (f32::INFINITY, 0.5), (0.75, 0.5), (1.0, 1.0)]);
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "x");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_bezier_curve_creation_neg_infinity() {
        let result = BezierCurve::new([
            (0.0, 0.0),
            (0.25, f32::NEG_INFINITY),
            (0.75, 0.5),
            (1.0, 1.0),
        ]);
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "y");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_bezier_linear_curve() {
        let curve = BezierCurve::linear();

        // Linear curve should map x to approximately x
        let tolerance = 0.01;
        assert!((curve.map(0.0) - 0.0).abs() < tolerance);
        assert!((curve.map(0.5) - 0.5).abs() < tolerance);
        assert!((curve.map(1.0) - 1.0).abs() < tolerance);
    }

    #[test]
    fn test_bezier_evaluate_endpoints() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));

        // At t=0, should be at P0
        let (x0, y0) = curve.evaluate(0.0);
        assert!((x0 - 0.0).abs() < 1e-6);
        assert!((y0 - 0.0).abs() < 1e-6);

        // At t=1, should be at P3
        let (x1, y1) = curve.evaluate(1.0);
        assert!((x1 - 1.0).abs() < 1e-6);
        assert!((y1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_bezier_map_endpoints() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));

        // Input 0 should map to output 0
        assert!((curve.map(0.0) - 0.0).abs() < 0.01);

        // Input 1 should map to output 1
        assert!((curve.map(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_bezier_map_clamping() {
        let curve = BezierCurve::linear();

        // Values outside [0,1] should be clamped
        let below = curve.map(-0.5);
        let above = curve.map(1.5);

        assert!((0.0..=1.0).contains(&below));
        assert!((0.0..=1.0).contains(&above));
    }

    #[test]
    fn test_curve_lut_from_linear_bezier() {
        let curve = BezierCurve::linear();
        let lut = CurveLut::from_bezier(&curve);

        // Linear curve LUT should be approximately identity
        let tolerance = 0.02;
        assert!((lut.lookup(0.0) - 0.0).abs() < tolerance);
        assert!((lut.lookup(0.5) - 0.5).abs() < tolerance);
        assert!((lut.lookup(1.0) - 1.0).abs() < tolerance);
    }

    #[test]
    fn test_curve_lut_lookup_interpolation() {
        let lut = CurveLut::linear();

        // Test that interpolation works between table entries
        // For a linear LUT, any input should equal the output
        for i in 0..100 {
            let input = i as f32 / 99.0;
            let output = lut.lookup(input);
            assert!(
                (output - input).abs() < 0.01,
                "Linear LUT failed at input {}: got {}",
                input,
                output
            );
        }
    }

    #[test]
    fn test_curve_lut_lookup_clamping() {
        let lut = CurveLut::linear();

        // Values outside [0,1] should be clamped
        let below = lut.lookup(-0.5);
        let above = lut.lookup(1.5);

        assert!((0.0..=1.0).contains(&below));
        assert!((0.0..=1.0).contains(&above));
        assert!((below - 0.0).abs() < 0.01);
        assert!((above - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_lut_output_range() {
        // Test with a non-linear curve
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.1, 0.9),
            (0.9, 0.1),
            (1.0, 1.0),
        ]));
        let lut = CurveLut::from_bezier(&curve);

        // All outputs should be in [0,1]
        for i in 0..=100 {
            let input = i as f32 / 100.0;
            let output = lut.lookup(input);
            assert!(
                (0.0..=1.0).contains(&output),
                "Output {} out of range for input {}",
                output,
                input
            );
        }
    }

    #[test]
    fn test_curve_lut_determinism() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));

        let lut1 = CurveLut::from_bezier(&curve);
        let lut2 = CurveLut::from_bezier(&curve);

        // Same curve should produce identical LUTs
        for i in 0..CurveLut::SIZE {
            assert_eq!(
                lut1.table()[i],
                lut2.table()[i],
                "LUT mismatch at index {}",
                i
            );
        }
    }

    #[test]
    fn test_curve_lut_no_allocation_in_lookup() {
        // This test verifies the lookup is RT-safe by checking it doesn't panic
        // and produces consistent results. The actual allocation tracking is
        // handled by the crate's allocation_tracker in test mode.
        let lut = CurveLut::linear();

        // Perform many lookups to ensure stability
        for _ in 0..10000 {
            let input = 0.5;
            let output = lut.lookup(input);
            assert!(output.is_finite());
        }
    }

    #[test]
    fn test_bezier_s_curve() {
        // Test an S-curve (common for FFB response)
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.0, 0.5),
            (1.0, 0.5),
            (1.0, 1.0),
        ]));
        let lut = CurveLut::from_bezier(&curve);

        // S-curve should have inflection around 0.5
        let low = lut.lookup(0.25);
        let mid = lut.lookup(0.5);
        let high = lut.lookup(0.75);

        // Verify monotonicity
        assert!(low < mid);
        assert!(mid < high);

        // Verify endpoints
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_bezier_serialization() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.25, 0.75),
            (0.75, 0.25),
            (1.0, 1.0),
        ]));

        // Test JSON serialization round-trip
        let json = serde_json::to_string(&curve);
        assert!(json.is_ok());

        let deserialized: Result<BezierCurve, _> =
            serde_json::from_str(json.as_ref().map_or("", |s| s));
        assert!(deserialized.is_ok());
        assert_eq!(curve, deserialized.map_or(BezierCurve::linear(), |c| c));
    }

    #[test]
    fn test_curve_error_display() {
        let err = CurveError::ControlPointOutOfRange {
            point_index: 1,
            coordinate: "x",
            value: 1.5,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Control point 1"));
        assert!(msg.contains("x coordinate"));
        assert!(msg.contains("1.5"));
    }

    #[test]
    fn test_default_implementations() {
        let curve = BezierCurve::default();
        assert_eq!(curve, BezierCurve::linear());

        let lut = CurveLut::default();
        // Default LUT should be linear
        assert!((lut.lookup(0.5) - 0.5).abs() < 0.01);
    }

    // ============================================================
    // CurveType Tests
    // ============================================================

    #[test]
    fn test_curve_type_linear_evaluate() {
        let curve = CurveType::Linear;

        // Linear should be identity mapping
        assert!((curve.evaluate(0.0) - 0.0).abs() < 1e-6);
        assert!((curve.evaluate(0.25) - 0.25).abs() < 1e-6);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 1e-6);
        assert!((curve.evaluate(0.75) - 0.75).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_curve_type_linear_endpoints() {
        let curve = CurveType::Linear;

        // Property 16: input 0.0 → ~0.0, input 1.0 → ~1.0
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
    fn test_curve_type_exponential_creation_invalid_neg_inf() {
        let result = CurveType::exponential(f32::NEG_INFINITY);
        assert!(result.is_err());
        match result {
            Err(CurveError::InvalidConfiguration(msg)) => {
                assert!(msg.contains("finite"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }

    #[test]
    fn test_curve_type_exponential_evaluate() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;

        // f(x) = x^2
        assert!((curve.evaluate(0.0) - 0.0).abs() < 1e-6);
        assert!((curve.evaluate(0.5) - 0.25).abs() < 1e-6); // 0.5^2 = 0.25
        assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_curve_type_exponential_endpoints() -> Result<(), CurveError> {
        // Property 16: input 0.0 → ~0.0, input 1.0 → ~1.0
        let curve = CurveType::exponential(2.0)?;
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);

        // Test with different exponents
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

        // All outputs should be in [0,1]
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
    fn test_curve_type_logarithmic_creation_invalid_inf() {
        let result = CurveType::logarithmic(f32::INFINITY);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_logarithmic_creation_invalid_neg_inf() {
        let result = CurveType::logarithmic(f32::NEG_INFINITY);
        assert!(result.is_err());
        match result {
            Err(CurveError::InvalidConfiguration(msg)) => {
                assert!(msg.contains("finite"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }

    #[test]
    fn test_curve_type_logarithmic_creation_invalid_negative() {
        let result = CurveType::logarithmic(-5.0);
        assert!(result.is_err());
        match result {
            Err(CurveError::InvalidConfiguration(msg)) => {
                assert!(msg.contains("must be > 1"));
            }
            _ => panic!("Expected InvalidConfiguration error"),
        }
    }

    #[test]
    fn test_curve_type_logarithmic_creation_invalid_zero() {
        let result = CurveType::logarithmic(0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_curve_type_logarithmic_evaluate() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;

        // f(x) = log_10(1 + x*9) / log_10(10) = log_10(1 + 9x)
        // At x=0: log_10(1) = 0
        // At x=1: log_10(10) = 1
        assert!((curve.evaluate(0.0) - 0.0).abs() < 1e-6);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_curve_type_logarithmic_endpoints() -> Result<(), CurveError> {
        // Property 16: input 0.0 → ~0.0, input 1.0 → ~1.0
        let curve = CurveType::logarithmic(10.0)?;
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);

        // Test with different bases
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

        // All outputs should be in [0,1]
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

        // Property 16: input 0.0 → ~0.0, input 1.0 → ~1.0
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_type_to_lut_linear() {
        let curve = CurveType::Linear;
        let lut = curve.to_lut();

        // Linear LUT should be identity
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.5).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_type_to_lut_exponential() -> Result<(), CurveError> {
        let curve = CurveType::exponential(2.0)?;
        let lut = curve.to_lut();

        // LUT should match direct evaluation
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.25).abs() < 0.02); // 0.5^2 = 0.25
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn test_curve_type_to_lut_logarithmic() -> Result<(), CurveError> {
        let curve = CurveType::logarithmic(10.0)?;
        let lut = curve.to_lut();

        // LUT should match direct evaluation
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

        // LUT endpoints should match
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

        let curve_neg_inf = CurveType::Exponential {
            exponent: f32::NEG_INFINITY,
        };
        assert!(curve_neg_inf.validate().is_err());
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

        let curve_neg_inf = CurveType::Logarithmic {
            base: f32::NEG_INFINITY,
        };
        assert!(curve_neg_inf.validate().is_err());

        let curve_negative = CurveType::Logarithmic { base: -5.0 };
        assert!(curve_negative.validate().is_err());

        let curve_zero = CurveType::Logarithmic { base: 0.0 };
        assert!(curve_zero.validate().is_err());
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
        // Create a Bezier with invalid control points (bypassing constructor validation)
        let bezier = BezierCurve {
            control_points: [(0.0, 0.0), (1.5, 0.5), (0.75, 0.25), (1.0, 1.0)],
        };
        let curve = CurveType::Bezier(bezier);
        assert!(curve.validate().is_err());
    }

    #[test]
    fn test_curve_type_validate_bezier_nan() {
        // Create a Bezier with NaN control point (bypassing constructor validation)
        let bezier = BezierCurve {
            control_points: [(0.0, 0.0), (f32::NAN, 0.5), (0.75, 0.25), (1.0, 1.0)],
        };
        let curve = CurveType::Bezier(bezier);
        let result = curve.validate();
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "x");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_curve_type_validate_bezier_infinity() {
        // Create a Bezier with Infinity control point (bypassing constructor validation)
        let bezier = BezierCurve {
            control_points: [(0.0, 0.0), (0.25, f32::INFINITY), (0.75, 0.25), (1.0, 1.0)],
        };
        let curve = CurveType::Bezier(bezier);
        let result = curve.validate();
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "y");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_curve_type_validate_bezier_neg_infinity() {
        // Create a Bezier with NEG_INFINITY control point (bypassing constructor validation)
        let bezier = BezierCurve {
            control_points: [
                (0.0, 0.0),
                (0.25, 0.5),
                (f32::NEG_INFINITY, 0.25),
                (1.0, 1.0),
            ],
        };
        let curve = CurveType::Bezier(bezier);
        let result = curve.validate();
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                ..
            }) => {
                assert_eq!(point_index, 2);
                assert_eq!(coordinate, "x");
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_curve_type_validate_bezier_negative_coordinate() {
        // Create a Bezier with negative control point (bypassing constructor validation)
        let bezier = BezierCurve {
            control_points: [(0.0, 0.0), (0.25, -0.5), (0.75, 0.25), (1.0, 1.0)],
        };
        let curve = CurveType::Bezier(bezier);
        let result = curve.validate();
        assert!(result.is_err());
        match result {
            Err(CurveError::ControlPointOutOfRange {
                point_index,
                coordinate,
                value,
            }) => {
                assert_eq!(point_index, 1);
                assert_eq!(coordinate, "y");
                assert!((value - (-0.5)).abs() < 1e-6);
            }
            _ => panic!("Expected ControlPointOutOfRange error"),
        }
    }

    #[test]
    fn test_curve_type_default() {
        let curve = CurveType::default();
        assert!(matches!(curve, CurveType::Linear));
    }

    #[test]
    fn test_curve_type_serialization() -> Result<(), CurveError> {
        // Test Linear
        let linear = CurveType::Linear;
        let json = serde_json::to_string(&linear);
        assert!(json.is_ok());
        let deserialized: Result<CurveType, _> =
            serde_json::from_str(json.as_ref().map_or("", |s| s));
        assert!(deserialized.is_ok());

        // Test Exponential
        let exp = CurveType::exponential(2.0)?;
        let json = serde_json::to_string(&exp);
        assert!(json.is_ok());
        let deserialized: Result<CurveType, _> =
            serde_json::from_str(json.as_ref().map_or("", |s| s));
        assert!(deserialized.is_ok());

        // Test Logarithmic
        let log = CurveType::logarithmic(10.0)?;
        let json = serde_json::to_string(&log);
        assert!(json.is_ok());
        let deserialized: Result<CurveType, _> =
            serde_json::from_str(json.as_ref().map_or("", |s| s));
        assert!(deserialized.is_ok());

        Ok(())
    }

    #[test]
    fn test_curve_type_clamping() -> Result<(), CurveError> {
        // All curve types should clamp input to [0,1]
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
        // Create a custom LUT
        let lut = CurveLut::linear();
        let curve = CurveType::Custom(Box::new(lut.clone()));

        // Custom should use the LUT directly
        assert!((curve.evaluate(0.0) - 0.0).abs() < 0.01);
        assert!((curve.evaluate(0.5) - 0.5).abs() < 0.01);
        assert!((curve.evaluate(1.0) - 1.0).abs() < 0.01);

        // to_lut should return a clone
        let lut2 = curve.to_lut();
        assert!((lut2.lookup(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_all_curve_types_endpoint_consistency() -> Result<(), CurveError> {
        // Property 16: All curve types should map 0→~0 and 1→~1
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

    // ============================================================
    // Validation Consistency Tests
    // ============================================================

    #[test]
    fn test_validation_consistency_exponential() {
        // Test that constructor and validate() agree on what's valid/invalid
        let invalid_values = [0.0, -1.0, -0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];

        for value in invalid_values {
            // Constructor should reject
            let constructor_result = CurveType::exponential(value);
            assert!(
                constructor_result.is_err(),
                "Constructor should reject exponent {}",
                value
            );

            // validate() should also reject if we bypass constructor
            let curve = CurveType::Exponential { exponent: value };
            let validate_result = curve.validate();
            assert!(
                validate_result.is_err(),
                "validate() should reject exponent {}",
                value
            );
        }

        // Valid values should pass both
        let valid_values = [0.001, 0.5, 1.0, 2.0, 10.0, 100.0];
        for value in valid_values {
            let constructor_result = CurveType::exponential(value);
            assert!(
                constructor_result.is_ok(),
                "Constructor should accept exponent {}",
                value
            );

            let curve = CurveType::Exponential { exponent: value };
            let validate_result = curve.validate();
            assert!(
                validate_result.is_ok(),
                "validate() should accept exponent {}",
                value
            );
        }
    }

    #[test]
    fn test_validation_consistency_logarithmic() {
        // Test that constructor and validate() agree on what's valid/invalid
        let invalid_values = [
            0.0,
            0.5,
            1.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        for value in invalid_values {
            // Constructor should reject
            let constructor_result = CurveType::logarithmic(value);
            assert!(
                constructor_result.is_err(),
                "Constructor should reject base {}",
                value
            );

            // validate() should also reject if we bypass constructor
            let curve = CurveType::Logarithmic { base: value };
            let validate_result = curve.validate();
            assert!(
                validate_result.is_err(),
                "validate() should reject base {}",
                value
            );
        }

        // Valid values should pass both
        let valid_values = [1.001, 2.0, std::f32::consts::E, 10.0, 100.0];
        for value in valid_values {
            let constructor_result = CurveType::logarithmic(value);
            assert!(
                constructor_result.is_ok(),
                "Constructor should accept base {}",
                value
            );

            let curve = CurveType::Logarithmic { base: value };
            let validate_result = curve.validate();
            assert!(
                validate_result.is_ok(),
                "validate() should accept base {}",
                value
            );
        }
    }

    #[test]
    fn test_validation_consistency_bezier() {
        // Test that constructor and validate() agree on what's valid/invalid
        let invalid_control_points = [
            [(0.0, 0.0), (1.5, 0.5), (0.75, 0.25), (1.0, 1.0)], // x > 1
            [(0.0, 0.0), (-0.1, 0.5), (0.75, 0.25), (1.0, 1.0)], // x < 0
            [(0.0, 0.0), (0.25, 1.5), (0.75, 0.25), (1.0, 1.0)], // y > 1
            [(0.0, 0.0), (0.25, -0.1), (0.75, 0.25), (1.0, 1.0)], // y < 0
        ];

        for points in invalid_control_points {
            // Constructor should reject
            let constructor_result = BezierCurve::new(points);
            assert!(
                constructor_result.is_err(),
                "Constructor should reject control points {:?}",
                points
            );

            // validate() should also reject if we bypass constructor
            let bezier = BezierCurve {
                control_points: points,
            };
            let curve = CurveType::Bezier(bezier);
            let validate_result = curve.validate();
            assert!(
                validate_result.is_err(),
                "validate() should reject control points {:?}",
                points
            );
        }

        // Valid control points should pass both
        let valid_control_points = [
            [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)],
            [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)],
            [(0.0, 0.0), (0.5, 0.5), (0.5, 0.5), (1.0, 1.0)],
        ];

        for points in valid_control_points {
            let constructor_result = BezierCurve::new(points);
            assert!(
                constructor_result.is_ok(),
                "Constructor should accept control points {:?}",
                points
            );

            let bezier = BezierCurve {
                control_points: points,
            };
            let curve = CurveType::Bezier(bezier);
            let validate_result = curve.validate();
            assert!(
                validate_result.is_ok(),
                "validate() should accept control points {:?}",
                points
            );
        }
    }

    #[test]
    fn test_error_messages_are_descriptive() {
        // Verify error messages contain useful information

        // Exponential error
        let exp_err = CurveType::exponential(-1.0).err();
        assert!(exp_err.is_some());
        let msg = exp_err.as_ref().map_or_else(String::new, |e| e.to_string());
        assert!(msg.contains("must be > 0") || msg.contains("-1"));

        // Logarithmic error
        let log_err = CurveType::logarithmic(0.5).err();
        assert!(log_err.is_some());
        let msg = log_err.as_ref().map_or_else(String::new, |e| e.to_string());
        assert!(msg.contains("must be > 1") || msg.contains("0.5"));

        // Bezier error
        let bezier_err = BezierCurve::new([(0.0, 0.0), (1.5, 0.5), (0.75, 0.25), (1.0, 1.0)]).err();
        assert!(bezier_err.is_some());
        let msg = bezier_err
            .as_ref()
            .map_or_else(String::new, |e| e.to_string());
        assert!(msg.contains("Control point") && msg.contains("1.5"));
    }
}
