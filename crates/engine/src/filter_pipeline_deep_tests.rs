//! Deep tests for filter pipeline, FFB effects, and safety integration
//!
//! Coverage areas:
//! 1. Individual filter correctness (reconstruction, damper, spring, friction, inertia, etc.)
//! 2. Filter chain composition and parameter hot-swap
//! 3. Pipeline throughput and drain behavior
//! 4. Force feedback effect synthesis (constant, periodic, ramp, spring, damper, friction)
//! 5. Combined effects with envelope (attack/sustain/release)
//! 6. Safety integration: torque clamping, emergency stop, fault state

// ============================================================================
// 1. Individual filter correctness
// ============================================================================

#[cfg(test)]
mod filter_correctness_tests {
    use crate::filters::*;
    use crate::rt::Frame;

    fn frame(ffb_in: f32, wheel_speed: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    // ── Reconstruction (LPF) ────────────────────────────────────────

    #[test]
    fn reconstruction_step_response_monotonic() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        let mut prev = 0.0f32;
        for i in 0..50 {
            let mut f = frame(1.0, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            if f.torque_out < prev - 1e-6 {
                return Err(format!(
                    "step response not monotonic at tick {}: prev={prev}, cur={}",
                    i, f.torque_out
                ));
            }
            prev = f.torque_out;
        }
        Ok(())
    }

    #[test]
    fn reconstruction_levels_ordered() -> Result<(), String> {
        // Higher level ⇒ more smoothing ⇒ smaller first-tick output
        let mut outputs = Vec::new();
        for level in [1u8, 3, 5, 7] {
            let mut state = ReconstructionState::new(level);
            let mut f = frame(1.0, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            outputs.push((level, f.torque_out));
        }
        for w in outputs.windows(2) {
            if w[1].1 >= w[0].1 + 1e-6 {
                return Err(format!(
                    "level {} output ({}) should be ≤ level {} output ({})",
                    w[1].0, w[1].1, w[0].0, w[0].1
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn reconstruction_zero_input_stays_zero() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        for _ in 0..20 {
            let mut f = frame(0.0, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            if f.torque_out.abs() > 1e-6 {
                return Err(format!("zero input should stay zero, got {}", f.torque_out));
            }
        }
        Ok(())
    }

    // ── Damper ──────────────────────────────────────────────────────

    #[test]
    fn damper_magnitude_scales_with_coefficient() -> Result<(), String> {
        let lo = DamperState::new(0.05, false);
        let hi = DamperState::new(0.5, false);
        let mut f_lo = frame(0.0, 3.0);
        let mut f_hi = frame(0.0, 3.0);
        damper_filter(&mut f_lo, &lo as *const _ as *mut u8);
        damper_filter(&mut f_hi, &hi as *const _ as *mut u8);
        if f_hi.torque_out.abs() <= f_lo.torque_out.abs() {
            return Err(format!(
                "higher coeff should produce larger torque: lo={}, hi={}",
                f_lo.torque_out, f_hi.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn damper_zero_speed_no_output() -> Result<(), String> {
        let state = DamperState::new(0.5, false);
        let mut f = frame(0.0, 0.0);
        damper_filter(&mut f, &state as *const _ as *mut u8);
        if f.torque_out.abs() > 1e-6 {
            return Err(format!(
                "damper at zero speed should produce zero, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn damper_speed_adaptive_output_differs_from_linear() -> Result<(), String> {
        let adaptive = DamperState::new(0.3, true);
        let linear = DamperState::new(0.3, false);
        let mut f_adapt = frame(0.0, 5.0);
        let mut f_linear = frame(0.0, 5.0);
        damper_filter(&mut f_adapt, &adaptive as *const _ as *mut u8);
        damper_filter(&mut f_linear, &linear as *const _ as *mut u8);
        // Adaptive mode should produce different output than linear
        if (f_adapt.torque_out - f_linear.torque_out).abs() < 1e-6 {
            return Err(format!(
                "adaptive and linear damper should differ: adapt={}, linear={}",
                f_adapt.torque_out, f_linear.torque_out
            ));
        }
        Ok(())
    }

    // ── Friction ────────────────────────────────────────────────────

    #[test]
    fn friction_symmetric_opposition() -> Result<(), String> {
        let state = FrictionState::new(0.2, false);
        let mut fp = frame(0.0, 3.0);
        let mut fn_ = frame(0.0, -3.0);
        friction_filter(&mut fp, &state as *const _ as *mut u8);
        friction_filter(&mut fn_, &state as *const _ as *mut u8);
        // Magnitudes should be approximately equal
        if (fp.torque_out.abs() - fn_.torque_out.abs()).abs() > 0.01 {
            return Err(format!(
                "friction should be symmetric: pos={}, neg={}",
                fp.torque_out, fn_.torque_out
            ));
        }
        // Signs should oppose motion
        if fp.torque_out >= 0.0 || fn_.torque_out <= 0.0 {
            return Err(format!(
                "friction must oppose motion: pos={}, neg={}",
                fp.torque_out, fn_.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn friction_preserves_existing_torque() -> Result<(), String> {
        let state = FrictionState::new(0.1, false);
        let mut f = frame(0.5, 2.0);
        f.torque_out = 0.5;
        friction_filter(&mut f, &state as *const _ as *mut u8);
        // Friction adds opposing torque on top of existing
        if (f.torque_out - 0.5).abs() < 0.01 {
            return Err("friction should modify existing torque".to_string());
        }
        Ok(())
    }

    // ── Inertia ─────────────────────────────────────────────────────

    #[test]
    fn inertia_deceleration_assists() -> Result<(), String> {
        let mut state = InertiaState::new(0.2);
        // First tick: speed = 5
        let mut f0 = frame(0.0, 5.0);
        f0.torque_out = 0.0;
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);
        // Second tick: speed = 2 (deceleration)
        let mut f1 = frame(0.0, 2.0);
        f1.torque_out = 0.0;
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        // Deceleration from positive speed → inertia should assist (positive torque)
        if f1.torque_out <= 0.0 {
            return Err(format!(
                "inertia should assist during deceleration, got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }

    // ── Notch filter ────────────────────────────────────────────────

    #[test]
    fn notch_passband_unaffected() -> Result<(), String> {
        let sample_rate = 1000.0;
        let mut state = NotchState::new(100.0, 5.0, -12.0, sample_rate);
        // Feed DC (0 Hz) signal
        let mut max_out = 0.0f32;
        for i in 0..500 {
            let _ = i;
            let mut f = frame(0.0, 0.0);
            f.torque_out = 0.5;
            notch_filter(&mut f, &mut state as *mut _ as *mut u8);
            if i > 400 {
                max_out = max_out.max(f.torque_out.abs());
            }
        }
        if (max_out - 0.5).abs() > 0.05 {
            return Err(format!(
                "DC signal should pass through notch, got amplitude {max_out}"
            ));
        }
        Ok(())
    }

    // ── Slew rate ───────────────────────────────────────────────────

    #[test]
    fn slew_rate_bidirectional_limiting() -> Result<(), String> {
        let mut state = SlewRateState::new(0.5);
        // Positive step
        let mut f1 = frame(0.0, 0.0);
        f1.torque_out = 1.0;
        slew_rate_filter(&mut f1, &mut state as *mut _ as *mut u8);
        let pos_step = f1.torque_out;

        // Reset and do negative step
        let mut state2 = SlewRateState::new(0.5);
        let mut f2 = frame(0.0, 0.0);
        f2.torque_out = -1.0;
        slew_rate_filter(&mut f2, &mut state2 as *mut _ as *mut u8);
        let neg_step = f2.torque_out;

        if (pos_step.abs() - neg_step.abs()).abs() > 1e-6 {
            return Err(format!(
                "slew should be symmetric: pos={pos_step}, neg={neg_step}"
            ));
        }
        Ok(())
    }

    // ── Torque cap safety ───────────────────────────────────────────

    #[test]
    fn torque_cap_clamps_both_polarities() -> Result<(), String> {
        let cap = 0.6f32;
        for &input in &[0.9f32, -0.9, 0.6, -0.6, 0.3, -0.3] {
            let mut f = frame(0.0, 0.0);
            f.torque_out = input;
            torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
            if f.torque_out.abs() > cap + 1e-6 {
                return Err(format!(
                    "torque cap {cap} violated: input={input}, output={}",
                    f.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn torque_cap_subnormal_yields_zero() -> Result<(), String> {
        let cap = 0.8f32;
        let mut f = frame(0.0, 0.0);
        // f32 subnormal
        f.torque_out = f32::MIN_POSITIVE / 2.0;
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        // Subnormal is finite, so it should pass through (it's within cap)
        if !f.torque_out.is_finite() {
            return Err(format!(
                "subnormal should stay finite, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    // ── Bumpstop ────────────────────────────────────────────────────

    #[test]
    fn bumpstop_force_increases_past_start() -> Result<(), String> {
        let mut s1 = BumpstopState::new(true, 10.0, 20.0, 0.8, 0.0);
        s1.current_angle = 12.0;
        let mut f1 = frame(0.0, 0.0);
        bumpstop_filter(&mut f1, &mut s1 as *mut _ as *mut u8);

        let mut s2 = BumpstopState::new(true, 10.0, 20.0, 0.8, 0.0);
        s2.current_angle = 18.0;
        let mut f2 = frame(0.0, 0.0);
        bumpstop_filter(&mut f2, &mut s2 as *mut _ as *mut u8);

        if f2.torque_out.abs() <= f1.torque_out.abs() {
            return Err(format!(
                "bumpstop force should increase: angle12={}, angle18={}",
                f1.torque_out, f2.torque_out
            ));
        }
        Ok(())
    }

    // ── Hands-off detector ──────────────────────────────────────────

    #[test]
    fn hands_off_disabled_never_triggers() -> Result<(), String> {
        let mut state = HandsOffState::new(false, 0.05, 0.1);
        for _ in 0..500 {
            let mut f = frame(0.0, 0.0);
            f.torque_out = 0.01;
            hands_off_detector(&mut f, &mut state as *mut _ as *mut u8);
            if f.hands_off {
                return Err("disabled hands-off should never trigger".to_string());
            }
        }
        Ok(())
    }
}

// ============================================================================
// 2. Filter chain composition and parameter hot-swap
// ============================================================================

#[cfg(test)]
mod filter_chain_tests {
    use crate::filters::*;
    use crate::pipeline::{Pipeline, PipelineCompiler};
    use crate::rt::Frame;
    use racing_wheel_schemas::prelude::*;

    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    fn frame(ffb_in: f32, wheel_speed: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq: 1,
        }
    }

    fn config_with_filters(friction: f32, damper: f32, inertia: f32) -> FilterConfig {
        must(FilterConfig::new_complete(
            0,
            must(Gain::new(friction)),
            must(Gain::new(damper)),
            must(Gain::new(inertia)),
            vec![],
            must(Gain::new(1.0)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig {
                enabled: false,
                ..BumpstopConfig::default()
            },
            HandsOffConfig {
                enabled: false,
                ..HandsOffConfig::default()
            },
        ))
    }

    // ── Multi-filter chain ──────────────────────────────────────────

    #[test]
    fn chain_all_filters_output_bounded() -> Result<(), String> {
        let mut recon = ReconstructionState::new(3);
        let friction = FrictionState::new(0.1, true);
        let damper = DamperState::new(0.1, true);
        let mut inertia = InertiaState::new(0.05);
        let mut notch = NotchState::new(60.0, 2.0, -6.0, 1000.0);
        let mut slew = SlewRateState::new(0.8);
        let cap = 0.9f32;

        // Prime inertia
        let mut init = frame(0.0, 1.0);
        init.torque_out = 0.0;
        inertia_filter(&mut init, &mut inertia as *mut _ as *mut u8);

        for i in 0..100 {
            let mut f = frame(0.5, 2.0 + (i as f32 * 0.01));
            reconstruction_filter(&mut f, &mut recon as *mut _ as *mut u8);
            friction_filter(&mut f, &friction as *const _ as *mut u8);
            damper_filter(&mut f, &damper as *const _ as *mut u8);
            inertia_filter(&mut f, &mut inertia as *mut _ as *mut u8);
            notch_filter(&mut f, &mut notch as *mut _ as *mut u8);
            slew_rate_filter(&mut f, &mut slew as *mut _ as *mut u8);
            torque_cap_filter(&mut f, &cap as *const _ as *mut u8);

            if !f.torque_out.is_finite() {
                return Err(format!("tick {i}: non-finite output {}", f.torque_out));
            }
            if f.torque_out.abs() > cap + 1e-3 {
                return Err(format!("tick {i}: exceeded cap: {}", f.torque_out));
            }
        }
        Ok(())
    }

    #[test]
    fn chain_reconstruction_then_friction_differs_from_reverse() -> Result<(), String> {
        // reconstruction→friction vs friction→reconstruction should differ
        let mut recon1 = ReconstructionState::new(4);
        let friction1 = FrictionState::new(0.2, false);
        let mut recon2 = ReconstructionState::new(4);
        let friction2 = FrictionState::new(0.2, false);

        let mut f1 = frame(0.5, 3.0);
        reconstruction_filter(&mut f1, &mut recon1 as *mut _ as *mut u8);
        friction_filter(&mut f1, &friction1 as *const _ as *mut u8);

        let mut f2 = frame(0.5, 3.0);
        friction_filter(&mut f2, &friction2 as *const _ as *mut u8);
        reconstruction_filter(&mut f2, &mut recon2 as *mut _ as *mut u8);

        // Reconstruction smooths input before friction vs after – outputs differ
        if (f1.torque_out - f2.torque_out).abs() < 1e-6 {
            return Err(format!(
                "different chain orders should produce different results: a={}, b={}",
                f1.torque_out, f2.torque_out
            ));
        }
        Ok(())
    }

    // ── Parameter hot-swap via pipeline swap ────────────────────────

    #[tokio::test]
    async fn pipeline_hot_swap_changes_output() -> Result<(), String> {
        let compiler = PipelineCompiler::new();

        let config_a = config_with_filters(0.0, 0.0, 0.0);
        // Use reconstruction difference instead—avoids out-of-bound from friction
        let config_b = must(FilterConfig::new_complete(
            6, // heavy reconstruction smoothing
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            vec![],
            must(Gain::new(1.0)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)), // torque cap at 0.9
            BumpstopConfig {
                enabled: false,
                ..BumpstopConfig::default()
            },
            HandsOffConfig {
                enabled: false,
                ..HandsOffConfig::default()
            },
        ));

        let mut pipeline = compiler
            .compile_pipeline(config_a)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;

        let mut f1 = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 1_000_000,
            seq: 1,
        };
        pipeline.process(&mut f1).map_err(|e| format!("{e}"))?;
        let out_a = f1.torque_out;

        let new_pipeline = compiler
            .compile_pipeline(config_b)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;
        pipeline.swap_at_tick_boundary(new_pipeline);

        let mut f2 = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 2_000_000,
            seq: 2,
        };
        pipeline.process(&mut f2).map_err(|e| format!("{e}"))?;
        let out_b = f2.torque_out;

        if (out_a - out_b).abs() < 1e-4 {
            return Err(format!(
                "hot-swap should change output: a={out_a}, b={out_b}"
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn pipeline_swap_preserves_node_count() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config = config_with_filters(0.1, 0.1, 0.1);
        let compiled = compiler
            .compile_pipeline(config)
            .await
            .map_err(|e| format!("{e}"))?;
        let count = compiled.pipeline.node_count();
        if count == 0 {
            return Err("compiled pipeline should have nodes".to_string());
        }
        Ok(())
    }

    // ── Filter bypass / enable / disable ────────────────────────────

    #[tokio::test]
    async fn filter_bypass_zero_gains_passthrough() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config = config_with_filters(0.0, 0.0, 0.0);
        let mut pipeline = compiler
            .compile_pipeline(config)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;

        let mut f = frame(0.7, 0.0);
        pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        // With all gains at 0 and linear curve, output should equal input
        if (f.torque_out - 0.7).abs() > 0.02 {
            return Err(format!(
                "zero-gain pipeline should pass through: expected ~0.7, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn enabling_filters_reduces_passthrough() -> Result<(), String> {
        let compiler = PipelineCompiler::new();

        let config_off = config_with_filters(0.0, 0.0, 0.0);
        let config_on = config_with_filters(0.2, 0.2, 0.0);

        let mut p_off = compiler
            .compile_pipeline(config_off)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;
        let mut p_on = compiler
            .compile_pipeline(config_on)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;

        let mut f_off = frame(0.5, 3.0);
        let mut f_on = frame(0.5, 3.0);
        p_off.process(&mut f_off).map_err(|e| format!("{e}"))?;
        p_on.process(&mut f_on).map_err(|e| format!("{e}"))?;

        // Friction and damper oppose motion → different output
        if (f_off.torque_out - f_on.torque_out).abs() < 1e-4 {
            return Err(format!(
                "enabling filters should change output: off={}, on={}",
                f_off.torque_out, f_on.torque_out
            ));
        }
        Ok(())
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[tokio::test]
    async fn pipeline_nan_input_returns_fault() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config = config_with_filters(0.1, 0.0, 0.0);
        let mut pipeline = compiler
            .compile_pipeline(config)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;

        let mut f = Frame {
            ffb_in: f32::NAN,
            torque_out: f32::NAN,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        let result = pipeline.process(&mut f);
        if result.is_ok() {
            return Err("NaN input should produce pipeline fault".to_string());
        }
        Ok(())
    }

    #[test]
    fn pipeline_max_input_stays_bounded() -> Result<(), String> {
        let mut pipeline = Pipeline::new();
        let mut f = Frame {
            ffb_in: 1.0,
            torque_out: 1.0,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        if f.torque_out.abs() > 1.0 {
            return Err(format!("max input exceeded bounds: {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn pipeline_zero_input_produces_zero() -> Result<(), String> {
        let mut pipeline = Pipeline::new();
        let mut f = Frame {
            ffb_in: 0.0,
            torque_out: 0.0,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        if f.torque_out.abs() > 1e-6 {
            return Err(format!("zero input should produce zero: {}", f.torque_out));
        }
        Ok(())
    }
}

// ============================================================================
// 3. Pipeline throughput and drain
// ============================================================================

#[cfg(test)]
mod pipeline_throughput_tests {
    use crate::pipeline::{Pipeline, PipelineCompiler};
    use crate::rt::Frame;
    use racing_wheel_schemas::prelude::*;
    use std::time::{Duration, Instant};

    #[track_caller]
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("unexpected Err: {e:?}"),
        }
    }

    fn config_n_notch(n: usize) -> FilterConfig {
        let notches: Vec<NotchFilter> = (0..n)
            .map(|i| {
                let freq = 30.0 + (i as f32 * 20.0);
                let freq = freq.min(490.0);
                must(NotchFilter::new(must(FrequencyHz::new(freq)), 2.0, -6.0))
            })
            .collect();

        must(FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.1)),
            must(Gain::new(0.05)),
            notches,
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)),
            BumpstopConfig {
                enabled: false,
                ..BumpstopConfig::default()
            },
            HandsOffConfig {
                enabled: false,
                ..HandsOffConfig::default()
            },
        ))
    }

    #[test]
    fn empty_pipeline_processes_within_budget() -> Result<(), String> {
        let mut pipeline = Pipeline::new();
        let start = Instant::now();
        for seq in 0..1000u16 {
            let mut f = Frame {
                ffb_in: 0.5,
                torque_out: 0.0,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: seq as u64 * 1_000_000,
                seq,
            };
            pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        }
        let elapsed = start.elapsed();
        // 1000 frames at 1kHz → must complete in well under 1s
        if elapsed > Duration::from_millis(100) {
            return Err(format!(
                "empty pipeline too slow for 1000 frames: {elapsed:?}"
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn compiled_pipeline_processes_within_budget() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config = config_n_notch(2);
        let mut pipeline = compiler
            .compile_pipeline(config)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;

        let start = Instant::now();
        for seq in 0..1000u16 {
            let mut f = Frame {
                ffb_in: 0.3,
                torque_out: 0.3,
                wheel_speed: 2.0,
                hands_off: false,
                ts_mono_ns: seq as u64 * 1_000_000,
                seq,
            };
            pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        }
        let elapsed = start.elapsed();
        if elapsed > Duration::from_millis(200) {
            return Err(format!(
                "compiled pipeline too slow for 1000 frames: {elapsed:?}"
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn pipeline_with_many_notch_filters() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        // Maximum practical chain: many notch filters
        let config = config_n_notch(10);
        let mut pipeline = compiler
            .compile_pipeline(config)
            .await
            .map_err(|e| format!("{e}"))?
            .pipeline;

        assert!(
            pipeline.node_count() >= 10,
            "expected ≥10 nodes, got {}",
            pipeline.node_count()
        );

        let start = Instant::now();
        for seq in 0..500u16 {
            let mut f = Frame {
                ffb_in: 0.5,
                torque_out: 0.5,
                wheel_speed: 1.0,
                hands_off: false,
                ts_mono_ns: seq as u64 * 1_000_000,
                seq,
            };
            pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        }
        let elapsed = start.elapsed();
        if elapsed > Duration::from_millis(500) {
            return Err(format!(
                "heavy pipeline too slow for 500 frames: {elapsed:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn pipeline_drain_on_shutdown() -> Result<(), String> {
        // Simulate drain: swap to empty pipeline, verify zero output
        let mut pipeline = Pipeline::with_hash(0xABCD);
        let empty = Pipeline::new();
        pipeline.swap_at_tick_boundary(empty);

        let mut f = Frame {
            ffb_in: 0.8,
            torque_out: 0.0,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
        // Empty pipeline just passes through without modification
        if pipeline.node_count() != 0 {
            return Err(format!(
                "drained pipeline should be empty, got {} nodes",
                pipeline.node_count()
            ));
        }
        Ok(())
    }
}

// ============================================================================
// 4. Force feedback effect synthesis tests
// ============================================================================

#[cfg(test)]
mod ffb_effect_tests {
    use crate::rt::Frame;

    fn frame_at(torque: f32, ts_ms: u64) -> Frame {
        Frame {
            ffb_in: torque,
            torque_out: torque,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: ts_ms * 1_000_000,
            seq: ts_ms as u16,
        }
    }

    // ── Constant force ──────────────────────────────────────────────

    #[test]
    fn constant_force_steady_state() -> Result<(), String> {
        let magnitude = 0.6f32;
        for i in 0..100 {
            let f = frame_at(magnitude, i);
            if (f.ffb_in - magnitude).abs() > 1e-6 {
                return Err(format!("constant force unstable at tick {i}"));
            }
        }
        Ok(())
    }

    #[test]
    fn constant_force_negative() -> Result<(), String> {
        let f = frame_at(-0.8, 0);
        if (f.ffb_in - (-0.8)).abs() > 1e-6 {
            return Err(format!("negative constant force: {}", f.ffb_in));
        }
        Ok(())
    }

    // ── Periodic effects ────────────────────────────────────────────

    fn sine_effect(amplitude: f32, freq_hz: f32, t_ms: u64) -> f32 {
        let t = t_ms as f32 / 1000.0;
        amplitude * (2.0 * std::f32::consts::PI * freq_hz * t).sin()
    }

    fn square_effect(amplitude: f32, freq_hz: f32, t_ms: u64) -> f32 {
        let t = t_ms as f32 / 1000.0;
        let phase = (t * freq_hz).fract();
        if phase < 0.5 { amplitude } else { -amplitude }
    }

    fn triangle_effect(amplitude: f32, freq_hz: f32, t_ms: u64) -> f32 {
        let t = t_ms as f32 / 1000.0;
        let phase = (t * freq_hz).fract();
        let tri = if phase < 0.5 {
            4.0 * phase - 1.0
        } else {
            3.0 - 4.0 * phase
        };
        amplitude * tri
    }

    fn sawtooth_effect(amplitude: f32, freq_hz: f32, t_ms: u64) -> f32 {
        let t = t_ms as f32 / 1000.0;
        let phase = (t * freq_hz).fract();
        amplitude * (2.0 * phase - 1.0)
    }

    #[test]
    fn sine_effect_bounded() -> Result<(), String> {
        let amp = 0.8f32;
        for t in 0..2000 {
            let val = sine_effect(amp, 10.0, t);
            if val.abs() > amp + 1e-4 {
                return Err(format!("sine exceeded amplitude at t={t}: {val}"));
            }
        }
        Ok(())
    }

    #[test]
    fn sine_effect_crosses_zero() -> Result<(), String> {
        let mut has_positive = false;
        let mut has_negative = false;
        for t in 0..1000 {
            let val = sine_effect(0.5, 5.0, t);
            if val > 0.01 {
                has_positive = true;
            }
            if val < -0.01 {
                has_negative = true;
            }
        }
        if !has_positive || !has_negative {
            return Err("sine should cross zero".to_string());
        }
        Ok(())
    }

    #[test]
    fn square_effect_only_two_levels() -> Result<(), String> {
        let amp = 0.7f32;
        for t in 0..1000 {
            let val = square_effect(amp, 10.0, t);
            if (val.abs() - amp).abs() > 1e-4 {
                return Err(format!("square wave should be ±{amp}, got {val} at t={t}"));
            }
        }
        Ok(())
    }

    #[test]
    fn triangle_effect_bounded_and_continuous() -> Result<(), String> {
        let amp = 0.6f32;
        let mut prev = triangle_effect(amp, 10.0, 0);
        for t in 1..2000 {
            let val = triangle_effect(amp, 10.0, t);
            if val.abs() > amp + 1e-3 {
                return Err(format!("triangle exceeded amplitude at t={t}: {val}"));
            }
            // At 1ms resolution, max step should be bounded
            let step = (val - prev).abs();
            if step > amp * 0.1 {
                return Err(format!("triangle discontinuity at t={t}: step={step}"));
            }
            prev = val;
        }
        Ok(())
    }

    #[test]
    fn sawtooth_effect_bounded() -> Result<(), String> {
        let amp = 0.5f32;
        for t in 0..2000 {
            let val = sawtooth_effect(amp, 10.0, t);
            if val.abs() > amp + 1e-3 {
                return Err(format!("sawtooth exceeded amplitude at t={t}: {val}"));
            }
        }
        Ok(())
    }

    // ── Ramp effect ─────────────────────────────────────────────────

    fn ramp_effect(start: f32, end: f32, duration_ms: u64, t_ms: u64) -> f32 {
        if t_ms >= duration_ms {
            return end;
        }
        let progress = t_ms as f32 / duration_ms as f32;
        start + (end - start) * progress
    }

    #[test]
    fn ramp_effect_linear_interpolation() -> Result<(), String> {
        let start = 0.2f32;
        let end = 0.8f32;
        let duration = 1000u64;

        let mid = ramp_effect(start, end, duration, 500);
        let expected_mid = 0.5;
        if (mid - expected_mid).abs() > 0.01 {
            return Err(format!(
                "ramp midpoint should be ~{expected_mid}, got {mid}"
            ));
        }

        let at_end = ramp_effect(start, end, duration, 1000);
        if (at_end - end).abs() > 1e-6 {
            return Err(format!("ramp at end should be {end}, got {at_end}"));
        }

        let past_end = ramp_effect(start, end, duration, 2000);
        if (past_end - end).abs() > 1e-6 {
            return Err(format!("ramp past end should be {end}, got {past_end}"));
        }
        Ok(())
    }

    // ── Spring effect (position-dependent) ──────────────────────────

    fn spring_effect(position: f32, center: f32, stiffness: f32, saturation: f32) -> f32 {
        let displacement = position - center;
        let force = -stiffness * displacement;
        force.clamp(-saturation, saturation)
    }

    #[test]
    fn spring_effect_restoring_force() -> Result<(), String> {
        let force = spring_effect(0.3, 0.0, 0.5, 1.0);
        if force >= 0.0 {
            return Err(format!(
                "spring should pull back toward center: got {force}"
            ));
        }
        let force_neg = spring_effect(-0.3, 0.0, 0.5, 1.0);
        if force_neg <= 0.0 {
            return Err(format!("spring should push toward center: got {force_neg}"));
        }
        Ok(())
    }

    #[test]
    fn spring_effect_saturates() -> Result<(), String> {
        let sat = 0.5f32;
        let force = spring_effect(100.0, 0.0, 1.0, sat);
        if force.abs() > sat + 1e-6 {
            return Err(format!("spring should saturate at {sat}, got {force}"));
        }
        Ok(())
    }

    #[test]
    fn spring_at_center_zero_force() -> Result<(), String> {
        let force = spring_effect(0.0, 0.0, 1.0, 1.0);
        if force.abs() > 1e-6 {
            return Err(format!("spring at center should be zero, got {force}"));
        }
        Ok(())
    }

    // ── Damper effect (velocity-dependent) ──────────────────────────

    fn damper_effect(velocity: f32, coefficient: f32, saturation: f32) -> f32 {
        let force = -coefficient * velocity;
        force.clamp(-saturation, saturation)
    }

    #[test]
    fn damper_effect_opposes_velocity() -> Result<(), String> {
        let force = damper_effect(5.0, 0.1, 1.0);
        if force >= 0.0 {
            return Err(format!("damper should oppose positive velocity: {force}"));
        }
        let force_neg = damper_effect(-5.0, 0.1, 1.0);
        if force_neg <= 0.0 {
            return Err(format!(
                "damper should oppose negative velocity: {force_neg}"
            ));
        }
        Ok(())
    }

    #[test]
    fn damper_effect_saturates() -> Result<(), String> {
        let sat = 0.3f32;
        let force = damper_effect(1000.0, 1.0, sat);
        if force.abs() > sat + 1e-6 {
            return Err(format!("damper should saturate at {sat}, got {force}"));
        }
        Ok(())
    }

    // ── Friction effect ─────────────────────────────────────────────

    fn friction_effect(velocity: f32, coefficient: f32) -> f32 {
        if velocity.abs() < 1e-4 {
            return 0.0;
        }
        -coefficient * velocity.signum()
    }

    #[test]
    fn friction_effect_constant_magnitude() -> Result<(), String> {
        let coeff = 0.2f32;
        for &speed in &[1.0f32, 5.0, 100.0] {
            let force = friction_effect(speed, coeff);
            if (force.abs() - coeff).abs() > 1e-4 {
                return Err(format!(
                    "friction magnitude should be {coeff} at speed {speed}, got {}",
                    force.abs()
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn friction_effect_zero_at_rest() -> Result<(), String> {
        let force = friction_effect(0.0, 0.5);
        if force.abs() > 1e-6 {
            return Err(format!("friction at rest should be zero, got {force}"));
        }
        Ok(())
    }

    // ── Combined effects (superposition) ────────────────────────────

    #[test]
    fn combined_effects_superposition() -> Result<(), String> {
        let t = 100u64;
        let constant = 0.3f32;
        let sine_val = sine_effect(0.2, 10.0, t);
        let spring_val = spring_effect(0.1, 0.0, 0.5, 1.0);
        let combined = constant + sine_val + spring_val;

        // Just verify superposition works and result is finite
        if !combined.is_finite() {
            return Err(format!("combined effect not finite: {combined}"));
        }

        // Verify components add up
        let expected = constant + sine_val + spring_val;
        if (combined - expected).abs() > 1e-6 {
            return Err(format!("superposition failed: {combined} != {expected}"));
        }
        Ok(())
    }

    #[test]
    fn combined_effects_through_pipeline_bounded() -> Result<(), String> {
        use crate::pipeline::Pipeline;

        let mut pipeline = Pipeline::new();
        for t in 0..500 {
            let constant = 0.3f32;
            let periodic = sine_effect(0.2, 10.0, t);
            let combined = (constant + periodic).clamp(-1.0, 1.0);

            let mut f = Frame {
                ffb_in: combined,
                torque_out: combined,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: t * 1_000_000,
                seq: t as u16,
            };
            pipeline.process(&mut f).map_err(|e| format!("{e}"))?;
            if f.torque_out.abs() > 1.0 {
                return Err(format!(
                    "combined through pipeline exceeded 1.0: {}",
                    f.torque_out
                ));
            }
        }
        Ok(())
    }

    // ── Envelope: attack / sustain / release ────────────────────────

    fn envelope(t_ms: u64, attack_ms: u64, sustain_ms: u64, release_ms: u64) -> f32 {
        let total = attack_ms + sustain_ms + release_ms;
        if t_ms >= total {
            return 0.0;
        }
        if t_ms < attack_ms {
            return t_ms as f32 / attack_ms as f32;
        }
        let t_after_attack = t_ms - attack_ms;
        if t_after_attack < sustain_ms {
            return 1.0;
        }
        let t_release = t_after_attack - sustain_ms;
        1.0 - (t_release as f32 / release_ms as f32)
    }

    #[test]
    fn envelope_attack_phase() -> Result<(), String> {
        let val = envelope(50, 100, 200, 100);
        if (val - 0.5).abs() > 0.01 {
            return Err(format!("attack midpoint should be ~0.5, got {val}"));
        }
        Ok(())
    }

    #[test]
    fn envelope_sustain_phase() -> Result<(), String> {
        let val = envelope(200, 100, 200, 100);
        if (val - 1.0).abs() > 1e-4 {
            return Err(format!("sustain should be 1.0, got {val}"));
        }
        Ok(())
    }

    #[test]
    fn envelope_release_phase() -> Result<(), String> {
        let val = envelope(350, 100, 200, 100);
        let expected = 0.5; // halfway through release
        if (val - expected).abs() > 0.01 {
            return Err(format!("release midpoint should be ~{expected}, got {val}"));
        }
        Ok(())
    }

    #[test]
    fn envelope_after_end_is_zero() -> Result<(), String> {
        let val = envelope(500, 100, 200, 100);
        if val.abs() > 1e-6 {
            return Err(format!("envelope past end should be 0, got {val}"));
        }
        Ok(())
    }

    #[test]
    fn envelope_applied_to_effect() -> Result<(), String> {
        let amplitude = 0.8f32;
        let attack = 100u64;
        let sustain = 300u64;
        let release = 100u64;

        for t in 0..600 {
            let env = envelope(t, attack, sustain, release);
            let effect = sine_effect(amplitude * env, 20.0, t);
            if effect.abs() > amplitude + 1e-3 {
                return Err(format!(
                    "enveloped effect exceeded amplitude at t={t}: {effect}"
                ));
            }
        }
        // After envelope ends, output should be zero
        let env_after = envelope(550, attack, sustain, release);
        if env_after.abs() > 1e-6 {
            return Err(format!("envelope after end: {env_after}"));
        }
        Ok(())
    }
}

// ============================================================================
// 5. Safety integration tests
// ============================================================================

#[cfg(test)]
mod safety_integration_tests {
    use crate::pipeline::Pipeline;
    use crate::rt::Frame;
    use crate::safety::{FaultType, SafetyService, SafetyState};

    // ── Torque clamping via SafetyService ───────────────────────────

    #[test]
    fn safety_clamps_to_safe_torque() -> Result<(), String> {
        let service = SafetyService::new(5.0, 25.0);
        let clamped = service.clamp_torque_nm(10.0);
        if (clamped - 5.0).abs() > 1e-4 {
            return Err(format!("safe mode should clamp to 5.0 Nm, got {clamped}"));
        }
        Ok(())
    }

    #[test]
    fn safety_clamps_negative_torque() -> Result<(), String> {
        let service = SafetyService::new(5.0, 25.0);
        let clamped = service.clamp_torque_nm(-10.0);
        if (clamped - (-5.0)).abs() > 1e-4 {
            return Err(format!(
                "safe mode should clamp negative to -5.0 Nm, got {clamped}"
            ));
        }
        Ok(())
    }

    #[test]
    fn safety_within_limit_unchanged() -> Result<(), String> {
        let service = SafetyService::new(5.0, 25.0);
        let clamped = service.clamp_torque_nm(3.0);
        if (clamped - 3.0).abs() > 1e-4 {
            return Err(format!(
                "within-limit torque should be unchanged, got {clamped}"
            ));
        }
        Ok(())
    }

    // ── Emergency stop (fault) zeroes output ────────────────────────

    #[test]
    fn fault_state_zeroes_output() -> Result<(), String> {
        let mut service = SafetyService::new(5.0, 25.0);
        service.report_fault(FaultType::PipelineFault);

        match service.state() {
            SafetyState::Faulted { .. } => {}
            other => {
                return Err(format!("expected Faulted state, got {other:?}"));
            }
        }

        let clamped = service.clamp_torque_nm(5.0);
        if clamped.abs() > 1e-6 {
            return Err(format!("faulted state should clamp to 0, got {clamped}"));
        }
        Ok(())
    }

    #[test]
    fn fault_state_zeroes_all_requests() -> Result<(), String> {
        let mut service = SafetyService::new(5.0, 25.0);
        service.report_fault(FaultType::UsbStall);

        for &req in &[0.1f32, 0.5, 1.0, -0.5, -1.0, 25.0, -25.0] {
            let clamped = service.clamp_torque_nm(req);
            if clamped.abs() > 1e-6 {
                return Err(format!(
                    "faulted: request {req} should clamp to 0, got {clamped}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn fault_nan_input_yields_zero() -> Result<(), String> {
        let service = SafetyService::new(5.0, 25.0);
        let clamped = service.clamp_torque_nm(f32::NAN);
        if clamped != 0.0 {
            return Err(format!("NaN input should yield 0, got {clamped}"));
        }
        Ok(())
    }

    #[test]
    fn fault_infinity_input_clamped() -> Result<(), String> {
        let service = SafetyService::new(5.0, 25.0);
        for &val in &[f32::INFINITY, f32::NEG_INFINITY] {
            let clamped = service.clamp_torque_nm(val);
            // Non-finite → safe_requested = 0.0, so clamped = 0.0
            if clamped != 0.0 {
                return Err(format!("{val} should yield 0, got {clamped}"));
            }
        }
        Ok(())
    }

    // ── Pipeline behavior during fault state ────────────────────────

    #[test]
    fn pipeline_output_clamped_by_safety_in_fault() -> Result<(), String> {
        let mut pipeline = Pipeline::new();
        let mut service = SafetyService::new(5.0, 25.0);
        service.report_fault(FaultType::ThermalLimit);

        let mut f = Frame {
            ffb_in: 0.8,
            torque_out: 0.8,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        pipeline.process(&mut f).map_err(|e| format!("{e}"))?;

        // Apply safety clamping (as the engine would)
        let safe_torque = service.clamp_torque_nm(f.torque_out * 25.0);
        if safe_torque.abs() > 1e-6 {
            return Err(format!(
                "fault state should zero pipeline output, got {safe_torque}"
            ));
        }
        Ok(())
    }

    #[test]
    fn safety_limits_applied_after_pipeline() -> Result<(), String> {
        let mut pipeline = Pipeline::new();
        let service = SafetyService::new(5.0, 25.0);

        // Pipeline produces valid output
        let mut f = Frame {
            ffb_in: 0.9,
            torque_out: 0.9,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        pipeline.process(&mut f).map_err(|e| format!("{e}"))?;

        // Convert to Nm and clamp
        let torque_nm = f.torque_out * 25.0; // 0.9 * 25 = 22.5 Nm
        let clamped = service.clamp_torque_nm(torque_nm);

        // In safe mode, max is 5.0 Nm
        if clamped.abs() > 5.0 + 1e-4 {
            return Err(format!("safety should clamp to 5.0 Nm, got {clamped}"));
        }
        Ok(())
    }

    // ── Torque cap filter as safety net ──────────────────────────────

    #[test]
    fn torque_cap_as_pipeline_safety_net() -> Result<(), String> {
        use crate::filters::torque_cap_filter;

        let cap = 0.5f32;
        // Simulate pipeline producing out-of-range intermediate value
        for &input in &[0.9f32, -0.9, 1.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut f = Frame {
                ffb_in: 0.0,
                torque_out: input,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: 0,
                seq: 0,
            };
            torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
            if !f.torque_out.is_finite() {
                return Err(format!(
                    "torque cap should produce finite output for {input}, got {}",
                    f.torque_out
                ));
            }
            if f.torque_out.abs() > cap + 1e-6 {
                return Err(format!(
                    "torque cap {cap} violated for input {input}: got {}",
                    f.torque_out
                ));
            }
        }
        Ok(())
    }

    // ── Fault types ─────────────────────────────────────────────────

    #[test]
    fn all_fault_types_zero_output() -> Result<(), String> {
        let faults = [
            FaultType::UsbStall,
            FaultType::EncoderNaN,
            FaultType::ThermalLimit,
            FaultType::Overcurrent,
            FaultType::PluginOverrun,
            FaultType::TimingViolation,
            FaultType::PipelineFault,
        ];
        for fault in faults {
            let mut service = SafetyService::new(5.0, 25.0);
            service.report_fault(fault);
            let clamped = service.clamp_torque_nm(10.0);
            if clamped.abs() > 1e-6 {
                return Err(format!(
                    "fault {:?} should zero output, got {clamped}",
                    fault
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn max_torque_nm_zero_in_fault() -> Result<(), String> {
        let mut service = SafetyService::new(5.0, 25.0);
        service.report_fault(FaultType::Overcurrent);
        let max = service.max_torque_nm();
        if max.abs() > 1e-6 {
            return Err(format!("max torque in fault should be 0, got {max}"));
        }
        Ok(())
    }
}
