//! Curve-Based FFB Effects
//!
//! This module re-exports curve types from the `openracing-curves` crate.
//!
//! See the `openracing-curves` crate documentation for details on:
//! - `CurveType` - Supported curve types for FFB response mapping
//! - `BezierCurve` - Cubic Bezier curve implementation
//! - `CurveLut` - Pre-computed lookup table for RT-safe evaluation
//! - `CurveError` - Error type for curve operations
//!
//! # RT Safety
//!
//! - `CurveLut::lookup()` is RT-safe (O(1), no allocations)
//! - `BezierCurve::map()` and `CurveType::evaluate()` are NOT RT-safe
//! - Use `to_lut()` at profile load time for RT-safe evaluation

pub use openracing_curves::{BezierCurve, CurveError, CurveLut, CurveType};
