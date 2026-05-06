//! Profile tuning data structures
//!
//! Provides the data schemas for custom interpolation points, Look-Up Tables
//! (LUTs), and tuning constraints.

use serde::{Deserialize, Serialize};

/// A single interpolation control point for a custom tuning curve.
///
/// X and Y should generally be in the normalized `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    /// The input value
    pub x: f32,
    /// The mapped output value
    pub y: f32,
}

impl CurvePoint {
    /// Create a new curve point at the given `(x, y)` position.
    ///
    /// Both `x` and `y` should be in the `[0.0, 1.0]` range. Out-of-range or
    /// non-finite values will be rejected by [`crate::validate_settings`].
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A custom tuning curve defined by a sequence of control points.
///
/// The curve defines how a raw input (like a pedal depression) maps to
/// an adjusted output response before it is fed to the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomCurve {
    /// The sorted control points forming this curve.
    pub points: Vec<CurvePoint>,
}

impl CustomCurve {
    /// Create a new custom curve from a list of control points.
    ///
    /// The `points` should be sorted by ascending `x` and span the full
    /// `[0.0, 1.0]` range. Use [`crate::validate_settings`] to verify
    /// monotonicity, anchor constraints, and finite-value invariants.
    pub fn new(points: Vec<CurvePoint>) -> Self {
        Self { points }
    }
}

impl Default for CustomCurve {
    /// Returns a linear identity curve `[(0, 0), (1, 1)]`.
    fn default() -> Self {
        Self {
            points: vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)],
        }
    }
}
