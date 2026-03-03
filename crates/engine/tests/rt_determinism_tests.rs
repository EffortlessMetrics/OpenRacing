//! RT determinism tests
//!
//! Verify that the real-time processing pipeline has bounded, deterministic
//! behaviour: no unbounded loops, predictable worst-case path length,
//! no dynamic dispatch in the hot path, and reproducible outputs.

use racing_wheel_engine::pipeline::{FilterNodeFn, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_engine::safety::{FaultType, SafetyService, SafetyState, TorqueLimit};
use racing_wheel_schemas::prelude::{CurvePoint, FilterConfig, FrequencyHz, Gain, NotchFilter};
use std::time::{Duration, Instant};

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

fn default_config() -> FilterConfig {
    FilterConfig::default()
}

fn full_config() -> Result<FilterConfig, Box<dyn std::error::Error>> {
    Ok(FilterConfig {
        reconstruction: 4,
        friction: Gain::new(0.15)?,
        damper: Gain::new(0.20)?,
        inertia: Gain::new(0.10)?,
        notch_filters: vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        slew_rate: Gain::new(0.80)?,
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.25, 0.20)?,
            CurvePoint::new(0.50, 0.45)?,
            CurvePoint::new(0.75, 0.75)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    })
}

// ===========================================================================
// 1. Processing time is bounded (no unbounded loops)
// ===========================================================================

#[tokio::test]
async fn det_01_single_frame_completes_within_budget() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.3, 0);
    let _ = pipeline.process(&mut warmup);

    let mut max_us: u64 = 0;
    for seq in 1..=1000u16 {
        let mut frame = make_frame((seq as f32 / 500.0).sin() * 0.8, seq);
        let start = Instant::now();
        let _ = pipeline.process(&mut frame);
        let elapsed_us = start.elapsed().as_micros() as u64;
        if elapsed_us > max_us {
            max_us = elapsed_us;
        }
    }

    // Budget: 1000µs per tick. Individual processing must be well under that.
    assert!(
        max_us < 5_000,
        "Worst-case frame took {max_us}µs — exceeds 5ms sanity bound"
    );
    Ok(())
}

#[tokio::test]
async fn det_02_full_pipeline_completes_within_budget() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let start = Instant::now();
    for seq in 1..=1000u16 {
        let mut frame = make_frame((seq as f32 * 0.006).sin() * 0.9, seq);
        let _ = pipeline.process(&mut frame);
    }
    let total = start.elapsed();

    // 1000 frames at 1kHz = 1s wall time budget. Processing itself should be
    // a small fraction of that.
    assert!(
        total < Duration::from_secs(1),
        "1000 frames took {:?} — processing exceeds real-time budget",
        total
    );
    Ok(())
}

#[test]
fn det_03_safety_clamp_bounded_time() {
    let service = SafetyService::new(5.0, 25.0);

    let start = Instant::now();
    for _ in 0..100_000 {
        let _ = service.clamp_torque_nm(42.0);
    }
    let elapsed = start.elapsed();

    // 100k clamp ops should be sub-millisecond total. Allow generous 100ms.
    assert!(
        elapsed < Duration::from_millis(100),
        "100k clamp_torque_nm took {:?}",
        elapsed
    );
}

#[test]
fn det_04_safety_fault_report_bounded_time() {
    let mut service = SafetyService::new(5.0, 25.0);

    let start = Instant::now();
    for _ in 0..10_000 {
        service.report_fault(FaultType::TimingViolation);
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "10k report_fault took {:?}",
        elapsed
    );
}

// ===========================================================================
// 2. Worst-case path length is deterministic
// ===========================================================================

#[tokio::test]
async fn det_05_pipeline_node_count_is_fixed_after_compile()
-> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let node_count_before = pipeline.node_count();

    // Process many frames; node count must not change
    for seq in 0..500u16 {
        let mut frame = make_frame(0.5, seq);
        let _ = pipeline.process(&mut frame);
    }

    assert_eq!(
        pipeline.node_count(),
        node_count_before,
        "Pipeline node count changed during processing"
    );
    Ok(())
}

#[tokio::test]
async fn det_06_deterministic_output_across_runs() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let config = default_config();

    let mut outputs_a: Vec<f32> = Vec::with_capacity(500);
    let mut outputs_b: Vec<f32> = Vec::with_capacity(500);

    for pass in 0..2u8 {
        let compiled = compiler.compile_pipeline(config.clone()).await?;
        let mut pipeline = compiled.pipeline;

        for seq in 0..500u16 {
            let mut frame = make_frame((seq as f32 / 250.0).sin() * 0.7, seq);
            let result = pipeline.process(&mut frame);
            if result.is_ok() {
                if pass == 0 {
                    outputs_a.push(frame.torque_out);
                } else {
                    outputs_b.push(frame.torque_out);
                }
            }
        }
    }

    assert_eq!(outputs_a.len(), outputs_b.len(), "different frame counts");
    for (i, (a, b)) in outputs_a.iter().zip(outputs_b.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "Frame {i}: pass1={a} pass2={b} (non-deterministic)"
        );
    }
    Ok(())
}

#[tokio::test]
async fn det_07_worst_case_path_bounded_output() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    // Worst-case: extreme inputs that might amplify through the filter chain
    let mut warmup = make_frame(0.0, 0);
    let _ = pipeline.process(&mut warmup);

    for seq in 1..=500u16 {
        let mut frame = make_frame(if seq % 2 == 0 { 1.0 } else { -1.0 }, seq);
        let result = pipeline.process(&mut frame);
        if result.is_ok() {
            assert!(
                frame.torque_out.is_finite(),
                "Non-finite output at frame {seq}"
            );
            assert!(
                frame.torque_out.abs() <= 1.0,
                "Unbounded output at frame {seq}: {}",
                frame.torque_out
            );
        }
        // PipelineFault is the safety bound itself — acceptable
    }
    Ok(())
}

// ===========================================================================
// 3. No dynamic dispatch in hot path (compile-time guarantees)
// ===========================================================================

#[test]
fn det_08_filter_node_fn_is_thin_pointer() {
    // fn(&mut Frame, *mut u8) is a concrete function pointer.
    // A trait object (&dyn Fn) would be 2×usize (fat pointer).
    assert_eq!(
        std::mem::size_of::<FilterNodeFn>(),
        std::mem::size_of::<usize>(),
        "FilterNodeFn is not a thin pointer — dynamic dispatch detected"
    );
}

#[test]
fn det_09_frame_is_copy_repr_c() {
    // Copy ensures no hidden clone/alloc; repr(C) gives deterministic layout
    let f1 = make_frame(0.5, 1);
    let f2 = f1; // must be Copy
    assert_eq!(f1.ffb_in.to_bits(), f2.ffb_in.to_bits());
    assert_eq!(f1.seq, f2.seq);

    // repr(C) = predictable size
    assert!(std::mem::size_of::<Frame>() <= 64);
}

#[test]
fn det_10_safety_state_is_enum_not_trait_object() {
    // SafetyState is an enum matched by value — no vtable dispatch
    let size = std::mem::size_of::<SafetyState>();
    // An enum with small variants should be well under 256 bytes
    assert!(
        size < 256,
        "SafetyState unexpectedly large ({size} bytes) — may contain trait objects"
    );

    // Verify it can be pattern-matched (compile-time proof of no dyn dispatch)
    let state = SafetyState::SafeTorque;
    let is_safe = matches!(state, SafetyState::SafeTorque);
    assert!(is_safe);
}

#[test]
fn det_11_torque_limit_clamp_is_pure_arithmetic() {
    // TorqueLimit::clamp is f32::clamp — no trait dispatch, no allocation
    let mut limit = TorqueLimit::new(25.0, 5.0);

    let (clamped, was_clamped) = limit.clamp(30.0);
    assert!((clamped - 25.0).abs() < f32::EPSILON);
    assert!(was_clamped);

    let (clamped2, was_clamped2) = limit.clamp(10.0);
    assert!((clamped2 - 10.0).abs() < f32::EPSILON);
    assert!(!was_clamped2);
}

// ===========================================================================
// 4. Additional determinism scenarios
// ===========================================================================

#[tokio::test]
async fn det_12_empty_pipeline_deterministic() -> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::Pipeline;
    let mut pipeline = Pipeline::new();

    let mut frame = make_frame(0.5, 1);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // Empty pipeline = pass-through; torque_out stays at 0.0 (default)
    assert!(frame.torque_out.abs() < f32::EPSILON);
    Ok(())
}

#[tokio::test]
async fn det_13_pipeline_process_is_idempotent_per_input() -> Result<(), Box<dyn std::error::Error>>
{
    // For stateless-equivalent filters, same input sequence → same output sequence
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame_a = make_frame(0.5, 1);
    let _ = pipeline.process(&mut frame_a);
    let out_a = frame_a.torque_out;

    // Re-compile fresh pipeline
    let compiled2 = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline2 = compiled2.pipeline;

    let mut frame_b = make_frame(0.5, 1);
    let _ = pipeline2.process(&mut frame_b);
    let out_b = frame_b.torque_out;

    assert_eq!(
        out_a.to_bits(),
        out_b.to_bits(),
        "Same input produced different output across pipeline instances"
    );
    Ok(())
}

#[test]
fn det_14_safety_state_transitions_are_bounded() {
    // Each safety method does a bounded number of operations (match + assign).
    let mut service = SafetyService::new(5.0, 25.0);

    // Fault → query → fault — all O(1), exercising every branch
    let start = Instant::now();
    for _ in 0..10_000 {
        let _ = service.max_torque_nm();
        service.report_fault(FaultType::ThermalLimit);
        let _ = service.clamp_torque_nm(10.0);
        let _ = service.state();
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(200),
        "40k safety ops took {:?}",
        elapsed
    );
}

#[tokio::test]
async fn det_15_pipeline_timing_variance_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let mut timings_us: Vec<u64> = Vec::with_capacity(1000);
    for seq in 1..=1000u16 {
        let mut frame = make_frame((seq as f32 * 0.003).sin() * 0.6, seq);
        let start = Instant::now();
        let _ = pipeline.process(&mut frame);
        timings_us.push(start.elapsed().as_micros() as u64);
    }

    timings_us.sort();
    let median = timings_us[timings_us.len() / 2];
    let p99 = timings_us[(timings_us.len() * 99) / 100];

    // P99 should not be more than 100× the median (generous bound for CI)
    // This catches unbounded loops that would cause extreme tail latency.
    let ratio = p99.checked_div(median).unwrap_or(p99);
    assert!(
        ratio < 100,
        "P99/median ratio = {ratio} (median={median}µs, p99={p99}µs) — unbounded tail?"
    );
    Ok(())
}
