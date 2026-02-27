//! Cubic Bezier curve implementation for FFB response mapping.

use serde::{Deserialize, Serialize};

use crate::error::CurveError;
use crate::lut::CurveLut;

/// Cubic Bezier curve for FFB response mapping.
///
/// A cubic Bezier curve defined by four control points P0, P1, P2, P3.
/// For FFB response mapping, P0 is typically (0,0) and P3 is (1,1),
/// with P1 and P2 controlling the curve shape.
///
/// The curve maps input values in [0,1] to output values in [0,1].
///
/// # RT Safety
///
/// **WARNING**: Direct evaluation via `map()` is NOT RT-safe.
/// It uses Newton-Raphson iteration which may take up to 8 iterations
/// with variable timing. Use `to_lut()` to create a lookup table
/// for RT-safe evaluation.
///
/// # Example
///
/// ```
/// use openracing_curves::BezierCurve;
///
/// // Create an S-curve for gradual response near center
/// let curve = BezierCurve::new([
///     (0.0, 0.0),
///     (0.0, 0.5),  // Pull start toward y=0.5
///     (1.0, 0.5),  // Pull end toward y=0.5
///     (1.0, 1.0),
/// ])?;
///
/// // Convert to LUT for RT-safe evaluation
/// let lut = curve.to_lut();
/// # Ok::<(), openracing_curves::CurveError>(())
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BezierCurve {
    /// Control points P0, P1, P2, P3 as (x, y) tuples.
    pub control_points: [(f32, f32); 4],
}

impl BezierCurve {
    /// Create a new Bezier curve with the given control points.
    ///
    /// # Arguments
    ///
    /// * `control_points` - Four control points [(x0,y0), (x1,y1), (x2,y2), (x3,y3)]
    ///
    /// # Returns
    ///
    /// * `Ok(BezierCurve)` if all control points are valid
    /// * `Err(CurveError)` if any control point is outside [0,1]² or non-finite
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_curves::BezierCurve;
    ///
    /// let curve = BezierCurve::new([
    ///     (0.0, 0.0),
    ///     (0.25, 0.5),
    ///     (0.75, 0.5),
    ///     (1.0, 1.0),
    /// ])?;
    /// # Ok::<(), openracing_curves::CurveError>(())
    /// ```
    pub fn new(control_points: [(f32, f32); 4]) -> Result<Self, CurveError> {
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

    /// Create a linear curve (identity mapping).
    ///
    /// The linear curve has control points that result in f(x) = x.
    pub fn linear() -> Self {
        Self {
            control_points: [(0.0, 0.0), (0.33, 0.33), (0.67, 0.67), (1.0, 1.0)],
        }
    }

    /// Create an "ease-in" curve (slow start, fast end).
    pub fn ease_in() -> Self {
        Self {
            control_points: [(0.0, 0.0), (0.0, 0.0), (1.0, 0.0), (1.0, 1.0)],
        }
    }

    /// Create an "ease-out" curve (fast start, slow end).
    pub fn ease_out() -> Self {
        Self {
            control_points: [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 1.0)],
        }
    }

    /// Create an "ease-in-out" S-curve (slow start and end, fast middle).
    pub fn ease_in_out() -> Self {
        Self {
            control_points: [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)],
        }
    }

    /// Check if a coordinate value is valid (finite and in [0,1] range).
    #[inline]
    fn is_valid_coordinate(value: f32) -> bool {
        value.is_finite() && (0.0..=1.0).contains(&value)
    }

    /// Evaluate the cubic Bezier curve at parameter t.
    ///
    /// Uses the formula: B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
    ///
    /// # Arguments
    ///
    /// * `t` - Parameter value in [0,1] (will be clamped)
    ///
    /// # Returns
    ///
    /// The (x, y) point on the curve at parameter t.
    #[inline]
    pub fn evaluate(&self, t: f32) -> (f32, f32) {
        let t = t.clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = self.control_points;

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
    ///
    /// * `target_x` - The x value to find (in [0,1])
    ///
    /// # Returns
    ///
    /// The parameter t that produces the target x value.
    ///
    /// # Note
    ///
    /// This method is NOT RT-safe. It may iterate up to 8 times.
    fn find_t_for_x(&self, target_x: f32) -> f32 {
        let target_x = target_x.clamp(0.0, 1.0);

        let mut t = target_x;

        const MAX_ITERATIONS: usize = 8;
        const EPSILON: f32 = 1e-6;

        for _ in 0..MAX_ITERATIONS {
            let (x, _) = self.evaluate(t);
            let error = x - target_x;

            if error.abs() < EPSILON {
                break;
            }

            let dx_dt = self.evaluate_x_derivative(t);

            if dx_dt.abs() < EPSILON {
                break;
            }

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

        let d0 = x1 - x0;
        let d1 = x2 - x1;
        let d2 = x3 - x2;

        3.0 * (mt * mt * d0 + 2.0 * mt * t * d1 + t * t * d2)
    }

    /// Map an input x value to an output y value.
    ///
    /// This finds the parameter t where the curve's x equals the input,
    /// then returns the corresponding y value.
    ///
    /// # WARNING: NOT RT-SAFE
    ///
    /// This method uses Newton-Raphson iteration and should only be
    /// called at profile load time. For RT evaluation, use `to_lut()`.
    ///
    /// # Arguments
    ///
    /// * `input` - Input value in [0,1] (will be clamped)
    ///
    /// # Returns
    ///
    /// Output value in [0,1].
    pub fn map(&self, input: f32) -> f32 {
        let t = self.find_t_for_x(input);
        let (_, y) = self.evaluate(t);
        y.clamp(0.0, 1.0)
    }

    /// Convert to a pre-computed lookup table for RT-safe evaluation.
    ///
    /// This should be called at profile load time, not in the RT path.
    ///
    /// # Returns
    ///
    /// A `CurveLut` ready for RT-safe lookups.
    pub fn to_lut(&self) -> CurveLut {
        CurveLut::from_bezier(self)
    }

    /// Validate the curve's control points.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if all control points are valid
    /// * `Err(CurveError)` if any control point is invalid
    pub fn validate(&self) -> Result<(), CurveError> {
        for (i, (x, y)) in self.control_points.iter().enumerate() {
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
}

impl Default for BezierCurve {
    fn default() -> Self {
        Self::linear()
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
    fn test_bezier_creation_valid() -> Result<(), CurveError> {
        let curve = BezierCurve::new([(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)])?;
        assert_eq!(curve.control_points[0], (0.0, 0.0));
        assert_eq!(curve.control_points[3], (1.0, 1.0));
        Ok(())
    }

    #[test]
    fn test_bezier_creation_invalid_x() {
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
    fn test_bezier_creation_invalid_y() {
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
    fn test_bezier_creation_nan() {
        let result = BezierCurve::new([(0.0, 0.0), (f32::NAN, 0.5), (0.75, 0.5), (1.0, 1.0)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bezier_creation_infinity() {
        let result = BezierCurve::new([(0.0, 0.0), (f32::INFINITY, 0.5), (0.75, 0.5), (1.0, 1.0)]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bezier_creation_neg_infinity() {
        let result = BezierCurve::new([
            (0.0, 0.0),
            (0.25, f32::NEG_INFINITY),
            (0.75, 0.5),
            (1.0, 1.0),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bezier_linear_curve() {
        let curve = BezierCurve::linear();

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

        let (x0, y0) = curve.evaluate(0.0);
        assert!((x0 - 0.0).abs() < 1e-6);
        assert!((y0 - 0.0).abs() < 1e-6);

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

        assert!((curve.map(0.0) - 0.0).abs() < 0.01);
        assert!((curve.map(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_bezier_map_clamping() {
        let curve = BezierCurve::linear();

        let below = curve.map(-0.5);
        let above = curve.map(1.5);

        assert!((0.0..=1.0).contains(&below));
        assert!((0.0..=1.0).contains(&above));
    }

    #[test]
    fn test_bezier_s_curve() {
        let curve = must(BezierCurve::new([
            (0.0, 0.0),
            (0.0, 0.5),
            (1.0, 0.5),
            (1.0, 1.0),
        ]));
        let lut = curve.to_lut();

        let low = lut.lookup(0.25);
        let mid = lut.lookup(0.5);
        let high = lut.lookup(0.75);

        assert!(low < mid);
        assert!(mid < high);

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

        let json = serde_json::to_string(&curve);
        assert!(json.is_ok());

        let json_str = json.expect("serialization failed");
        let deserialized: Result<BezierCurve, _> = serde_json::from_str(&json_str);
        assert!(deserialized.is_ok());
        assert_eq!(curve, deserialized.expect("deserialization failed"));
    }

    #[test]
    fn test_bezier_default() {
        let curve = BezierCurve::default();
        assert_eq!(curve, BezierCurve::linear());
    }

    #[test]
    fn test_bezier_presets() {
        let ease_in = BezierCurve::ease_in();
        let ease_out = BezierCurve::ease_out();
        let ease_in_out = BezierCurve::ease_in_out();

        assert!(ease_in.validate().is_ok());
        assert!(ease_out.validate().is_ok());
        assert!(ease_in_out.validate().is_ok());

        assert!((ease_in.map(0.0) - 0.0).abs() < 0.01);
        assert!((ease_in.map(1.0) - 1.0).abs() < 0.01);
        assert!((ease_out.map(0.0) - 0.0).abs() < 0.01);
        assert!((ease_out.map(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_bezier_validate() {
        let curve = BezierCurve::linear();
        assert!(curve.validate().is_ok());

        let mut invalid = BezierCurve::linear();
        invalid.control_points[1] = (1.5, 0.5);
        assert!(invalid.validate().is_err());
    }
}
