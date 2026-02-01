//! Property-Based Tests for Curve-Based FFB Effects
//!
//! This module contains property-based tests for the Bezier curve and LUT
//! implementations used in the FFB response mapping system.
//!
//! **Validates: Requirements 10.1, 10.2, 10.3**

use proptest::prelude::*;

use crate::curves::{BezierCurve, CurveLut, CurveType};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation
    //
    // *For any* valid Bezier curve with control points in [0,1]², and any input
    // value in [0,1], the curve lookup SHALL return a value in [0,1] without
    // heap allocation.
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_bezier_curve_output_in_valid_range(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        input in 0.0f32..=1.0,
    ) {
        // Create a valid Bezier curve with control points in [0,1]²
        // P0 = (0,0) and P3 = (1,1) are standard for response curves
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        // Curve creation should succeed for valid control points
        prop_assert!(curve_result.is_ok(), "Failed to create curve with valid control points");

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        // Direct curve mapping should produce output in [0,1]
        let direct_output = curve.map(input);
        prop_assert!(
            (0.0..=1.0).contains(&direct_output),
            "Direct curve output {} out of range [0,1] for input {}",
            direct_output,
            input
        );
    }

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation (LUT variant)
    //
    // Tests that the pre-computed LUT also produces values in [0,1] for any
    // valid input, ensuring RT-safe evaluation maintains the output range.
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_curve_lut_output_in_valid_range(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        input in 0.0f32..=1.0,
    ) {
        // Create a valid Bezier curve
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        // Build LUT from the curve (this is done at profile load time, not RT)
        let lut = CurveLut::from_bezier(&curve);

        // LUT lookup should produce output in [0,1]
        let lut_output = lut.lookup(input);
        prop_assert!(
            (0.0..=1.0).contains(&lut_output),
            "LUT output {} out of range [0,1] for input {}",
            lut_output,
            input
        );
    }

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation (endpoint consistency)
    //
    // Tests that curves with standard endpoints (P0=(0,0), P3=(1,1)) map
    // input 0 to approximately 0 and input 1 to approximately 1.
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_bezier_curve_endpoint_consistency(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
    ) {
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        // Input 0 should map to approximately 0
        let output_at_zero = curve.map(0.0);
        prop_assert!(
            (output_at_zero - 0.0).abs() < 0.01,
            "Curve output at input 0.0 should be ~0.0, got {}",
            output_at_zero
        );

        // Input 1 should map to approximately 1
        let output_at_one = curve.map(1.0);
        prop_assert!(
            (output_at_one - 1.0).abs() < 0.01,
            "Curve output at input 1.0 should be ~1.0, got {}",
            output_at_one
        );
    }

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation (LUT endpoint consistency)
    //
    // Tests that LUT lookups at endpoints produce correct values.
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_curve_lut_endpoint_consistency(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
    ) {
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        let lut = CurveLut::from_bezier(&curve);

        // LUT at input 0 should be approximately 0
        let lut_at_zero = lut.lookup(0.0);
        prop_assert!(
            (lut_at_zero - 0.0).abs() < 0.02,
            "LUT output at input 0.0 should be ~0.0, got {}",
            lut_at_zero
        );

        // LUT at input 1 should be approximately 1
        let lut_at_one = lut.lookup(1.0);
        prop_assert!(
            (lut_at_one - 1.0).abs() < 0.02,
            "LUT output at input 1.0 should be ~1.0, got {}",
            lut_at_one
        );
    }

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation (LUT approximates curve)
    //
    // Tests that the LUT lookup produces values close to the direct curve
    // evaluation, ensuring the pre-computation is accurate.
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_curve_lut_approximates_direct_evaluation(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        input in 0.0f32..=1.0,
    ) {
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        let lut = CurveLut::from_bezier(&curve);

        let direct_output = curve.map(input);
        let lut_output = lut.lookup(input);

        // LUT should approximate direct evaluation within reasonable tolerance
        // (256-entry LUT with linear interpolation should be within ~1% for smooth curves)
        let tolerance = 0.05; // 5% tolerance for edge cases with steep curves
        prop_assert!(
            (direct_output - lut_output).abs() < tolerance,
            "LUT output {} differs from direct output {} by more than {} for input {}",
            lut_output,
            direct_output,
            tolerance,
            input
        );
    }

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation (input clamping)
    //
    // Tests that inputs outside [0,1] are properly clamped and still produce
    // valid outputs in [0,1].
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_curve_lut_clamps_out_of_range_inputs(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        // Generate inputs outside the valid range
        input in prop::strategy::Union::new(vec![
            (-10.0f32..0.0).boxed(),
            (1.0f32..10.0).boxed(),
        ]),
    ) {
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        let lut = CurveLut::from_bezier(&curve);

        // Out-of-range inputs should still produce valid outputs
        let lut_output = lut.lookup(input);
        prop_assert!(
            (0.0..=1.0).contains(&lut_output),
            "LUT output {} out of range [0,1] for out-of-range input {}",
            lut_output,
            input
        );

        // Direct curve mapping should also handle out-of-range inputs
        let direct_output = curve.map(input);
        prop_assert!(
            (0.0..=1.0).contains(&direct_output),
            "Direct curve output {} out of range [0,1] for out-of-range input {}",
            direct_output,
            input
        );
    }

    // Feature: release-roadmap-v1, Property 15: Bezier Curve Interpolation (determinism)
    //
    // Tests that the same curve and input always produce the same output,
    // which is essential for RT determinism.
    //
    // **Validates: Requirements 10.1, 10.2**
    #[test]
    fn prop_curve_lut_is_deterministic(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        input in 0.0f32..=1.0,
    ) {
        let curve_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);

        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!("Curve creation failed: {:?}", e))),
        };

        // Create two LUTs from the same curve
        let lut1 = CurveLut::from_bezier(&curve);
        let lut2 = CurveLut::from_bezier(&curve);

        // Both should produce identical outputs
        let output1 = lut1.lookup(input);
        let output2 = lut2.lookup(input);

        prop_assert_eq!(
            output1,
            output2,
            "LUT outputs differ for same curve and input: {} vs {}",
            output1,
            output2
        );
    }

    // ============================================================
    // Property 16: Curve Type Consistency
    // ============================================================

    // Feature: release-roadmap-v1, Property 16: Curve Type Consistency
    //
    // *For any* curve type (linear, exponential, logarithmic, Bezier), applying
    // the curve to input 0.0 SHALL return approximately 0.0, and input 1.0 SHALL
    // return approximately 1.0 (within floating-point tolerance).
    //
    // **Validates: Requirements 10.3**
    #[test]
    fn prop_curve_type_linear_endpoint_consistency(
        // Linear has no parameters, but we test multiple times for consistency
        _dummy in 0u8..10,
    ) {
        let curve = CurveType::Linear;
        let tolerance = 0.01;

        // Input 0.0 should return approximately 0.0
        let output_at_zero = curve.evaluate(0.0);
        prop_assert!(
            (output_at_zero - 0.0).abs() < tolerance,
            "Linear curve at input 0.0 returned {} (expected ~0.0)",
            output_at_zero
        );

        // Input 1.0 should return approximately 1.0
        let output_at_one = curve.evaluate(1.0);
        prop_assert!(
            (output_at_one - 1.0).abs() < tolerance,
            "Linear curve at input 1.0 returned {} (expected ~1.0)",
            output_at_one
        );

        // Also verify via LUT
        let lut = curve.to_lut();
        let lut_at_zero = lut.lookup(0.0);
        let lut_at_one = lut.lookup(1.0);

        prop_assert!(
            (lut_at_zero - 0.0).abs() < tolerance,
            "Linear LUT at input 0.0 returned {} (expected ~0.0)",
            lut_at_zero
        );
        prop_assert!(
            (lut_at_one - 1.0).abs() < tolerance,
            "Linear LUT at input 1.0 returned {} (expected ~1.0)",
            lut_at_one
        );
    }

    // Feature: release-roadmap-v1, Property 16: Curve Type Consistency (Exponential)
    //
    // Tests that exponential curves with valid exponents (> 0) map endpoints correctly.
    //
    // **Validates: Requirements 10.3**
    #[test]
    fn prop_curve_type_exponential_endpoint_consistency(
        // Generate valid exponents: must be > 0
        // Using a range that covers typical use cases: 0.1 to 10.0
        exponent in 0.1f32..10.0,
    ) {
        let curve_result = CurveType::exponential(exponent);
        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create exponential curve with exponent {}: {:?}",
                exponent, e
            ))),
        };

        let tolerance = 0.01;

        // Input 0.0 should return approximately 0.0 (0^exponent = 0 for exponent > 0)
        let output_at_zero = curve.evaluate(0.0);
        prop_assert!(
            (output_at_zero - 0.0).abs() < tolerance,
            "Exponential curve (exponent={}) at input 0.0 returned {} (expected ~0.0)",
            exponent,
            output_at_zero
        );

        // Input 1.0 should return approximately 1.0 (1^exponent = 1)
        let output_at_one = curve.evaluate(1.0);
        prop_assert!(
            (output_at_one - 1.0).abs() < tolerance,
            "Exponential curve (exponent={}) at input 1.0 returned {} (expected ~1.0)",
            exponent,
            output_at_one
        );

        // Also verify via LUT
        let lut = curve.to_lut();
        let lut_at_zero = lut.lookup(0.0);
        let lut_at_one = lut.lookup(1.0);

        prop_assert!(
            (lut_at_zero - 0.0).abs() < tolerance,
            "Exponential LUT (exponent={}) at input 0.0 returned {} (expected ~0.0)",
            exponent,
            lut_at_zero
        );
        prop_assert!(
            (lut_at_one - 1.0).abs() < tolerance,
            "Exponential LUT (exponent={}) at input 1.0 returned {} (expected ~1.0)",
            exponent,
            lut_at_one
        );
    }

    // Feature: release-roadmap-v1, Property 16: Curve Type Consistency (Logarithmic)
    //
    // Tests that logarithmic curves with valid bases (> 1) map endpoints correctly.
    //
    // **Validates: Requirements 10.3**
    #[test]
    fn prop_curve_type_logarithmic_endpoint_consistency(
        // Generate valid bases: must be > 1
        // Using a range that covers typical use cases: 1.1 to 100.0
        base in 1.1f32..100.0,
    ) {
        let curve_result = CurveType::logarithmic(base);
        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create logarithmic curve with base {}: {:?}",
                base, e
            ))),
        };

        let tolerance = 0.01;

        // Input 0.0 should return approximately 0.0
        // log_base(1 + 0*(base-1)) / log_base(base) = log_base(1) / 1 = 0
        let output_at_zero = curve.evaluate(0.0);
        prop_assert!(
            (output_at_zero - 0.0).abs() < tolerance,
            "Logarithmic curve (base={}) at input 0.0 returned {} (expected ~0.0)",
            base,
            output_at_zero
        );

        // Input 1.0 should return approximately 1.0
        // log_base(1 + 1*(base-1)) / log_base(base) = log_base(base) / 1 = 1
        let output_at_one = curve.evaluate(1.0);
        prop_assert!(
            (output_at_one - 1.0).abs() < tolerance,
            "Logarithmic curve (base={}) at input 1.0 returned {} (expected ~1.0)",
            base,
            output_at_one
        );

        // Also verify via LUT
        let lut = curve.to_lut();
        let lut_at_zero = lut.lookup(0.0);
        let lut_at_one = lut.lookup(1.0);

        prop_assert!(
            (lut_at_zero - 0.0).abs() < tolerance,
            "Logarithmic LUT (base={}) at input 0.0 returned {} (expected ~0.0)",
            base,
            lut_at_zero
        );
        prop_assert!(
            (lut_at_one - 1.0).abs() < tolerance,
            "Logarithmic LUT (base={}) at input 1.0 returned {} (expected ~1.0)",
            base,
            lut_at_one
        );
    }

    // Feature: release-roadmap-v1, Property 16: Curve Type Consistency (Bezier)
    //
    // Tests that Bezier curves with standard endpoints (P0=(0,0), P3=(1,1))
    // and valid control points map endpoints correctly.
    //
    // **Validates: Requirements 10.3**
    #[test]
    fn prop_curve_type_bezier_endpoint_consistency(
        // Generate valid control points in [0,1]²
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
    ) {
        let bezier_result = BezierCurve::new([
            (0.0, 0.0),  // P0 - standard start point
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),  // P3 - standard end point
        ]);

        let bezier = match bezier_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create Bezier curve: {:?}",
                e
            ))),
        };

        let curve = CurveType::Bezier(bezier);
        let tolerance = 0.01;

        // Input 0.0 should return approximately 0.0
        let output_at_zero = curve.evaluate(0.0);
        prop_assert!(
            (output_at_zero - 0.0).abs() < tolerance,
            "Bezier curve at input 0.0 returned {} (expected ~0.0)",
            output_at_zero
        );

        // Input 1.0 should return approximately 1.0
        let output_at_one = curve.evaluate(1.0);
        prop_assert!(
            (output_at_one - 1.0).abs() < tolerance,
            "Bezier curve at input 1.0 returned {} (expected ~1.0)",
            output_at_one
        );

        // Also verify via LUT
        let lut = curve.to_lut();
        // LUT tolerance is slightly higher due to discretization
        let lut_tolerance = 0.02;
        let lut_at_zero = lut.lookup(0.0);
        let lut_at_one = lut.lookup(1.0);

        prop_assert!(
            (lut_at_zero - 0.0).abs() < lut_tolerance,
            "Bezier LUT at input 0.0 returned {} (expected ~0.0)",
            lut_at_zero
        );
        prop_assert!(
            (lut_at_one - 1.0).abs() < lut_tolerance,
            "Bezier LUT at input 1.0 returned {} (expected ~1.0)",
            lut_at_one
        );
    }

    // Feature: release-roadmap-v1, Property 16: Curve Type Consistency (All Types Combined)
    //
    // Tests all curve types together to ensure consistent endpoint behavior
    // across the entire CurveType enum.
    //
    // **Validates: Requirements 10.3**
    #[test]
    fn prop_curve_type_all_types_endpoint_consistency(
        // Parameters for generating different curve types
        exponent in 0.1f32..10.0,
        base in 1.1f32..100.0,
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        // Select which curve type to test (0-3)
        curve_type_selector in 0u8..4,
    ) {
        let curve: CurveType = match curve_type_selector {
            0 => CurveType::Linear,
            1 => {
                match CurveType::exponential(exponent) {
                    Ok(c) => c,
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create exponential curve: {:?}", e
                    ))),
                }
            }
            2 => {
                match CurveType::logarithmic(base) {
                    Ok(c) => c,
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create logarithmic curve: {:?}", e
                    ))),
                }
            }
            _ => {
                let bezier_result = BezierCurve::new([
                    (0.0, 0.0),
                    (p1_x, p1_y),
                    (p2_x, p2_y),
                    (1.0, 1.0),
                ]);
                match bezier_result {
                    Ok(b) => CurveType::Bezier(b),
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create Bezier curve: {:?}", e
                    ))),
                }
            }
        };

        let tolerance = 0.01;

        // Input 0.0 should return approximately 0.0
        let output_at_zero = curve.evaluate(0.0);
        prop_assert!(
            (output_at_zero - 0.0).abs() < tolerance,
            "Curve type {:?} at input 0.0 returned {} (expected ~0.0)",
            curve,
            output_at_zero
        );

        // Input 1.0 should return approximately 1.0
        let output_at_one = curve.evaluate(1.0);
        prop_assert!(
            (output_at_one - 1.0).abs() < tolerance,
            "Curve type {:?} at input 1.0 returned {} (expected ~1.0)",
            curve,
            output_at_one
        );
    }

    // ============================================================
    // Property 17: Curve Application to Torque
    // ============================================================

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque
    //
    // *For any* profile with a response curve and any torque output, the final
    // torque SHALL equal the curve-transformed value of the raw torque.
    //
    // This property tests that when a pipeline has a response curve set,
    // the pipeline correctly applies the curve transformation to the torque output.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_to_torque_linear(
        // Generate random torque values in [-1.0, 1.0]
        raw_torque in -1.0f32..=1.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create a pipeline with a linear response curve
        let mut pipeline = Pipeline::new();
        let curve = CurveType::Linear;
        pipeline.set_response_curve_from_type(&curve);

        // Create a frame with the raw torque as output
        // (simulating the state after filter nodes have processed)
        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        // Process through pipeline (which applies the response curve)
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // For linear curve, output should equal input (preserving sign)
        let expected = raw_torque; // Linear: f(x) = x
        let tolerance = 0.01;

        prop_assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Linear curve: expected torque {} but got {} for raw torque {}",
            expected,
            frame.torque_out,
            raw_torque
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (Exponential)
    //
    // Tests that exponential curves are correctly applied to torque output.
    // The curve is applied to the absolute value, then the sign is restored.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_to_torque_exponential(
        // Generate random torque values in [-1.0, 1.0]
        raw_torque in -1.0f32..=1.0,
        // Generate valid exponents
        exponent in 0.5f32..3.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create exponential curve
        let curve_result = CurveType::exponential(exponent);
        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create exponential curve: {:?}", e
            ))),
        };

        // Create a pipeline with the exponential response curve
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve_from_type(&curve);

        // Create a frame with the raw torque as output
        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        // Process through pipeline
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Calculate expected output:
        // The pipeline applies curve to abs(torque), then restores sign
        let abs_input = raw_torque.abs().clamp(0.0, 1.0);
        let curve_output = curve.evaluate(abs_input);
        let expected = raw_torque.signum() * curve_output;

        let tolerance = 0.02; // Slightly higher tolerance for LUT approximation

        prop_assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Exponential curve (exp={}): expected torque {} but got {} for raw torque {}",
            exponent,
            expected,
            frame.torque_out,
            raw_torque
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (Logarithmic)
    //
    // Tests that logarithmic curves are correctly applied to torque output.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_to_torque_logarithmic(
        // Generate random torque values in [-1.0, 1.0]
        raw_torque in -1.0f32..=1.0,
        // Generate valid bases
        base in 2.0f32..50.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create logarithmic curve
        let curve_result = CurveType::logarithmic(base);
        let curve = match curve_result {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create logarithmic curve: {:?}", e
            ))),
        };

        // Create a pipeline with the logarithmic response curve
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve_from_type(&curve);

        // Create a frame with the raw torque as output
        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        // Process through pipeline
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Calculate expected output
        let abs_input = raw_torque.abs().clamp(0.0, 1.0);
        let curve_output = curve.evaluate(abs_input);
        let expected = raw_torque.signum() * curve_output;

        let tolerance = 0.02;

        prop_assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Logarithmic curve (base={}): expected torque {} but got {} for raw torque {}",
            base,
            expected,
            frame.torque_out,
            raw_torque
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (Bezier)
    //
    // Tests that Bezier curves are correctly applied to torque output.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_to_torque_bezier(
        // Generate random torque values in [-1.0, 1.0]
        raw_torque in -1.0f32..=1.0,
        // Generate valid control points
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create Bezier curve
        let bezier_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);
        let bezier = match bezier_result {
            Ok(b) => b,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create Bezier curve: {:?}", e
            ))),
        };
        let curve = CurveType::Bezier(bezier.clone());

        // Create a pipeline with the Bezier response curve
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve_from_type(&curve);

        // Create a frame with the raw torque as output
        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        // Process through pipeline
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Calculate expected output using the LUT (same as pipeline uses)
        let lut = curve.to_lut();
        let abs_input = raw_torque.abs().clamp(0.0, 1.0);
        let curve_output = lut.lookup(abs_input);
        let expected = raw_torque.signum() * curve_output;

        let tolerance = 0.02;

        prop_assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Bezier curve: expected torque {} but got {} for raw torque {}",
            expected,
            frame.torque_out,
            raw_torque
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (All Types)
    //
    // Comprehensive test that verifies curve application for all curve types
    // with random parameters.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_to_torque_all_types(
        // Generate random torque values in [-1.0, 1.0]
        raw_torque in -1.0f32..=1.0,
        // Parameters for different curve types
        exponent in 0.5f32..3.0,
        base in 2.0f32..50.0,
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        // Select curve type (0-3)
        curve_type_selector in 0u8..4,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create the selected curve type
        let curve: CurveType = match curve_type_selector {
            0 => CurveType::Linear,
            1 => {
                match CurveType::exponential(exponent) {
                    Ok(c) => c,
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create exponential curve: {:?}", e
                    ))),
                }
            }
            2 => {
                match CurveType::logarithmic(base) {
                    Ok(c) => c,
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create logarithmic curve: {:?}", e
                    ))),
                }
            }
            _ => {
                let bezier_result = BezierCurve::new([
                    (0.0, 0.0),
                    (p1_x, p1_y),
                    (p2_x, p2_y),
                    (1.0, 1.0),
                ]);
                match bezier_result {
                    Ok(b) => CurveType::Bezier(b),
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create Bezier curve: {:?}", e
                    ))),
                }
            }
        };

        // Create a pipeline with the response curve
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve_from_type(&curve);

        // Create a frame with the raw torque as output
        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        // Process through pipeline
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Calculate expected output using the LUT
        let lut = curve.to_lut();
        let abs_input = raw_torque.abs().clamp(0.0, 1.0);
        let curve_output = lut.lookup(abs_input);
        let expected = raw_torque.signum() * curve_output;

        let tolerance = 0.02;

        prop_assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Curve type {:?}: expected torque {} but got {} for raw torque {}",
            curve,
            expected,
            frame.torque_out,
            raw_torque
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (Sign Preservation)
    //
    // Tests that the sign of the torque is preserved after curve application.
    // Positive torque stays positive, negative stays negative.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_preserves_sign(
        // Generate non-zero torque values
        raw_torque in prop::strategy::Union::new(vec![
            (-1.0f32..-0.01).boxed(),
            (0.01f32..1.0).boxed(),
        ]),
        exponent in 0.5f32..3.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create exponential curve (non-linear to make the test meaningful)
        let curve = match CurveType::exponential(exponent) {
            Ok(c) => c,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create exponential curve: {:?}", e
            ))),
        };

        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve_from_type(&curve);

        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Sign should be preserved
        let input_sign = raw_torque.signum();
        let output_sign = frame.torque_out.signum();

        prop_assert_eq!(
            input_sign,
            output_sign,
            "Sign not preserved: input {} (sign {}) -> output {} (sign {})",
            raw_torque,
            input_sign,
            frame.torque_out,
            output_sign
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (Output Range)
    //
    // Tests that the output torque is always within [-1.0, 1.0] after curve application.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_curve_application_output_in_valid_range(
        raw_torque in -1.0f32..=1.0,
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create a Bezier curve (can have various shapes)
        let bezier_result = BezierCurve::new([
            (0.0, 0.0),
            (p1_x, p1_y),
            (p2_x, p2_y),
            (1.0, 1.0),
        ]);
        let bezier = match bezier_result {
            Ok(b) => b,
            Err(e) => return Err(TestCaseError::fail(format!(
                "Failed to create Bezier curve: {:?}", e
            ))),
        };
        let curve = CurveType::Bezier(bezier);

        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve_from_type(&curve);

        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Output should be in valid range
        prop_assert!(
            frame.torque_out >= -1.0 && frame.torque_out <= 1.0,
            "Output torque {} out of valid range [-1.0, 1.0] for input {}",
            frame.torque_out,
            raw_torque
        );
    }

    // Feature: release-roadmap-v1, Property 17: Curve Application to Torque (No Curve)
    //
    // Tests that when no response curve is set, the torque passes through unchanged.
    //
    // **Validates: Requirements 10.4**
    #[test]
    fn prop_no_curve_passes_through_unchanged(
        raw_torque in -1.0f32..=1.0,
    ) {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create a pipeline WITHOUT a response curve
        let mut pipeline = Pipeline::new();
        // Note: No set_response_curve_from_type call

        let mut frame = Frame {
            ffb_in: raw_torque,
            torque_out: raw_torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);

        // Without a curve, torque should pass through unchanged
        prop_assert_eq!(
            frame.torque_out,
            raw_torque,
            "Torque changed without response curve: {} -> {}",
            raw_torque,
            frame.torque_out
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the property tests module compiles and basic functionality works.
    /// This is a sanity check that doesn't use proptest.
    #[test]
    fn test_property_test_module_compiles() -> Result<(), Box<dyn std::error::Error>> {
        // Create a simple curve
        let curve = BezierCurve::new([(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)])?;
        let lut = CurveLut::from_bezier(&curve);

        // Basic sanity checks
        let output = lut.lookup(0.5);
        assert!((0.0..=1.0).contains(&output));

        Ok(())
    }

    /// Test that curve application to torque works correctly for a simple case.
    /// This is a sanity check for Property 17.
    #[test]
    fn test_curve_application_to_torque_basic() -> Result<(), Box<dyn std::error::Error>> {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        // Create a pipeline with an exponential curve (exponent=2)
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve_from_type(&curve);

        // Test with torque = 0.5
        let mut frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        pipeline
            .process(&mut frame)
            .map_err(|e| format!("Pipeline processing failed: {:?}", e))?;

        // Expected: 0.5^2 = 0.25
        let expected = 0.25;
        let tolerance = 0.02;
        assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Expected {} but got {}",
            expected,
            frame.torque_out
        );

        Ok(())
    }

    /// Test that negative torque is handled correctly (sign preserved).
    #[test]
    fn test_curve_application_negative_torque() -> Result<(), Box<dyn std::error::Error>> {
        use crate::pipeline::Pipeline;
        use crate::rt::Frame;

        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve_from_type(&curve);

        // Test with negative torque = -0.5
        let mut frame = Frame {
            ffb_in: -0.5,
            torque_out: -0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };

        pipeline
            .process(&mut frame)
            .map_err(|e| format!("Pipeline processing failed: {:?}", e))?;

        // Expected: -1 * (0.5^2) = -0.25
        let expected = -0.25;
        let tolerance = 0.02;
        assert!(
            (frame.torque_out - expected).abs() < tolerance,
            "Expected {} but got {}",
            expected,
            frame.torque_out
        );

        Ok(())
    }
}

// ============================================================
// Property 18: Curve Parameter Validation
// ============================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: release-roadmap-v1, Property 18: Curve Parameter Validation
    //
    // *For any* curve parameters outside valid ranges (e.g., negative exponents,
    // control points outside [0,1]), the curve system SHALL reject the configuration
    // with a validation error.
    //
    // **Validates: Requirements 10.5**

    // ============================================================
    // Exponential Curve Parameter Validation
    // ============================================================

    // Test that negative exponents are rejected
    #[test]
    fn prop_exponential_rejects_negative_exponents(
        // Generate negative exponents
        exponent in -100.0f32..-0.001,
    ) {
        // Constructor should reject negative exponents
        let constructor_result = CurveType::exponential(exponent);
        prop_assert!(
            constructor_result.is_err(),
            "CurveType::exponential() should reject negative exponent {}, but it succeeded",
            exponent
        );

        // validate() should also reject if we bypass constructor
        let curve = CurveType::Exponential { exponent };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject negative exponent {}, but it succeeded",
            exponent
        );
    }

    // Test that zero exponent is rejected
    #[test]
    fn prop_exponential_rejects_zero_exponent(
        // Generate values very close to zero (both positive and negative tiny values)
        exponent in prop::strategy::Union::new(vec![
            (-0.0001f32..0.0001).boxed(),
        ]),
    ) {
        // Zero and near-zero values should be rejected
        // Note: The implementation rejects exponent <= 0.0
        if exponent <= 0.0 {
            let constructor_result = CurveType::exponential(exponent);
            prop_assert!(
                constructor_result.is_err(),
                "CurveType::exponential() should reject exponent {}, but it succeeded",
                exponent
            );

            let curve = CurveType::Exponential { exponent };
            let validate_result = curve.validate();
            prop_assert!(
                validate_result.is_err(),
                "CurveType::validate() should reject exponent {}, but it succeeded",
                exponent
            );
        }
    }

    // Test that valid positive exponents are accepted
    #[test]
    fn prop_exponential_accepts_valid_exponents(
        // Generate valid positive exponents
        exponent in 0.001f32..100.0,
    ) {
        // Constructor should accept valid exponents
        let constructor_result = CurveType::exponential(exponent);
        prop_assert!(
            constructor_result.is_ok(),
            "CurveType::exponential() should accept valid exponent {}, but it failed: {:?}",
            exponent,
            constructor_result.err()
        );

        // validate() should also accept
        let curve = CurveType::Exponential { exponent };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_ok(),
            "CurveType::validate() should accept valid exponent {}, but it failed: {:?}",
            exponent,
            validate_result.err()
        );
    }

    // ============================================================
    // Logarithmic Curve Parameter Validation
    // ============================================================

    // Test that bases <= 1 are rejected
    #[test]
    fn prop_logarithmic_rejects_invalid_bases(
        // Generate invalid bases: <= 1.0
        base in -100.0f32..=1.0,
    ) {
        // Constructor should reject invalid bases
        let constructor_result = CurveType::logarithmic(base);
        prop_assert!(
            constructor_result.is_err(),
            "CurveType::logarithmic() should reject base {}, but it succeeded",
            base
        );

        // validate() should also reject if we bypass constructor
        let curve = CurveType::Logarithmic { base };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject base {}, but it succeeded",
            base
        );
    }

    // Test that valid bases (> 1) are accepted
    #[test]
    fn prop_logarithmic_accepts_valid_bases(
        // Generate valid bases: > 1.0
        base in 1.001f32..1000.0,
    ) {
        // Constructor should accept valid bases
        let constructor_result = CurveType::logarithmic(base);
        prop_assert!(
            constructor_result.is_ok(),
            "CurveType::logarithmic() should accept valid base {}, but it failed: {:?}",
            base,
            constructor_result.err()
        );

        // validate() should also accept
        let curve = CurveType::Logarithmic { base };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_ok(),
            "CurveType::validate() should accept valid base {}, but it failed: {:?}",
            base,
            validate_result.err()
        );
    }

    // ============================================================
    // Bezier Curve Control Point Validation
    // ============================================================

    // Test that control points with x > 1 are rejected
    #[test]
    fn prop_bezier_rejects_x_greater_than_one(
        // Generate x values > 1.0
        invalid_x in 1.001f32..100.0,
        // Valid y value
        y in 0.0f32..=1.0,
        // Which control point to make invalid (1 or 2, since 0 and 3 are typically fixed)
        point_index in 1usize..=2,
    ) {
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (invalid_x, y);

        // Constructor should reject
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject control point {} with x={}, but it succeeded",
            point_index,
            invalid_x
        );

        // validate() should also reject if we bypass constructor
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject control point {} with x={}, but it succeeded",
            point_index,
            invalid_x
        );
    }

    // Test that control points with x < 0 are rejected
    #[test]
    fn prop_bezier_rejects_x_less_than_zero(
        // Generate x values < 0.0
        invalid_x in -100.0f32..-0.001,
        // Valid y value
        y in 0.0f32..=1.0,
        // Which control point to make invalid
        point_index in 1usize..=2,
    ) {
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (invalid_x, y);

        // Constructor should reject
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject control point {} with x={}, but it succeeded",
            point_index,
            invalid_x
        );

        // validate() should also reject if we bypass constructor
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject control point {} with x={}, but it succeeded",
            point_index,
            invalid_x
        );
    }

    // Test that control points with y > 1 are rejected
    #[test]
    fn prop_bezier_rejects_y_greater_than_one(
        // Valid x value
        x in 0.0f32..=1.0,
        // Generate y values > 1.0
        invalid_y in 1.001f32..100.0,
        // Which control point to make invalid
        point_index in 1usize..=2,
    ) {
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (x, invalid_y);

        // Constructor should reject
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject control point {} with y={}, but it succeeded",
            point_index,
            invalid_y
        );

        // validate() should also reject if we bypass constructor
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject control point {} with y={}, but it succeeded",
            point_index,
            invalid_y
        );
    }

    // Test that control points with y < 0 are rejected
    #[test]
    fn prop_bezier_rejects_y_less_than_zero(
        // Valid x value
        x in 0.0f32..=1.0,
        // Generate y values < 0.0
        invalid_y in -100.0f32..-0.001,
        // Which control point to make invalid
        point_index in 1usize..=2,
    ) {
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (x, invalid_y);

        // Constructor should reject
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject control point {} with y={}, but it succeeded",
            point_index,
            invalid_y
        );

        // validate() should also reject if we bypass constructor
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject control point {} with y={}, but it succeeded",
            point_index,
            invalid_y
        );
    }

    // Test that valid control points in [0,1]² are accepted
    #[test]
    fn prop_bezier_accepts_valid_control_points(
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
    ) {
        let control_points = [(0.0, 0.0), (p1_x, p1_y), (p2_x, p2_y), (1.0, 1.0)];

        // Constructor should accept valid control points
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_ok(),
            "BezierCurve::new() should accept valid control points {:?}, but it failed: {:?}",
            control_points,
            constructor_result.err()
        );

        // validate() should also accept
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_ok(),
            "CurveType::validate() should accept valid control points {:?}, but it failed: {:?}",
            control_points,
            validate_result.err()
        );
    }

    // ============================================================
    // NaN and Infinity Value Validation
    // ============================================================

    // Test that NaN exponents are rejected for exponential curves
    #[test]
    fn prop_exponential_rejects_nan(
        // Use a dummy parameter to run multiple times
        _dummy in 0u8..10,
    ) {
        let exponent = f32::NAN;

        // Constructor should reject NaN
        let constructor_result = CurveType::exponential(exponent);
        prop_assert!(
            constructor_result.is_err(),
            "CurveType::exponential() should reject NaN exponent, but it succeeded"
        );

        // validate() should also reject
        let curve = CurveType::Exponential { exponent };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject NaN exponent, but it succeeded"
        );
    }

    // Test that Infinity exponents are rejected for exponential curves
    #[test]
    fn prop_exponential_rejects_infinity(
        // Test both positive and negative infinity
        is_negative in proptest::bool::ANY,
    ) {
        let exponent = if is_negative { f32::NEG_INFINITY } else { f32::INFINITY };

        // Constructor should reject Infinity
        let constructor_result = CurveType::exponential(exponent);
        prop_assert!(
            constructor_result.is_err(),
            "CurveType::exponential() should reject {} exponent, but it succeeded",
            exponent
        );

        // validate() should also reject
        let curve = CurveType::Exponential { exponent };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject {} exponent, but it succeeded",
            exponent
        );
    }

    // Test that NaN bases are rejected for logarithmic curves
    #[test]
    fn prop_logarithmic_rejects_nan(
        // Use a dummy parameter to run multiple times
        _dummy in 0u8..10,
    ) {
        let base = f32::NAN;

        // Constructor should reject NaN
        let constructor_result = CurveType::logarithmic(base);
        prop_assert!(
            constructor_result.is_err(),
            "CurveType::logarithmic() should reject NaN base, but it succeeded"
        );

        // validate() should also reject
        let curve = CurveType::Logarithmic { base };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject NaN base, but it succeeded"
        );
    }

    // Test that Infinity bases are rejected for logarithmic curves
    #[test]
    fn prop_logarithmic_rejects_infinity(
        // Test both positive and negative infinity
        is_negative in proptest::bool::ANY,
    ) {
        let base = if is_negative { f32::NEG_INFINITY } else { f32::INFINITY };

        // Constructor should reject Infinity
        let constructor_result = CurveType::logarithmic(base);
        prop_assert!(
            constructor_result.is_err(),
            "CurveType::logarithmic() should reject {} base, but it succeeded",
            base
        );

        // validate() should also reject
        let curve = CurveType::Logarithmic { base };
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject {} base, but it succeeded",
            base
        );
    }

    // Test that NaN control points are rejected for Bezier curves
    #[test]
    fn prop_bezier_rejects_nan_x(
        // Valid y value
        y in 0.0f32..=1.0,
        // Which control point to make invalid
        point_index in 0usize..=3,
    ) {
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (f32::NAN, y);

        // Constructor should reject NaN
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject NaN x at control point {}, but it succeeded",
            point_index
        );

        // validate() should also reject
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject NaN x at control point {}, but it succeeded",
            point_index
        );
    }

    // Test that NaN y control points are rejected for Bezier curves
    #[test]
    fn prop_bezier_rejects_nan_y(
        // Valid x value
        x in 0.0f32..=1.0,
        // Which control point to make invalid
        point_index in 0usize..=3,
    ) {
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (x, f32::NAN);

        // Constructor should reject NaN
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject NaN y at control point {}, but it succeeded",
            point_index
        );

        // validate() should also reject
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject NaN y at control point {}, but it succeeded",
            point_index
        );
    }

    // Test that Infinity control points are rejected for Bezier curves
    #[test]
    fn prop_bezier_rejects_infinity_x(
        // Valid y value
        y in 0.0f32..=1.0,
        // Which control point to make invalid
        point_index in 0usize..=3,
        // Test both positive and negative infinity
        is_negative in proptest::bool::ANY,
    ) {
        let invalid_x = if is_negative { f32::NEG_INFINITY } else { f32::INFINITY };
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (invalid_x, y);

        // Constructor should reject Infinity
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject {} x at control point {}, but it succeeded",
            invalid_x,
            point_index
        );

        // validate() should also reject
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject {} x at control point {}, but it succeeded",
            invalid_x,
            point_index
        );
    }

    // Test that Infinity y control points are rejected for Bezier curves
    #[test]
    fn prop_bezier_rejects_infinity_y(
        // Valid x value
        x in 0.0f32..=1.0,
        // Which control point to make invalid
        point_index in 0usize..=3,
        // Test both positive and negative infinity
        is_negative in proptest::bool::ANY,
    ) {
        let invalid_y = if is_negative { f32::NEG_INFINITY } else { f32::INFINITY };
        let mut control_points = [(0.0, 0.0), (0.25, 0.5), (0.75, 0.5), (1.0, 1.0)];
        control_points[point_index] = (x, invalid_y);

        // Constructor should reject Infinity
        let constructor_result = BezierCurve::new(control_points);
        prop_assert!(
            constructor_result.is_err(),
            "BezierCurve::new() should reject {} y at control point {}, but it succeeded",
            invalid_y,
            point_index
        );

        // validate() should also reject
        let bezier = BezierCurve { control_points };
        let curve = CurveType::Bezier(bezier);
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_err(),
            "CurveType::validate() should reject {} y at control point {}, but it succeeded",
            invalid_y,
            point_index
        );
    }

    // ============================================================
    // Comprehensive Validation Tests
    // ============================================================

    // Test that all curve types with valid parameters pass validation
    #[test]
    fn prop_all_valid_curves_pass_validation(
        // Parameters for different curve types
        exponent in 0.001f32..100.0,
        base in 1.001f32..1000.0,
        p1_x in 0.0f32..=1.0,
        p1_y in 0.0f32..=1.0,
        p2_x in 0.0f32..=1.0,
        p2_y in 0.0f32..=1.0,
        // Select curve type (0-3)
        curve_type_selector in 0u8..4,
    ) {
        let curve: CurveType = match curve_type_selector {
            0 => CurveType::Linear,
            1 => {
                match CurveType::exponential(exponent) {
                    Ok(c) => c,
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create exponential curve: {:?}", e
                    ))),
                }
            }
            2 => {
                match CurveType::logarithmic(base) {
                    Ok(c) => c,
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create logarithmic curve: {:?}", e
                    ))),
                }
            }
            _ => {
                let bezier_result = BezierCurve::new([
                    (0.0, 0.0),
                    (p1_x, p1_y),
                    (p2_x, p2_y),
                    (1.0, 1.0),
                ]);
                match bezier_result {
                    Ok(b) => CurveType::Bezier(b),
                    Err(e) => return Err(TestCaseError::fail(format!(
                        "Failed to create Bezier curve: {:?}", e
                    ))),
                }
            }
        };

        // All valid curves should pass validation
        let validate_result = curve.validate();
        prop_assert!(
            validate_result.is_ok(),
            "Valid curve {:?} should pass validation, but it failed: {:?}",
            curve,
            validate_result.err()
        );
    }

    // Test that validation errors contain useful information
    #[test]
    fn prop_validation_errors_are_descriptive(
        // Generate invalid values
        invalid_exponent in -100.0f32..-0.001,
        invalid_base in -100.0f32..=1.0,
        invalid_coord in prop::strategy::Union::new(vec![
            (-100.0f32..-0.001).boxed(),
            (1.001f32..100.0).boxed(),
        ]),
        // Select which error type to test (0-2)
        error_type_selector in 0u8..3,
    ) {
        let error = match error_type_selector {
            0 => {
                // Exponential error
                CurveType::exponential(invalid_exponent).err()
            }
            1 => {
                // Logarithmic error
                CurveType::logarithmic(invalid_base).err()
            }
            _ => {
                // Bezier error
                BezierCurve::new([
                    (0.0, 0.0),
                    (invalid_coord, 0.5),
                    (0.75, 0.5),
                    (1.0, 1.0),
                ]).err()
            }
        };

        // Error should exist
        prop_assert!(
            error.is_some(),
            "Expected an error for invalid parameters"
        );

        // Error message should be non-empty
        let error_msg = format!("{}", error.as_ref().map_or_else(
            || "no error".to_string(),
            |e| e.to_string()
        ));
        prop_assert!(
            !error_msg.is_empty() && error_msg != "no error",
            "Error message should be non-empty and descriptive"
        );
    }
}
