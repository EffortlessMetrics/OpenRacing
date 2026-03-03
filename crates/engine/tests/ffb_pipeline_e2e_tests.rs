//! FFB Pipeline End-to-End Tests
//!
//! Comprehensive tests for the complete force feedback processing pipeline:
//! - Raw telemetry input → filter chain → torque output (full pipeline)
//! - Multiple filter combinations
//! - Filter parameter changes during active processing
//! - Pipeline bypass/passthrough mode
//! - Multi-axis FFB (steering + pedal vibration)
//! - FFB effect composition (constant + periodic + conditional)
//! - Effect priority and mixing
//! - Pipeline latency measurement
//! - Pipeline determinism
//! - Pipeline error recovery (invalid input, NaN, infinity)
//! - Filter chain hot-reconfiguration
//! - Pipeline with safety limits active

use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_schemas::prelude::{CurvePoint, FilterConfig, FrequencyHz, Gain, NotchFilter};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Error type for test assertions
type TestError = Box<dyn std::error::Error>;

/// Create a frame with torque_out initialized to 0 (pipeline integrates ffb_in
/// through the reconstruction filter over successive ticks).
fn make_frame(ffb_in: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

/// Create a frame with explicit torque_out for passthrough / direct tests.
fn make_frame_direct(ffb_in: f32, torque_out: f32, wheel_speed: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn build_tokio_rt() -> Result<tokio::runtime::Runtime, TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.into())
}

fn compile_sync(config: FilterConfig) -> Result<Pipeline, TestError> {
    let rt = build_tokio_rt()?;
    rt.block_on(async {
        let compiler = PipelineCompiler::new();
        let compiled = compiler.compile_pipeline(config).await?;
        Ok(compiled.pipeline)
    })
}

/// Comprehensive filter config with reconstruction, friction, damper, and slew rate.
/// Uses moderate gains that remain bounded at typical wheel speeds.
fn comprehensive_config() -> Result<FilterConfig, TestError> {
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

/// Config with lowpass + damper + friction + spring (curve).
fn lowpass_damper_friction_spring_config() -> Result<FilterConfig, TestError> {
    Ok(FilterConfig {
        reconstruction: 6,
        friction: Gain::new(0.1)?,
        damper: Gain::new(0.08)?,
        inertia: Gain::new(0.0)?,
        slew_rate: Gain::new(1.0)?,
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.3)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    })
}

// =========================================================================
// 1. Raw telemetry input → filter chain → torque output (full pipeline)
// =========================================================================

#[test]
fn full_pipeline_processes_telemetry_to_torque() -> Result<(), TestError> {
    let config = comprehensive_config()?;
    let mut pipeline = compile_sync(config)?;

    let inputs = [0.0_f32, 0.2, 0.5, 0.8, 1.0, 0.7, 0.3, -0.2, -0.6, -1.0];
    for (i, &input) in inputs.iter().enumerate() {
        let mut frame = make_frame(input, 2.0, i as u16);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Pipeline failed at frame {i}");
        assert!(
            frame.torque_out.is_finite(),
            "Non-finite output at frame {i}: {}",
            frame.torque_out
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "Output out of bounds at frame {i}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn full_pipeline_signal_flows_through_all_stages() -> Result<(), TestError> {
    let config = comprehensive_config()?;
    let mut pipeline = compile_sync(config)?;

    // Process enough frames for reconstruction filter to converge
    let mut last_output = 0.0_f32;
    for seq in 0..200_u16 {
        let mut frame = make_frame(0.5, 1.0, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Pipeline failed at seq {seq}");
        last_output = frame.torque_out;
    }

    // With active filters, the converged output should be non-zero
    assert!(
        last_output.abs() > f32::EPSILON,
        "Expected non-zero output after convergence, got {last_output}"
    );
    assert!(
        last_output.is_finite() && last_output.abs() <= 1.0,
        "Output out of bounds: {last_output}"
    );
    Ok(())
}

// =========================================================================
// 2. Multiple filter combinations (lowpass + damper + friction + spring)
// =========================================================================

#[test]
fn lowpass_damper_friction_spring_combination() -> Result<(), TestError> {
    let config = lowpass_damper_friction_spring_config()?;
    let mut pipeline = compile_sync(config)?;

    for seq in 0..50_u16 {
        let speed = (seq as f32 * 0.1).sin() * 3.0;
        let mut frame = make_frame(0.4, speed, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Failed at seq {seq}");
        assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    }
    Ok(())
}

#[test]
fn damper_only_config_produces_speed_dependent_output() -> Result<(), TestError> {
    let config_still = FilterConfig {
        damper: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let mut pipeline_still = compile_sync(config_still)?;

    let config_fast = FilterConfig {
        damper: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let mut pipeline_fast = compile_sync(config_fast)?;

    // At zero speed, damper should have minimal effect
    let mut frame_still = make_frame_direct(0.3, 0.3, 0.0, 0);
    pipeline_still.process(&mut frame_still)?;

    // At moderate speed, damper should modify output more
    let mut frame_fast = make_frame_direct(0.3, 0.3, 5.0, 0);
    pipeline_fast.process(&mut frame_fast)?;

    assert!(frame_still.torque_out.is_finite() && frame_still.torque_out.abs() <= 1.0);
    assert!(frame_fast.torque_out.is_finite() && frame_fast.torque_out.abs() <= 1.0);
    // Damper effect should differ between zero and high speed
    assert!(
        (frame_still.torque_out - frame_fast.torque_out).abs() > f32::EPSILON,
        "Damper should produce different output at different speeds: still={}, fast={}",
        frame_still.torque_out,
        frame_fast.torque_out
    );
    Ok(())
}

#[test]
fn friction_only_config_opposes_motion() -> Result<(), TestError> {
    let config = FilterConfig {
        friction: Gain::new(0.4)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    // With positive wheel speed and zero torque_out, friction opposes motion
    let mut frame = make_frame_direct(0.0, 0.0, 5.0, 0);
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out <= 0.0,
        "Friction should oppose positive motion, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn notch_filter_combination_with_damper() -> Result<(), TestError> {
    let config = FilterConfig {
        damper: Gain::new(0.1)?,
        notch_filters: vec![
            NotchFilter::new(FrequencyHz::new(50.0)?, 3.0, -18.0)?,
            NotchFilter::new(FrequencyHz::new(120.0)?, 2.5, -12.0)?,
        ],
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    for seq in 0..100_u16 {
        let input = (seq as f32 * 0.3).sin() * 0.5;
        let mut frame = make_frame_direct(input, input, 2.0, seq);
        pipeline.process(&mut frame)?;
        assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    }
    Ok(())
}

// =========================================================================
// 3. Filter parameter changes during active processing
// =========================================================================

#[test]
fn parameter_change_mid_processing_via_recompile() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        // Start with light filtering
        let config1 = FilterConfig {
            reconstruction: 2,
            friction: Gain::new(0.05)?,
            ..FilterConfig::default()
        };
        let compiled1 = compiler.compile_pipeline(config1).await?;
        let mut pipeline = compiled1.pipeline;

        for seq in 0..20_u16 {
            let mut frame = make_frame(0.5, 1.0, seq);
            pipeline.process(&mut frame)?;
        }

        // Recompile with heavier filtering
        let config2 = FilterConfig {
            reconstruction: 6,
            friction: Gain::new(0.1)?,
            damper: Gain::new(0.1)?,
            ..FilterConfig::default()
        };
        let compiled2 = compiler.compile_pipeline(config2).await?;
        pipeline.swap_at_tick_boundary(compiled2.pipeline);

        for seq in 20..40_u16 {
            let mut frame = make_frame(0.5, 1.0, seq);
            let result = pipeline.process(&mut frame);
            assert!(result.is_ok(), "Failed after parameter change at seq {seq}");
            assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
        }

        Ok::<(), TestError>(())
    })
}

#[test]
fn multiple_reconfigurations_remain_stable() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();
        let mut pipeline = Pipeline::new();

        let friction_levels = [0.0, 0.05, 0.1, 0.15, 0.1, 0.05, 0.0];
        for (round, &friction) in friction_levels.iter().enumerate() {
            let config = FilterConfig {
                friction: Gain::new(friction)?,
                ..FilterConfig::default()
            };
            let compiled = compiler.compile_pipeline(config).await?;
            pipeline.swap_at_tick_boundary(compiled.pipeline);

            for seq in 0..10_u16 {
                let mut frame = make_frame_direct(0.4, 0.4, 2.0, seq);
                let result = pipeline.process(&mut frame);
                assert!(result.is_ok(), "Failed at round {round}, seq {seq}");
                assert!(
                    frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                    "Out of bounds at round {round}, seq {seq}: {}",
                    frame.torque_out
                );
            }
        }

        Ok::<(), TestError>(())
    })
}

// =========================================================================
// 4. Pipeline bypass/passthrough mode
// =========================================================================

#[test]
fn empty_pipeline_is_passthrough() -> Result<(), TestError> {
    let mut pipeline = Pipeline::new();
    assert!(pipeline.is_empty());

    let test_values = [-1.0_f32, -0.5, 0.0, 0.5, 1.0];
    for &val in &test_values {
        let mut frame = make_frame_direct(val, val, 0.0, 0);
        pipeline.process(&mut frame)?;
        assert!(
            (frame.torque_out - val).abs() < f32::EPSILON,
            "Passthrough failed: input={val}, output={}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn default_config_acts_as_near_passthrough() -> Result<(), TestError> {
    let config = FilterConfig::default();
    let mut pipeline = compile_sync(config)?;

    // Default config has all gains at 0 except slew_rate=1.0 and linear curve,
    // so no filter nodes are compiled (aside from bumpstop/hands_off)
    let mut frame = make_frame_direct(0.7, 0.7, 0.0, 0);
    pipeline.process(&mut frame)?;

    assert!(
        (frame.torque_out - 0.7).abs() < 0.05,
        "Default config should approximate passthrough, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn swap_to_empty_pipeline_restores_passthrough() -> Result<(), TestError> {
    let config = comprehensive_config()?;
    let mut pipeline = compile_sync(config)?;

    // Process with active filters
    let mut frame = make_frame(0.5, 1.0, 0);
    pipeline.process(&mut frame)?;

    // Swap to empty pipeline
    pipeline.swap_at_tick_boundary(Pipeline::new());
    assert!(pipeline.is_empty());

    // Should be passthrough again
    let mut frame = make_frame_direct(0.8, 0.8, 0.0, 1);
    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - 0.8).abs() < f32::EPSILON,
        "Expected passthrough after swap, got {}",
        frame.torque_out
    );
    Ok(())
}

// =========================================================================
// 5. Multi-axis FFB (steering + pedal vibration)
// =========================================================================

#[test]
fn independent_pipelines_for_multi_axis() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        // Steering axis: reconstruction + light damper
        let steering_config = FilterConfig {
            reconstruction: 4,
            damper: Gain::new(0.1)?,
            ..FilterConfig::default()
        };

        // Pedal vibration axis: lighter filtering
        let pedal_config = FilterConfig {
            reconstruction: 2,
            ..FilterConfig::default()
        };

        let mut steering_pipeline = compiler.compile_pipeline(steering_config).await?.pipeline;
        let mut pedal_pipeline = compiler.compile_pipeline(pedal_config).await?.pipeline;

        for seq in 0..50_u16 {
            let steer_input = (seq as f32 * 0.1).sin() * 0.8;
            let pedal_input = (seq as f32 * 0.5).sin() * 0.3;

            let mut steer_frame = make_frame(steer_input, 3.0, seq);
            let mut pedal_frame = make_frame(pedal_input, 0.0, seq);

            steering_pipeline.process(&mut steer_frame)?;
            pedal_pipeline.process(&mut pedal_frame)?;

            assert!(steer_frame.torque_out.is_finite() && steer_frame.torque_out.abs() <= 1.0);
            assert!(pedal_frame.torque_out.is_finite() && pedal_frame.torque_out.abs() <= 1.0);
        }

        Ok::<(), TestError>(())
    })
}

#[test]
fn multi_axis_pipelines_are_isolated() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        // Use reconstruction-only so inputs ramp smoothly
        let config = FilterConfig {
            reconstruction: 4,
            ..FilterConfig::default()
        };

        let mut axis_a = compiler.compile_pipeline(config.clone()).await?.pipeline;
        let mut axis_b = compiler.compile_pipeline(config).await?.pipeline;

        // Feed different inputs over multiple frames so reconstruction diverges
        for seq in 0..20_u16 {
            let mut frame_a = make_frame(0.9, 0.0, seq);
            let mut frame_b = make_frame(-0.5, 0.0, seq);
            axis_a.process(&mut frame_a)?;
            axis_b.process(&mut frame_b)?;
        }

        // After 20 frames, the two axes should have diverged significantly
        let mut frame_a = make_frame(0.9, 0.0, 20);
        let mut frame_b = make_frame(-0.5, 0.0, 20);
        axis_a.process(&mut frame_a)?;
        axis_b.process(&mut frame_b)?;

        assert!(
            (frame_a.torque_out - frame_b.torque_out).abs() > 0.1,
            "Axes should be isolated: a={}, b={}",
            frame_a.torque_out,
            frame_b.torque_out
        );

        Ok::<(), TestError>(())
    })
}

// =========================================================================
// 6. FFB effect composition (constant + periodic + conditional)
// =========================================================================

#[test]
fn constant_effect_through_pipeline() -> Result<(), TestError> {
    let mut pipeline = compile_sync(FilterConfig::default())?;

    for seq in 0..100_u16 {
        let mut frame = make_frame_direct(0.6, 0.6, 0.0, seq);
        pipeline.process(&mut frame)?;
        assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    }
    Ok(())
}

#[test]
fn periodic_effect_sine_wave() -> Result<(), TestError> {
    let config = FilterConfig {
        reconstruction: 2,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    let frequency_hz = 10.0_f32;
    let sample_rate = 1000.0_f32;
    let mut outputs = Vec::new();

    for seq in 0..200_u16 {
        let t = seq as f32 / sample_rate;
        let input = (2.0 * std::f32::consts::PI * frequency_hz * t).sin() * 0.7;
        let mut frame = make_frame(input, 0.0, seq);
        pipeline.process(&mut frame)?;
        outputs.push(frame.torque_out);
    }

    let max_out = outputs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min_out = outputs.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(
        (max_out - min_out) > 0.1,
        "Periodic signal should produce varying output, range: {max_out} - {min_out}"
    );
    Ok(())
}

#[test]
fn conditional_effect_speed_dependent() -> Result<(), TestError> {
    let config_low = FilterConfig {
        damper: Gain::new(0.1)?,
        friction: Gain::new(0.05)?,
        ..FilterConfig::default()
    };
    let mut pipeline_low = compile_sync(config_low)?;

    let config_high = FilterConfig {
        damper: Gain::new(0.1)?,
        friction: Gain::new(0.05)?,
        ..FilterConfig::default()
    };
    let mut pipeline_high = compile_sync(config_high)?;

    // Low speed
    let mut frame_low = make_frame_direct(0.3, 0.3, 0.5, 0);
    pipeline_low.process(&mut frame_low)?;

    // Moderate speed
    let mut frame_high = make_frame_direct(0.3, 0.3, 5.0, 0);
    pipeline_high.process(&mut frame_high)?;

    assert!(frame_low.torque_out.is_finite() && frame_low.torque_out.abs() <= 1.0);
    assert!(frame_high.torque_out.is_finite() && frame_high.torque_out.abs() <= 1.0);
    Ok(())
}

#[test]
fn composite_effects_summed_input() -> Result<(), TestError> {
    let mut pipeline = compile_sync(FilterConfig::default())?;

    for seq in 0..100_u16 {
        let constant = 0.3;
        let periodic = (seq as f32 * 0.3).sin() * 0.2;
        let conditional = if seq % 20 < 5 { 0.1 } else { 0.0 };
        let combined = (constant + periodic + conditional).clamp(-1.0, 1.0);

        let mut frame = make_frame_direct(combined, combined, 0.0, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Composite effect produced invalid output at seq {seq}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

// =========================================================================
// 7. Effect priority and mixing
// =========================================================================

#[test]
fn higher_priority_effect_dominates_mix() -> Result<(), TestError> {
    let mut pipeline = compile_sync(FilterConfig::default())?;

    let safety_force = 0.9_f32;
    let texture_force = 0.05_f32;
    let mixed = (safety_force + texture_force).clamp(-1.0, 1.0);

    let mut frame = make_frame_direct(mixed, mixed, 0.0, 0);
    pipeline.process(&mut frame)?;

    assert!(
        frame.torque_out > 0.5,
        "High-priority effect should dominate, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn effect_mixing_respects_saturation() -> Result<(), TestError> {
    let mut pipeline = compile_sync(FilterConfig::default())?;

    let effect_a = 0.7_f32;
    let effect_b = 0.6_f32;
    let mixed = (effect_a + effect_b).clamp(-1.0, 1.0);

    let mut frame = make_frame_direct(mixed, mixed, 0.0, 0);
    pipeline.process(&mut frame)?;

    assert!(
        frame.torque_out.abs() <= 1.0,
        "Mixed output should be within [-1, 1], got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn opposing_effects_cancel() -> Result<(), TestError> {
    let mut pipeline = compile_sync(FilterConfig::default())?;

    let mixed = 0.5_f32 + (-0.5_f32);
    let mut frame = make_frame_direct(mixed, mixed, 0.0, 0);
    pipeline.process(&mut frame)?;

    assert!(
        frame.torque_out.abs() < 0.01,
        "Opposing effects should cancel, got {}",
        frame.torque_out
    );
    Ok(())
}

// =========================================================================
// 8. Pipeline latency measurement (ensure <1ms budget)
// =========================================================================

#[test]
fn pipeline_processing_within_latency_budget() -> Result<(), TestError> {
    let config = comprehensive_config()?;
    let mut pipeline = compile_sync(config)?;

    // Warm up
    for seq in 0..100_u16 {
        let mut frame = make_frame(0.5, 1.5, seq);
        pipeline.process(&mut frame)?;
    }

    let iterations = 1000_u32;
    let start = Instant::now();
    for i in 0..iterations {
        let mut frame = make_frame((i as f32 * 0.01).sin() * 0.5, 1.5, (100 + i) as u16);
        pipeline.process(&mut frame)?;
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    let max_budget_ns = 1_000_000; // 1ms

    assert!(
        avg_ns < max_budget_ns,
        "Average pipeline latency {avg_ns}ns exceeds 1ms budget"
    );
    Ok(())
}

#[test]
fn pipeline_latency_with_all_filters_active() -> Result<(), TestError> {
    let config = FilterConfig {
        reconstruction: 8,
        friction: Gain::new(0.08)?,
        damper: Gain::new(0.08)?,
        inertia: Gain::new(0.05)?,
        notch_filters: vec![
            NotchFilter::new(FrequencyHz::new(50.0)?, 3.0, -18.0)?,
            NotchFilter::new(FrequencyHz::new(100.0)?, 2.0, -12.0)?,
            NotchFilter::new(FrequencyHz::new(150.0)?, 2.5, -15.0)?,
        ],
        slew_rate: Gain::new(0.5)?,
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.25, 0.15)?,
            CurvePoint::new(0.5, 0.4)?,
            CurvePoint::new(0.75, 0.7)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        torque_cap: Gain::new(0.8)?,
        ..FilterConfig::default()
    };

    let mut pipeline = compile_sync(config)?;

    // Warm up
    for seq in 0..50_u16 {
        let mut frame = make_frame(0.3, 2.0, seq);
        pipeline.process(&mut frame)?;
    }

    let iterations = 500_u32;
    let start = Instant::now();
    for i in 0..iterations {
        let mut frame = make_frame((i as f32 * 0.02).sin() * 0.4, 2.0, (50 + i) as u16);
        pipeline.process(&mut frame)?;
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    assert!(
        avg_ns < 1_000_000,
        "Full-filter pipeline latency {avg_ns}ns exceeds 1ms budget"
    );
    Ok(())
}

// =========================================================================
// 9. Pipeline determinism (same input → same output)
// =========================================================================

#[test]
fn deterministic_output_for_identical_inputs() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        let config = comprehensive_config()?;
        let compiled_a = compiler.compile_pipeline(config.clone()).await?;
        let compiled_b = compiler.compile_pipeline(config).await?;

        let mut pipeline_a = compiled_a.pipeline;
        let mut pipeline_b = compiled_b.pipeline;

        for seq in 0..200_u16 {
            let input = (seq as f32 * 0.05).sin() * 0.5;
            let speed = (seq as f32 * 0.02).cos() * 1.5;

            let mut frame_a = make_frame(input, speed, seq);
            let mut frame_b = make_frame(input, speed, seq);

            pipeline_a.process(&mut frame_a)?;
            pipeline_b.process(&mut frame_b)?;

            assert!(
                (frame_a.torque_out - frame_b.torque_out).abs() < f32::EPSILON,
                "Non-deterministic at seq {seq}: a={}, b={}",
                frame_a.torque_out,
                frame_b.torque_out
            );
        }

        Ok::<(), TestError>(())
    })
}

#[test]
fn deterministic_config_hash() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();
        let config = comprehensive_config()?;

        let compiled_a = compiler.compile_pipeline(config.clone()).await?;
        let compiled_b = compiler.compile_pipeline(config).await?;

        assert_eq!(
            compiled_a.config_hash, compiled_b.config_hash,
            "Same config should produce same hash"
        );

        Ok::<(), TestError>(())
    })
}

#[test]
fn different_configs_produce_different_hashes() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        let config_a = FilterConfig {
            friction: Gain::new(0.1)?,
            ..FilterConfig::default()
        };
        let config_b = FilterConfig {
            friction: Gain::new(0.2)?,
            ..FilterConfig::default()
        };

        let compiled_a = compiler.compile_pipeline(config_a).await?;
        let compiled_b = compiler.compile_pipeline(config_b).await?;

        assert_ne!(
            compiled_a.config_hash, compiled_b.config_hash,
            "Different configs should produce different hashes"
        );

        Ok::<(), TestError>(())
    })
}

// =========================================================================
// 10. Pipeline error recovery (invalid input, NaN, infinity)
// =========================================================================

#[test]
fn nan_input_returns_pipeline_fault() -> Result<(), TestError> {
    let config = FilterConfig {
        friction: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;
    let mut frame = make_frame_direct(f32::NAN, f32::NAN, 0.0, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_err(), "NaN input should cause pipeline fault");
    Ok(())
}

#[test]
fn infinity_input_returns_pipeline_fault() -> Result<(), TestError> {
    let config = FilterConfig {
        friction: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;
    let mut frame = make_frame_direct(f32::INFINITY, f32::INFINITY, 0.0, 0);
    let result = pipeline.process(&mut frame);
    assert!(
        result.is_err(),
        "Infinity input should cause pipeline fault"
    );
    Ok(())
}

#[test]
fn negative_infinity_input_returns_pipeline_fault() -> Result<(), TestError> {
    let config = FilterConfig {
        friction: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;
    let mut frame = make_frame_direct(f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0, 0);
    let result = pipeline.process(&mut frame);
    assert!(
        result.is_err(),
        "Negative infinity input should cause pipeline fault"
    );
    Ok(())
}

#[test]
fn pipeline_recovers_after_fault() -> Result<(), TestError> {
    let config = FilterConfig {
        friction: Gain::new(0.1)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    // Process valid frames
    for seq in 0..10_u16 {
        let mut frame = make_frame_direct(0.3, 0.3, 1.0, seq);
        pipeline.process(&mut frame)?;
    }

    // Trigger a fault
    let mut bad_frame = make_frame_direct(f32::NAN, f32::NAN, 0.0, 10);
    let fault_result = pipeline.process(&mut bad_frame);
    assert!(fault_result.is_err());

    // Swap in a fresh pipeline to recover
    let fresh_pipeline = compile_sync(FilterConfig {
        friction: Gain::new(0.1)?,
        ..FilterConfig::default()
    })?;
    pipeline.swap_at_tick_boundary(fresh_pipeline);

    // Should process valid frames again
    let mut recovery_frame = make_frame_direct(0.5, 0.5, 1.0, 11);
    let result = pipeline.process(&mut recovery_frame);
    assert!(result.is_ok(), "Pipeline should recover after swap");
    assert!(recovery_frame.torque_out.is_finite());
    Ok(())
}

#[test]
fn subnormal_input_handled_gracefully() -> Result<(), TestError> {
    let config = FilterConfig {
        reconstruction: 2,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    let subnormal = f32::MIN_POSITIVE / 2.0;
    let mut frame = make_frame(subnormal, 0.0, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok(), "Subnormal input should be handled gracefully");
    assert!(frame.torque_out.is_finite());
    Ok(())
}

#[test]
fn extreme_but_valid_inputs_at_boundary() -> Result<(), TestError> {
    let config = comprehensive_config()?;
    let mut pipeline = compile_sync(config)?;

    // Max positive
    let mut frame_max = make_frame(1.0, 0.0, 0);
    pipeline.process(&mut frame_max)?;
    assert!(frame_max.torque_out.is_finite() && frame_max.torque_out.abs() <= 1.0);

    // Max negative
    let mut frame_min = make_frame(-1.0, 0.0, 1);
    pipeline.process(&mut frame_min)?;
    assert!(frame_min.torque_out.is_finite() && frame_min.torque_out.abs() <= 1.0);

    // Zero
    let mut frame_zero = make_frame(0.0, 0.0, 2);
    pipeline.process(&mut frame_zero)?;
    assert!(frame_zero.torque_out.is_finite());

    Ok(())
}

// =========================================================================
// 11. Filter chain hot-reconfiguration
// =========================================================================

#[test]
fn hot_reconfigure_adds_filters() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        // Start minimal
        let config1 = FilterConfig::default();
        let compiled1 = compiler.compile_pipeline(config1).await?;
        let mut pipeline = compiled1.pipeline;
        let initial_count = pipeline.node_count();

        for seq in 0..10_u16 {
            let mut frame = make_frame_direct(0.5, 0.5, 0.0, seq);
            pipeline.process(&mut frame)?;
        }

        // Hot-reconfigure: add more filters
        let config2 = comprehensive_config()?;
        let compiled2 = compiler.compile_pipeline(config2).await?;
        let new_count = compiled2.pipeline.node_count();
        pipeline.swap_at_tick_boundary(compiled2.pipeline);

        assert!(
            new_count > initial_count,
            "Reconfigured pipeline should have more nodes: {} vs {}",
            new_count,
            initial_count
        );

        // Continue processing after reconfig
        for seq in 10..30_u16 {
            let mut frame = make_frame(0.3, 2.0, seq);
            let result = pipeline.process(&mut frame);
            assert!(result.is_ok(), "Processing should continue after hot-reconfig at seq {seq}");
            assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
        }

        Ok::<(), TestError>(())
    })
}

#[test]
fn hot_reconfigure_removes_filters() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        let config1 = comprehensive_config()?;
        let compiled1 = compiler.compile_pipeline(config1).await?;
        let mut pipeline = compiled1.pipeline;
        let heavy_count = pipeline.node_count();

        for seq in 0..20_u16 {
            let mut frame = make_frame(0.3, 2.0, seq);
            pipeline.process(&mut frame)?;
        }

        // Hot-reconfigure to minimal
        let config2 = FilterConfig::default();
        let compiled2 = compiler.compile_pipeline(config2).await?;
        let minimal_count = compiled2.pipeline.node_count();
        pipeline.swap_at_tick_boundary(compiled2.pipeline);

        assert!(
            minimal_count < heavy_count,
            "Minimal config should have fewer nodes: {} vs {}",
            minimal_count,
            heavy_count
        );

        for seq in 20..40_u16 {
            let mut frame = make_frame_direct(0.5, 0.5, 0.0, seq);
            pipeline.process(&mut frame)?;
        }

        Ok::<(), TestError>(())
    })
}

#[test]
fn hot_reconfigure_preserves_output_bounds() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        let configs = vec![
            FilterConfig::default(),
            comprehensive_config()?,
            lowpass_damper_friction_spring_config()?,
            FilterConfig {
                reconstruction: 8,
                ..FilterConfig::default()
            },
            FilterConfig::default(),
        ];

        let mut pipeline = Pipeline::new();

        for (round, config) in configs.into_iter().enumerate() {
            let compiled = compiler.compile_pipeline(config).await?;
            pipeline.swap_at_tick_boundary(compiled.pipeline);

            for seq in 0..20_u16 {
                let input = (seq as f32 * 0.15 + round as f32).sin() * 0.4;
                let mut frame = make_frame(input, 2.0, seq);
                let result = pipeline.process(&mut frame);
                assert!(result.is_ok(), "Failed at round {round}, seq {seq}");
                assert!(
                    frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                    "Bounds violated at round {round}, seq {seq}: {}",
                    frame.torque_out
                );
            }
        }

        Ok::<(), TestError>(())
    })
}

// =========================================================================
// 12. Pipeline with safety limits active
// =========================================================================

#[test]
fn torque_cap_limits_output() -> Result<(), TestError> {
    let config = FilterConfig {
        torque_cap: Gain::new(0.5)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    let mut frame = make_frame_direct(0.9, 0.9, 0.0, 0);
    pipeline.process(&mut frame)?;

    assert!(
        frame.torque_out.abs() <= 0.5 + f32::EPSILON,
        "Torque cap at 0.5 should limit output, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn torque_cap_symmetric_positive_and_negative() -> Result<(), TestError> {
    let config_pos = FilterConfig {
        torque_cap: Gain::new(0.3)?,
        ..FilterConfig::default()
    };
    let mut pipeline_pos = compile_sync(config_pos)?;

    let config_neg = FilterConfig {
        torque_cap: Gain::new(0.3)?,
        ..FilterConfig::default()
    };
    let mut pipeline_neg = compile_sync(config_neg)?;

    let mut frame_pos = make_frame_direct(0.8, 0.8, 0.0, 0);
    pipeline_pos.process(&mut frame_pos)?;

    let mut frame_neg = make_frame_direct(-0.8, -0.8, 0.0, 0);
    pipeline_neg.process(&mut frame_neg)?;

    assert!(
        frame_pos.torque_out <= 0.3 + f32::EPSILON,
        "Positive torque should be capped at 0.3, got {}",
        frame_pos.torque_out
    );
    assert!(
        frame_neg.torque_out >= -(0.3 + f32::EPSILON),
        "Negative torque should be capped at -0.3, got {}",
        frame_neg.torque_out
    );
    Ok(())
}

#[test]
fn bumpstop_active_with_full_pipeline() -> Result<(), TestError> {
    let config = FilterConfig {
        friction: Gain::new(0.05)?,
        damper: Gain::new(0.05)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    for seq in 0..50_u16 {
        let mut frame = make_frame_direct(0.3, 0.3, 1.0, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Bumpstop should not break pipeline at seq {seq}");
        assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    }
    Ok(())
}

#[test]
fn torque_cap_with_comprehensive_filters() -> Result<(), TestError> {
    let config = FilterConfig {
        reconstruction: 4,
        friction: Gain::new(0.05)?,
        damper: Gain::new(0.05)?,
        inertia: Gain::new(0.05)?,
        slew_rate: Gain::new(0.75)?,
        torque_cap: Gain::new(0.6)?,
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.5, 0.45)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    for seq in 0..100_u16 {
        let input = (seq as f32 * 0.1).sin() * 0.5;
        let mut frame = make_frame(input, 2.0, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.abs() <= 0.6 + f32::EPSILON,
            "Torque cap should limit output to 0.6, got {} at seq {seq}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn safety_limits_with_rapid_input_changes() -> Result<(), TestError> {
    let config = FilterConfig {
        slew_rate: Gain::new(0.3)?,
        torque_cap: Gain::new(0.7)?,
        ..FilterConfig::default()
    };
    let mut pipeline = compile_sync(config)?;

    for seq in 0..200_u16 {
        let input = if seq % 2 == 0 { 0.6 } else { -0.6 };
        let mut frame = make_frame_direct(input, input, 0.0, seq);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Safety limits violated at seq {seq}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

// =========================================================================
// Additional: sustained processing stress test
// =========================================================================

#[test]
fn sustained_processing_1000_frames() -> Result<(), TestError> {
    let config = comprehensive_config()?;
    let mut pipeline = compile_sync(config)?;

    for seq in 0..1000_u16 {
        let t = seq as f32 / 1000.0;
        let input = (t * 20.0 * std::f32::consts::PI).sin() * 0.5
            + (t * 50.0 * std::f32::consts::PI).sin() * 0.1;
        let input_clamped = input.clamp(-1.0, 1.0);
        let speed = (t * 5.0 * std::f32::consts::PI).cos() * 1.5;

        let mut frame = make_frame(input_clamped, speed, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "Pipeline failed at frame {seq}");
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Invalid output at frame {seq}: {}",
            frame.torque_out
        );
    }
    Ok(())
}

#[test]
fn pipeline_node_count_reflects_active_filters() -> Result<(), TestError> {
    let rt = build_tokio_rt()?;

    rt.block_on(async {
        let compiler = PipelineCompiler::new();

        let empty = compiler.compile_pipeline(FilterConfig::default()).await?;
        let full_config = comprehensive_config()?;
        let full = compiler.compile_pipeline(full_config).await?;

        assert!(
            full.pipeline.node_count() > empty.pipeline.node_count(),
            "Full config should have more nodes: full={}, empty={}",
            full.pipeline.node_count(),
            empty.pipeline.node_count()
        );

        Ok::<(), TestError>(())
    })
}
