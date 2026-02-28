//! Prelude for the curves crate.
//!
//! This module re-exports the most commonly used types and traits.
//!
//! # Example
//!
//! ```
//! use openracing_curves::prelude::*;
//!
//! let curve = BezierCurve::ease_in_out();
//! let lut = curve.to_lut();
//! let output = lut.lookup(0.5);
//! ```

pub use crate::bezier::BezierCurve;
pub use crate::curve_type::CurveType;
pub use crate::error::CurveError;
pub use crate::lut::CurveLut;
