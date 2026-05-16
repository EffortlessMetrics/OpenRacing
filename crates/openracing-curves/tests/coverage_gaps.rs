//! Additional coverage tests for `openracing-curves`.
//!
//! Pins behaviour the existing suite leaves implicit:
//!
//! * `CurveLut::min_value` / `max_value` on a non-monotonic LUT.
//! * `CurveLut::is_monotonic` for a strictly-decreasing and a constant
//!   LUT (covers the `<` boundary in the existing implementation).
//! * `CurveLut::table` returns a reference to all 256 entries.
//! * `BezierCurve::evaluate` clamps `t` below 0 and above 1.
//! * `BezierCurve::find_t_for_x` early-out when `dx_dt` is near zero
//!   (coincident-x control points — covers the `EPSILON` break branch
//!   without exploding).
//! * `CurveType::Custom(...)::to_lut` clones the inner LUT (equality
//!   check via `PartialEq`).
//! * `CurveType::Custom(...)::evaluate` clamps out-of-range input.
//! * `CurveError::PartialEq` for matching and differing field cases.
//! * `CurveLut` `Deserialize` rejects payloads larger than 256 entries.
//! * `CurveType::logarithmic(_)?.evaluate(0.0)` and `evaluate(1.0)`
//!   short-circuit to exact `0.0` and `1.0` (strict equality, not
//!   tolerance).
//! * `BezierCurve` `Clone` / `Debug` are visibly exercised.

use openracing_curves::{BezierCurve, CurveError, CurveLut, CurveType};

// ---------------------------------------------------------------------------
// CurveLut min/max/is_monotonic/table
// ---------------------------------------------------------------------------

#[test]
fn lut_min_max_on_non_monotonic_lut() {
    // Tent function: rises 0 → 1 over [0, 0.5], descends 1 → 0 over [0.5, 1].
    let lut = CurveLut::from_fn(|x| if x < 0.5 { x * 2.0 } else { (1.0 - x) * 2.0 });
    let min = lut.min_value();
    let max = lut.max_value();
    assert!((min - 0.0).abs() < 0.05, "min should be near 0, got {min}");
    assert!((max - 1.0).abs() < 0.05, "max should be near 1, got {max}");
    assert!(!lut.is_monotonic(), "tent function is not monotonic");
}

#[test]
fn lut_is_monotonic_strictly_decreasing_is_false() {
    let lut = CurveLut::from_fn(|x| 1.0 - x);
    assert!(!lut.is_monotonic());
}

#[test]
fn lut_is_monotonic_constant_is_true() {
    // Boundary: equal consecutive entries should satisfy "non-decreasing".
    let lut = CurveLut::from_fn(|_| 0.7);
    assert!(lut.is_monotonic());
}

#[test]
fn lut_table_has_256_entries_and_endpoints() {
    let lut = CurveLut::linear();
    let t = lut.table();
    assert_eq!(t.len(), 256);
    assert!((t[0] - 0.0).abs() < f32::EPSILON);
    assert!((t[255] - 1.0).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// BezierCurve evaluate clamping
// ---------------------------------------------------------------------------

#[test]
fn bezier_evaluate_clamps_t_below_zero() -> Result<(), CurveError> {
    let curve = BezierCurve::ease_in_out();
    let at_zero = curve.evaluate(0.0);
    let below = curve.evaluate(-1.0);
    assert_eq!(at_zero, below);
    Ok(())
}

#[test]
fn bezier_evaluate_clamps_t_above_one() -> Result<(), CurveError> {
    let curve = BezierCurve::ease_in_out();
    let at_one = curve.evaluate(1.0);
    let above = curve.evaluate(2.0);
    assert_eq!(at_one, above);
    Ok(())
}

// ---------------------------------------------------------------------------
// BezierCurve::map with coincident-x control points
// ---------------------------------------------------------------------------

#[test]
fn bezier_map_with_coincident_x_does_not_diverge() -> Result<(), CurveError> {
    // Two interior control points share an x value, which makes the x
    // derivative near zero on a stretch of the curve. The Newton-Raphson
    // inside `map` must not panic and must produce a finite y.
    let curve = BezierCurve::new([(0.0, 0.0), (0.5, 0.2), (0.5, 0.8), (1.0, 1.0)])?;
    let y = curve.map(0.3);
    assert!(y.is_finite(), "y should be finite, got {y}");
    assert!((0.0..=1.0).contains(&y), "y out of unit range: {y}");
    Ok(())
}

// ---------------------------------------------------------------------------
// CurveType::Custom round-trip via to_lut and evaluate clamping
// ---------------------------------------------------------------------------

#[test]
fn curve_type_custom_to_lut_clones_table() {
    let original = CurveLut::from_fn(|x| x * x);
    let custom = CurveType::Custom(Box::new(original.clone()));
    let lut = custom.to_lut();
    assert_eq!(lut, original);
}

#[test]
fn curve_type_custom_evaluate_clamps_input_below_zero() {
    let lut = CurveLut::from_fn(|x| x * x);
    let custom = CurveType::Custom(Box::new(lut.clone()));
    assert_eq!(custom.evaluate(-1.0), lut.lookup(0.0));
}

#[test]
fn curve_type_custom_evaluate_clamps_input_above_one() {
    let lut = CurveLut::from_fn(|x| x * x);
    let custom = CurveType::Custom(Box::new(lut.clone()));
    assert_eq!(custom.evaluate(2.0), lut.lookup(1.0));
}

// ---------------------------------------------------------------------------
// CurveError PartialEq
// ---------------------------------------------------------------------------

#[test]
fn curve_error_partial_eq_matching_and_differing_fields() {
    let a = CurveError::ControlPointOutOfRange {
        point_index: 1,
        coordinate: "x",
        value: 1.5,
    };
    let b = CurveError::ControlPointOutOfRange {
        point_index: 1,
        coordinate: "x",
        value: 1.5,
    };
    assert_eq!(a, b);

    let c = CurveError::ControlPointOutOfRange {
        point_index: 2,
        coordinate: "x",
        value: 1.5,
    };
    assert_ne!(a, c);

    let d = CurveError::InvalidConfiguration("foo".to_string());
    let e = CurveError::InvalidConfiguration("bar".to_string());
    assert_ne!(d, e);
    assert_ne!(a, d);
}

// ---------------------------------------------------------------------------
// CurveLut Deserialize rejects oversized payload
// ---------------------------------------------------------------------------

#[test]
fn curve_lut_deserialize_rejects_too_many_entries() {
    let oversize: Vec<f32> = vec![0.5; 512];
    let json = serde_json::to_string(&oversize).expect("serialize Vec<f32>");
    let result: Result<CurveLut, _> = serde_json::from_str(&json);
    assert!(result.is_err());
    let msg = result.expect_err("checked above").to_string();
    assert!(
        msg.contains("Expected 256") || msg.contains("got 512"),
        "got error: {msg}"
    );
}

#[test]
fn curve_lut_deserialize_rejects_too_few_entries() {
    let undersize: Vec<f32> = vec![0.5; 100];
    let json = serde_json::to_string(&undersize).expect("serialize Vec<f32>");
    let result: Result<CurveLut, _> = serde_json::from_str(&json);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Logarithmic endpoints — exact equality (no tolerance)
// ---------------------------------------------------------------------------

#[test]
fn logarithmic_evaluate_at_exact_endpoints() -> Result<(), CurveError> {
    let curve = CurveType::logarithmic(10.0)?;
    // Strict equality — the implementation short-circuits these.
    assert_eq!(curve.evaluate(0.0), 0.0);
    assert_eq!(curve.evaluate(1.0), 1.0);
    Ok(())
}

#[test]
fn logarithmic_evaluate_matches_to_lut_endpoints() -> Result<(), CurveError> {
    // `evaluate` and `to_lut` should agree at the endpoints.
    let curve = CurveType::logarithmic(2.5)?;
    let lut = curve.to_lut();
    assert!((curve.evaluate(0.0) - lut.lookup(0.0)).abs() < 1e-5);
    assert!((curve.evaluate(1.0) - lut.lookup(1.0)).abs() < 1e-5);
    Ok(())
}

// ---------------------------------------------------------------------------
// BezierCurve derived traits
// ---------------------------------------------------------------------------

#[test]
fn bezier_curve_clone_and_debug() {
    let curve = BezierCurve::ease_out();
    let cloned = curve.clone();
    assert_eq!(curve, cloned);
    let dbg = format!("{curve:?}");
    assert!(dbg.contains("BezierCurve"), "got {dbg}");
}

// ---------------------------------------------------------------------------
// CurveLut lookup at the high grid boundary
// ---------------------------------------------------------------------------

#[test]
fn curve_lut_lookup_at_input_one_reaches_table_last_entry() {
    let lut = CurveLut::from_fn(|x| x * x);
    // At input=1.0 the implementation clamps `index_low` to `SIZE - 2`
    // and uses fraction=1.0, so it should land exactly on the last entry.
    let at_one = lut.lookup(1.0);
    assert!((at_one - lut.table()[255]).abs() < f32::EPSILON);
}
