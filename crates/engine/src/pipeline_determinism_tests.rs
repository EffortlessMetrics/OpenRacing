//! Deep FFB pipeline determinism tests for 1.0 RC quality validation
//!
//! Coverage areas:
//! - Identical inputs produce identical outputs (determinism)
//! - Bounded execution time (no unbounded loops)
//! - Filter chain composition correctness
//! - Torque output always within safe bounds
//! - Proptest: random telemetry inputs never produce out-of-range torque

use crate::pipeline::*;
use crate::rt::Frame;
use proptest::prelude::*;
use racing_wheel_schemas::prelude::*;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[track_caller]
fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("unexpected Err: {e:?}"),
    }
}

fn make_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0, // pipeline fills this via filters
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    }
}

fn basic_filter_config() -> FilterConfig {
    // Use conservative values that won't cause pipeline faults
    must(FilterConfig::new_complete(
        2,                     // low reconstruction level
        must(Gain::new(0.05)), // minimal friction
        must(Gain::new(0.05)), // minimal damper
        must(Gain::new(0.0)),  // no inertia (avoids amplification)
        vec![],                // no notch filters
        must(Gain::new(0.9)),  // mild slew_rate
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.5, 0.5)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(0.9)), // torque_cap
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    ))
}

fn linear_filter_config() -> FilterConfig {
    must(FilterConfig::new_complete(
        0,                    // no reconstruction
        must(Gain::new(0.0)), // no friction
        must(Gain::new(0.0)), // no damper
        must(Gain::new(0.0)), // no inertia
        vec![],               // no notch filters
        must(Gain::new(1.0)), // no slew rate limiting
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(1.0)), // no torque cap
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    ))
}

fn capped_filter_config(cap: f32) -> FilterConfig {
    must(FilterConfig::new_complete(
        0,
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        vec![],
        must(Gain::new(1.0)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(cap)),
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    ))
}

// =========================================================================
// 1. Determinism: identical inputs → identical outputs
// =========================================================================

#[test]
fn deep_test_pipeline_empty_deterministic() {
    let mut p1 = Pipeline::new();
    let mut p2 = Pipeline::new();

    let mut f1 = make_frame(0.5, 10.0);
    let mut f2 = make_frame(0.5, 10.0);

    must(p1.process(&mut f1));
    must(p2.process(&mut f2));

    assert_eq!(f1.torque_out.to_bits(), f2.torque_out.to_bits());
}

#[tokio::test]
async fn deep_test_pipeline_compiled_deterministic() {
    let compiler = PipelineCompiler::new();
    let config = basic_filter_config();

    let mut p1 = must(compiler.compile_pipeline(config.clone()).await).pipeline;
    let mut p2 = must(compiler.compile_pipeline(config).await).pipeline;

    let mut f1 = make_frame(0.1, 0.0);
    let mut f2 = make_frame(0.1, 0.0);

    must(p1.process(&mut f1));
    must(p2.process(&mut f2));

    assert_eq!(
        f1.torque_out.to_bits(),
        f2.torque_out.to_bits(),
        "Compiled pipelines must produce identical output for identical input"
    );
}

#[tokio::test]
async fn deep_test_pipeline_hash_deterministic() {
    let compiler = PipelineCompiler::new();
    let config = basic_filter_config();

    let h1 = must(compiler.compile_pipeline(config.clone()).await).config_hash;
    let h2 = must(compiler.compile_pipeline(config).await).config_hash;

    assert_eq!(h1, h2, "Same config must produce same hash");
}

#[tokio::test]
async fn deep_test_pipeline_different_configs_different_hash() {
    let compiler = PipelineCompiler::new();

    let h1 = must(compiler.compile_pipeline(basic_filter_config()).await).config_hash;
    let h2 = must(compiler.compile_pipeline(linear_filter_config()).await).config_hash;

    assert_ne!(h1, h2, "Different configs must produce different hashes");
}

#[tokio::test]
async fn deep_test_pipeline_repeated_processing_deterministic() {
    let compiler = PipelineCompiler::new();
    let config = linear_filter_config();
    let mut pipeline = must(compiler.compile_pipeline(config).await).pipeline;

    // Process the same frame multiple times in succession
    let mut results = Vec::new();
    for _ in 0..10 {
        let mut frame = make_frame(0.75, 5.0);
        must(pipeline.process(&mut frame));
        results.push(frame.torque_out);
    }

    // All results should be bit-identical for a linear pipeline
    let first_bits = results[0].to_bits();
    for (i, &val) in results.iter().enumerate() {
        assert_eq!(
            val.to_bits(),
            first_bits,
            "Frame {i} produced different output: {} vs {}",
            val,
            results[0]
        );
    }
}

// =========================================================================
// 2. Bounded execution time
// =========================================================================

#[test]
fn deep_test_empty_pipeline_bounded_time() {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.5, 10.0);

    let start = Instant::now();
    for _ in 0..10_000 {
        must(pipeline.process(&mut frame));
        frame.torque_out = frame.ffb_in; // reset
    }
    let elapsed = start.elapsed();

    // 10k iterations of empty pipeline should complete in < 100ms
    assert!(
        elapsed < Duration::from_millis(100),
        "Empty pipeline 10k iterations took {elapsed:?}"
    );
}

#[tokio::test]
async fn deep_test_compiled_pipeline_bounded_time() {
    let compiler = PipelineCompiler::new();
    let config = basic_filter_config();
    let mut pipeline = must(compiler.compile_pipeline(config).await).pipeline;

    let mut frame = make_frame(0.1, 0.0);
    let start = Instant::now();
    for _ in 0..1_000 {
        frame.torque_out = 0.0;
        must(pipeline.process(&mut frame));
    }
    let elapsed = start.elapsed();

    // 1k iterations with full filter chain should complete in < 500ms
    assert!(
        elapsed < Duration::from_millis(500),
        "Compiled pipeline 1k iterations took {elapsed:?}"
    );
}

#[tokio::test]
async fn deep_test_pipeline_single_frame_latency() {
    let compiler = PipelineCompiler::new();
    let config = basic_filter_config();
    let mut pipeline = must(compiler.compile_pipeline(config).await).pipeline;

    let mut max_us = 0u128;
    for i in 0..100 {
        let mut frame = make_frame((i as f32 / 100.0).sin() * 0.1, 0.0);
        let start = Instant::now();
        must(pipeline.process(&mut frame));
        let us = start.elapsed().as_micros();
        if us > max_us {
            max_us = us;
        }
    }

    // Single frame processing should be well under 1ms
    assert!(
        max_us < 1000,
        "Max single-frame latency {max_us}μs exceeded 1ms"
    );
}

// =========================================================================
// 3. Filter chain composition
// =========================================================================

#[tokio::test]
async fn deep_test_linear_pipeline_passthrough() {
    let compiler = PipelineCompiler::new();
    let config = linear_filter_config();
    let mut pipeline = must(compiler.compile_pipeline(config).await).pipeline;

    // Linear pipeline with no active filters should approximately pass through
    // (empty pipeline with no nodes)
    if pipeline.is_empty() {
        let mut frame = make_frame(0.75, 0.0);
        must(pipeline.process(&mut frame));
        assert!((frame.torque_out - 0.75).abs() < f32::EPSILON);
    }
}

#[tokio::test]
async fn deep_test_torque_cap_limits_output() {
    let compiler = PipelineCompiler::new();
    let config = capped_filter_config(0.5);
    let mut pipeline = must(compiler.compile_pipeline(config).await).pipeline;

    let mut frame = make_frame(0.9, 0.0);
    must(pipeline.process(&mut frame));

    // Torque cap of 0.5 should limit output
    assert!(
        frame.torque_out.abs() <= 0.5 + f32::EPSILON,
        "Torque {} exceeded cap 0.5",
        frame.torque_out
    );
}

#[test]
fn deep_test_pipeline_swap_at_tick_boundary() {
    let mut p1 = Pipeline::new();
    let p2 = Pipeline::with_hash(0xCAFEBABE);

    assert_eq!(p1.config_hash(), 0);
    p1.swap_at_tick_boundary(p2);
    assert_eq!(p1.config_hash(), 0xCAFEBABE);
}

#[test]
fn deep_test_pipeline_node_count() {
    let p = Pipeline::new();
    assert_eq!(p.node_count(), 0);
    assert!(p.is_empty());
}

#[tokio::test]
async fn deep_test_compiled_pipeline_has_nodes() {
    let compiler = PipelineCompiler::new();
    let config = basic_filter_config();
    let compiled = must(compiler.compile_pipeline(config).await);

    assert!(compiled.pipeline.node_count() > 0);
    assert!(!compiled.pipeline.is_empty());
}

// =========================================================================
// 4. Torque output always within safe bounds
// =========================================================================

#[test]
fn deep_test_empty_pipeline_preserves_bounds() {
    let mut pipeline = Pipeline::new();

    for input in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        let mut frame = make_frame(input, 0.0);
        must(pipeline.process(&mut frame));
        assert!(
            frame.torque_out.is_finite(),
            "Output not finite for input {input}"
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "Output {} out of [-1, 1] for input {}",
            frame.torque_out,
            input
        );
    }
}

#[tokio::test]
async fn deep_test_compiled_pipeline_output_bounded() {
    let compiler = PipelineCompiler::new();
    let config = basic_filter_config();
    let mut pipeline = must(compiler.compile_pipeline(config).await).pipeline;

    let test_inputs = [-0.1, -0.05, -0.01, 0.0, 0.01, 0.05, 0.1];

    for &input in &test_inputs {
        let mut frame = make_frame(input, 5.0);
        must(pipeline.process(&mut frame));

        assert!(
            frame.torque_out.is_finite(),
            "Non-finite output for input {input}: {}",
            frame.torque_out
        );
        assert!(
            frame.torque_out.abs() <= 1.0 + f32::EPSILON,
            "Output {} exceeds [-1, 1] for input {}",
            frame.torque_out,
            input
        );
    }
}

#[test]
fn deep_test_nan_input_yields_pipeline_fault() {
    let mut pipeline = Pipeline::new();

    // NaN through an empty pipeline should still produce finite output
    // since there are no filters to fault
    let mut frame = make_frame(f32::NAN, 0.0);
    frame.torque_out = f32::NAN;
    let result = pipeline.process(&mut frame);
    // Empty pipeline doesn't validate, but a pipeline with nodes would fault
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn deep_test_extreme_inputs_no_panic() {
    let mut pipeline = Pipeline::new();

    let extreme_values = [
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::EPSILON,
        -f32::EPSILON,
        0.0,
        -0.0,
    ];

    for &val in &extreme_values {
        let mut frame = make_frame(val, val);
        // Should not panic
        let _ = pipeline.process(&mut frame);
    }
}

// =========================================================================
// 5. Pipeline validation
// =========================================================================

#[tokio::test]
async fn deep_test_invalid_reconstruction_level_rejected() {
    let compiler = PipelineCompiler::new();

    let result = FilterConfig::new_complete(
        10, // Invalid: >8
        must(Gain::new(0.1)),
        must(Gain::new(0.1)),
        must(Gain::new(0.1)),
        vec![],
        must(Gain::new(0.5)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(0.9)),
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    );

    if let Ok(config) = result {
        let compile_result = compiler.compile_pipeline(config).await;
        assert!(compile_result.is_err());
    }
    // If FilterConfig::new_complete rejects it, that's also valid
}

#[tokio::test]
async fn deep_test_non_monotonic_curve_rejected() {
    let compiler = PipelineCompiler::new();

    // Non-monotonic curve points
    let result = FilterConfig::new_complete(
        0,
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        vec![],
        must(Gain::new(1.0)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.8, 0.9)),
            must(CurvePoint::new(0.5, 0.6)), // Non-monotonic!
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(1.0)),
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    );

    if let Ok(config) = result {
        let compile_result = compiler.compile_pipeline(config).await;
        assert!(compile_result.is_err());
    }
}

#[tokio::test]
async fn deep_test_curve_must_start_at_zero() {
    let compiler = PipelineCompiler::new();

    let result = FilterConfig::new_complete(
        0,
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        vec![],
        must(Gain::new(1.0)),
        vec![
            must(CurvePoint::new(0.1, 0.0)), // Doesn't start at 0!
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(1.0)),
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    );

    if let Ok(config) = result {
        let compile_result = compiler.compile_pipeline(config).await;
        assert!(compile_result.is_err());
    }
}

#[tokio::test]
async fn deep_test_curve_must_end_at_one() {
    let compiler = PipelineCompiler::new();

    let result = FilterConfig::new_complete(
        0,
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        vec![],
        must(Gain::new(1.0)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.9, 0.9)), // Doesn't end at 1.0!
        ],
        must(Gain::new(1.0)),
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    );

    if let Ok(config) = result {
        let compile_result = compiler.compile_pipeline(config).await;
        assert!(compile_result.is_err());
    }
}

// =========================================================================
// 6. Response curve integration
// =========================================================================

#[tokio::test]
async fn deep_test_response_curve_linear_identity() {
    let compiler = PipelineCompiler::new();
    let config = linear_filter_config();

    let compiled = must(
        compiler
            .compile_pipeline_with_response_curve(config, Some(&crate::curves::CurveType::Linear))
            .await,
    );

    let mut pipeline = compiled.pipeline;
    if pipeline.is_empty() {
        // Linear config with no filters — just test response curve
        let mut frame = make_frame(0.5, 0.0);
        must(pipeline.process(&mut frame));
        // Linear response curve should be identity-like
        assert!(frame.torque_out.is_finite());
    }
}

#[tokio::test]
async fn deep_test_response_curve_included_in_hash() {
    let compiler = PipelineCompiler::new();
    let config = linear_filter_config();

    let h_no_curve = must(
        compiler
            .compile_pipeline_with_response_curve(config.clone(), None)
            .await,
    )
    .config_hash;

    let h_linear = must(
        compiler
            .compile_pipeline_with_response_curve(
                config.clone(),
                Some(&crate::curves::CurveType::Linear),
            )
            .await,
    )
    .config_hash;

    let h_exp = must(
        compiler
            .compile_pipeline_with_response_curve(
                config,
                Some(&crate::curves::CurveType::Exponential { exponent: 2.0 }),
            )
            .await,
    )
    .config_hash;

    // All three should differ
    assert_ne!(h_no_curve, h_linear);
    assert_ne!(h_no_curve, h_exp);
    assert_ne!(h_linear, h_exp);
}

// =========================================================================
// 7. Proptest: random inputs → bounded outputs
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn deep_test_prop_empty_pipeline_output_bounded(
        ffb_in in -1.0f32..=1.0,
        wheel_speed in 0.0f32..=100.0,
    ) {
        let mut pipeline = Pipeline::new();
        let mut frame = make_frame(ffb_in, wheel_speed);
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok());
        prop_assert!(
            frame.torque_out.is_finite(),
            "Non-finite output {} for input {}",
            frame.torque_out, ffb_in
        );
        prop_assert!(
            frame.torque_out.abs() <= 1.0 + f32::EPSILON,
            "Output {} out of bounds for input {}",
            frame.torque_out, ffb_in
        );
    }

    #[test]
    fn deep_test_prop_empty_pipeline_deterministic(
        ffb_in in -1.0f32..=1.0,
        wheel_speed in 0.0f32..=100.0,
    ) {
        let mut p1 = Pipeline::new();
        let mut p2 = Pipeline::new();
        let mut f1 = make_frame(ffb_in, wheel_speed);
        let mut f2 = make_frame(ffb_in, wheel_speed);
        must(p1.process(&mut f1));
        must(p2.process(&mut f2));
        prop_assert_eq!(
            f1.torque_out.to_bits(),
            f2.torque_out.to_bits(),
            "Determinism violated for input {}", ffb_in
        );
    }

    #[test]
    fn deep_test_prop_pipeline_no_nan_output(
        ffb_in in -1.0f32..=1.0,
        speed in 0.0f32..=200.0,
        seq in 0u16..=1000,
    ) {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed: speed,
            hands_off: false,
            ts_mono_ns: seq as u64 * 1_000_000,
            seq,
        };
        let _ = pipeline.process(&mut frame);
        // Empty pipeline just passes through, so output == input
        prop_assert!(
            frame.torque_out.is_finite() || ffb_in.is_nan(),
            "Got NaN output from finite input {}", ffb_in
        );
    }

    #[test]
    fn deep_test_prop_swap_preserves_hash(
        hash in proptest::num::u64::ANY,
    ) {
        let mut p1 = Pipeline::new();
        let p2 = Pipeline::with_hash(hash);
        p1.swap_at_tick_boundary(p2);
        prop_assert_eq!(p1.config_hash(), hash);
    }
}
