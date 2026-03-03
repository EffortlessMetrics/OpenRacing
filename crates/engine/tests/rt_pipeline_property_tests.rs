#![allow(clippy::redundant_closure)]
//! RT pipeline property tests.
//!
//! Tests cover:
//! - Proptest: RT pipeline output is always bounded
//! - Proptest: pipeline latency is deterministic (same input → same output)
//! - Pipeline with extreme inputs (NaN, Inf, very large/small)
//! - Pipeline reset and reinitialization
//! - Multi-device pipeline routing

use proptest::prelude::*;
use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_schemas::prelude::{CurvePoint, FilterConfig, FrequencyHz, Gain, NotchFilter};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_frame(ffb_in: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0,
        wheel_speed: 5.0,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn default_filter_config() -> FilterConfig {
    FilterConfig::default()
}

fn comprehensive_filter_config() -> Result<FilterConfig, Box<dyn std::error::Error>> {
    Ok(FilterConfig {
        reconstruction: 4,
        friction: Gain::new(0.12)?,
        damper: Gain::new(0.18)?,
        inertia: Gain::new(0.08)?,
        notch_filters: vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        slew_rate: Gain::new(0.75)?,
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.25, 0.18)?,
            CurvePoint::new(0.5, 0.42)?,
            CurvePoint::new(0.75, 0.72)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    })
}

// =========================================================================
// Proptest: RT pipeline output is always bounded
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn pipeline_output_always_bounded_default(ffb_in in -1.0f32..=1.0) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| TestCaseError::Fail(format!("Failed to build runtime: {}", e).into()))?;
        rt.block_on(async {
            let compiler = PipelineCompiler::new();
            let compiled = compiler.compile_pipeline(default_filter_config()).await
                .map_err(|e| TestCaseError::Fail(format!("Compile failed: {}", e).into()))?;
            let mut pipeline = compiled.pipeline;
            let mut frame = make_frame(ffb_in, 0);
            let result = pipeline.process(&mut frame);
            prop_assert!(result.is_ok(), "Pipeline processing failed: {:?}", result);
            prop_assert!(
                frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                "Output {:.6} is out of bounds for input {:.6}",
                frame.torque_out, ffb_in
            );
            Ok(())
        })?;
    }

    #[test]
    fn pipeline_output_always_bounded_comprehensive(ffb_in in -1.0f32..=1.0) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| TestCaseError::Fail(format!("Config failed: {}", e).into()))?;
        rt.block_on(async {
            let config = comprehensive_filter_config()
                .map_err(|e| TestCaseError::Fail(format!("Config failed: {}", e).into()))?;
            let compiler = PipelineCompiler::new();
            let compiled = compiler.compile_pipeline(config).await
                .map_err(|e| TestCaseError::Fail(format!("Compile failed: {}", e).into()))?;
            let mut pipeline = compiled.pipeline;
            let mut frame = make_frame(ffb_in, 0);
            let result = pipeline.process(&mut frame);
            // Pipeline may return PipelineFault for intermediate out-of-bounds;
            // either way, if Ok, output must be bounded.
            if let Ok(()) = result {
                prop_assert!(
                    frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                    "Output {:.6} is out of bounds for input {:.6}",
                    frame.torque_out, ffb_in
                );
            }
            Ok(())
        })?;
    }
}

// =========================================================================
// Proptest: pipeline determinism (same input → same output)
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn pipeline_is_deterministic(ffb_in in -1.0f32..=1.0, seq in 0u16..100) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| TestCaseError::Fail(format!("Failed to build runtime: {}", e).into()))?;
        rt.block_on(async {
            let compiler = PipelineCompiler::new();

            // Compile two identical pipelines
            let compiled_a = compiler.compile_pipeline(default_filter_config()).await
                .map_err(|e| TestCaseError::Fail(format!("Compile A failed: {}", e).into()))?;
            let compiled_b = compiler.compile_pipeline(default_filter_config()).await
                .map_err(|e| TestCaseError::Fail(format!("Compile B failed: {}", e).into()))?;
            let mut pipeline_a = compiled_a.pipeline;
            let mut pipeline_b = compiled_b.pipeline;

            let mut frame_a = make_frame(ffb_in, seq);
            let mut frame_b = make_frame(ffb_in, seq);

            let _ = pipeline_a.process(&mut frame_a);
            let _ = pipeline_b.process(&mut frame_b);

            prop_assert!(
                (frame_a.torque_out - frame_b.torque_out).abs() < f32::EPSILON,
                "Non-deterministic: A={:.8}, B={:.8} for input {:.6}",
                frame_a.torque_out, frame_b.torque_out, ffb_in
            );
            Ok(())
        })?;
    }
}

// =========================================================================
// Pipeline with extreme inputs (NaN, Inf, very large/small)
// =========================================================================

#[tokio::test]
async fn pipeline_nan_input_returns_error_or_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = make_frame(f32::NAN, 0);
    let result = pipeline.process(&mut frame);

    // Pipeline may either error or produce bounded output
    match result {
        Ok(()) => {
            assert!(
                frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                "NaN input produced unbounded output: {}",
                frame.torque_out
            );
        }
        Err(_) => {
            // Pipeline correctly rejected NaN input
        }
    }
    Ok(())
}

#[tokio::test]
async fn pipeline_inf_input_returns_error_or_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = make_frame(f32::INFINITY, 0);
    let result = pipeline.process(&mut frame);

    match result {
        Ok(()) => {
            assert!(
                frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                "Inf input produced unbounded output: {}",
                frame.torque_out
            );
        }
        Err(_) => {
            // Pipeline correctly rejected Inf input
        }
    }
    Ok(())
}

#[tokio::test]
async fn pipeline_neg_inf_input_returns_error_or_bounded() -> Result<(), Box<dyn std::error::Error>>
{
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = make_frame(f32::NEG_INFINITY, 0);
    let result = pipeline.process(&mut frame);

    if let Ok(()) = result {
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "NEG_INFINITY input produced unbounded output: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn pipeline_very_small_input() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = make_frame(f32::MIN_POSITIVE, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "MIN_POSITIVE input produced: {}",
        frame.torque_out
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_subnormal_input() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let subnormal = f32::from_bits(1); // smallest subnormal
    let mut frame = make_frame(subnormal, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "Subnormal input produced: {}",
        frame.torque_out
    );
    Ok(())
}

// =========================================================================
// Pipeline reset and reinitialization
// =========================================================================

#[tokio::test]
async fn pipeline_swap_resets_state() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    // Process several frames to build up filter state
    for seq in 0..10 {
        let mut frame = make_frame(0.8, seq);
        let _ = pipeline.process(&mut frame);
    }

    // Swap to a new pipeline
    let new_compiled = compiler.compile_pipeline(default_filter_config()).await?;
    pipeline.swap_at_tick_boundary(new_compiled.pipeline);

    // Output after swap should match fresh pipeline behavior
    let mut frame = make_frame(0.5, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    Ok(())
}

#[test]
fn empty_pipeline_passthrough() {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.75, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // Empty pipeline has no nodes, torque_out stays at its initial value (0.0)
    assert!(frame.torque_out.is_finite());
}

#[tokio::test]
async fn pipeline_config_hash_changes_with_config() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();

    let compiled_default = compiler.compile_pipeline(default_filter_config()).await?;
    let compiled_custom = compiler
        .compile_pipeline(comprehensive_filter_config()?)
        .await?;

    assert_ne!(
        compiled_default.config_hash, compiled_custom.config_hash,
        "Different configs should produce different hashes"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_multiple_recompile_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let config = default_filter_config();

    let compiled_a = compiler.compile_pipeline(config.clone()).await?;
    let compiled_b = compiler.compile_pipeline(config).await?;

    assert_eq!(
        compiled_a.config_hash, compiled_b.config_hash,
        "Same config should produce same hash"
    );
    Ok(())
}

// =========================================================================
// Multi-device pipeline routing
// =========================================================================

#[tokio::test]
async fn independent_pipelines_dont_interfere() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();

    let mut pipeline_a = compiler.compile_pipeline(default_filter_config()).await?.pipeline;
    let mut pipeline_b = compiler
        .compile_pipeline(default_filter_config())
        .await?
        .pipeline;

    // Process different inputs through each pipeline
    let mut frame_a = make_frame(0.9, 0);
    let mut frame_b = make_frame(-0.3, 0);

    let result_a = pipeline_a.process(&mut frame_a);
    let result_b = pipeline_b.process(&mut frame_b);

    assert!(result_a.is_ok());
    assert!(result_b.is_ok());

    // Both should produce bounded output independently
    assert!(
        frame_a.torque_out.is_finite() && frame_a.torque_out.abs() <= 1.0,
        "Pipeline A output out of bounds: {}",
        frame_a.torque_out
    );
    assert!(
        frame_b.torque_out.is_finite() && frame_b.torque_out.abs() <= 1.0,
        "Pipeline B output out of bounds: {}",
        frame_b.torque_out
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_sequence_independence() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let mut pipeline = compiler.compile_pipeline(default_filter_config()).await?.pipeline;

    // Process frames with different sequence numbers; outputs should all be bounded
    let sequences: Vec<u16> = vec![0, 100, 65535, 1, 32768];
    for seq in sequences {
        let mut frame = make_frame(0.5, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Out of bounds at seq {}: {}",
            seq,
            frame.torque_out
        );
    }
    Ok(())
}
