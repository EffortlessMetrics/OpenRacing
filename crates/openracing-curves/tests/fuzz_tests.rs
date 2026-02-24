//! Fuzzing tests for curve evaluation with edge cases.
//!
//! These tests verify that curves handle NaN, Infinity, and other edge values correctly.

use openracing_curves::{BezierCurve, CurveError, CurveLut, CurveType};

type TestResult = Result<(), CurveError>;

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic!("unexpected error: {:?}", e),
    }
}

#[test]
fn fuzz_nan_input() -> TestResult {
    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0)?,
        CurveType::logarithmic(10.0)?,
        CurveType::Bezier(BezierCurve::linear()),
    ];

    for curve in curves {
        let output = curve.evaluate(f32::NAN);
        assert!(output.is_nan() || (0.0..=1.0).contains(&output));
    }

    let lut = CurveLut::linear();
    let output = lut.lookup(f32::NAN);
    assert!(output.is_nan() || (0.0..=1.0).contains(&output));

    Ok(())
}

#[test]
fn fuzz_infinity_input() -> TestResult {
    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0)?,
        CurveType::logarithmic(10.0)?,
        CurveType::Bezier(BezierCurve::linear()),
    ];

    for curve in &curves {
        let output_pos = curve.evaluate(f32::INFINITY);
        let output_neg = curve.evaluate(f32::NEG_INFINITY);

        assert!(
            (0.0..=1.0).contains(&output_pos) || output_pos.is_nan(),
            "Positive infinity produced: {}",
            output_pos
        );
        assert!(
            (0.0..=1.0).contains(&output_neg) || output_neg.is_nan(),
            "Negative infinity produced: {}",
            output_neg
        );
    }

    let lut = CurveLut::linear();
    let output_pos = lut.lookup(f32::INFINITY);
    let output_neg = lut.lookup(f32::NEG_INFINITY);

    assert!((0.0..=1.0).contains(&output_pos));
    assert!((0.0..=1.0).contains(&output_neg));

    Ok(())
}

#[test]
fn fuzz_tiny_values() -> TestResult {
    let tiny_values = [
        f32::MIN_POSITIVE,
        f32::EPSILON,
        1e-30,
        1e-20,
        1e-10,
        -f32::MIN_POSITIVE,
        -f32::EPSILON,
        -1e-30,
        -1e-20,
        -1e-10,
    ];

    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0)?,
        CurveType::logarithmic(10.0)?,
    ];

    for curve in &curves {
        for &value in &tiny_values {
            let output = curve.evaluate(value);
            assert!(
                (0.0..=1.0).contains(&output),
                "Curve produced {} for tiny input {}",
                output,
                value
            );
        }
    }

    let lut = CurveLut::linear();
    for &value in &tiny_values {
        let output = lut.lookup(value);
        assert!(
            (0.0..=1.0).contains(&output),
            "LUT produced {} for tiny input {}",
            output,
            value
        );
    }

    Ok(())
}

#[test]
fn fuzz_large_values() -> TestResult {
    let large_values = [
        100.0,
        1000.0,
        10000.0,
        f32::MAX / 2.0,
        f32::MAX,
        -100.0,
        -1000.0,
        -10000.0,
        f32::MIN / 2.0,
        f32::MIN,
    ];

    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0)?,
        CurveType::logarithmic(10.0)?,
    ];

    for curve in &curves {
        for &value in &large_values {
            let output = curve.evaluate(value);
            assert!(
                (0.0..=1.0).contains(&output),
                "Curve produced {} for large input {}",
                output,
                value
            );
        }
    }

    let lut = CurveLut::linear();
    for &value in &large_values {
        let output = lut.lookup(value);
        assert!(
            (0.0..=1.0).contains(&output),
            "LUT produced {} for large input {}",
            output,
            value
        );
    }

    Ok(())
}

#[test]
fn fuzz_bezier_nan_control_points() {
    let result = BezierCurve::new([(0.0, 0.0), (f32::NAN, 0.5), (0.75, 0.25), (1.0, 1.0)]);
    assert!(result.is_err());

    let result = BezierCurve::new([(0.0, 0.0), (0.25, f32::NAN), (0.75, 0.25), (1.0, 1.0)]);
    assert!(result.is_err());
}

#[test]
fn fuzz_bezier_infinity_control_points() {
    let result = BezierCurve::new([(0.0, 0.0), (f32::INFINITY, 0.5), (0.75, 0.25), (1.0, 1.0)]);
    assert!(result.is_err());

    let result = BezierCurve::new([
        (0.0, 0.0),
        (f32::NEG_INFINITY, 0.5),
        (0.75, 0.25),
        (1.0, 1.0),
    ]);
    assert!(result.is_err());
}

#[test]
fn fuzz_exponential_nan_exponent() {
    let result = CurveType::exponential(f32::NAN);
    assert!(result.is_err());
}

#[test]
fn fuzz_exponential_infinity_exponent() {
    let result = CurveType::exponential(f32::INFINITY);
    assert!(result.is_err());

    let result = CurveType::exponential(f32::NEG_INFINITY);
    assert!(result.is_err());
}

#[test]
fn fuzz_logarithmic_nan_base() {
    let result = CurveType::logarithmic(f32::NAN);
    assert!(result.is_err());
}

#[test]
fn fuzz_logarithmic_infinity_base() {
    let result = CurveType::logarithmic(f32::INFINITY);
    assert!(result.is_err());

    let result = CurveType::logarithmic(f32::NEG_INFINITY);
    assert!(result.is_err());
}

#[test]
fn fuzz_zero_input() -> TestResult {
    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0)?,
        CurveType::logarithmic(10.0)?,
        CurveType::Bezier(BezierCurve::linear()),
    ];

    for curve in &curves {
        let output = curve.evaluate(0.0);
        assert!(
            (output - 0.0).abs() < 0.01,
            "Curve produced {} for zero input",
            output
        );
    }

    Ok(())
}

#[test]
fn fuzz_one_input() -> TestResult {
    let curves: Vec<CurveType> = vec![
        CurveType::Linear,
        CurveType::exponential(2.0)?,
        CurveType::logarithmic(10.0)?,
        CurveType::Bezier(BezierCurve::linear()),
    ];

    for curve in &curves {
        let output = curve.evaluate(1.0);
        assert!(
            (output - 1.0).abs() < 0.01,
            "Curve produced {} for one input",
            output
        );
    }

    Ok(())
}

#[test]
fn fuzz_boundary_values() -> TestResult {
    let boundary_values = [
        0.0 - f32::EPSILON,
        0.0 + f32::EPSILON,
        1.0 - f32::EPSILON,
        1.0 + f32::EPSILON,
        f32::from_bits(0x3F800000 - 1),
        f32::from_bits(0x3F800000 + 1),
    ];

    let lut = CurveLut::linear();
    for &value in &boundary_values {
        let output = lut.lookup(value);
        assert!(
            (0.0..=1.0).contains(&output),
            "LUT produced {} for boundary input {}",
            output,
            value
        );
    }

    Ok(())
}

#[test]
fn fuzz_denormalized_values() -> TestResult {
    let denorm_values: Vec<f32> = (0..10)
        .map(|i| f32::from_bits(1 << i))
        .chain((0..10).map(|i| f32::from_bits(1 << i | 0x80000000)))
        .collect();

    let lut = CurveLut::linear();
    for &value in &denorm_values {
        let output = lut.lookup(value);
        assert!(
            output.is_finite(),
            "LUT produced non-finite for denorm: {}",
            value
        );
    }

    Ok(())
}
