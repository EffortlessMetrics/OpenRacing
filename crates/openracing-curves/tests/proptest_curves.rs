#![allow(clippy::redundant_closure)]
//! Proptest-based property tests for `openracing-curves`.
//!
//! Complements the quickcheck tests in `property_tests.rs` with additional
//! proptest invariants covering serde round-trips, LUT fidelity,
//! validation consistency, and edge-case curve parameters.

use openracing_curves::{BezierCurve, CurveError, CurveLut, CurveType};
use proptest::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const LUT_TOLERANCE: f32 = 0.02;
const ENDPOINT_TOLERANCE: f32 = 0.01;

// ---------------------------------------------------------------------------
// Helpers / strategies
// ---------------------------------------------------------------------------

fn unit_f32() -> impl Strategy<Value = f32> {
    (0u32..=10_000u32).prop_map(|v| v as f32 / 10_000.0)
}

fn valid_exponent() -> impl Strategy<Value = f32> {
    (1u32..=10_000u32).prop_map(|v| v as f32 / 1_000.0) // 0.001 .. 10.0
}

fn valid_log_base() -> impl Strategy<Value = f32> {
    (1100u32..=100_000u32).prop_map(|v| v as f32 / 1_000.0) // 1.1 .. 100.0
}

// ---------------------------------------------------------------------------
// 1. Serde round-trip for CurveType
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn prop_linear_serde_roundtrip(_ in 0u8..1u8) {
        let curve = CurveType::Linear;
        let json = serde_json::to_string(&curve)?;
        let back: CurveType = serde_json::from_str(&json)?;
        prop_assert_eq!(curve, back);
    }

    #[test]
    fn prop_exponential_serde_roundtrip(exp in valid_exponent()) {
        let curve = CurveType::exponential(exp)?;
        let json = serde_json::to_string(&curve)?;
        let back: CurveType = serde_json::from_str(&json)?;
        prop_assert_eq!(curve, back);
    }

    #[test]
    fn prop_logarithmic_serde_roundtrip(base in valid_log_base()) {
        let curve = CurveType::logarithmic(base)?;
        let json = serde_json::to_string(&curve)?;
        let back: CurveType = serde_json::from_str(&json)?;
        prop_assert_eq!(curve, back);
    }

    #[test]
    fn prop_bezier_serde_roundtrip(
        x1 in unit_f32(), y1 in unit_f32(),
        x2 in unit_f32(), y2 in unit_f32(),
    ) {
        let bezier = BezierCurve::new([(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)])?;
        let curve = CurveType::Bezier(bezier);
        let json = serde_json::to_string(&curve)?;
        let back: CurveType = serde_json::from_str(&json)?;
        prop_assert_eq!(curve, back);
    }
}

// ---------------------------------------------------------------------------
// 2. LUT serialization round-trip
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_lut_serde_roundtrip(exp in valid_exponent()) {
        let curve = CurveType::exponential(exp)?;
        let lut = curve.to_lut();
        let json = serde_json::to_string(&lut)?;
        let back: CurveLut = serde_json::from_str(&json)?;
        for i in 0..CurveLut::SIZE {
            prop_assert!(
                (lut.table()[i] - back.table()[i]).abs() < 1e-6,
                "LUT mismatch at index {}: {} vs {}",
                i, lut.table()[i], back.table()[i],
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. BezierCurve::new validation
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Valid control points always produce Ok.
    #[test]
    fn prop_valid_bezier_accepted(
        x1 in unit_f32(), y1 in unit_f32(),
        x2 in unit_f32(), y2 in unit_f32(),
    ) {
        let result = BezierCurve::new([(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)]);
        prop_assert!(result.is_ok());
    }

    /// Out-of-range x produces Err.
    #[test]
    fn prop_oob_x_rejected(x in 1.001f32..100.0f32) {
        let result = BezierCurve::new([(0.0, 0.0), (x, 0.5), (0.5, 0.5), (1.0, 1.0)]);
        prop_assert!(result.is_err());
    }

    /// Negative y produces Err.
    #[test]
    fn prop_negative_y_rejected(y in -100.0f32..-0.001f32) {
        let result = BezierCurve::new([(0.0, 0.0), (0.5, y), (0.5, 0.5), (1.0, 1.0)]);
        prop_assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// 4. Validate consistency: constructor success ↔ validate().is_ok()
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn prop_exponential_validate_matches_ctor(exp in -5.0f32..15.0f32) {
        if !exp.is_finite() {
            return Ok(());
        }
        let ctor = CurveType::exponential(exp);
        match ctor {
            Ok(curve) => prop_assert!(curve.validate().is_ok()),
            Err(_) => {
                // Build manually and verify validate also fails
                let manual = CurveType::Exponential { exponent: exp };
                prop_assert!(manual.validate().is_err());
            }
        }
    }

    #[test]
    fn prop_logarithmic_validate_matches_ctor(base in -2.0f32..200.0f32) {
        if !base.is_finite() {
            return Ok(());
        }
        let ctor = CurveType::logarithmic(base);
        match ctor {
            Ok(curve) => prop_assert!(curve.validate().is_ok()),
            Err(_) => {
                let manual = CurveType::Logarithmic { base };
                prop_assert!(manual.validate().is_err());
            }
        }
    }

    #[test]
    fn prop_bezier_validate_matches_ctor(
        x1 in -0.5f32..1.5f32, y1 in -0.5f32..1.5f32,
        x2 in -0.5f32..1.5f32, y2 in -0.5f32..1.5f32,
    ) {
        let ctor = BezierCurve::new([(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)]);
        match ctor {
            Ok(curve) => prop_assert!(curve.validate().is_ok()),
            Err(_) => {
                let manual = BezierCurve {
                    control_points: [(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)],
                };
                prop_assert!(manual.validate().is_err());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. from_fn produces valid LUT
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_from_fn_output_bounded(exp in valid_exponent()) {
        let lut = CurveLut::from_fn(|x| x.powf(exp));
        for i in 0..CurveLut::SIZE {
            let v = lut.table()[i];
            prop_assert!(
                (0.0..=1.0).contains(&v),
                "from_fn value out of range at {}: {}", i, v,
            );
        }
    }

    #[test]
    fn prop_from_fn_monotonic_for_powf(exp in valid_exponent()) {
        let lut = CurveLut::from_fn(|x| x.powf(exp));
        prop_assert!(lut.is_monotonic(), "powf({exp}) LUT should be monotonic");
    }
}

// ---------------------------------------------------------------------------
// 6. Deterministic evaluation
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn prop_evaluate_deterministic(input in unit_f32(), exp in valid_exponent()) {
        let curve = CurveType::exponential(exp)?;
        let a = curve.evaluate(input);
        let b = curve.evaluate(input);
        prop_assert_eq!(a, b, "evaluate should be deterministic");
    }

    #[test]
    fn prop_lut_lookup_deterministic(input in unit_f32()) {
        let lut = CurveLut::linear();
        let a = lut.lookup(input);
        let b = lut.lookup(input);
        prop_assert_eq!(a, b, "lookup should be deterministic");
    }
}

// ---------------------------------------------------------------------------
// 7. LUT fidelity: from_bezier matches direct map
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_lut_fidelity_bezier(
        input in unit_f32(),
        x1 in unit_f32(), y1 in unit_f32(),
        x2 in unit_f32(), y2 in unit_f32(),
    ) {
        let bezier = BezierCurve::new([(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)])?;
        let lut = CurveLut::from_bezier(&bezier);
        let direct = bezier.map(input);
        let via_lut = lut.lookup(input);
        prop_assert!(
            (direct - via_lut).abs() < LUT_TOLERANCE,
            "LUT vs direct mismatch at {input}: direct={direct}, lut={via_lut}",
        );
    }

    #[test]
    fn prop_lut_fidelity_exponential(input in unit_f32(), exp in valid_exponent()) {
        let curve = CurveType::exponential(exp)?;
        let direct = curve.evaluate(input);
        let via_lut = curve.to_lut().lookup(input);
        prop_assert!(
            (direct - via_lut).abs() < LUT_TOLERANCE,
            "LUT vs direct at {input}: direct={direct}, lut={via_lut}",
        );
    }

    #[test]
    fn prop_lut_fidelity_logarithmic(input in unit_f32(), base in valid_log_base()) {
        let curve = CurveType::logarithmic(base)?;
        let direct = curve.evaluate(input);
        let via_lut = curve.to_lut().lookup(input);
        prop_assert!(
            (direct - via_lut).abs() < LUT_TOLERANCE,
            "LUT vs direct at {input}: direct={direct}, lut={via_lut}",
        );
    }
}

// ---------------------------------------------------------------------------
// 8. Edge cases
// ---------------------------------------------------------------------------

#[test]
fn edge_extreme_exponent_small() -> TestResult {
    let curve = CurveType::exponential(0.001)?;
    let out = curve.evaluate(0.5);
    assert!((0.0..=1.0).contains(&out));
    assert!((curve.evaluate(0.0) - 0.0).abs() < ENDPOINT_TOLERANCE);
    assert!((curve.evaluate(1.0) - 1.0).abs() < ENDPOINT_TOLERANCE);
    Ok(())
}

#[test]
fn edge_extreme_exponent_large() -> TestResult {
    let curve = CurveType::exponential(10.0)?;
    let out = curve.evaluate(0.5);
    assert!((0.0..=1.0).contains(&out));
    assert!((curve.evaluate(0.0) - 0.0).abs() < ENDPOINT_TOLERANCE);
    assert!((curve.evaluate(1.0) - 1.0).abs() < ENDPOINT_TOLERANCE);
    Ok(())
}

#[test]
fn edge_log_base_near_one() -> TestResult {
    let curve = CurveType::logarithmic(1.1)?;
    let out = curve.evaluate(0.5);
    assert!((0.0..=1.0).contains(&out));
    assert!((curve.evaluate(0.0) - 0.0).abs() < ENDPOINT_TOLERANCE);
    assert!((curve.evaluate(1.0) - 1.0).abs() < ENDPOINT_TOLERANCE);
    Ok(())
}

#[test]
fn edge_log_base_large() -> TestResult {
    let curve = CurveType::logarithmic(100.0)?;
    let out = curve.evaluate(0.5);
    assert!((0.0..=1.0).contains(&out));
    assert!((curve.evaluate(0.0) - 0.0).abs() < ENDPOINT_TOLERANCE);
    assert!((curve.evaluate(1.0) - 1.0).abs() < ENDPOINT_TOLERANCE);
    Ok(())
}

#[test]
fn edge_nan_exponent_rejected() {
    assert!(CurveType::exponential(f32::NAN).is_err());
}

#[test]
fn edge_inf_exponent_rejected() {
    assert!(CurveType::exponential(f32::INFINITY).is_err());
    assert!(CurveType::exponential(f32::NEG_INFINITY).is_err());
}

#[test]
fn edge_nan_log_base_rejected() {
    assert!(CurveType::logarithmic(f32::NAN).is_err());
}

#[test]
fn edge_inf_log_base_rejected() {
    assert!(CurveType::logarithmic(f32::INFINITY).is_err());
}

#[test]
fn edge_bezier_nan_rejected() {
    assert!(BezierCurve::new([(0.0, 0.0), (f32::NAN, 0.5), (0.5, 0.5), (1.0, 1.0)]).is_err());
    assert!(BezierCurve::new([(0.0, 0.0), (0.5, f32::NAN), (0.5, 0.5), (1.0, 1.0)]).is_err());
}

#[test]
fn edge_lut_linear_min_max() {
    let lut = CurveLut::linear();
    assert!((lut.min_value() - 0.0).abs() < 0.01);
    assert!((lut.max_value() - 1.0).abs() < 0.01);
}

#[test]
fn edge_lut_deserialization_wrong_size() {
    let bad = serde_json::to_string(&vec![0.0f32; 100]);
    assert!(bad.is_ok());
    let result: Result<CurveLut, _> = serde_json::from_str(bad.as_deref().unwrap_or("[]"));
    assert!(result.is_err());
}

#[test]
fn edge_bezier_presets_endpoints() {
    for curve in [
        BezierCurve::linear(),
        BezierCurve::ease_in(),
        BezierCurve::ease_out(),
        BezierCurve::ease_in_out(),
    ] {
        let at_zero = curve.map(0.0);
        let at_one = curve.map(1.0);
        assert!(
            (at_zero - 0.0).abs() < ENDPOINT_TOLERANCE,
            "preset at 0.0 = {at_zero}"
        );
        assert!(
            (at_one - 1.0).abs() < ENDPOINT_TOLERANCE,
            "preset at 1.0 = {at_one}"
        );
    }
}

#[test]
fn edge_custom_lut_roundtrip_through_curvetype() -> TestResult {
    let lut = CurveLut::from_fn(|x| x * x);
    let curve = CurveType::Custom(Box::new(lut.clone()));
    let json = serde_json::to_string(&curve)?;
    let back: CurveType = serde_json::from_str(&json)?;
    if let CurveType::Custom(back_lut) = &back {
        for i in 0..CurveLut::SIZE {
            assert!(
                (lut.table()[i] - back_lut.table()[i]).abs() < 1e-6,
                "Custom LUT roundtrip mismatch at {i}"
            );
        }
    } else {
        panic!("Expected CurveType::Custom after round-trip");
    }
    Ok(())
}

#[test]
fn edge_default_curvetype_is_linear() {
    let curve = CurveType::default();
    assert!(matches!(curve, CurveType::Linear));
    assert!((curve.evaluate(0.5) - 0.5).abs() < 1e-6);
}

#[test]
fn edge_default_bezier_is_linear() {
    let bezier = BezierCurve::default();
    assert_eq!(bezier, BezierCurve::linear());
}

#[test]
fn edge_error_display() {
    let err = CurveError::ControlPointOutOfRange {
        point_index: 1,
        coordinate: "x",
        value: 1.5,
    };
    let msg = format!("{err}");
    assert!(msg.contains("Control point 1"));
    assert!(msg.contains("x"));
    assert!(msg.contains("1.5"));
}
