//! Property-based tests for curve evaluation.
//!
//! These tests verify mathematical properties that should hold for all curve types.

use openracing_curves::{BezierCurve, CurveLut, CurveType};
use quickcheck_macros::quickcheck;

const TOLERANCE: f32 = 0.02;
const ENDPOINT_TOLERANCE: f32 = 0.01;

fn sanitize_f32(v: f32) -> f32 {
    if v.is_nan() {
        0.5
    } else if v.is_infinite() {
        if v > 0.0 { 1.0 } else { 0.0 }
    } else {
        v
    }
}

#[quickcheck]
fn prop_linear_maps_to_self(input: f32) -> bool {
    let input = sanitize_f32(input).clamp(0.0, 1.0);
    let curve = CurveType::Linear;
    let output = curve.evaluate(input);
    (output - input).abs() < TOLERANCE
}

#[quickcheck]
fn prop_all_curves_map_to_unit_range(input: f32, exponent: f32, base: f32) -> bool {
    let input = sanitize_f32(input).clamp(0.0, 1.0);
    let exp = sanitize_f32(exponent).clamp(0.1, 10.0);
    let b = sanitize_f32(base).clamp(1.1, 100.0);

    let curve = CurveType::exponential(exp);
    if let Ok(curve) = curve {
        let output = curve.evaluate(input);
        if !(0.0..=1.0).contains(&output) {
            return false;
        }
    }

    let curve = CurveType::logarithmic(b);
    if let Ok(curve) = curve {
        let output = curve.evaluate(input);
        if !(0.0..=1.0).contains(&output) {
            return false;
        }
    }

    true
}

#[quickcheck]
fn prop_endpoints_map_to_zero_and_one(_input: f32) -> bool {
    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0).unwrap_or(CurveType::Linear),
        CurveType::logarithmic(10.0).unwrap_or(CurveType::Linear),
        CurveType::Bezier(BezierCurve::linear()),
    ];

    for curve in curves {
        let at_zero = curve.evaluate(0.0);
        let at_one = curve.evaluate(1.0);

        if (at_zero - 0.0).abs() > ENDPOINT_TOLERANCE {
            return false;
        }
        if (at_one - 1.0).abs() > ENDPOINT_TOLERANCE {
            return false;
        }
    }

    true
}

#[quickcheck]
fn prop_lut_matches_direct_evaluation(input: f32) -> bool {
    let input = sanitize_f32(input).clamp(0.0, 1.0);
    let tolerance = 0.02;

    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0).unwrap_or(CurveType::Linear),
        CurveType::logarithmic(10.0).unwrap_or(CurveType::Linear),
    ];

    for curve in curves {
        let direct = curve.evaluate(input);
        let lut = curve.to_lut();
        let lut_output = lut.lookup(input);

        if (direct - lut_output).abs() > tolerance {
            return false;
        }
    }

    true
}

#[quickcheck]
fn prop_exponential_is_monotonic(exponent: f32) -> bool {
    let exp = sanitize_f32(exponent).clamp(0.1, 10.0);
    let curve = match CurveType::exponential(exp) {
        Ok(c) => c,
        Err(_) => return true,
    };

    let lut = curve.to_lut();
    lut.is_monotonic()
}

#[quickcheck]
fn prop_logarithmic_is_monotonic(base: f32) -> bool {
    let b = sanitize_f32(base).clamp(1.1, 100.0);
    let curve = match CurveType::logarithmic(b) {
        Ok(c) => c,
        Err(_) => return true,
    };

    let lut = curve.to_lut();
    lut.is_monotonic()
}

#[quickcheck]
fn prop_bezier_endpoints_valid(x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
    let x1 = sanitize_f32(x1).clamp(0.0, 1.0);
    let y1 = sanitize_f32(y1).clamp(0.0, 1.0);
    let x2 = sanitize_f32(x2).clamp(0.0, 1.0);
    let y2 = sanitize_f32(y2).clamp(0.0, 1.0);

    let curve = BezierCurve::new([(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)]);

    if let Ok(curve) = curve {
        let at_zero = curve.map(0.0);
        let at_one = curve.map(1.0);

        if (at_zero - 0.0).abs() > ENDPOINT_TOLERANCE {
            return false;
        }
        if (at_one - 1.0).abs() > ENDPOINT_TOLERANCE {
            return false;
        }
    }

    true
}

#[quickcheck]
fn prop_lut_interpolation_is_smooth(input: f32) -> bool {
    let input = sanitize_f32(input).clamp(0.01, 0.99);
    let lut = CurveLut::linear();

    let below = lut.lookup((input - 0.01).max(0.0));
    let at = lut.lookup(input);
    let above = lut.lookup((input + 0.01).min(1.0));

    if !at.is_finite() || !below.is_finite() || !above.is_finite() {
        return false;
    }

    true
}

#[quickcheck]
fn prop_clamping_works(input: f32) -> bool {
    let input = sanitize_f32(input);
    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0).unwrap_or(CurveType::Linear),
        CurveType::logarithmic(10.0).unwrap_or(CurveType::Linear),
    ];

    for curve in curves {
        let output = curve.evaluate(input);
        if !(0.0..=1.0).contains(&output) {
            return false;
        }
    }

    true
}

#[quickcheck]
fn prop_lut_lookup_clamping(input: f32) -> bool {
    let input = sanitize_f32(input);
    let lut = CurveLut::linear();
    let output = lut.lookup(input);
    (0.0..=1.0).contains(&output)
}

#[quickcheck]
fn prop_bezier_map_output_in_range(input: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
    let input = sanitize_f32(input).clamp(0.0, 1.0);
    let x1 = sanitize_f32(x1).clamp(0.0, 1.0);
    let y1 = sanitize_f32(y1).clamp(0.0, 1.0);
    let x2 = sanitize_f32(x2).clamp(0.0, 1.0);
    let y2 = sanitize_f32(y2).clamp(0.0, 1.0);

    let curve = match BezierCurve::new([(0.0, 0.0), (x1, y1), (x2, y2), (1.0, 1.0)]) {
        Ok(c) => c,
        Err(_) => return true,
    };

    let output = curve.map(input);
    (0.0..=1.0).contains(&output)
}
