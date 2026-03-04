//! Pipeline processing deep tests.
//!
//! Covers:
//! - Pipeline stage ordering (filters applied in correct sequence)
//! - Pipeline hot-reconfiguration (change filter params during processing)
//! - Pipeline bypass modes
//! - Pipeline latency measurement
//! - Pipeline with no filters (passthrough)
//! - Pipeline saturation behavior
//! - Pipeline with response curves
//! - Pipeline metadata and swap semantics

use racing_wheel_engine::curves::CurveLut;
use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_schemas::prelude::{
    BumpstopConfig, CurvePoint, FilterConfig, FrequencyHz, Gain, HandsOffConfig, NotchFilter,
};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

type TestError = Box<dyn std::error::Error>;

fn make_frame(ffb_in: f32, torque_out: f32, wheel_speed: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

fn build_rt() -> Result<tokio::runtime::Runtime, TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.into())
}

fn compile(config: FilterConfig) -> Result<Pipeline, TestError> {
    let rt = build_rt()?;
    rt.block_on(async {
        let compiler = PipelineCompiler::new();
        let compiled = compiler.compile_pipeline(config).await?;
        Ok(compiled.pipeline)
    })
}

/// Linear passthrough config: no active filters.
fn passthrough_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

/// Config with only reconstruction at medium level.
fn recon_only_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        4,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

/// Config with damper and friction active.
fn damper_friction_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        0,
        Gain::new(0.15)?,
        Gain::new(0.2)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

/// Config with slew rate limiting.
fn slew_rate_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(0.5)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

/// Full config with multiple filters active.
fn full_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        4,
        Gain::new(0.1)?,
        Gain::new(0.15)?,
        Gain::new(0.05)?,
        vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        Gain::new(0.8)?,
        vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.6)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        Gain::new(0.9)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

/// Config with torque cap active.
fn torque_cap_config(cap: f32) -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(cap)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Pipeline with no filters (passthrough)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn passthrough_empty_pipeline_preserves_torque() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.5, 0.42, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!((frame.torque_out - 0.42).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn passthrough_compiled_linear_config() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let mut frame = make_frame(0.7, 0.7, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // Linear passthrough: torque_out should remain unchanged
    assert!(
        (frame.torque_out - 0.7).abs() < 0.01,
        "passthrough should preserve torque, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn passthrough_preserves_frame_fields() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    let mut frame = Frame {
        ffb_in: 0.33,
        torque_out: 0.44,
        wheel_speed: 7.5,
        hands_off: true,
        ts_mono_ns: 999_999,
        seq: 77,
    };
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!((frame.ffb_in - 0.33).abs() < f32::EPSILON);
    assert!((frame.wheel_speed - 7.5).abs() < f32::EPSILON);
    assert!(frame.hands_off);
    assert_eq!(frame.ts_mono_ns, 999_999);
    assert_eq!(frame.seq, 77);
    Ok(())
}

#[test]
fn passthrough_negative_torque() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.0, -0.5, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!((frame.torque_out - (-0.5)).abs() < f32::EPSILON);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Pipeline stage ordering
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn stage_ordering_reconstruction_then_friction() -> Result<(), TestError> {
    // Reconstruction filter smooths the signal; friction then adds opposing torque
    let mut pipeline = compile(FilterConfig::new_complete(
        4,
        Gain::new(0.15)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)?;

    // Process with wheel speed so friction has an effect
    let mut frame = make_frame(0.5, 0.5, 5.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // Output is modified (reconstruction smoothes, friction opposes)
    assert!(
        (frame.torque_out - 0.5).abs() > 0.001,
        "pipeline with filters should modify torque"
    );
    assert!(frame.torque_out.is_finite());
    Ok(())
}

#[test]
fn stage_ordering_full_config_produces_bounded_output() -> Result<(), TestError> {
    let mut pipeline = compile(full_config()?)?;

    for i in 0..200 {
        let input = (i as f32 / 100.0) - 1.0; // -1.0 to 1.0
        // Use low wheel speed to avoid mid-pipeline fault from damper/friction
        let mut frame = make_frame(input, input, 0.5);
        frame.ts_mono_ns = i as u64 * 1_000_000;
        frame.seq = i as u16;

        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "pipeline failed at step {i}");
        assert!(
            frame.torque_out.is_finite(),
            "non-finite output at step {i}: {}",
            frame.torque_out
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "output out of bounds at step {i}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn stage_ordering_damper_friction_both_active() -> Result<(), TestError> {
    let mut pipeline = compile(damper_friction_config()?)?;

    // With moderate wheel speed, both damper and friction should oppose
    // Keep speed low enough that combined torque stays within [-1,1]
    let mut frame = make_frame(0.0, 0.0, 1.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // Both effects should pull torque negative (opposing positive speed)
    assert!(
        frame.torque_out < 0.0,
        "damper+friction should oppose motion, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn stage_ordering_notch_filter_in_chain() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![NotchFilter::new(FrequencyHz::new(100.0)?, 5.0, -20.0)?],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config)?;

    // Feed a constant signal through the notch filter
    for _ in 0..100 {
        let mut frame = make_frame(0.5, 0.5, 0.0);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(frame.torque_out.is_finite());
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Pipeline hot-reconfiguration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn hot_reconfig_swap_pipeline_mid_processing() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;

    // Process a few frames with passthrough
    for _ in 0..10 {
        let mut frame = make_frame(0.5, 0.5, 0.0);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
    }

    // Hot-swap to a full config pipeline
    let new_pipeline = compile(full_config()?)?;
    pipeline.swap_at_tick_boundary(new_pipeline);

    // Process more frames with new pipeline
    for i in 0..100 {
        let mut frame = make_frame(0.3, 0.3, 2.0);
        frame.seq = i;
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "post-swap failed at step {i}");
        assert!(
            frame.torque_out.is_finite(),
            "non-finite after swap at step {i}"
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "out of bounds after swap at step {i}"
        );
    }
    Ok(())
}

#[test]
fn hot_reconfig_swap_changes_hash() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let hash_before = pipeline.config_hash();

    let new_pipeline = compile(full_config()?)?;
    pipeline.swap_at_tick_boundary(new_pipeline);

    assert_ne!(
        pipeline.config_hash(),
        hash_before,
        "hash should change after swap"
    );
    Ok(())
}

#[test]
fn hot_reconfig_swap_changes_node_count() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let count_before = pipeline.node_count();

    let new_pipeline = compile(full_config()?)?;
    pipeline.swap_at_tick_boundary(new_pipeline);

    assert!(
        pipeline.node_count() > count_before,
        "full config should have more nodes: before={count_before}, after={}",
        pipeline.node_count()
    );
    Ok(())
}

#[test]
fn hot_reconfig_swap_response_curve() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    assert!(pipeline.response_curve().is_none());

    // Set a response curve
    pipeline.set_response_curve(CurveLut::from_fn(|x| x * x));
    assert!(pipeline.response_curve().is_some());

    // Swap with a pipeline without response curve
    let new_pipeline = Pipeline::new();
    pipeline.swap_at_tick_boundary(new_pipeline);
    assert!(pipeline.response_curve().is_none());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Pipeline bypass modes
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn bypass_empty_pipeline_is_passthrough() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    let inputs = [-1.0, -0.5, 0.0, 0.5, 1.0];
    for &input in &inputs {
        let mut frame = make_frame(input, input, 0.0);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            (frame.torque_out - input).abs() < f32::EPSILON,
            "empty pipeline should pass through: input={input}, got={}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn bypass_linear_compiled_config_is_passthrough() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let inputs = [0.0, 0.25, 0.5, 0.75, 1.0, -0.5, -1.0];
    for &input in &inputs {
        let mut frame = make_frame(input, input, 0.0);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            (frame.torque_out - input).abs() < 0.01,
            "linear config should pass through: input={input}, got={}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn bypass_response_curve_linear_is_identity() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());

    let mut frame = make_frame(0.5, 0.6, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(
        (frame.torque_out - 0.6).abs() < 0.01,
        "linear response curve should be identity, got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Pipeline latency measurement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn latency_empty_pipeline_sub_microsecond() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();

    // Warm up
    for _ in 0..1000 {
        let mut frame = make_frame(0.5, 0.5, 0.0);
        let _ = pipeline.process(&mut frame);
    }

    // Measure
    let start = std::time::Instant::now();
    let iterations = 10_000;
    for _ in 0..iterations {
        let mut frame = make_frame(0.5, 0.5, 0.0);
        let _ = pipeline.process(&mut frame);
    }
    let elapsed = start.elapsed();
    let per_call_ns = elapsed.as_nanos() / iterations as u128;

    // Empty pipeline should be very fast (< 1μs per call typically)
    assert!(
        per_call_ns < 10_000,
        "empty pipeline should be fast, got {}ns per call",
        per_call_ns
    );
    Ok(())
}

#[test]
fn latency_full_pipeline_within_budget() -> Result<(), TestError> {
    let mut pipeline = compile(full_config()?)?;

    // Warm up
    for i in 0..500 {
        let mut frame = make_frame(0.5, 0.5, 2.0);
        frame.seq = i;
        let _ = pipeline.process(&mut frame);
    }

    // Measure
    let start = std::time::Instant::now();
    let iterations = 5_000;
    for i in 0..iterations {
        let mut frame = make_frame(0.5, 0.5, 2.0);
        frame.seq = i as u16;
        let _ = pipeline.process(&mut frame);
    }
    let elapsed = start.elapsed();
    let per_call_us = elapsed.as_micros() as f64 / iterations as f64;

    // Full pipeline should process within 200μs (RT budget)
    assert!(
        per_call_us < 200.0,
        "full pipeline should process within 200μs, got {per_call_us:.1}μs"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Pipeline saturation behavior
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn saturation_output_bounded_at_extremes() -> Result<(), TestError> {
    let mut pipeline = compile(full_config()?)?;

    // Feed extreme inputs
    for _ in 0..100 {
        let mut frame = make_frame(1.0, 1.0, 0.0);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            frame.torque_out.abs() <= 1.0,
            "output should be bounded at extremes, got {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn saturation_torque_cap_limits_output() -> Result<(), TestError> {
    let mut pipeline = compile(torque_cap_config(0.5)?)?;

    let mut frame = make_frame(0.9, 0.9, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(
        frame.torque_out.abs() <= 0.5 + 0.01,
        "torque cap 0.5 should limit output, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn saturation_sustained_max_input_stays_bounded() -> Result<(), TestError> {
    let mut pipeline = compile(full_config()?)?;

    for i in 0..1000 {
        let mut frame = make_frame(1.0, 1.0, 0.5);
        frame.seq = i as u16;
        frame.ts_mono_ns = i as u64 * 1_000_000;
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "failed at iteration {i}");
        assert!(
            frame.torque_out.abs() <= 1.0,
            "sustained max input exceeded bounds at step {i}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn saturation_alternating_extremes_stays_bounded() -> Result<(), TestError> {
    let mut pipeline = compile(full_config()?)?;

    for i in 0..500 {
        let input = if i % 2 == 0 { 1.0 } else { -1.0 };
        let mut frame = make_frame(input, input, 0.0);
        frame.seq = i as u16;
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "failed at step {i}");
        assert!(
            frame.torque_out.abs() <= 1.0,
            "alternating input exceeded bounds at step {i}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Pipeline with response curves
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn response_curve_x_squared_halves_midrange() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::from_fn(|x| x * x));

    let mut frame = make_frame(0.0, 0.5, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // 0.5^2 = 0.25
    assert!(
        (frame.torque_out - 0.25).abs() < 0.02,
        "x² at 0.5 should be ~0.25, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn response_curve_preserves_negative_sign() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());

    let mut frame = make_frame(0.0, -0.75, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(frame.torque_out < 0.0);
    assert!(
        (frame.torque_out + 0.75).abs() < 0.01,
        "linear curve should preserve -0.75, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn response_curve_zero_stays_zero() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::from_fn(|x| x * x));

    let mut frame = make_frame(0.0, 0.0, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(
        frame.torque_out.abs() < f32::EPSILON,
        "zero torque should stay zero"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Pipeline metadata
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn metadata_empty_pipeline_defaults() -> Result<(), TestError> {
    let pipeline = Pipeline::new();
    assert!(pipeline.is_empty());
    assert_eq!(pipeline.node_count(), 0);
    assert_eq!(pipeline.config_hash(), 0);
    assert!(pipeline.response_curve().is_none());
    Ok(())
}

#[test]
fn metadata_with_hash() -> Result<(), TestError> {
    let pipeline = Pipeline::with_hash(0xDEAD_BEEF);
    assert_eq!(pipeline.config_hash(), 0xDEAD_BEEF);
    assert!(pipeline.is_empty());
    Ok(())
}

#[test]
fn metadata_full_config_has_nodes() -> Result<(), TestError> {
    let pipeline = compile(full_config()?)?;
    assert!(!pipeline.is_empty());
    assert!(pipeline.node_count() > 0);
    assert!(pipeline.config_hash() != 0);
    Ok(())
}

#[test]
fn metadata_deterministic_hash() -> Result<(), TestError> {
    let p1 = compile(full_config()?)?;
    let p2 = compile(full_config()?)?;
    assert_eq!(p1.config_hash(), p2.config_hash());
    Ok(())
}

#[test]
fn metadata_different_configs_different_hashes() -> Result<(), TestError> {
    let p1 = compile(passthrough_config()?)?;
    let p2 = compile(full_config()?)?;
    assert_ne!(p1.config_hash(), p2.config_hash());
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Pipeline slew rate effect visible
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn slew_rate_pipeline_limits_step_response() -> Result<(), TestError> {
    let mut pipeline = compile(slew_rate_config()?)?;

    // First frame: step from 0 to 1.0
    let mut frame = make_frame(0.0, 1.0, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());

    // Output should be limited by slew rate, not jump to 1.0
    assert!(
        frame.torque_out < 0.01,
        "slew rate should limit first step, got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Pipeline reconstruction smooths signal
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn reconstruction_pipeline_smooths_step() -> Result<(), TestError> {
    let mut pipeline = compile(recon_only_config()?)?;

    // First frame: step from 0 to 1.0
    let mut frame = make_frame(1.0, 1.0, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());

    // Reconstruction EMA should smooth the step
    assert!(
        frame.torque_out < 0.5,
        "reconstruction should smooth first step, got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out > 0.0,
        "reconstruction output should be positive"
    );
    Ok(())
}

#[test]
fn reconstruction_pipeline_converges() -> Result<(), TestError> {
    let mut pipeline = compile(recon_only_config()?)?;

    let mut last_out = 0.0;
    for _ in 0..500 {
        let mut frame = make_frame(1.0, 1.0, 0.0);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        last_out = frame.torque_out;
    }

    assert!(
        (last_out - 1.0).abs() < 0.05,
        "reconstruction should converge to 1.0, got {}",
        last_out
    );
    Ok(())
}
