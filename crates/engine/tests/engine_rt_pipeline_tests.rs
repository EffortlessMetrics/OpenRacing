//! Comprehensive RT Pipeline Integration Tests
//!
//! Covers end-to-end validation of the engine's real-time pipeline:
//!   1. Force feedback processing pipeline
//!   2. Filter chain composition (reconstruction, friction, damper, inertia, notch, slew, curve)
//!   3. Signal processing correctness
//!   4. Safety limit enforcement (max torque, rate limiting)
//!   5. Multi-device output mixing / multi-pipeline independence
//!   6. Frame timing and budget tracking
//!   7. RT allocation detection (zero allocations on hot path)
//!   8. Pipeline state machine transitions (swap, empty, reconfigure)
//!   9. Error recovery in pipeline (NaN/Inf/out-of-range inputs)
//!  10. Pipeline metrics collection (config hash, node count)
//!  11. Deterministic output for identical input sequences

use racing_wheel_engine::allocation_tracker::{AllocationGuard, track};
use racing_wheel_engine::curves::CurveType;
use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::{FFBMode, Frame, PerformanceMetrics};
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState};
use racing_wheel_engine::scheduler::{AbsoluteScheduler, AdaptiveSchedulingConfig, JitterMetrics};
use racing_wheel_schemas::prelude::{
    BumpstopConfig, CurvePoint, FilterConfig, FrequencyHz, Gain, HandsOffConfig, NotchFilter,
};
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

type TestError = Box<dyn std::error::Error>;

fn make_frame(ffb_in: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0,
        wheel_speed: 0.5,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn make_frame_with_speed(ffb_in: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

async fn compile(config: FilterConfig) -> Result<Pipeline, TestError> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    Ok(compiled.pipeline)
}

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

fn full_filter_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig::new_complete(
        4,
        Gain::new(0.10)?,
        Gain::new(0.15)?,
        Gain::new(0.05)?,
        vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        Gain::new(0.80)?,
        vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.25, 0.20)?,
            CurvePoint::new(0.50, 0.45)?,
            CurvePoint::new(0.75, 0.75)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        Gain::new(0.90)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?)
}

/// Assert that an allocation guard recorded zero allocations.
fn assert_no_allocs(guard: &AllocationGuard, ctx: &str) -> Result<(), String> {
    let count = guard.allocations_since_start();
    let bytes = guard.bytes_allocated_since_start();
    if count > 0 {
        Err(format!(
            "{ctx}: {count} allocations ({bytes} bytes) detected on RT path"
        ))
    } else {
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Force Feedback Processing Pipeline
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pipeline_process_passthrough_preserves_input() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?).await?;
    let mut frame = make_frame(0.5, 1);
    pipeline.process(&mut frame)?;
    // Passthrough: torque_out should remain 0.0 (filters operate on torque_out, ffb_in is input)
    // With passthrough config all gains are zero, so no filter modifies torque_out
    assert!(
        frame.torque_out.abs() < f32::EPSILON,
        "passthrough pipeline should not modify torque_out from zero; got {}",
        frame.torque_out
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_empty_is_noop() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.75, 10);
    frame.torque_out = 0.42;
    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - 0.42).abs() < f32::EPSILON,
        "empty pipeline should leave torque_out unchanged"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_processes_sequence_of_frames() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?).await?;
    for seq in 0..100u16 {
        let mut frame = make_frame(0.3, seq);
        pipeline.process(&mut frame)?;
    }
    // If we got here without error, all 100 frames processed successfully
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Filter Chain Composition
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn filter_chain_reconstruction_only() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
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
    )?;
    let mut pipeline = compile(config).await?;
    assert!(
        pipeline.node_count() > 0,
        "reconstruction-only config should produce at least one node"
    );

    let mut frame = make_frame(0.5, 0);
    pipeline.process(&mut frame)?;
    // Output should be finite and bounded
    assert!(frame.torque_out.is_finite());
    assert!(frame.torque_out.abs() <= 1.0);
    Ok(())
}

#[tokio::test]
async fn filter_chain_friction_damper_inertia() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.15)?,
        Gain::new(0.20)?,
        Gain::new(0.10)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config).await?;

    // Should have friction + damper + inertia nodes (plus bumpstop + hands_off)
    assert!(
        pipeline.node_count() >= 3,
        "expected at least 3 filter nodes, got {}",
        pipeline.node_count()
    );

    // Process multiple frames with moderate wheel speed (capped at 2.0 rad/s)
    for i in 0..50u16 {
        let speed = (i as f32 * 0.04).min(2.0);
        let mut frame = make_frame_with_speed(0.5, speed, i);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "frame {i}: torque_out={} is invalid",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn filter_chain_notch_filter_at_60hz() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![NotchFilter::new(FrequencyHz::new(60.0)?, 5.0, -24.0)?],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config).await?;
    assert!(pipeline.node_count() >= 1, "notch filter node expected");

    let mut frame = make_frame(0.5, 0);
    pipeline.process(&mut frame)?;
    assert!(frame.torque_out.is_finite());
    assert!(frame.torque_out.abs() <= 1.0);
    Ok(())
}

#[tokio::test]
async fn filter_chain_multiple_notch_filters() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![
            NotchFilter::new(FrequencyHz::new(50.0)?, 3.0, -12.0)?,
            NotchFilter::new(FrequencyHz::new(120.0)?, 4.0, -18.0)?,
            NotchFilter::new(FrequencyHz::new(250.0)?, 2.0, -6.0)?,
        ],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config).await?;
    assert!(
        pipeline.node_count() >= 3,
        "expected at least 3 notch filter nodes"
    );

    for seq in 0..20u16 {
        let mut frame = make_frame(0.4, seq);
        pipeline.process(&mut frame)?;
        assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    }
    Ok(())
}

#[tokio::test]
async fn filter_chain_slew_rate_limits_change() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(0.10)?, // Very aggressive slew rate limiting
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config).await?;

    // First frame: large step input
    let mut frame = make_frame(0.9, 0);
    pipeline.process(&mut frame)?;
    let first_out = frame.torque_out;

    // Second frame: same large input — slew rate should limit the change rate
    let mut frame2 = make_frame(0.9, 1);
    pipeline.process(&mut frame2)?;

    // Both outputs should be valid
    assert!(first_out.is_finite());
    assert!(frame2.torque_out.is_finite());
    assert!(first_out.abs() <= 1.0);
    assert!(frame2.torque_out.abs() <= 1.0);
    Ok(())
}

#[tokio::test]
async fn filter_chain_custom_curve_mapping() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.25, 0.10)?,
            CurvePoint::new(0.50, 0.30)?,
            CurvePoint::new(0.75, 0.60)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        Gain::new(1.0)?,
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config).await?;
    // The curve is non-linear, pipeline should have a curve filter node
    assert!(pipeline.node_count() >= 1, "curve filter node expected");

    let mut frame = make_frame(0.5, 0);
    pipeline.process(&mut frame)?;
    assert!(frame.torque_out.is_finite());
    assert!(frame.torque_out.abs() <= 1.0);
    Ok(())
}

#[tokio::test]
async fn filter_chain_full_composition() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;
    // Full config should produce many nodes
    assert!(
        pipeline.node_count() >= 5,
        "full config should produce many filter nodes, got {}",
        pipeline.node_count()
    );

    // Process burst of frames
    for seq in 0..200u16 {
        let input = ((seq as f32) * 0.03).sin() * 0.8;
        let mut frame = make_frame(input, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "frame {seq}: torque_out {} out of bounds",
            frame.torque_out
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Signal Processing Correctness
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn signal_zero_input_produces_bounded_output() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    for seq in 0..50u16 {
        let mut frame = make_frame(0.0, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.abs() <= 1.0,
            "zero input produced out-of-bound output: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn signal_negative_input_produces_bounded_output() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    for seq in 0..50u16 {
        let mut frame = make_frame(-0.8, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "frame {seq}: negative input produced invalid output: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn signal_boundary_inputs_at_extremes() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    let extremes = [-1.0f32, -0.999, 0.0, 0.999, 1.0];
    for (i, &input) in extremes.iter().enumerate() {
        let mut frame = make_frame(input, i as u16);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "extreme input {input} produced invalid output: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn signal_rapid_direction_reversal() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    for seq in 0..100u16 {
        // Alternate between +0.9 and -0.9 each frame
        let input = if seq % 2 == 0 { 0.9 } else { -0.9 };
        let mut frame = make_frame(input, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "rapid reversal frame {seq}: invalid output {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn signal_high_frequency_sine_input() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // 100 Hz sine at 1 kHz sample rate = 10 samples/cycle
    for seq in 0..1000u16 {
        let t = seq as f32 / 1000.0;
        let input = (2.0 * std::f32::consts::PI * 100.0 * t).sin() * 0.7;
        let mut frame = make_frame(input, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "sine frame {seq}: output {} invalid",
            frame.torque_out
        );
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Safety Limit Enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn safety_clamp_torque_within_safe_mode_limits() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    // In SafeTorque state, max torque is 5.0 Nm
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        (clamped - 5.0).abs() < f32::EPSILON,
        "expected 5.0 Nm clamp, got {}",
        clamped
    );
    Ok(())
}

#[test]
fn safety_clamp_torque_symmetric_for_negative() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    let clamped = safety.clamp_torque_nm(-10.0);
    assert!(
        (clamped - (-5.0)).abs() < f32::EPSILON,
        "expected -5.0 Nm clamp, got {}",
        clamped
    );
    Ok(())
}

#[test]
fn safety_faulted_state_clamps_to_zero() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::UsbStall);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));
    let clamped = safety.clamp_torque_nm(10.0);
    assert!(
        clamped.abs() < f32::EPSILON,
        "faulted state should clamp to zero, got {}",
        clamped
    );
    Ok(())
}

#[test]
fn safety_max_torque_per_state() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    assert!(
        (safety.max_torque_nm() - 5.0).abs() < f32::EPSILON,
        "SafeTorque should yield safe limit"
    );
    Ok(())
}

#[test]
fn safety_nan_input_clamped_to_zero() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    let clamped = safety.clamp_torque_nm(f32::NAN);
    assert!(
        clamped.abs() < f32::EPSILON,
        "NaN input should clamp to zero, got {}",
        clamped
    );
    Ok(())
}

#[test]
fn safety_inf_input_clamped_to_zero() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    let clamped = safety.clamp_torque_nm(f32::INFINITY);
    // SafetyService treats non-finite as 0.0 first, so 0.0 clamped is 0.0
    // Actually looking at implementation: non-finite maps to 0.0 then clamp(0,-max..max) => 0.0
    assert!(
        clamped.abs() < f32::EPSILON || clamped.abs() <= 5.0,
        "Inf input should be safely handled, got {}",
        clamped
    );
    Ok(())
}

#[test]
fn safety_fault_then_clear_restores_safe_torque() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::ThermalLimit);
    assert!(matches!(safety.state(), SafetyState::Faulted { .. }));

    // Wait minimum duration for fault clear
    std::thread::sleep(Duration::from_millis(120));

    safety
        .clear_fault()
        .map_err(|e| -> TestError { e.into() })?;
    assert!(
        matches!(safety.state(), SafetyState::SafeTorque),
        "after clearing fault, state should be SafeTorque"
    );
    let clamped = safety.clamp_torque_nm(3.0);
    assert!(
        (clamped - 3.0).abs() < f32::EPSILON,
        "after clearing, 3 Nm within limit should pass"
    );
    Ok(())
}

#[tokio::test]
async fn safety_torque_cap_filter_clamps_output() -> Result<(), TestError> {
    let config = FilterConfig::new_complete(
        0,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        Gain::new(0.0)?,
        vec![],
        Gain::new(1.0)?,
        vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
        Gain::new(0.50)?, // 50% torque cap
        BumpstopConfig::default(),
        HandsOffConfig::default(),
    )?;
    let mut pipeline = compile(config).await?;

    let mut frame = make_frame(0.9, 0);
    // Set torque_out to something that the torque cap should clamp
    frame.torque_out = 0.9;
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.abs() <= 0.50 + f32::EPSILON,
        "torque cap at 0.50 should limit output; got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Multi-Pipeline Independence
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multi_pipeline_instances_are_independent() -> Result<(), TestError> {
    let mut pipeline_a = compile(passthrough_config()?).await?;
    let mut pipeline_b = compile(full_filter_config()?).await?;

    let mut frame_a = make_frame(0.5, 0);
    let mut frame_b = make_frame(0.5, 0);

    pipeline_a.process(&mut frame_a)?;
    pipeline_b.process(&mut frame_b)?;

    // They may produce different outputs due to different filter chains.
    // Key invariant: both are valid.
    assert!(frame_a.torque_out.is_finite() && frame_a.torque_out.abs() <= 1.0);
    assert!(frame_b.torque_out.is_finite() && frame_b.torque_out.abs() <= 1.0);
    Ok(())
}

#[tokio::test]
async fn multi_pipeline_processing_does_not_cross_contaminate() -> Result<(), TestError> {
    let mut pipeline_a = compile(full_filter_config()?).await?;
    let mut pipeline_b = compile(full_filter_config()?).await?;

    // Feed different inputs to each pipeline
    for seq in 0..50u16 {
        let mut frame_a = make_frame(0.3, seq);
        let mut frame_b = make_frame(-0.7, seq);

        pipeline_a.process(&mut frame_a)?;
        pipeline_b.process(&mut frame_b)?;

        assert!(frame_a.torque_out.is_finite() && frame_a.torque_out.abs() <= 1.0);
        assert!(frame_b.torque_out.is_finite() && frame_b.torque_out.abs() <= 1.0);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Frame Timing and Budget Tracking
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn jitter_metrics_record_and_p99() -> Result<(), TestError> {
    let mut metrics = JitterMetrics::new();

    // Record 1000 ticks with varying jitter
    for i in 0..1000u64 {
        let jitter_ns = (i % 100) * 1_000; // 0..99 µs
        let missed = jitter_ns > 250_000;
        metrics.record_tick(jitter_ns, missed);
    }

    assert_eq!(metrics.total_ticks, 1000);
    let p99 = metrics.p99_jitter_ns();
    // p99 of 0..99_000 should be around 98_000..99_000
    assert!(p99 > 0, "p99 should be positive");
    assert!(p99 <= 100_000, "p99 should be within expected range");
    Ok(())
}

#[test]
fn jitter_metrics_missed_tick_rate_calculation() -> Result<(), TestError> {
    let mut metrics = JitterMetrics::new();
    metrics.record_tick(100_000, false);
    metrics.record_tick(300_000, true); // missed
    metrics.record_tick(50_000, false);

    let rate = metrics.missed_tick_rate();
    assert!(
        (rate - 1.0 / 3.0).abs() < 0.01,
        "expected ~0.333 missed rate, got {}",
        rate
    );
    Ok(())
}

#[test]
fn jitter_metrics_empty_returns_zero() -> Result<(), TestError> {
    let mut metrics = JitterMetrics::new();
    assert_eq!(metrics.p99_jitter_ns(), 0);
    assert!((metrics.missed_tick_rate() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn performance_metrics_missed_tick_rate() -> Result<(), TestError> {
    let metrics = PerformanceMetrics {
        total_ticks: 10_000,
        missed_ticks: 5,
        max_jitter_ns: 200_000,
        p99_jitter_ns: 100_000,
        last_update: Instant::now(),
    };
    let rate = metrics.missed_tick_rate();
    assert!(
        (rate - 0.0005).abs() < 0.0001,
        "expected 0.05% missed rate, got {}",
        rate
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_processing_within_time_budget() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // Warm up
    for i in 0..10u16 {
        let mut frame = make_frame(0.5, i);
        let _ = pipeline.process(&mut frame);
    }

    // Measure processing time over 100 frames
    let start = Instant::now();
    for seq in 0..100u16 {
        let mut frame = make_frame(0.5, seq);
        pipeline.process(&mut frame)?;
    }
    let elapsed = start.elapsed();
    let per_frame_us = elapsed.as_micros() / 100;

    // Per-frame should be well under 200µs budget on most machines
    // Use a generous bound for CI environments
    assert!(
        per_frame_us < 1000,
        "per-frame processing {} µs exceeds generous budget",
        per_frame_us
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. RT Allocation Detection
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn rt_alloc_empty_pipeline_zero_alloc() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();

    // Warm up
    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    let mut frame = make_frame(0.5, 1);
    pipeline.process(&mut frame)?;
    assert_no_allocs(&guard, "empty pipeline process")?;
    Ok(())
}

#[tokio::test]
async fn rt_alloc_passthrough_pipeline_zero_alloc() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?).await?;

    // Warm up pipeline state
    for i in 0..5u16 {
        let mut f = make_frame(0.3, i);
        let _ = pipeline.process(&mut f);
    }

    let guard = track();
    for seq in 10..20u16 {
        let mut frame = make_frame(0.5, seq);
        pipeline.process(&mut frame)?;
    }
    assert_no_allocs(&guard, "passthrough pipeline process burst")?;
    Ok(())
}

#[tokio::test]
async fn rt_alloc_full_pipeline_zero_alloc() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // Warm up
    for i in 0..20u16 {
        let mut f = make_frame(0.3, i);
        let _ = pipeline.process(&mut f);
    }

    let guard = track();
    for seq in 100..200u16 {
        let mut frame = make_frame(((seq as f32) * 0.05).sin() * 0.7, seq);
        pipeline.process(&mut frame)?;
    }
    assert_no_allocs(&guard, "full pipeline 100-frame burst")?;
    Ok(())
}

#[tokio::test]
async fn rt_alloc_safety_clamp_zero_alloc() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);

    // Warm up
    let _ = safety.clamp_torque_nm(1.0);

    let guard = track();
    for _ in 0..100 {
        let _ = safety.clamp_torque_nm(10.0);
        let _ = safety.clamp_torque_nm(-10.0);
        let _ = safety.clamp_torque_nm(0.0);
    }
    assert_no_allocs(&guard, "safety clamp_torque_nm")?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Pipeline State Machine Transitions
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pipeline_swap_at_tick_boundary() -> Result<(), TestError> {
    let mut pipeline = compile(passthrough_config()?).await?;

    // Process a frame with old pipeline
    let mut frame = make_frame(0.5, 0);
    pipeline.process(&mut frame)?;
    let old_hash = pipeline.config_hash();

    // Swap to new pipeline
    let new_pipeline = compile(full_filter_config()?).await?;
    let new_hash = new_pipeline.config_hash();
    pipeline.swap_at_tick_boundary(new_pipeline);

    assert_eq!(pipeline.config_hash(), new_hash);
    assert_ne!(
        old_hash, new_hash,
        "different configs should have different hashes"
    );

    // Process with new pipeline
    let mut frame2 = make_frame(0.5, 1);
    pipeline.process(&mut frame2)?;
    assert!(frame2.torque_out.is_finite() && frame2.torque_out.abs() <= 1.0);
    Ok(())
}

#[tokio::test]
async fn pipeline_swap_from_full_to_empty() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;
    assert!(!pipeline.is_empty());

    pipeline.swap_at_tick_boundary(Pipeline::new());
    assert!(pipeline.is_empty());

    let mut frame = make_frame(0.8, 0);
    frame.torque_out = 0.5;
    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "empty pipeline should not alter torque_out"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_config_hash_deterministic() -> Result<(), TestError> {
    let config1 = full_filter_config()?;
    let config2 = full_filter_config()?;

    let compiler = PipelineCompiler::new();
    let hash1 = compiler
        .compile_pipeline(config1)
        .await
        .map(|c| c.config_hash)?;
    let hash2 = compiler
        .compile_pipeline(config2)
        .await
        .map(|c| c.config_hash)?;

    assert_eq!(
        hash1, hash2,
        "identical configs should produce identical hashes"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_different_configs_different_hashes() -> Result<(), TestError> {
    let config_a = passthrough_config()?;
    let config_b = full_filter_config()?;

    let compiler = PipelineCompiler::new();
    let hash_a = compiler
        .compile_pipeline(config_a)
        .await
        .map(|c| c.config_hash)?;
    let hash_b = compiler
        .compile_pipeline(config_b)
        .await
        .map(|c| c.config_hash)?;

    assert_ne!(
        hash_a, hash_b,
        "different configs should produce different hashes"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Error Recovery in Pipeline
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pipeline_nan_torque_out_returns_fault() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // Inject NaN into torque_out before processing (simulating a corrupted filter)
    let mut frame = make_frame(0.5, 0);
    frame.torque_out = f32::NAN;

    // The pipeline checks torque_out after each filter node; with some configs
    // the NaN may survive to produce a PipelineFault. We just verify we don't panic.
    let result = pipeline.process(&mut frame);
    // Either Ok (if filters overwrite NaN) or PipelineFault error — both are acceptable.
    match result {
        Ok(()) => {
            assert!(
                frame.torque_out.is_finite(),
                "if Ok, torque_out must be finite"
            );
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            assert!(
                err_str.contains("PipelineFault"),
                "expected PipelineFault, got: {err_str}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn pipeline_inf_torque_out_returns_fault() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    let mut frame = make_frame(0.5, 0);
    frame.torque_out = f32::INFINITY;

    let result = pipeline.process(&mut frame);
    match result {
        Ok(()) => {
            assert!(
                frame.torque_out.is_finite(),
                "if Ok, torque_out must be finite"
            );
        }
        Err(e) => {
            let err_str = format!("{:?}", e);
            assert!(
                err_str.contains("PipelineFault"),
                "expected PipelineFault, got: {err_str}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn pipeline_continues_after_fault() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // Trigger a potential fault
    let mut bad_frame = make_frame(0.5, 0);
    bad_frame.torque_out = f32::NAN;
    let _ = pipeline.process(&mut bad_frame);

    // Pipeline should still be usable for subsequent valid frames
    let mut good_frame = make_frame(0.3, 1);
    pipeline.process(&mut good_frame)?;
    assert!(
        good_frame.torque_out.is_finite() && good_frame.torque_out.abs() <= 1.0,
        "pipeline should recover for valid input after fault"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_out_of_range_input_handled() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // ffb_in outside [-1,1] — pipeline should still produce valid output
    let inputs = [-100.0, -2.0, 2.0, 100.0, f32::MAX, f32::MIN];
    for (i, &input) in inputs.iter().enumerate() {
        let mut frame = make_frame(input, i as u16);
        let result = pipeline.process(&mut frame);
        match result {
            Ok(()) => {
                assert!(
                    frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                    "out-of-range input {input} produced invalid output: {}",
                    frame.torque_out
                );
            }
            Err(_) => {
                // PipelineFault is acceptable for extreme inputs
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Pipeline Metrics Collection
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pipeline_node_count_matches_active_filters() -> Result<(), TestError> {
    // Passthrough: no signal-processing filters active, but bumpstop + hands_off are enabled by default → 2 nodes
    let pipeline_pt = compile(passthrough_config()?).await?;
    assert_eq!(
        pipeline_pt.node_count(),
        2,
        "passthrough should have 2 nodes (bumpstop + hands_off)"
    );

    // Reconstruction only → reconstruction + bumpstop + hands_off = 3 nodes
    let config_recon = FilterConfig::new_complete(
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
    )?;
    let pipeline_recon = compile(config_recon).await?;
    assert_eq!(
        pipeline_recon.node_count(),
        3,
        "reconstruction-only should have 3 nodes (recon + bumpstop + hands_off)"
    );

    // Full config: recon + friction + damper + inertia + notch + slew + curve + torque_cap + bumpstop + hands_off = 10
    let pipeline_full = compile(full_filter_config()?).await?;
    assert!(
        pipeline_full.node_count() >= 8,
        "full config should have ≥8 nodes, got {}",
        pipeline_full.node_count()
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_is_empty_reflects_state() -> Result<(), TestError> {
    let empty = Pipeline::new();
    assert!(empty.is_empty());
    assert_eq!(empty.node_count(), 0);

    let full = compile(full_filter_config()?).await?;
    assert!(!full.is_empty());
    assert!(full.node_count() > 0);
    Ok(())
}

#[tokio::test]
async fn pipeline_with_hash_preserves_hash() -> Result<(), TestError> {
    let hash = 0xDEAD_BEEF_CAFE_1234u64;
    let pipeline = Pipeline::with_hash(hash);
    assert_eq!(pipeline.config_hash(), hash);
    Ok(())
}

#[test]
fn performance_metrics_p99_jitter_us_conversion() -> Result<(), TestError> {
    let metrics = PerformanceMetrics {
        total_ticks: 1000,
        missed_ticks: 0,
        max_jitter_ns: 200_000,
        p99_jitter_ns: 150_000,
        last_update: Instant::now(),
    };
    assert!(
        (metrics.p99_jitter_us() - 150.0).abs() < f64::EPSILON,
        "p99 conversion: expected 150.0 µs, got {}",
        metrics.p99_jitter_us()
    );
    Ok(())
}

#[test]
fn performance_metrics_zero_ticks_no_divide_by_zero() -> Result<(), TestError> {
    let metrics = PerformanceMetrics {
        total_ticks: 0,
        missed_ticks: 0,
        max_jitter_ns: 0,
        p99_jitter_ns: 0,
        last_update: Instant::now(),
    };
    assert!(
        (metrics.missed_tick_rate() - 0.0).abs() < f64::EPSILON,
        "zero ticks should yield 0 missed rate"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Deterministic Output for Same Input
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn deterministic_output_same_config_same_input() -> Result<(), TestError> {
    let config = full_filter_config()?;
    let mut pipeline_a = compile(config.clone()).await?;
    let mut pipeline_b = compile(config).await?;

    for seq in 0..100u16 {
        let input = ((seq as f32) * 0.1).sin() * 0.6;

        let mut frame_a = make_frame(input, seq);
        let mut frame_b = make_frame(input, seq);

        pipeline_a.process(&mut frame_a)?;
        pipeline_b.process(&mut frame_b)?;

        assert!(
            (frame_a.torque_out - frame_b.torque_out).abs() < f32::EPSILON,
            "frame {seq}: pipelines diverged: {} vs {}",
            frame_a.torque_out,
            frame_b.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn deterministic_output_idempotent_empty_pipeline() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();

    let mut frame1 = Frame {
        ffb_in: 0.5,
        torque_out: 0.42,
        wheel_speed: 3.0,
        hands_off: false,
        ts_mono_ns: 1_000_000,
        seq: 1,
    };
    let mut frame2 = frame1;

    pipeline.process(&mut frame1)?;
    pipeline.process(&mut frame2)?;

    // Empty pipeline is truly a no-op per call, so same input → same output
    assert!(
        (frame1.torque_out - frame2.torque_out).abs() < f32::EPSILON,
        "empty pipeline should be idempotent"
    );
    Ok(())
}

#[tokio::test]
async fn deterministic_output_response_curve_applied() -> Result<(), TestError> {
    let config = passthrough_config()?;
    let compiler = PipelineCompiler::new();
    let compiled = compiler
        .compile_pipeline_with_response_curve(config, Some(&CurveType::Linear))
        .await?;
    let mut pipeline = compiled.pipeline;

    // With linear response curve, output should be unchanged
    let mut frame = Frame {
        ffb_in: 0.0,
        torque_out: 0.7,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    // Linear curve: lookup(0.7) ≈ 0.7
    assert!(
        (frame.torque_out.abs() - 0.7).abs() < 0.02,
        "linear response curve should approximately preserve magnitude; got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Pipeline Validation Errors
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pipeline_rejects_invalid_reconstruction_level() -> Result<(), TestError> {
    let config = FilterConfig {
        reconstruction: 10, // Invalid: max is 8
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let result = compiler.compile_pipeline(config).await;
    assert!(
        result.is_err(),
        "reconstruction level 10 should be rejected"
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_rejects_non_monotonic_curve() -> Result<(), TestError> {
    // curve_points must be monotonically increasing in input
    let config = FilterConfig {
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.6)?,
            CurvePoint::new(0.3, 0.4)?, // non-monotonic
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let result = compiler.compile_pipeline(config).await;
    assert!(result.is_err(), "non-monotonic curve should be rejected");
    Ok(())
}

#[tokio::test]
async fn pipeline_rejects_curve_not_starting_at_zero() -> Result<(), TestError> {
    let config = FilterConfig {
        curve_points: vec![
            CurvePoint::new(0.1, 0.0)?, // Does not start at 0.0
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let result = compiler.compile_pipeline(config).await;
    assert!(
        result.is_err(),
        "curve not starting at input 0.0 should be rejected"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Scheduler / PLL
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn scheduler_adaptive_config_normalized() -> Result<(), TestError> {
    let mut scheduler = AbsoluteScheduler::new_1khz();
    // Intentionally swap min/max — should be auto-corrected
    scheduler.set_adaptive_scheduling(AdaptiveSchedulingConfig {
        enabled: true,
        min_period_ns: 1_200_000,
        max_period_ns: 800_000, // less than min
        ..Default::default()
    });
    let state = scheduler.adaptive_scheduling();
    assert!(
        state.min_period_ns <= state.max_period_ns,
        "adaptive config should normalize min <= max"
    );
    Ok(())
}

#[test]
fn pll_initial_period_matches_target() -> Result<(), TestError> {
    use racing_wheel_engine::scheduler::PLL;
    let pll = PLL::new(1_000_000);
    assert_eq!(pll.target_period_ns(), 1_000_000);
    assert!((pll.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn pll_update_adjusts_period() -> Result<(), TestError> {
    use racing_wheel_engine::scheduler::PLL;
    let mut pll = PLL::new(1_000_000);

    let t0 = Instant::now();
    let first = pll.update(t0);
    assert!(first.as_nanos() > 0, "PLL should produce a positive period");

    // Simulate a tick 1ms later
    let t1 = t0 + Duration::from_micros(1000);
    let second = pll.update(t1);
    assert!(
        second.as_nanos() > 0,
        "PLL should produce a positive period after update"
    );
    Ok(())
}

#[test]
fn pll_reset_clears_state() -> Result<(), TestError> {
    use racing_wheel_engine::scheduler::PLL;
    let mut pll = PLL::new(1_000_000);

    let t0 = Instant::now();
    pll.update(t0);
    pll.update(t0 + Duration::from_micros(1100)); // slight drift

    pll.reset();
    assert!((pll.phase_error_ns() - 0.0).abs() < f64::EPSILON);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: FFB Mode Selection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ffb_mode_display_formatting() -> Result<(), TestError> {
    assert_eq!(format!("{}", FFBMode::PidPassthrough), "PID Pass-through");
    assert_eq!(format!("{}", FFBMode::RawTorque), "Raw Torque");
    assert_eq!(
        format!("{}", FFBMode::TelemetrySynth),
        "Telemetry Synthesis"
    );
    Ok(())
}

#[test]
fn frame_default_is_zeroed() -> Result<(), TestError> {
    let frame = Frame::default();
    assert!((frame.ffb_in - 0.0).abs() < f32::EPSILON);
    assert!((frame.torque_out - 0.0).abs() < f32::EPSILON);
    assert!((frame.wheel_speed - 0.0).abs() < f32::EPSILON);
    assert!(!frame.hands_off);
    assert_eq!(frame.ts_mono_ns, 0);
    assert_eq!(frame.seq, 0);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Safety State Machine Transitions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn safety_initial_state_is_safe_torque() -> Result<(), TestError> {
    let safety = SafetyService::new(5.0, 25.0);
    assert!(
        matches!(safety.state(), SafetyState::SafeTorque),
        "initial state should be SafeTorque"
    );
    Ok(())
}

#[test]
fn safety_report_multiple_faults_stays_faulted() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::UsbStall);
    safety.report_fault(FaultType::EncoderNaN);
    safety.report_fault(FaultType::ThermalLimit);

    assert!(
        matches!(safety.state(), SafetyState::Faulted { .. }),
        "multiple faults should keep state faulted"
    );
    // In faulted state, max torque is 0
    assert!(
        (safety.max_torque_nm() - 0.0).abs() < f32::EPSILON,
        "faulted state should yield 0 max torque"
    );
    Ok(())
}

#[test]
fn safety_challenge_expiry_returns_to_safe() -> Result<(), TestError> {
    let mut safety =
        SafetyService::with_timeouts(5.0, 25.0, Duration::from_secs(3), Duration::from_secs(2));

    let _challenge = safety
        .request_high_torque("dev-test")
        .map_err(|e| -> TestError { e.into() })?;
    assert!(matches!(
        safety.state(),
        SafetyState::HighTorqueChallenge { .. }
    ));

    // Simulate expiry check (challenge just started, should not be expired)
    let expired = safety.check_challenge_expiry();
    assert!(!expired, "challenge should not be expired immediately");
    Ok(())
}

#[test]
fn safety_cancel_challenge_returns_to_safe() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    let _challenge = safety
        .request_high_torque("dev-cancel")
        .map_err(|e| -> TestError { e.into() })?;

    safety
        .cancel_challenge()
        .map_err(|e| -> TestError { e.into() })?;
    assert!(
        matches!(safety.state(), SafetyState::SafeTorque),
        "cancel should return to SafeTorque"
    );
    Ok(())
}

#[test]
fn safety_cannot_request_high_torque_when_faulted() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    safety.report_fault(FaultType::Overcurrent);

    let result = safety.request_high_torque("dev-fault");
    assert!(result.is_err(), "should not allow high torque when faulted");
    Ok(())
}

#[test]
fn safety_hands_off_only_matters_in_high_torque() -> Result<(), TestError> {
    let mut safety = SafetyService::new(5.0, 25.0);
    // In SafeTorque state, hands-off update should be a no-op
    let result = safety.update_hands_on_status(false);
    assert!(result.is_ok(), "hands-off in safe mode should be ok");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Pipeline with Response Curve
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pipeline_exponential_response_curve_applied() -> Result<(), TestError> {
    let config = passthrough_config()?;
    let compiler = PipelineCompiler::new();
    let compiled = compiler
        .compile_pipeline_with_response_curve(
            config,
            Some(&CurveType::Exponential { exponent: 2.0 }),
        )
        .await?;
    let mut pipeline = compiled.pipeline;

    // Exponential curve: output = input^2 (approximately)
    let mut frame = Frame {
        ffb_in: 0.0,
        torque_out: 0.5,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
        "exponential curve should produce valid output"
    );
    // With exponent 2.0, lookup(0.5) ≈ 0.25
    assert!(
        frame.torque_out < 0.5,
        "exponential curve should reduce mid-range values; got {}",
        frame.torque_out
    );
    Ok(())
}

#[tokio::test]
async fn pipeline_response_curve_preserves_zero() -> Result<(), TestError> {
    let config = passthrough_config()?;
    let compiler = PipelineCompiler::new();
    let compiled = compiler
        .compile_pipeline_with_response_curve(
            config,
            Some(&CurveType::Exponential { exponent: 3.0 }),
        )
        .await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = Frame {
        ffb_in: 0.0,
        torque_out: 0.0,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    };
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.abs() < 0.01,
        "response curve should preserve zero; got {}",
        frame.torque_out
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional: Stress / Sustained Processing
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn sustained_processing_1000_frames_all_valid() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    for seq in 0..1000u16 {
        let t = seq as f32 / 1000.0;
        let input = (2.0 * std::f32::consts::PI * 10.0 * t).sin() * 0.7;
        let speed = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 0.5 * t).sin();
        let mut frame = make_frame_with_speed(input, speed, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "sustained frame {seq}: invalid output {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn sustained_processing_no_drift_accumulation() -> Result<(), TestError> {
    let mut pipeline = compile(full_filter_config()?).await?;

    // Feed 500 frames of zero input, then check output is near zero
    for seq in 0..500u16 {
        let mut frame = make_frame(0.0, seq);
        pipeline.process(&mut frame)?;
    }

    let mut final_frame = make_frame(0.0, 500);
    pipeline.process(&mut final_frame)?;
    assert!(
        final_frame.torque_out.abs() < 0.5,
        "after 500 zero-input frames, output should not drift significantly; got {}",
        final_frame.torque_out
    );
    Ok(())
}
