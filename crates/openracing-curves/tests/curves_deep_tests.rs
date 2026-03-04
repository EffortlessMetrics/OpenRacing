//! Deep tests for curve evaluation: interpolation, Bezier, LUT, monotonicity,
//! boundary behavior, control points, composition, numerical stability, and performance.

use openracing_curves::{BezierCurve, CurveLut, CurveType};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Linear interpolation correctness
// ---------------------------------------------------------------------------

mod linear_interpolation_tests {
    use super::*;

    #[test]
    fn linear_lut_matches_identity_at_grid_points() -> TestResult {
        let lut = CurveLut::linear();
        for i in 0..CurveLut::SIZE {
            let input = i as f32 / (CurveLut::SIZE - 1) as f32;
            let output = lut.lookup(input);
            assert!(
                (output - input).abs() < 1e-5,
                "grid point {i}: expected {input}, got {output}"
            );
        }
        Ok(())
    }

    #[test]
    fn linear_lut_interpolates_midpoints_accurately() -> TestResult {
        let lut = CurveLut::linear();
        for i in 0..(CurveLut::SIZE - 1) {
            let low = i as f32 / (CurveLut::SIZE - 1) as f32;
            let high = (i + 1) as f32 / (CurveLut::SIZE - 1) as f32;
            let mid = (low + high) / 2.0;
            let output = lut.lookup(mid);
            assert!(
                (output - mid).abs() < 0.005,
                "midpoint between {i} and {}: expected {mid}, got {output}",
                i + 1
            );
        }
        Ok(())
    }

    #[test]
    fn exponential_lut_interpolation_within_tolerance() -> TestResult {
        let curve = CurveType::exponential(2.0)?;
        let lut = curve.to_lut();
        let test_points = [0.123, 0.234, 0.456, 0.789, 0.999];
        for &x in &test_points {
            let direct = curve.evaluate(x);
            let via_lut = lut.lookup(x);
            assert!(
                (direct - via_lut).abs() < 0.02,
                "at {x}: direct={direct}, lut={via_lut}"
            );
        }
        Ok(())
    }

    #[test]
    fn logarithmic_lut_interpolation_within_tolerance() -> TestResult {
        let curve = CurveType::logarithmic(10.0)?;
        let lut = curve.to_lut();
        let test_points = [0.01, 0.1, 0.333, 0.667, 0.99];
        for &x in &test_points {
            let direct = curve.evaluate(x);
            let via_lut = lut.lookup(x);
            assert!(
                (direct - via_lut).abs() < 0.02,
                "at {x}: direct={direct}, lut={via_lut}"
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Cubic Bezier evaluation
// ---------------------------------------------------------------------------

mod bezier_evaluation_tests {
    use super::*;

    #[test]
    fn bezier_evaluate_at_t_half_on_linear() -> TestResult {
        let curve = BezierCurve::linear();
        let (x, y) = curve.evaluate(0.5);
        assert!((x - 0.5).abs() < 0.01, "linear bezier x at t=0.5: {x}");
        assert!((y - 0.5).abs() < 0.01, "linear bezier y at t=0.5: {y}");
        Ok(())
    }

    #[test]
    fn bezier_evaluate_parametric_is_continuous() -> TestResult {
        let curve = BezierCurve::new([(0.0, 0.0), (0.25, 0.75), (0.75, 0.25), (1.0, 1.0)])?;
        let steps = 1000;
        let mut prev = curve.evaluate(0.0);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let curr = curve.evaluate(t);
            let dx = (curr.0 - prev.0).abs();
            let dy = (curr.1 - prev.1).abs();
            assert!(
                dx < 0.01 && dy < 0.01,
                "discontinuity at t={t}: prev={prev:?}, curr={curr:?}"
            );
            prev = curr;
        }
        Ok(())
    }

    #[test]
    fn bezier_ease_in_below_linear_at_low_input() -> TestResult {
        let ease_in = BezierCurve::ease_in();
        let linear = BezierCurve::linear();
        let ei_val = ease_in.map(0.3);
        let lin_val = linear.map(0.3);
        assert!(
            ei_val < lin_val + 0.01,
            "ease-in at 0.3 should be ≤ linear: ei={ei_val}, lin={lin_val}"
        );
        Ok(())
    }

    #[test]
    fn bezier_ease_out_above_linear_at_low_input() -> TestResult {
        let ease_out = BezierCurve::ease_out();
        let linear = BezierCurve::linear();
        let eo_val = ease_out.map(0.3);
        let lin_val = linear.map(0.3);
        assert!(
            eo_val > lin_val - 0.01,
            "ease-out at 0.3 should be ≥ linear: eo={eo_val}, lin={lin_val}"
        );
        Ok(())
    }

    #[test]
    fn bezier_ease_in_out_midpoint_near_half() -> TestResult {
        let curve = BezierCurve::ease_in_out();
        let mid = curve.map(0.5);
        assert!(
            (mid - 0.5).abs() < 0.05,
            "ease-in-out at 0.5 should be near 0.5: {mid}"
        );
        Ok(())
    }

    #[test]
    fn bezier_s_curve_symmetry() -> TestResult {
        // ease_in_out is symmetric: f(x) + f(1-x) ≈ 1.0
        let curve = BezierCurve::ease_in_out();
        let low = curve.map(0.25);
        let high = curve.map(0.75);
        assert!(
            (low + high - 1.0).abs() < 0.1,
            "S-curve symmetry: f(0.25)={low}, f(0.75)={high}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Curve fitting from sample points via from_fn
// ---------------------------------------------------------------------------

mod curve_fitting_tests {
    use super::*;

    #[test]
    fn from_fn_quadratic() -> TestResult {
        let lut = CurveLut::from_fn(|x| x * x);
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.25).abs() < 0.02);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        assert!(lut.is_monotonic());
        Ok(())
    }

    #[test]
    fn from_fn_sqrt() -> TestResult {
        let lut = CurveLut::from_fn(|x| x.sqrt());
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.25) - 0.5).abs() < 0.02);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        assert!(lut.is_monotonic());
        Ok(())
    }

    #[test]
    fn from_fn_cubic() -> TestResult {
        let lut = CurveLut::from_fn(|x| x * x * x);
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(0.5) - 0.125).abs() < 0.02);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        assert!(lut.is_monotonic());
        Ok(())
    }

    #[test]
    fn from_fn_sin_response() -> TestResult {
        let lut = CurveLut::from_fn(|x| (x * std::f32::consts::FRAC_PI_2).sin());
        assert!((lut.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((lut.lookup(1.0) - 1.0).abs() < 0.01);
        assert!(lut.is_monotonic());
        Ok(())
    }

    #[test]
    fn from_fn_clamps_out_of_range() -> TestResult {
        let lut = CurveLut::from_fn(|x| x * 2.0);
        for i in 0..CurveLut::SIZE {
            let v = lut.table()[i];
            assert!(
                (0.0..=1.0).contains(&v),
                "from_fn should clamp: table[{i}] = {v}"
            );
        }
        Ok(())
    }

    #[test]
    fn from_fn_negative_clamped_to_zero() -> TestResult {
        let lut = CurveLut::from_fn(|x| x - 0.5);
        assert!(
            (lut.lookup(0.0) - 0.0).abs() < 0.01,
            "negative clamped to 0"
        );
        assert!((lut.lookup(1.0) - 0.5).abs() < 0.02);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Monotonicity enforcement
// ---------------------------------------------------------------------------

mod monotonicity_tests {
    use super::*;

    #[test]
    fn linear_lut_is_monotonic() -> TestResult {
        assert!(CurveLut::linear().is_monotonic());
        Ok(())
    }

    #[test]
    fn all_preset_bezier_luts_are_monotonic() -> TestResult {
        for curve in [
            BezierCurve::linear(),
            BezierCurve::ease_in(),
            BezierCurve::ease_out(),
            BezierCurve::ease_in_out(),
        ] {
            let lut = curve.to_lut();
            assert!(lut.is_monotonic(), "preset {curve:?} LUT is not monotonic");
        }
        Ok(())
    }

    #[test]
    fn exponential_curves_are_monotonic() -> TestResult {
        let exponents = [0.1, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0];
        for &exp in &exponents {
            let curve = CurveType::exponential(exp)?;
            let lut = curve.to_lut();
            assert!(lut.is_monotonic(), "exp({exp}) LUT not monotonic");
        }
        Ok(())
    }

    #[test]
    fn logarithmic_curves_are_monotonic() -> TestResult {
        let bases = [1.1, 2.0, std::f32::consts::E, 10.0, 100.0];
        for &base in &bases {
            let curve = CurveType::logarithmic(base)?;
            let lut = curve.to_lut();
            assert!(lut.is_monotonic(), "log({base}) LUT not monotonic");
        }
        Ok(())
    }

    #[test]
    fn non_monotonic_lut_detected() -> TestResult {
        // Triangle wave: rises to 1.0, then falls back to 0.0
        let lut = CurveLut::from_fn(|x| if x < 0.5 { x * 2.0 } else { (1.0 - x) * 2.0 });
        assert!(!lut.is_monotonic(), "triangle wave should not be monotonic");
        Ok(())
    }

    #[test]
    fn constant_lut_is_monotonic() -> TestResult {
        let lut = CurveLut::from_fn(|_| 0.5);
        assert!(
            lut.is_monotonic(),
            "constant LUT should be monotonic (non-decreasing)"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Boundary behavior
// ---------------------------------------------------------------------------

mod boundary_tests {
    use super::*;

    #[test]
    fn lut_from_fn_matches_at_all_grid_points() -> TestResult {
        let f = |x: f32| x * x;
        let lut = CurveLut::from_fn(f);
        for i in 0..CurveLut::SIZE {
            let input = i as f32 / (CurveLut::SIZE - 1) as f32;
            let expected = f(input).clamp(0.0, 1.0);
            let got = lut.table()[i];
            assert!(
                (expected - got).abs() < 1e-6,
                "grid {i}: expected {expected}, got {got}"
            );
        }
        Ok(())
    }

    #[test]
    fn all_curve_types_output_in_range_comprehensive() -> TestResult {
        let curves: Vec<CurveType> = vec![
            CurveType::Linear,
            CurveType::exponential(0.1)?,
            CurveType::exponential(0.5)?,
            CurveType::exponential(1.0)?,
            CurveType::exponential(2.0)?,
            CurveType::exponential(5.0)?,
            CurveType::logarithmic(1.5)?,
            CurveType::logarithmic(2.0)?,
            CurveType::logarithmic(10.0)?,
            CurveType::logarithmic(50.0)?,
            CurveType::Bezier(BezierCurve::linear()),
            CurveType::Bezier(BezierCurve::ease_in()),
            CurveType::Bezier(BezierCurve::ease_out()),
            CurveType::Bezier(BezierCurve::ease_in_out()),
        ];
        let test_inputs = [
            -1.0, -0.1, 0.0, 0.001, 0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99, 0.999, 1.0, 1.1, 2.0,
        ];
        for curve in &curves {
            for &x in &test_inputs {
                let out = curve.evaluate(x);
                assert!((0.0..=1.0).contains(&out), "{curve:?} at {x} = {out}");
            }
        }
        Ok(())
    }

    #[test]
    fn lut_lookup_fine_increment_monotonic() -> TestResult {
        let lut = CurveLut::from_fn(|x| x.sqrt());
        let mut prev = 0.0f32;
        for i in 0..=1000 {
            let x = i as f32 / 1000.0;
            let out = lut.lookup(x);
            assert!((0.0..=1.0).contains(&out), "out of range at {x}: {out}");
            assert!(
                out >= prev - 0.001,
                "decreased at {x}: prev={prev}, out={out}"
            );
            prev = out;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Control point manipulation
// ---------------------------------------------------------------------------

mod control_point_tests {
    use super::*;

    #[test]
    fn modifying_control_points_changes_output() -> TestResult {
        let curve_a = BezierCurve::new([(0.0, 0.0), (0.1, 0.1), (0.9, 0.9), (1.0, 1.0)])?;
        let curve_b = BezierCurve::new([(0.0, 0.0), (0.1, 0.9), (0.9, 0.1), (1.0, 1.0)])?;
        // Test at x=0.25 to avoid midpoint symmetry
        let out_a = curve_a.map(0.25);
        let out_b = curve_b.map(0.25);
        assert!(
            (out_a - out_b).abs() > 0.01,
            "different control points should differ: a={out_a}, b={out_b}"
        );
        Ok(())
    }

    #[test]
    fn p0_and_p3_determine_endpoints() -> TestResult {
        let curve = BezierCurve::new([(0.0, 0.0), (0.5, 0.5), (0.5, 0.5), (1.0, 1.0)])?;
        let (x0, y0) = curve.evaluate(0.0);
        let (x1, y1) = curve.evaluate(1.0);
        assert!((x0 - 0.0).abs() < 1e-6 && (y0 - 0.0).abs() < 1e-6);
        assert!((x1 - 1.0).abs() < 1e-6 && (y1 - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn corner_control_points_produce_step_like_curve() -> TestResult {
        let curve = BezierCurve::new([(0.0, 0.0), (0.0, 0.0), (1.0, 1.0), (1.0, 1.0)])?;
        let low = curve.map(0.1);
        let high = curve.map(0.9);
        assert!(low < 0.3, "step-like: low end at 0.1 = {low}");
        assert!(high > 0.7, "step-like: high end at 0.9 = {high}");
        Ok(())
    }

    #[test]
    fn sliding_control_point_smoothly_changes_output() -> TestResult {
        let mut outputs = Vec::new();
        for i in 0..=10 {
            let y1 = i as f32 / 10.0;
            let curve = BezierCurve::new([(0.0, 0.0), (0.25, y1), (0.75, 0.5), (1.0, 1.0)])?;
            outputs.push(curve.map(0.3));
        }
        for i in 1..outputs.len() {
            let diff = (outputs[i] - outputs[i - 1]).abs();
            assert!(
                diff < 0.2,
                "jump at step {i}: prev={}, curr={}, diff={diff}",
                outputs[i - 1],
                outputs[i]
            );
        }
        Ok(())
    }

    #[test]
    fn validate_rejects_nan_after_manual_edit() -> TestResult {
        let mut curve = BezierCurve::linear();
        curve.control_points[2] = (0.5, f32::NAN);
        assert!(curve.validate().is_err());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Curve composition (chain, blend via LUT)
// ---------------------------------------------------------------------------

mod composition_tests {
    use super::*;

    #[test]
    fn chaining_two_linear_luts_is_linear() -> TestResult {
        let lut = CurveLut::linear();
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            let chained = lut.lookup(lut.lookup(x));
            assert!(
                (chained - x).abs() < 0.02,
                "linear∘linear at {x}: {chained}"
            );
        }
        Ok(())
    }

    #[test]
    fn chaining_sqrt_then_square_approximates_identity() -> TestResult {
        let sqrt_lut = CurveLut::from_fn(|x| x.sqrt());
        let sq_lut = CurveLut::from_fn(|x| x * x);
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            let chained = sq_lut.lookup(sqrt_lut.lookup(x));
            assert!((chained - x).abs() < 0.05, "square(sqrt({x})): {chained}");
        }
        Ok(())
    }

    #[test]
    fn blending_two_luts_averages() -> TestResult {
        let linear = CurveLut::linear();
        let quadratic = CurveLut::from_fn(|x| x * x);
        let blended = CurveLut::from_fn(|x| {
            let a = linear.lookup(x);
            let b = quadratic.lookup(x);
            (a + b) / 2.0
        });
        // At x=0.5: linear=0.5, quadratic=0.25, blend=0.375
        let out = blended.lookup(0.5);
        assert!((out - 0.375).abs() < 0.03, "blended at 0.5: {out}");
        Ok(())
    }

    #[test]
    fn chaining_exponential_curves() -> TestResult {
        let exp2 = CurveType::exponential(2.0)?;
        let lut2 = exp2.to_lut();
        // x^2 chained with x^2 ≈ x^4
        let exp4 = CurveType::exponential(4.0)?;
        let lut4 = exp4.to_lut();
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            let chained = lut2.lookup(lut2.lookup(x));
            let direct = lut4.lookup(x);
            assert!(
                (chained - direct).abs() < 0.06,
                "x^2∘x^2 vs x^4 at {x}: chained={chained}, direct={direct}"
            );
        }
        Ok(())
    }

    #[test]
    fn composed_lut_preserves_endpoints() -> TestResult {
        let lut_a = CurveLut::from_fn(|x| x.sqrt());
        let lut_b = CurveLut::from_fn(|x| x * x);
        let composed = CurveLut::from_fn(|x| lut_b.lookup(lut_a.lookup(x)));
        assert!((composed.lookup(0.0) - 0.0).abs() < 0.02);
        assert!((composed.lookup(1.0) - 1.0).abs() < 0.02);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Numerical stability with extreme values
// ---------------------------------------------------------------------------

mod numerical_stability_tests {
    use super::*;

    #[test]
    fn subnormal_inputs_produce_finite_output() -> TestResult {
        let lut = CurveLut::linear();
        let subnormals: Vec<f32> = (0..10).map(|i| f32::from_bits(1u32 << i)).collect();
        for &v in &subnormals {
            let out = lut.lookup(v);
            assert!(out.is_finite(), "subnormal {v} produced non-finite: {out}");
        }
        Ok(())
    }

    #[test]
    fn f32_max_input_clamped_to_one() -> TestResult {
        let lut = CurveLut::linear();
        let out = lut.lookup(f32::MAX);
        assert!((out - 1.0).abs() < 0.01, "f32::MAX → 1.0: {out}");
        Ok(())
    }

    #[test]
    fn f32_min_input_clamped_to_zero() -> TestResult {
        let lut = CurveLut::linear();
        let out = lut.lookup(f32::MIN);
        assert!((out - 0.0).abs() < 0.01, "f32::MIN → 0.0: {out}");
        Ok(())
    }

    #[test]
    fn near_zero_exponent_stable() -> TestResult {
        let curve = CurveType::exponential(0.001)?;
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            let out = curve.evaluate(x);
            assert!(out.is_finite(), "exp(0.001) at {x}: {out}");
            assert!(
                (0.0..=1.0).contains(&out),
                "exp(0.001) at {x} out of range: {out}"
            );
        }
        Ok(())
    }

    #[test]
    fn large_exponent_stable() -> TestResult {
        let curve = CurveType::exponential(10.0)?;
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            let out = curve.evaluate(x);
            assert!(out.is_finite(), "exp(10) at {x}: {out}");
            assert!(
                (0.0..=1.0).contains(&out),
                "exp(10) at {x} out of range: {out}"
            );
        }
        Ok(())
    }

    #[test]
    fn log_base_near_one_stable() -> TestResult {
        let curve = CurveType::logarithmic(1.001)?;
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            let out = curve.evaluate(x);
            assert!(out.is_finite(), "log(1.001) at {x}: {out}");
            assert!(
                (0.0..=1.0).contains(&out),
                "log(1.001) at {x} out of range: {out}"
            );
        }
        Ok(())
    }

    #[test]
    fn large_log_base_stable() -> TestResult {
        let curve = CurveType::logarithmic(1000.0)?;
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            let out = curve.evaluate(x);
            assert!(out.is_finite(), "log(1000) at {x}: {out}");
            assert!(
                (0.0..=1.0).contains(&out),
                "log(1000) at {x} out of range: {out}"
            );
        }
        Ok(())
    }

    #[test]
    fn bezier_with_coincident_control_points() -> TestResult {
        let curve = BezierCurve::new([(0.0, 0.0), (0.5, 0.5), (0.5, 0.5), (1.0, 1.0)])?;
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            let out = curve.map(x);
            assert!(out.is_finite(), "coincident points at {x}: {out}");
            assert!((0.0..=1.0).contains(&out));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Performance of curve evaluation (< 1μs per lookup)
// ---------------------------------------------------------------------------

mod performance_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn lut_lookup_under_one_microsecond() -> TestResult {
        let lut = CurveLut::from_fn(|x| x * x);
        let iterations = 10_000;

        // Warm up
        for i in 0..100 {
            let _ = lut.lookup(i as f32 / 100.0);
        }

        let start = Instant::now();
        let mut sum = 0.0f32;
        for i in 0..iterations {
            let input = (i as f32 / iterations as f32) % 1.0;
            sum += lut.lookup(input);
        }
        let elapsed = start.elapsed();
        let per_lookup_ns = elapsed.as_nanos() as f64 / iterations as f64;

        // Prevent optimizer from removing the loop
        assert!(sum.is_finite());
        assert!(
            per_lookup_ns < 1000.0,
            "LUT lookup too slow: {per_lookup_ns:.1}ns per lookup (limit: 1000ns)"
        );
        Ok(())
    }

    #[test]
    fn linear_evaluate_under_one_microsecond() -> TestResult {
        let curve = CurveType::Linear;
        let iterations = 10_000;

        let start = Instant::now();
        let mut sum = 0.0f32;
        for i in 0..iterations {
            sum += curve.evaluate(i as f32 / iterations as f32);
        }
        let elapsed = start.elapsed();
        let per_eval_ns = elapsed.as_nanos() as f64 / iterations as f64;

        assert!(sum.is_finite());
        assert!(
            per_eval_ns < 1000.0,
            "Linear evaluate too slow: {per_eval_ns:.1}ns (limit: 1000ns)"
        );
        Ok(())
    }
}
