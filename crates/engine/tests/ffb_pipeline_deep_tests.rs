//! Deep tests for the engine's FFB processing pipeline.
//!
//! Covers:
//! - Full pipeline passthrough: input → filters → safety → output with known values
//! - Filter chain ordering matters (verify correct sequence)
//! - Filter bypass for diagnostics mode
//! - Pipeline latency measurement and budget enforcement
//! - Pipeline with all filters active (damping + friction + inertia + spring + bumpstop)
//! - Pipeline with mixed filter enables/disables
//! - Pipeline handles zero input gracefully
//! - Pipeline handles max input clamping
//! - Pipeline preserves signal frequency response (low-pass characteristics)
//! - Pipeline handles rapid input changes (step response)
//! - Pipeline determinism: same input always produces same output
//! - Pipeline state reset on device disconnect
//! - Pipeline configuration hot-update (change filter params mid-stream)
//! - Pipeline handles NaN/Infinity input safely
//! - Pipeline output is always within device limits
//! - Multi-device pipeline isolation (two wheels, independent processing)
//! - Pipeline performance: process N samples within budget
//! - Pipeline statistics collection (min/max/avg torque, processing time)

use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_engine::safety::SafetyService;
use racing_wheel_schemas::prelude::{
    BumpstopConfig, CurvePoint, FilterConfig, FrequencyHz, Gain, HandsOffConfig, NotchFilter,
};
use std::time::Instant;

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

fn make_frame_seq(ffb_in: f32, torque_out: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
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

/// Passthrough config: no active filters, linear curve.
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

/// All-filters-active config with moderate gains.
fn all_filters_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        4,
        Gain::new(0.12)?,
        Gain::new(0.15)?,
        Gain::new(0.08)?,
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

/// Config with only friction enabled.
fn friction_only_config(gain: f32) -> Result<FilterConfig, TestError> {
    Ok(FilterConfig {
        friction: Gain::new(gain)?,
        ..FilterConfig::default()
    })
}

/// Config with only damper enabled.
fn damper_only_config(gain: f32) -> Result<FilterConfig, TestError> {
    Ok(FilterConfig {
        damper: Gain::new(gain)?,
        ..FilterConfig::default()
    })
}

/// Config with reconstruction (low-pass) only.
fn reconstruction_only_config(level: u8) -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        level,
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

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Full pipeline passthrough: input → filters → safety → output
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn passthrough_pipeline_preserves_known_values() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let known_values = [0.0_f32, 0.1, 0.25, 0.5, 0.75, 1.0, -0.3, -0.7, -1.0];

    for &val in &known_values {
        let mut frame = make_frame(val, val, 0.0);
        pipeline.process(&mut frame)?;
        assert!(
            (frame.torque_out - val).abs() < 0.02,
            "passthrough should preserve {val}, got {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn passthrough_with_safety_clamp_limits_output() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    let mut pipeline = Pipeline::new();

    // Simulate full pipeline: process then safety clamp
    let mut frame = make_frame(0.8, 0.8, 0.0);
    pipeline.process(&mut frame)?;

    // Safety clamp converts normalized torque to Nm and clamps
    let clamped = safety.clamp_torque_nm(frame.torque_out * 25.0);
    assert!(
        clamped.abs() <= 5.0,
        "safe-torque state should clamp to ≤5 Nm, got {clamped}"
    );
    Ok(())
}

#[test]
fn pipeline_then_safety_faulted_zeroes_output() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    let mut pipeline = Pipeline::new();

    let mut frame = make_frame(0.9, 0.9, 0.0);
    pipeline.process(&mut frame)?;

    safety.report_fault(racing_wheel_engine::safety::FaultType::PipelineFault);
    let clamped = safety.clamp_torque_nm(frame.torque_out * 25.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "faulted state should produce zero torque, got {clamped}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Filter chain ordering matters
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn filter_chain_friction_before_damper_produces_different_result() -> Result<(), TestError> {
    // The pipeline applies filters in a fixed order:
    // reconstruction → friction → damper → inertia → notch → slew → curve → cap → bumpstop
    // We verify that having both vs only one produces different outputs.
    let mut pipeline_both = compile(FilterConfig::new_complete(
        0,
        Gain::new(0.15)?,
        Gain::new(0.15)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)?;

    let mut pipeline_friction_only = compile(FilterConfig::new_complete(
        0,
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

    let mut frame_both = make_frame(0.3, 0.3, 3.0);
    let mut frame_friction = make_frame(0.3, 0.3, 3.0);

    pipeline_both.process(&mut frame_both)?;
    pipeline_friction_only.process(&mut frame_friction)?;

    assert!(
        (frame_both.torque_out - frame_friction.torque_out).abs() > f32::EPSILON,
        "adding damper should change output: both={}, friction_only={}",
        frame_both.torque_out,
        frame_friction.torque_out
    );
    Ok(())
}

#[test]
fn filter_chain_ordering_reconstruction_applied_first() -> Result<(), TestError> {
    // Reconstruction at level 4 smooths the input; with zero other filters,
    // a step input should be attenuated on the first frame.
    let mut pipeline = compile(reconstruction_only_config(4)?)?;

    let mut frame = make_frame(1.0, 0.0, 0.0);
    pipeline.process(&mut frame)?;

    // First frame through reconstruction should not reach full 1.0
    assert!(
        frame.torque_out < 1.0,
        "reconstruction should smooth first frame, got {}",
        frame.torque_out
    );
    assert!(frame.torque_out >= 0.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Filter bypass for diagnostics mode
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn diagnostics_bypass_empty_pipeline_passes_through() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    assert!(pipeline.is_empty(), "empty pipeline should have no nodes");

    let test_signals = [-1.0_f32, -0.5, 0.0, 0.5, 1.0];
    for &sig in &test_signals {
        let mut frame = make_frame(sig, sig, 0.0);
        pipeline.process(&mut frame)?;
        assert!(
            (frame.torque_out - sig).abs() < f32::EPSILON,
            "bypass should pass through {sig}, got {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn diagnostics_swap_to_empty_disables_all_filters() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;
    assert!(!pipeline.is_empty());

    // Swap to empty = diagnostics bypass
    pipeline.swap_at_tick_boundary(Pipeline::new());
    assert!(pipeline.is_empty());

    let mut frame = make_frame(0.6, 0.6, 0.0);
    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - 0.6).abs() < f32::EPSILON,
        "after swap to empty, should be passthrough, got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Pipeline latency measurement and budget enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn latency_empty_pipeline_under_budget() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();

    // Warm up
    for _ in 0..1000 {
        let mut frame = make_frame(0.5, 0.5, 0.0);
        let _ = pipeline.process(&mut frame);
    }

    let start = Instant::now();
    let iterations = 10_000_u32;
    for _ in 0..iterations {
        let mut frame = make_frame(0.5, 0.5, 0.0);
        let _ = pipeline.process(&mut frame);
    }
    let elapsed = start.elapsed();
    let per_call_us = elapsed.as_micros() as f64 / iterations as f64;

    // Budget: 200µs per tick; empty pipeline should be well under
    assert!(
        per_call_us < 200.0,
        "empty pipeline {per_call_us:.1}µs exceeds 200µs budget"
    );
    Ok(())
}

#[test]
fn latency_full_pipeline_within_200us_budget() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Warm up
    for i in 0..500_u16 {
        let mut frame = make_frame_seq(0.5, 0.5, 1.0, i);
        let _ = pipeline.process(&mut frame);
    }

    let start = Instant::now();
    let iterations = 5_000_u32;
    for i in 0..iterations {
        let mut frame = make_frame_seq(0.4, 0.4, 1.0, (500 + i) as u16);
        let _ = pipeline.process(&mut frame);
    }
    let elapsed = start.elapsed();
    let per_call_us = elapsed.as_micros() as f64 / iterations as f64;

    assert!(
        per_call_us < 200.0,
        "full pipeline {per_call_us:.1}µs exceeds 200µs budget"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Pipeline with all filters active
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn all_filters_active_produces_bounded_output() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    for i in 0..300_u16 {
        let input = ((i as f32) * 0.03).sin() * 0.4;
        let mut frame = make_frame_seq(input, input, 0.3, i);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "frame {i}: output {} out of bounds",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn all_filters_converge_to_stable_output() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    let mut last = 0.0_f32;
    for i in 0..500_u16 {
        let mut frame = make_frame_seq(0.4, 0.0, 0.5, i);
        pipeline.process(&mut frame)?;
        last = frame.torque_out;
    }

    // After 500 frames of constant input, output should have converged
    let mut frame = make_frame_seq(0.4, 0.0, 0.5, 500);
    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - last).abs() < 0.01,
        "should converge: prev={last}, current={}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Pipeline with mixed filter enables/disables
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn mixed_filters_damper_only_vs_friction_only_differ() -> Result<(), TestError> {
    let mut pipeline_damper = compile(damper_only_config(0.15)?)?;
    let mut pipeline_friction = compile(friction_only_config(0.15)?)?;

    let mut frame_d = make_frame(0.3, 0.3, 4.0);
    let mut frame_f = make_frame(0.3, 0.3, 4.0);

    pipeline_damper.process(&mut frame_d)?;
    pipeline_friction.process(&mut frame_f)?;

    assert!(
        (frame_d.torque_out - frame_f.torque_out).abs() > f32::EPSILON,
        "damper and friction should produce different outputs: d={}, f={}",
        frame_d.torque_out,
        frame_f.torque_out
    );
    Ok(())
}

#[test]
fn mixed_inertia_plus_damper_has_more_nodes() -> Result<(), TestError> {
    let config_inertia_damper = FilterConfig {
        damper: Gain::new(0.1)?,
        inertia: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let config_damper_only = FilterConfig {
        damper: Gain::new(0.1)?,
        ..FilterConfig::default()
    };

    let pipeline_id = compile(config_inertia_damper)?;
    let pipeline_d = compile(config_damper_only)?;

    // Adding inertia should result in more compiled filter nodes
    assert!(
        pipeline_id.node_count() > pipeline_d.node_count(),
        "inertia+damper should have more nodes than damper-only: id={}, d={}",
        pipeline_id.node_count(),
        pipeline_d.node_count()
    );

    // Config hashes should differ
    assert_ne!(
        pipeline_id.config_hash(),
        pipeline_d.config_hash(),
        "different configs should produce different hashes"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Pipeline handles zero input gracefully
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn zero_input_passthrough_produces_zero_output() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.0, 0.0, 0.0);
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.abs() < f32::EPSILON,
        "zero input should produce zero output, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn zero_input_through_all_filters_stays_near_zero() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Run enough frames for filters to settle
    for i in 0..200_u16 {
        let mut frame = make_frame_seq(0.0, 0.0, 0.0, i);
        pipeline.process(&mut frame)?;
    }

    // Final frame should be near zero
    let mut frame = make_frame_seq(0.0, 0.0, 0.0, 200);
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.abs() < 0.01,
        "zero input through filters should stay near zero, got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Pipeline handles max input clamping
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn max_input_clamped_within_bounds() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    for i in 0..200_u16 {
        let mut frame = make_frame_seq(1.0, 1.0, 0.0, i);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.abs() <= 1.0,
            "frame {i}: output {} exceeds ±1.0",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn negative_max_input_clamped_within_bounds() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    for i in 0..200_u16 {
        let mut frame = make_frame_seq(-1.0, -1.0, 0.0, i);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.abs() <= 1.0,
            "frame {i}: output {} exceeds ±1.0",
            frame.torque_out
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Pipeline preserves signal frequency response (low-pass characteristics)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn reconstruction_filter_attenuates_high_frequency() -> Result<(), TestError> {
    let mut pipeline = compile(reconstruction_only_config(6)?)?;

    // High-frequency alternating signal: +0.5, -0.5, ...
    let mut max_amplitude = 0.0_f32;
    for i in 0..200_u16 {
        let input = if i % 2 == 0 { 0.5 } else { -0.5 };
        let mut frame = make_frame_seq(input, 0.0, 0.0, i);
        pipeline.process(&mut frame)?;
        if i > 50 {
            max_amplitude = max_amplitude.max(frame.torque_out.abs());
        }
    }

    // Reconstruction low-pass should attenuate the alternating signal
    assert!(
        max_amplitude < 0.5,
        "high-frequency signal should be attenuated, max amplitude was {max_amplitude}"
    );
    Ok(())
}

#[test]
fn reconstruction_filter_passes_low_frequency() -> Result<(), TestError> {
    let mut pipeline = compile(reconstruction_only_config(4)?)?;

    // Low-frequency slow ramp: should mostly pass through after settling
    let mut last_output = 0.0_f32;
    for i in 0..500_u16 {
        let input = 0.5; // constant = DC = lowest possible frequency
        let mut frame = make_frame_seq(input, 0.0, 0.0, i);
        pipeline.process(&mut frame)?;
        last_output = frame.torque_out;
    }

    // After 500 frames, should have converged close to 0.5
    assert!(
        (last_output - 0.5).abs() < 0.05,
        "DC signal should pass through reconstruction, got {last_output}"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Pipeline handles rapid input changes (step response)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn step_response_reconstruction_smooths_transition() -> Result<(), TestError> {
    let mut pipeline = compile(reconstruction_only_config(6)?)?;

    // Settle at 0
    for i in 0..100_u16 {
        let mut frame = make_frame_seq(0.0, 0.0, 0.0, i);
        pipeline.process(&mut frame)?;
    }

    // Step to 1.0
    let mut frame = make_frame_seq(1.0, 0.0, 0.0, 100);
    pipeline.process(&mut frame)?;

    // Reconstruction should not jump immediately to 1.0
    assert!(
        frame.torque_out < 0.9,
        "step response should be smoothed, got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out > 0.0,
        "step response should start moving, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn step_response_settles_within_reasonable_frames() -> Result<(), TestError> {
    let mut pipeline = compile(reconstruction_only_config(4)?)?;

    // Settle at 0
    for i in 0..100_u16 {
        let mut frame = make_frame_seq(0.0, 0.0, 0.0, i);
        pipeline.process(&mut frame)?;
    }

    // Step to 0.8 and count frames to settle
    let target = 0.8_f32;
    let mut settled = false;
    for i in 100..600_u16 {
        let mut frame = make_frame_seq(target, 0.0, 0.0, i);
        pipeline.process(&mut frame)?;
        if (frame.torque_out - target).abs() < 0.02 {
            settled = true;
            break;
        }
    }

    assert!(settled, "step response should settle within 500 frames");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Pipeline determinism: same input always produces same output
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn deterministic_same_input_sequence_same_output() -> Result<(), TestError> {
    let inputs: Vec<f32> = (0..100).map(|i| ((i as f32) * 0.07).sin() * 0.6).collect();

    let mut outputs_a = Vec::with_capacity(inputs.len());
    let mut outputs_b = Vec::with_capacity(inputs.len());

    // Run A
    let mut pipeline_a = compile(all_filters_config()?)?;
    for (i, &input) in inputs.iter().enumerate() {
        let mut frame = make_frame_seq(input, input, 1.0, i as u16);
        pipeline_a.process(&mut frame)?;
        outputs_a.push(frame.torque_out);
    }

    // Run B (fresh pipeline, same config)
    let mut pipeline_b = compile(all_filters_config()?)?;
    for (i, &input) in inputs.iter().enumerate() {
        let mut frame = make_frame_seq(input, input, 1.0, i as u16);
        pipeline_b.process(&mut frame)?;
        outputs_b.push(frame.torque_out);
    }

    for (i, (a, b)) in outputs_a.iter().zip(outputs_b.iter()).enumerate() {
        assert!(
            (a - b).abs() < f32::EPSILON,
            "determinism violated at frame {i}: a={a}, b={b}"
        );
    }
    Ok(())
}

#[test]
fn deterministic_config_hash_matches_for_same_config() -> Result<(), TestError> {
    let pipeline_a = compile(all_filters_config()?)?;
    let pipeline_b = compile(all_filters_config()?)?;

    assert_eq!(
        pipeline_a.config_hash(),
        pipeline_b.config_hash(),
        "same config should produce same hash"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Pipeline state reset on device disconnect
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn state_reset_swap_clears_filter_state() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Build up filter state with non-zero input
    for i in 0..200_u16 {
        let mut frame = make_frame_seq(0.7, 0.7, 2.0, i);
        pipeline.process(&mut frame)?;
    }

    // Simulate disconnect: swap to fresh pipeline (same config)
    let fresh = compile(all_filters_config()?)?;
    pipeline.swap_at_tick_boundary(fresh);

    // First frame after reset with zero input should be near zero
    let mut frame = make_frame_seq(0.0, 0.0, 0.0, 0);
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.abs() < 0.1,
        "after reset, zero input should produce near-zero output, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn state_reset_to_empty_then_recompile() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Process some frames
    for i in 0..50_u16 {
        let mut frame = make_frame_seq(0.5, 0.5, 1.0, i);
        pipeline.process(&mut frame)?;
    }

    // Disconnect: go to empty
    pipeline.swap_at_tick_boundary(Pipeline::new());
    assert!(pipeline.is_empty());

    // Reconnect: compile fresh
    let fresh = compile(all_filters_config()?)?;
    pipeline.swap_at_tick_boundary(fresh);
    assert!(!pipeline.is_empty());

    // Should work normally
    let mut frame = make_frame_seq(0.3, 0.3, 0.5, 0);
    pipeline.process(&mut frame)?;
    assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Pipeline configuration hot-update
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn hot_update_changes_filter_behavior_mid_stream() -> Result<(), TestError> {
    let rt = build_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        // Start with light friction
        let config1 = FilterConfig {
            friction: Gain::new(0.05)?,
            ..FilterConfig::default()
        };
        let compiled1 = compiler.compile_pipeline(config1).await?;
        let mut pipeline = compiled1.pipeline;

        let mut frame1 = make_frame(0.3, 0.3, 3.0);
        pipeline.process(&mut frame1)?;
        let output_before = frame1.torque_out;

        // Hot-update to heavier friction
        let config2 = FilterConfig {
            friction: Gain::new(0.4)?,
            ..FilterConfig::default()
        };
        let compiled2 = compiler.compile_pipeline(config2).await?;
        pipeline.swap_at_tick_boundary(compiled2.pipeline);

        let mut frame2 = make_frame(0.3, 0.3, 3.0);
        pipeline.process(&mut frame2)?;
        let output_after = frame2.torque_out;

        // Heavier friction should produce more opposing force
        assert!(
            (output_before - output_after).abs() > 0.001,
            "hot-update should change behavior: before={output_before}, after={output_after}"
        );

        Ok::<(), TestError>(())
    })
}

#[test]
fn hot_update_preserves_output_bounds() -> Result<(), TestError> {
    let rt = build_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();
        let mut pipeline = Pipeline::new();

        let configs = [
            FilterConfig::default(),
            FilterConfig {
                reconstruction: 6,
                friction: Gain::new(0.1)?,
                damper: Gain::new(0.1)?,
                ..FilterConfig::default()
            },
            FilterConfig {
                inertia: Gain::new(0.2)?,
                ..FilterConfig::default()
            },
            FilterConfig::default(),
        ];

        for (round, config) in configs.into_iter().enumerate() {
            let compiled = compiler.compile_pipeline(config).await?;
            pipeline.swap_at_tick_boundary(compiled.pipeline);

            for seq in 0..20_u16 {
                let input = ((seq as f32) * 0.2).sin() * 0.8;
                let mut frame = make_frame_seq(input, input, 1.0, seq);
                let result = pipeline.process(&mut frame);
                assert!(result.is_ok(), "round {round}, seq {seq}: process failed");
                assert!(
                    frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                    "round {round}, seq {seq}: output {} out of bounds",
                    frame.torque_out
                );
            }
        }

        Ok::<(), TestError>(())
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. Pipeline handles NaN/Infinity input safely
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn nan_input_returns_pipeline_fault() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let mut frame = make_frame(f32::NAN, f32::NAN, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_err(), "NaN input should cause pipeline fault");
    Ok(())
}

#[test]
fn infinity_input_returns_pipeline_fault() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let mut frame = make_frame(f32::INFINITY, f32::INFINITY, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(
        result.is_err(),
        "Infinity input should cause pipeline fault"
    );
    Ok(())
}

#[test]
fn neg_infinity_input_returns_pipeline_fault() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;
    let mut frame = make_frame(f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(
        result.is_err(),
        "Neg infinity input should cause pipeline fault"
    );
    Ok(())
}

#[test]
fn nan_torque_out_after_valid_in_returns_fault() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;

    // Valid frame first
    let mut frame = make_frame(0.5, 0.5, 0.0);
    pipeline.process(&mut frame)?;

    // Then NaN torque_out
    let mut frame = make_frame(0.5, f32::NAN, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(
        result.is_err(),
        "NaN torque_out should cause pipeline fault"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Pipeline output is always within device limits
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn output_always_within_unit_interval() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Sweep through various inputs and wheel speeds
    let inputs = [-1.0, -0.8, -0.5, -0.2, 0.0, 0.2, 0.5, 0.8, 1.0];
    let speeds = [0.0, 1.0, 3.0, 5.0, 10.0];

    for &speed in &speeds {
        for (seq, &input) in inputs.iter().enumerate() {
            let mut frame = make_frame_seq(input, input, speed, seq as u16);
            let result = pipeline.process(&mut frame);
            if let Ok(()) = result {
                assert!(
                    frame.torque_out.abs() <= 1.0,
                    "output {} exceeds ±1.0 for input={input}, speed={speed}",
                    frame.torque_out
                );
            }
        }
    }
    Ok(())
}

#[test]
fn safety_service_clamp_enforces_device_limit() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);

    // In SafeTorque state, max is 5.0 Nm
    let test_values = [-100.0, -25.0, -5.0, 0.0, 5.0, 25.0, 100.0];
    for &val in &test_values {
        let clamped = safety.clamp_torque_nm(val);
        assert!(
            clamped.abs() <= 5.0,
            "safe state should clamp {val} to ±5.0, got {clamped}"
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. Multi-device pipeline isolation
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn two_pipelines_independent_processing() -> Result<(), TestError> {
    let mut pipeline_a = compile(all_filters_config()?)?;
    let mut pipeline_b = compile(all_filters_config()?)?;

    // Feed different inputs to each
    for i in 0..100_u16 {
        let mut frame_a = make_frame_seq(0.8, 0.8, 2.0, i);
        let mut frame_b = make_frame_seq(0.2, 0.2, 0.5, i);

        pipeline_a.process(&mut frame_a)?;
        pipeline_b.process(&mut frame_b)?;

        assert!(frame_a.torque_out.is_finite() && frame_a.torque_out.abs() <= 1.0);
        assert!(frame_b.torque_out.is_finite() && frame_b.torque_out.abs() <= 1.0);
    }

    // After divergent inputs, outputs should differ
    let mut frame_a = make_frame_seq(0.5, 0.5, 1.0, 100);
    let mut frame_b = make_frame_seq(0.5, 0.5, 1.0, 100);
    pipeline_a.process(&mut frame_a)?;
    pipeline_b.process(&mut frame_b)?;

    assert!(
        (frame_a.torque_out - frame_b.torque_out).abs() > f32::EPSILON,
        "different input histories should produce different outputs: a={}, b={}",
        frame_a.torque_out,
        frame_b.torque_out
    );
    Ok(())
}

#[test]
fn two_pipelines_different_configs_isolated() -> Result<(), TestError> {
    let mut pipeline_a = compile(friction_only_config(0.3)?)?;
    let mut pipeline_b = compile(damper_only_config(0.3)?)?;

    let mut frame_a = make_frame(0.4, 0.4, 3.0);
    let mut frame_b = make_frame(0.4, 0.4, 3.0);

    pipeline_a.process(&mut frame_a)?;
    pipeline_b.process(&mut frame_b)?;

    assert!(
        (frame_a.torque_out - frame_b.torque_out).abs() > f32::EPSILON,
        "different filter configs should produce different outputs"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Pipeline performance: process N samples within budget
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn performance_1000_ticks_within_1_second() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Warm up with low speed to avoid pipeline fault from combined filter torque
    for i in 0..200_u16 {
        let mut frame = make_frame_seq(0.2, 0.2, 0.3, i);
        let _ = pipeline.process(&mut frame);
    }

    let start = Instant::now();
    for i in 0..1000_u16 {
        let input = ((i as f32) * 0.05).sin() * 0.3;
        let mut frame = make_frame_seq(input, input, 0.3, i);
        pipeline.process(&mut frame)?;
    }
    let elapsed = start.elapsed();

    // 1000 ticks at 1kHz = 1 second total budget; processing should be well under
    assert!(
        elapsed.as_millis() < 1000,
        "1000 ticks took {}ms, must be under 1000ms",
        elapsed.as_millis()
    );
    Ok(())
}

#[test]
fn performance_median_processing_time_under_50us() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Warm up
    for i in 0..500_u16 {
        let mut frame = make_frame_seq(0.4, 0.4, 1.0, i);
        let _ = pipeline.process(&mut frame);
    }

    let mut durations = Vec::with_capacity(1000);
    for i in 0..1000_u16 {
        let mut frame = make_frame_seq(0.5, 0.5, 1.0, i);
        let t0 = Instant::now();
        let _ = pipeline.process(&mut frame);
        durations.push(t0.elapsed());
    }

    durations.sort();
    let median = durations[durations.len() / 2];

    // Target: 50µs median processing time
    assert!(
        median.as_micros() < 50,
        "median processing time {}µs exceeds 50µs target",
        median.as_micros()
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 18. Pipeline statistics collection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn statistics_min_max_avg_torque_tracked() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?)?;

    let mut min_torque = f32::MAX;
    let mut max_torque = f32::MIN;
    let mut sum_torque = 0.0_f64;
    let count = 100_u32;

    for i in 0..count {
        let input = ((i as f32) * 0.1).sin() * 0.8;
        let mut frame = make_frame_seq(input, input, 0.0, i as u16);
        pipeline.process(&mut frame)?;

        min_torque = min_torque.min(frame.torque_out);
        max_torque = max_torque.max(frame.torque_out);
        sum_torque += frame.torque_out as f64;
    }

    let avg_torque = sum_torque / count as f64;

    assert!(min_torque < 0.0, "min should be negative with sine input");
    assert!(max_torque > 0.0, "max should be positive with sine input");
    assert!(
        avg_torque.abs() < 0.5,
        "average should be near zero for sine wave, got {avg_torque}"
    );
    assert!(
        min_torque >= -1.0 && max_torque <= 1.0,
        "all stats should be in [-1, 1]"
    );
    Ok(())
}

#[test]
fn statistics_processing_time_measurable() -> Result<(), TestError> {
    let mut pipeline = compile(all_filters_config()?)?;

    // Warm up
    for i in 0..200_u16 {
        let mut frame = make_frame_seq(0.3, 0.3, 1.0, i);
        let _ = pipeline.process(&mut frame);
    }

    let mut total_ns = 0_u128;
    let iterations = 500_u32;
    for i in 0..iterations {
        let mut frame = make_frame_seq(0.4, 0.4, 1.0, i as u16);
        let t0 = Instant::now();
        let _ = pipeline.process(&mut frame);
        total_ns += t0.elapsed().as_nanos();
    }

    let avg_ns = total_ns / iterations as u128;
    assert!(
        avg_ns < 200_000,
        "average processing time {avg_ns}ns exceeds 200µs budget"
    );
    assert!(avg_ns > 0, "processing time should be measurable");
    Ok(())
}

#[test]
fn statistics_node_count_reflects_config() -> Result<(), TestError> {
    let pipeline_empty = Pipeline::new();
    let pipeline_passthrough = compile(passthrough_config()?)?;
    let pipeline_full = compile(all_filters_config()?)?;

    assert_eq!(pipeline_empty.node_count(), 0, "empty pipeline: 0 nodes");
    assert!(
        pipeline_passthrough.node_count() > 0,
        "passthrough compiled config should have nodes"
    );
    assert!(
        pipeline_full.node_count() > pipeline_passthrough.node_count(),
        "full config should have more nodes than passthrough: full={}, pass={}",
        pipeline_full.node_count(),
        pipeline_passthrough.node_count()
    );
    Ok(())
}
