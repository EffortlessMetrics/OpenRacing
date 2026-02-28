//! Curve-Based FFB Effects for OpenRacing
//!
//! This crate implements curve-based force feedback response mapping,
//! including Bezier curves and pre-computed lookup tables for RT-safe evaluation.
//!
//! # Overview
//!
//! The curve system supports:
//! - **Linear**: Identity mapping (input = output)
//! - **Exponential**: Power curve for enhanced response
//! - **Logarithmic**: Compressed response for fine control
//! - **Bezier**: Custom cubic Bezier curves
//! - **Custom**: Pre-computed lookup tables
//!
//! # RT Safety Guarantees
//!
//! ## RT-Safe: `CurveLut::lookup()`
//! The lookup table provides O(1) evaluation with:
//! - No heap allocations
//! - No syscalls or I/O
//! - Bounded execution time
//! - Linear interpolation between entries
//!
//! ## NOT RT-Safe: `BezierCurve::map()`, `CurveType::evaluate()`
//! Direct evaluation uses Newton-Raphson iteration which:
//! - May iterate up to 8 times
//! - Uses floating-point operations that may have variable latency
//! - Should only be used at profile load time
//!
//! # Example
//!
//! ```
//! use openracing_curves::{CurveType, BezierCurve, CurveLut};
//!
//! // Create a Bezier curve (at profile load time)
//! let curve = BezierCurve::new([
//!     (0.0, 0.0),
//!     (0.25, 0.75),
//!     (0.75, 0.25),
//!     (1.0, 1.0),
//! ])?;
//!
//! // Convert to LUT for RT-safe evaluation
//! let lut = curve.to_lut();
//!
//! // RT-safe lookup (1kHz FFB loop)
//! let output = lut.lookup(0.5);
//! assert!(output >= 0.0 && output <= 1.0);
//! # Ok::<(), openracing_curves::CurveError>(())
//! ```

#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]
#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

pub mod bezier;
pub mod curve_type;
pub mod error;
pub mod lut;
pub mod prelude;

pub use bezier::BezierCurve;
pub use curve_type::CurveType;
pub use error::CurveError;
pub use lut::CurveLut;
