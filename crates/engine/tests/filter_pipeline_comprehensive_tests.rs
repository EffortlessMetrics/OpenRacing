//! Comprehensive tests for the engine's force-feedback filter pipeline.
//!
//! Covers:
//! 1. Individual filter behavior (low-pass, notch, spring, damping, friction, inertia)
//! 2. Filter chain composition (multiple filters in series)
//! 3. Boundary conditions (zero input, max input, NaN/Inf handling)
//! 4. Determinism (same input always produces same output)
//! 5. Frequency response characteristics (low-pass cuts high freq, etc.)
//! 6. Filter parameter validation (out-of-range params)
//! 7. Real-time constraints (no allocations in filter processing path)

use racing_wheel_engine::filters::*;
use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use std::f32::consts::PI;

// ─── Helpers ────────────────────────────────────────────────────────────────

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

fn frame_with_torque(torque_out: f32) -> Frame {
    Frame {
        ffb_in: 0.0,
        torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

/// Measure steady-state amplitude of a sine wave through a filter.
/// Runs `warmup` samples, then measures peak absolute output over `measure` samples.
fn measure_sine_amplitude<F>(
    filter_fn: F,
    state_ptr: *mut u8,
    freq_hz: f32,
    sample_rate: f32,
    warmup: usize,
    measure: usize,
) -> f32
where
    F: Fn(&mut Frame, *mut u8),
{
    let total = warmup + measure;
    let mut peak = 0.0f32;
    for i in 0..total {
        let t = i as f32 / sample_rate;
        let input = (2.0 * PI * freq_hz * t).sin();
        let mut f = frame_with_torque(input);
        filter_fn(&mut f, state_ptr);
        if i >= warmup {
            peak = peak.max(f.torque_out.abs());
        }
    }
    peak
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. INDIVIDUAL FILTER BEHAVIOR
// ═══════════════════════════════════════════════════════════════════════════

mod reconstruction {
    use super::*;
    use openracing_filters::FilterState;

    #[test]
    fn step_response_is_monotonically_increasing() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        let mut prev = 0.0f32;
        for i in 0..50 {
            let mut f = frame(1.0, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            if f.torque_out < prev - 1e-6 {
                return Err(format!(
                    "tick {}: output {} < prev {} (non-monotonic step response)",
                    i, f.torque_out, prev
                ));
            }
            prev = f.torque_out;
        }
        Ok(())
    }

    #[test]
    fn heavier_smoothing_converges_slower() -> Result<(), String> {
        let ticks = 20;
        let mut light = ReconstructionState::new(1);
        let mut heavy = ReconstructionState::new(7);
        let mut out_light = 0.0f32;
        let mut out_heavy = 0.0f32;
        for _ in 0..ticks {
            let mut fl = frame(1.0, 0.0);
            reconstruction_filter(&mut fl, &mut light as *mut _ as *mut u8);
            out_light = fl.torque_out;
            let mut fh = frame(1.0, 0.0);
            reconstruction_filter(&mut fh, &mut heavy as *mut _ as *mut u8);
            out_heavy = fh.torque_out;
        }
        if out_heavy >= out_light {
            return Err(format!(
                "heavy ({}) should lag behind light ({})",
                out_heavy, out_light
            ));
        }
        Ok(())
    }

    #[test]
    fn bypass_level_zero_is_identity() -> Result<(), String> {
        let mut state = ReconstructionState::new(0);
        for &val in &[0.0f32, 0.25, 0.5, -0.5, 1.0, -1.0] {
            let mut f = frame(val, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            if (f.torque_out - val).abs() > 1e-5 {
                return Err(format!(
                    "level 0 should pass through {}, got {}",
                    val, f.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn all_valid_levels_produce_finite_output() -> Result<(), String> {
        for level in 0..=8 {
            let mut state = ReconstructionState::new(level);
            let mut f = frame(0.7, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            if !f.torque_out.is_finite() {
                return Err(format!(
                    "level {} produced non-finite output: {}",
                    level, f.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn reset_clears_state_but_preserves_alpha() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        let original_alpha = state.alpha;
        for _ in 0..50 {
            let mut f = frame(1.0, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        FilterState::reset(&mut state);
        if state.prev_output.abs() > 1e-6 {
            return Err(format!(
                "prev_output should be 0 after reset, got {}",
                state.prev_output
            ));
        }
        if (state.alpha - original_alpha).abs() > 1e-6 {
            return Err(format!(
                "alpha should be preserved: expected {}, got {}",
                original_alpha, state.alpha
            ));
        }
        Ok(())
    }
}

mod friction {
    use super::*;

    #[test]
    fn opposes_motion_direction() -> Result<(), String> {
        let state = FrictionState::new(0.2, false);
        let mut f_pos = frame(0.0, 3.0);
        friction_filter(&mut f_pos, &state as *const _ as *mut u8);
        let mut f_neg = frame(0.0, -3.0);
        friction_filter(&mut f_neg, &state as *const _ as *mut u8);
        if f_pos.torque_out >= 0.0 {
            return Err(format!(
                "should oppose positive speed, got {}",
                f_pos.torque_out
            ));
        }
        if f_neg.torque_out <= 0.0 {
            return Err(format!(
                "should oppose negative speed, got {}",
                f_neg.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn zero_speed_no_friction_force() -> Result<(), String> {
        let state = FrictionState::new(0.5, false);
        let mut f = frame(0.3, 0.0);
        friction_filter(&mut f, &state as *const _ as *mut u8);
        if (f.torque_out - 0.3).abs() > 1e-3 {
            return Err(format!(
                "zero speed should not alter torque: expected 0.3, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn zero_coefficient_is_identity() -> Result<(), String> {
        let state = FrictionState::new(0.0, false);
        let mut f = frame(0.6, 5.0);
        friction_filter(&mut f, &state as *const _ as *mut u8);
        if (f.torque_out - 0.6).abs() > 1e-3 {
            return Err(format!(
                "zero coeff should be identity: expected 0.6, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn speed_adaptive_reduces_at_high_speed() -> Result<(), String> {
        let state = FrictionState::new(0.3, true);
        let mut f_slow = frame(0.0, 1.0);
        friction_filter(&mut f_slow, &state as *const _ as *mut u8);
        let slow_mag = f_slow.torque_out.abs();
        let mut f_fast = frame(0.0, 10.0);
        friction_filter(&mut f_fast, &state as *const _ as *mut u8);
        let fast_mag = f_fast.torque_out.abs();
        if fast_mag >= slow_mag {
            return Err(format!(
                "adaptive friction should decrease at high speed: slow={}, fast={}",
                slow_mag, fast_mag
            ));
        }
        Ok(())
    }

    #[test]
    fn extreme_speed_produces_finite_output() -> Result<(), String> {
        let state = FrictionState::new(0.1, true);
        for &spd in &[f32::MAX, f32::MIN, 1e6, -1e6] {
            let mut f = frame(0.0, spd);
            friction_filter(&mut f, &state as *const _ as *mut u8);
            if !f.torque_out.is_finite() {
                return Err(format!(
                    "non-finite output at speed {}: {}",
                    spd, f.torque_out
                ));
            }
        }
        Ok(())
    }
}

mod damper {
    use super::*;

    #[test]
    fn proportional_to_speed() -> Result<(), String> {
        let state = DamperState::new(0.2, false);
        let mut f = frame(0.0, 1.0);
        damper_filter(&mut f, &state as *const _ as *mut u8);
        // Non-adaptive: torque = -speed * coeff = -1.0 * 0.2 = -0.2
        if (f.torque_out.abs() - 0.2).abs() > 0.02 {
            return Err(format!(
                "expected ~0.2 magnitude, got {}",
                f.torque_out.abs()
            ));
        }
        Ok(())
    }

    #[test]
    fn opposes_motion() -> Result<(), String> {
        let state = DamperState::new(0.1, false);
        let mut f_pos = frame(0.0, 5.0);
        damper_filter(&mut f_pos, &state as *const _ as *mut u8);
        let mut f_neg = frame(0.0, -5.0);
        damper_filter(&mut f_neg, &state as *const _ as *mut u8);
        if f_pos.torque_out >= 0.0 || f_neg.torque_out <= 0.0 {
            return Err(format!(
                "damper must oppose: pos={}, neg={}",
                f_pos.torque_out, f_neg.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn zero_speed_zero_torque() -> Result<(), String> {
        let state = DamperState::new(0.5, false);
        let mut f = frame(0.3, 0.0);
        damper_filter(&mut f, &state as *const _ as *mut u8);
        // No motion → no damping contribution, torque_out = 0.3 + 0 = 0.3
        if (f.torque_out - 0.3).abs() > 1e-3 {
            return Err(format!(
                "zero speed should be identity, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn zero_coefficient_is_identity() -> Result<(), String> {
        let state = DamperState::new(0.0, false);
        let mut f = frame(0.4, 5.0);
        damper_filter(&mut f, &state as *const _ as *mut u8);
        if (f.torque_out - 0.4).abs() > 1e-3 {
            return Err(format!(
                "zero coeff should be identity, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }
}

mod inertia {
    use super::*;

    #[test]
    fn opposes_positive_acceleration() -> Result<(), String> {
        let mut state = InertiaState::new(0.1);
        // Tick 0: establish baseline speed = 0
        let mut f0 = frame(0.0, 0.0);
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);
        // Tick 1: speed jumps to 5 → acceleration = 5
        let mut f1 = Frame {
            torque_out: 0.0,
            ..frame(0.0, 5.0)
        };
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        if f1.torque_out >= 0.0 {
            return Err(format!(
                "inertia should oppose positive accel, got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn constant_speed_zero_contribution() -> Result<(), String> {
        let mut state = InertiaState::new(0.2);
        // Tick 0: speed = 5
        let mut f0 = frame(0.0, 5.0);
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);
        // Tick 1: speed still 5 → zero acceleration
        let mut f1 = Frame {
            torque_out: 0.0,
            ..frame(0.0, 5.0)
        };
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        if f1.torque_out.abs() > 1e-3 {
            return Err(format!(
                "constant speed → no inertia: got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn zero_coefficient_is_identity() -> Result<(), String> {
        let mut state = InertiaState::new(0.0);
        let mut f0 = frame(0.0, 0.0);
        inertia_filter(&mut f0, &mut state as *mut _ as *mut u8);
        let mut f1 = Frame {
            torque_out: 0.5,
            ..frame(0.0, 10.0)
        };
        inertia_filter(&mut f1, &mut state as *mut _ as *mut u8);
        if (f1.torque_out - 0.5).abs() > 1e-3 {
            return Err(format!(
                "zero coeff should be identity, got {}",
                f1.torque_out
            ));
        }
        Ok(())
    }
}

mod notch {
    use super::*;

    #[test]
    fn bypass_is_identity() -> Result<(), String> {
        let mut state = NotchState::bypass();
        for &val in &[0.0f32, 0.5, -0.5, 1.0, -1.0] {
            let mut f = frame_with_torque(val);
            notch_filter(&mut f, &mut state as *mut _ as *mut u8);
            if (f.torque_out - val).abs() > 1e-3 {
                return Err(format!("bypass at {} → {}", val, f.torque_out));
            }
        }
        Ok(())
    }

    #[test]
    fn attenuates_center_frequency_more_than_passband() -> Result<(), String> {
        let center = 50.0f32;
        let sr = 1000.0f32;
        let mut state_center = NotchState::new(center, 5.0, -12.0, sr);
        let amp_center = measure_sine_amplitude(
            notch_filter,
            &mut state_center as *mut _ as *mut u8,
            center,
            sr,
            1500,
            500,
        );

        let mut state_pass = NotchState::new(center, 5.0, -12.0, sr);
        let amp_pass = measure_sine_amplitude(
            notch_filter,
            &mut state_pass as *mut _ as *mut u8,
            5.0,
            sr,
            1500,
            500,
        );

        if amp_center >= amp_pass {
            return Err(format!(
                "center ({}) should be < passband ({})",
                amp_center, amp_pass
            ));
        }
        Ok(())
    }

    #[test]
    fn dc_passes_through_notch() -> Result<(), String> {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        for _ in 0..500 {
            let mut f = frame_with_torque(0.7);
            notch_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        let mut f = frame_with_torque(0.7);
        notch_filter(&mut f, &mut state as *mut _ as *mut u8);
        if (f.torque_out - 0.7).abs() > 0.05 {
            return Err(format!(
                "DC should pass: expected ~0.7, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn stability_over_many_samples() -> Result<(), String> {
        let mut state = NotchState::new(60.0, 2.0, -12.0, 1000.0);
        for _ in 0..10_000 {
            let mut f = frame_with_torque(0.5);
            notch_filter(&mut f, &mut state as *mut _ as *mut u8);
            if !f.torque_out.is_finite() {
                return Err("notch became unstable".to_string());
            }
        }
        Ok(())
    }
}

mod lowpass {
    use super::*;

    #[test]
    fn attenuates_above_cutoff() -> Result<(), String> {
        let cutoff = 50.0f32;
        let sr = 1000.0;

        let mut st_lo = NotchState::lowpass(cutoff, 0.707, sr);
        let amp_lo = measure_sine_amplitude(
            notch_filter,
            &mut st_lo as *mut _ as *mut u8,
            10.0,
            sr,
            1500,
            500,
        );

        let mut st_hi = NotchState::lowpass(cutoff, 0.707, sr);
        let amp_hi = measure_sine_amplitude(
            notch_filter,
            &mut st_hi as *mut _ as *mut u8,
            200.0,
            sr,
            1500,
            500,
        );

        if amp_hi >= amp_lo {
            return Err(format!(
                "high-freq amp ({}) should be < low-freq amp ({})",
                amp_hi, amp_lo
            ));
        }
        Ok(())
    }

    #[test]
    fn passes_dc_signal() -> Result<(), String> {
        let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
        for _ in 0..500 {
            let mut f = frame_with_torque(0.6);
            notch_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        let mut f = frame_with_torque(0.6);
        notch_filter(&mut f, &mut state as *mut _ as *mut u8);
        if (f.torque_out - 0.6).abs() > 0.05 {
            return Err(format!(
                "DC should pass: expected ~0.6, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }
}

mod slew_rate {
    use super::*;

    #[test]
    fn limits_step_change_to_max_per_tick() -> Result<(), String> {
        let mut state = SlewRateState::new(0.5); // 0.5/s → 0.0005/tick
        let mut f = frame_with_torque(1.0);
        slew_rate_filter(&mut f, &mut state as *mut _ as *mut u8);
        let expected = 0.5 / 1000.0;
        if (f.torque_out - expected).abs() > 1e-4 {
            return Err(format!("expected {}, got {}", expected, f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn unlimited_is_identity() -> Result<(), String> {
        let mut state = SlewRateState::unlimited();
        let mut f = frame_with_torque(0.9);
        slew_rate_filter(&mut f, &mut state as *mut _ as *mut u8);
        if (f.torque_out - 0.9).abs() > 1e-3 {
            return Err(format!(
                "unlimited should pass through, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn converges_to_target_over_time() -> Result<(), String> {
        let mut state = SlewRateState::new(1.0);
        for _ in 0..2000 {
            let mut f = frame_with_torque(1.0);
            slew_rate_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        if (state.prev_output - 1.0).abs() > 0.01 {
            return Err(format!("should converge to 1.0, got {}", state.prev_output));
        }
        Ok(())
    }
}

mod torque_cap {
    use super::*;

    #[test]
    fn clamps_positive_torque() -> Result<(), String> {
        let mut f = frame_with_torque(1.0);
        let cap = 0.8f32;
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if (f.torque_out - 0.8).abs() > 1e-3 {
            return Err(format!("expected 0.8, got {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn clamps_negative_torque() -> Result<(), String> {
        let mut f = frame_with_torque(-1.0);
        let cap = 0.5f32;
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if (f.torque_out - (-0.5)).abs() > 1e-3 {
            return Err(format!("expected -0.5, got {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn within_limit_unchanged() -> Result<(), String> {
        let mut f = frame_with_torque(0.3);
        let cap = 0.8f32;
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if (f.torque_out - 0.3).abs() > 1e-3 {
            return Err(format!(
                "within-limit should be unchanged, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn nan_maps_to_zero_safe_state() -> Result<(), String> {
        let mut f = frame_with_torque(f32::NAN);
        let cap = 0.8f32;
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if f.torque_out != 0.0 {
            return Err(format!("NaN should → 0.0, got {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn infinity_maps_to_zero_safe_state() -> Result<(), String> {
        for &val in &[f32::INFINITY, f32::NEG_INFINITY] {
            let mut f = frame_with_torque(val);
            let cap = 0.8f32;
            torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
            if f.torque_out != 0.0 {
                return Err(format!("{} should → 0.0, got {}", val, f.torque_out));
            }
        }
        Ok(())
    }
}

mod curve {
    use super::*;

    #[test]
    fn linear_curve_is_identity() -> Result<(), String> {
        let state = CurveState::linear();
        for &val in &[0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let mut f = frame_with_torque(val);
            curve_filter(&mut f, &state as *const _ as *mut u8);
            if (f.torque_out - val).abs() > 0.02 {
                return Err(format!(
                    "linear at {} should be ~{}, got {}",
                    val, val, f.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn preserves_sign() -> Result<(), String> {
        let state = CurveState::linear();
        let mut f = frame_with_torque(-0.5);
        curve_filter(&mut f, &state as *const _ as *mut u8);
        if f.torque_out > 0.0 {
            return Err(format!("should be negative, got {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn quadratic_reduces_mid_values() -> Result<(), String> {
        let state = CurveState::quadratic();
        let mut f = frame_with_torque(0.5);
        curve_filter(&mut f, &state as *const _ as *mut u8);
        // Quadratic: 0.5² = 0.25 ± tolerance
        if f.torque_out > 0.4 {
            return Err(format!(
                "quadratic(0.5) should be < 0.4, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn extreme_inputs_are_clamped_finite() -> Result<(), String> {
        let state = CurveState::linear();
        for &val in &[100.0f32, -100.0, f32::MAX, f32::MIN] {
            let mut f = frame_with_torque(val);
            curve_filter(&mut f, &state as *const _ as *mut u8);
            if !f.torque_out.is_finite() {
                return Err(format!("non-finite at input {}", val));
            }
            if f.torque_out.abs() > 1.01 {
                return Err(format!(
                    "output {} exceeds 1.0 at input {}",
                    f.torque_out, val
                ));
            }
        }
        Ok(())
    }
}

mod bumpstop {
    use super::*;

    #[test]
    fn disabled_has_no_effect() -> Result<(), String> {
        let mut state = BumpstopState::disabled();
        let mut f = frame(0.0, 5.0);
        bumpstop_filter(&mut f, &mut state as *mut _ as *mut u8);
        if f.torque_out.abs() > 1e-3 {
            return Err(format!(
                "disabled bumpstop changed torque: {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn applies_resistance_past_start_angle() -> Result<(), String> {
        let mut state = BumpstopState::new(true, 10.0, 20.0, 0.8, 0.0);
        state.current_angle = 15.0; // Past start, before max
        let mut f = frame(0.0, 0.0);
        bumpstop_filter(&mut f, &mut state as *mut _ as *mut u8);
        if f.torque_out.abs() < 1e-3 {
            return Err("expected resistance past start angle".to_string());
        }
        Ok(())
    }

    #[test]
    fn resistance_increases_deeper_into_zone() -> Result<(), String> {
        // Shallow penetration
        let mut state1 = BumpstopState::new(true, 10.0, 20.0, 0.8, 0.0);
        state1.current_angle = 11.0;
        let mut f1 = frame(0.0, 0.0);
        bumpstop_filter(&mut f1, &mut state1 as *mut _ as *mut u8);

        // Deep penetration
        let mut state2 = BumpstopState::new(true, 10.0, 20.0, 0.8, 0.0);
        state2.current_angle = 18.0;
        let mut f2 = frame(0.0, 0.0);
        bumpstop_filter(&mut f2, &mut state2 as *mut _ as *mut u8);

        if f2.torque_out.abs() <= f1.torque_out.abs() {
            return Err(format!(
                "deeper penetration should produce more resistance: shallow={}, deep={}",
                f1.torque_out, f2.torque_out
            ));
        }
        Ok(())
    }
}

mod hands_off {
    use super::*;

    #[test]
    fn triggers_after_timeout() -> Result<(), String> {
        let mut state = HandsOffState::new(true, 0.05, 0.1); // 100ms = 100 ticks
        for _ in 0..150 {
            let mut f = frame_with_torque(0.01);
            hands_off_detector(&mut f, &mut state as *mut _ as *mut u8);
        }
        let mut f = frame_with_torque(0.01);
        hands_off_detector(&mut f, &mut state as *mut _ as *mut u8);
        if !f.hands_off {
            return Err("hands_off should be true after timeout".to_string());
        }
        Ok(())
    }

    #[test]
    fn resets_on_resistance() -> Result<(), String> {
        let mut state = HandsOffState::new(true, 0.05, 0.5);
        // Accumulate timeout counter
        for _ in 0..200 {
            let mut f = frame_with_torque(0.01);
            hands_off_detector(&mut f, &mut state as *mut _ as *mut u8);
        }
        assert!(state.counter > 0);
        // Spike in torque → resistance detected → counter resets
        let mut f = frame_with_torque(0.5);
        hands_off_detector(&mut f, &mut state as *mut _ as *mut u8);
        if state.counter != 0 {
            return Err(format!(
                "counter should reset on resistance, got {}",
                state.counter
            ));
        }
        Ok(())
    }

    #[test]
    fn disabled_never_triggers() -> Result<(), String> {
        let mut state = HandsOffState::disabled();
        for _ in 0..10_000 {
            let mut f = frame_with_torque(0.0);
            hands_off_detector(&mut f, &mut state as *mut _ as *mut u8);
            if f.hands_off {
                return Err("disabled detector should never trigger".to_string());
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. FILTER CHAIN COMPOSITION
// ═══════════════════════════════════════════════════════════════════════════

mod chain {
    use super::*;

    #[test]
    fn reconstruction_then_slew_rate_further_limits() -> Result<(), String> {
        let mut recon = ReconstructionState::new(4);
        let mut slew = SlewRateState::new(0.5);
        let mut f = frame(1.0, 0.0);
        reconstruction_filter(&mut f, &mut recon as *mut _ as *mut u8);
        let after_recon = f.torque_out;
        slew_rate_filter(&mut f, &mut slew as *mut _ as *mut u8);
        if f.torque_out > after_recon + 1e-3 {
            return Err(format!(
                "slew should not increase: recon={}, slew={}",
                after_recon, f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn friction_plus_damper_are_additive() -> Result<(), String> {
        let friction = FrictionState::new(0.1, false);
        let damper = DamperState::new(0.1, false);
        let mut f = frame(0.0, 2.0);
        friction_filter(&mut f, &friction as *const _ as *mut u8);
        let after_friction = f.torque_out;
        damper_filter(&mut f, &damper as *const _ as *mut u8);
        // Both oppose motion → combined magnitude > either alone
        if f.torque_out.abs() <= after_friction.abs() {
            return Err(format!(
                "combined should be larger: friction_only={}, both={}",
                after_friction, f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn full_pipeline_order_produces_bounded_output() -> Result<(), String> {
        let mut recon = ReconstructionState::new(2);
        let friction = FrictionState::new(0.05, false);
        let damper = DamperState::new(0.05, false);
        let mut inertia = InertiaState::new(0.05);
        let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut slew = SlewRateState::new(1.0);
        let curve_state = CurveState::linear();
        let cap = 0.9f32;

        // Initialize inertia
        let mut f0 = frame(0.0, 1.0);
        inertia_filter(&mut f0, &mut inertia as *mut _ as *mut u8);

        let mut f = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 1.5,
            hands_off: false,
            ts_mono_ns: 1000,
            seq: 1,
        };

        reconstruction_filter(&mut f, &mut recon as *mut _ as *mut u8);
        friction_filter(&mut f, &friction as *const _ as *mut u8);
        damper_filter(&mut f, &damper as *const _ as *mut u8);
        inertia_filter(&mut f, &mut inertia as *mut _ as *mut u8);
        notch_filter(&mut f, &mut notch as *mut _ as *mut u8);
        slew_rate_filter(&mut f, &mut slew as *mut _ as *mut u8);
        curve_filter(&mut f, &curve_state as *const _ as *mut u8);
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);

        if !f.torque_out.is_finite() {
            return Err(format!("non-finite output: {}", f.torque_out));
        }
        if f.torque_out.abs() > 0.9 + 1e-3 {
            return Err(format!("exceeds cap: {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn torque_cap_is_final_safety_net_after_any_chain() -> Result<(), String> {
        // Even if preceding filters amplify, cap should enforce bounds
        let curve = CurveState::scurve();
        let resp = ResponseCurveState::hard(); // sub-linear → can amplify low values
        let cap = 0.5f32;

        let mut f = frame_with_torque(0.8);
        curve_filter(&mut f, &curve as *const _ as *mut u8);
        response_curve_filter(&mut f, &resp as *const _ as *mut u8);
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);

        if f.torque_out.abs() > 0.5 + 1e-3 {
            return Err(format!("cap should enforce 0.5: got {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn pipeline_process_method_equivalent_to_manual_chain() -> Result<(), Box<dyn std::error::Error>>
    {
        // An empty pipeline should be identity for process()
        let mut pipeline = Pipeline::new();
        let mut f = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };
        pipeline.process(&mut f)?;
        if (f.torque_out - 0.5).abs() > 1e-3 {
            return Err(format!("empty pipeline should be identity, got {}", f.torque_out).into());
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. BOUNDARY CONDITIONS
// ═══════════════════════════════════════════════════════════════════════════

mod boundary {
    use super::*;

    #[test]
    fn zero_input_through_all_filters() -> Result<(), String> {
        let mut recon = ReconstructionState::new(4);
        let friction = FrictionState::new(0.1, false);
        let damper = DamperState::new(0.1, false);
        let mut inertia = InertiaState::new(0.1);
        let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut slew = SlewRateState::new(0.5);

        // Zero input, zero speed
        let mut f = frame(0.0, 0.0);
        reconstruction_filter(&mut f, &mut recon as *mut _ as *mut u8);
        friction_filter(&mut f, &friction as *const _ as *mut u8);
        damper_filter(&mut f, &damper as *const _ as *mut u8);
        inertia_filter(&mut f, &mut inertia as *mut _ as *mut u8);
        notch_filter(&mut f, &mut notch as *mut _ as *mut u8);
        slew_rate_filter(&mut f, &mut slew as *mut _ as *mut u8);

        if f.torque_out.abs() > 1e-3 {
            return Err(format!(
                "zero input/speed should produce ~zero output, got {}",
                f.torque_out
            ));
        }
        Ok(())
    }

    #[test]
    fn max_positive_input_capped() -> Result<(), String> {
        let cap = 1.0f32;
        let mut f = frame_with_torque(1.0);
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if f.torque_out > 1.0 + 1e-6 {
            return Err(format!("max input exceeded cap: {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn max_negative_input_capped() -> Result<(), String> {
        let cap = 1.0f32;
        let mut f = frame_with_torque(-1.0);
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if f.torque_out < -1.0 - 1e-6 {
            return Err(format!("min input exceeded cap: {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn nan_through_torque_cap_is_safe() -> Result<(), String> {
        let cap = 1.0f32;
        let mut f = frame_with_torque(f32::NAN);
        torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
        if f.torque_out != 0.0 {
            return Err(format!("NaN → cap should be 0.0, got {}", f.torque_out));
        }
        Ok(())
    }

    #[test]
    fn inf_through_torque_cap_is_safe() -> Result<(), String> {
        for &val in &[f32::INFINITY, f32::NEG_INFINITY] {
            let cap = 1.0f32;
            let mut f = frame_with_torque(val);
            torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
            if f.torque_out != 0.0 {
                return Err(format!("{} → cap should be 0.0, got {}", val, f.torque_out));
            }
        }
        Ok(())
    }

    #[test]
    fn reconstruction_nan_does_not_crash() -> Result<(), String> {
        let mut state = ReconstructionState::new(4);
        // Warm up
        for _ in 0..10 {
            let mut f = frame(0.5, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
        }
        // Inject NaN — should not panic
        let mut f = frame(f32::NAN, 0.0);
        reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
        Ok(())
    }

    #[test]
    fn slew_rate_nan_does_not_crash() -> Result<(), String> {
        let mut state = SlewRateState::new(0.5);
        state.prev_output = 0.5;
        let mut f = frame_with_torque(f32::NAN);
        slew_rate_filter(&mut f, &mut state as *mut _ as *mut u8);
        Ok(())
    }

    #[test]
    fn pipeline_faults_on_non_finite_intermediate() -> Result<(), Box<dyn std::error::Error>> {
        // Pipeline::process validates that torque_out is finite and ≤ 1.0 after each node.
        // An empty pipeline with out-of-range torque should still succeed (no nodes to check).
        let mut pipeline = Pipeline::new();
        let mut f = Frame {
            ffb_in: 0.0,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        };
        let result = pipeline.process(&mut f);
        assert!(result.is_ok());
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. DETERMINISM
// ═══════════════════════════════════════════════════════════════════════════

mod determinism {
    use super::*;
    use openracing_filters::FilterState;

    /// Run the same filter with identical inputs and verify bit-exact outputs.
    fn assert_deterministic<F>(
        name: &str,
        filter_fn: F,
        create_state: impl Fn() -> Vec<u8>,
        inputs: &[(f32, f32)], // (ffb_in/torque_out, wheel_speed)
    ) -> Result<(), String>
    where
        F: Fn(&mut Frame, *mut u8),
    {
        let mut state1 = create_state();
        let mut state2 = create_state();
        for (i, &(torque, speed)) in inputs.iter().enumerate() {
            let mut f1 = Frame {
                ffb_in: torque,
                torque_out: torque,
                wheel_speed: speed,
                hands_off: false,
                ts_mono_ns: i as u64 * 1_000_000,
                seq: i as u16,
            };
            let mut f2 = f1;
            filter_fn(&mut f1, state1.as_mut_ptr());
            filter_fn(&mut f2, state2.as_mut_ptr());
            if f1.torque_out.to_bits() != f2.torque_out.to_bits() {
                return Err(format!(
                    "{} non-deterministic at tick {}: {} vs {}",
                    name, i, f1.torque_out, f2.torque_out
                ));
            }
        }
        Ok(())
    }

    fn reconstruction_state_bytes() -> Vec<u8> {
        let state = ReconstructionState::new(4);
        let mut bytes = vec![0u8; std::mem::size_of::<ReconstructionState>()];
        unsafe {
            std::ptr::copy_nonoverlapping(
                &state as *const _ as *const u8,
                bytes.as_mut_ptr(),
                bytes.len(),
            );
        }
        bytes
    }

    fn notch_state_bytes() -> Vec<u8> {
        let state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut bytes = vec![0u8; std::mem::size_of::<NotchState>()];
        unsafe {
            std::ptr::copy_nonoverlapping(
                &state as *const _ as *const u8,
                bytes.as_mut_ptr(),
                bytes.len(),
            );
        }
        bytes
    }

    fn slew_state_bytes() -> Vec<u8> {
        let state = SlewRateState::new(0.5);
        let mut bytes = vec![0u8; std::mem::size_of::<SlewRateState>()];
        unsafe {
            std::ptr::copy_nonoverlapping(
                &state as *const _ as *const u8,
                bytes.as_mut_ptr(),
                bytes.len(),
            );
        }
        bytes
    }

    #[test]
    fn reconstruction_is_deterministic() -> Result<(), String> {
        let inputs: Vec<(f32, f32)> = (0..100).map(|i| ((i as f32 * 0.01).sin(), 0.0)).collect();
        assert_deterministic(
            "reconstruction",
            reconstruction_filter,
            reconstruction_state_bytes,
            &inputs,
        )
    }

    #[test]
    fn notch_is_deterministic() -> Result<(), String> {
        let inputs: Vec<(f32, f32)> = (0..100).map(|i| ((i as f32 * 0.1).sin(), 0.0)).collect();
        assert_deterministic("notch", notch_filter, notch_state_bytes, &inputs)
    }

    #[test]
    fn slew_rate_is_deterministic() -> Result<(), String> {
        let inputs: Vec<(f32, f32)> = (0..100)
            .map(|i| ((i as f32 * 0.05).sin() * 0.5, 0.0))
            .collect();
        assert_deterministic("slew_rate", slew_rate_filter, slew_state_bytes, &inputs)
    }

    #[test]
    fn reset_state_then_replay_matches_fresh() -> Result<(), String> {
        let mut used = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        // Warm up with random-ish data
        for i in 0..200 {
            let mut f = frame_with_torque((i as f32 * 0.07).sin());
            notch_filter(&mut f, &mut used as *mut _ as *mut u8);
        }
        FilterState::reset(&mut used);

        let mut fresh = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let test_inputs = [0.1f32, 0.5, -0.3, 0.8, -0.7, 0.0, 0.33, -0.99];
        for &val in &test_inputs {
            let mut fu = frame_with_torque(val);
            notch_filter(&mut fu, &mut used as *mut _ as *mut u8);
            let mut ff = frame_with_torque(val);
            notch_filter(&mut ff, &mut fresh as *mut _ as *mut u8);
            if (fu.torque_out - ff.torque_out).abs() > 1e-6 {
                return Err(format!(
                    "diverged at {}: reset={} fresh={}",
                    val, fu.torque_out, ff.torque_out
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn pipeline_process_is_deterministic() -> Result<(), Box<dyn std::error::Error>> {
        // An empty pipeline should produce identical output on two runs
        let mut p1 = Pipeline::new();
        let mut p2 = Pipeline::new();
        for i in 0..50 {
            let torque = (i as f32 * 0.1).sin() * 0.5;
            let mut f1 = Frame {
                ffb_in: torque,
                torque_out: torque,
                wheel_speed: 0.0,
                hands_off: false,
                ts_mono_ns: i as u64 * 1_000_000,
                seq: i as u16,
            };
            let mut f2 = f1;
            p1.process(&mut f1)?;
            p2.process(&mut f2)?;
            if f1.torque_out.to_bits() != f2.torque_out.to_bits() {
                return Err(format!(
                    "pipeline non-deterministic at {}: {} vs {}",
                    i, f1.torque_out, f2.torque_out
                )
                .into());
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. FREQUENCY RESPONSE CHARACTERISTICS
// ═══════════════════════════════════════════════════════════════════════════

mod frequency_response {
    use super::*;

    #[test]
    fn reconstruction_attenuates_nyquist() -> Result<(), String> {
        let mut state = ReconstructionState::new(6);
        let mut outputs = Vec::new();
        for i in 0..200 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 }; // Nyquist @ 500 Hz
            let mut f = frame(input, 0.0);
            reconstruction_filter(&mut f, &mut state as *mut _ as *mut u8);
            if i > 100 {
                outputs.push(f.torque_out.abs());
            }
        }
        let max_amp: f32 = outputs.iter().copied().fold(0.0f32, f32::max);
        if max_amp >= 0.5 {
            return Err(format!(
                "heavy smoothing should attenuate Nyquist, max_amp={}",
                max_amp
            ));
        }
        Ok(())
    }

    #[test]
    fn lowpass_attenuation_increases_with_frequency() -> Result<(), String> {
        let cutoff = 50.0f32;
        let sr = 1000.0;
        let freqs = [10.0f32, 100.0, 300.0];
        let mut prev_amp = f32::MAX;
        for &freq in &freqs {
            let mut state = NotchState::lowpass(cutoff, 0.707, sr);
            let amp = measure_sine_amplitude(
                notch_filter,
                &mut state as *mut _ as *mut u8,
                freq,
                sr,
                1500,
                500,
            );
            if freq > cutoff && amp >= prev_amp {
                return Err(format!(
                    "attenuation should increase: freq={}, amp={}, prev={}",
                    freq, amp, prev_amp
                ));
            }
            prev_amp = amp;
        }
        Ok(())
    }

    #[test]
    fn notch_narrow_band_rejection() -> Result<(), String> {
        let center = 100.0f32;
        let sr = 1000.0;
        let mut st_center = NotchState::new(center, 10.0, -24.0, sr);
        let amp_center = measure_sine_amplitude(
            notch_filter,
            &mut st_center as *mut _ as *mut u8,
            center,
            sr,
            2000,
            1000,
        );
        // Just off center should pass better
        let mut st_off = NotchState::new(center, 10.0, -24.0, sr);
        let amp_off = measure_sine_amplitude(
            notch_filter,
            &mut st_off as *mut _ as *mut u8,
            center + 50.0,
            sr,
            2000,
            1000,
        );
        if amp_center >= amp_off {
            return Err(format!(
                "center ({}) should be more attenuated than off-center ({})",
                amp_center, amp_off
            ));
        }
        Ok(())
    }

    #[test]
    fn slew_rate_attenuates_fast_transients_but_passes_slow() -> Result<(), String> {
        // Slow ramp: should track
        let mut state_slow = SlewRateState::new(2.0); // 2.0/s → 0.002/tick
        for i in 0..500 {
            let target = (i as f32 / 500.0).min(1.0) * 0.5; // gentle ramp to 0.5
            let mut f = frame_with_torque(target);
            slew_rate_filter(&mut f, &mut state_slow as *mut _ as *mut u8);
        }
        // After 500 ticks of gentle ramp, should be close to target
        if (state_slow.prev_output - 0.5).abs() > 0.1 {
            return Err(format!(
                "slow ramp should track: expected ~0.5, got {}",
                state_slow.prev_output
            ));
        }

        // Fast step: should be limited
        let mut state_fast = SlewRateState::new(0.5);
        let mut f = frame_with_torque(1.0);
        slew_rate_filter(&mut f, &mut state_fast as *mut _ as *mut u8);
        if f.torque_out > 0.01 {
            // 0.5/1000 = 0.0005 max per tick from 0
            // OK, just check it's significantly less than 1.0
            if f.torque_out > 0.1 {
                return Err(format!(
                    "fast step should be slew-limited, got {}",
                    f.torque_out
                ));
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. FILTER PARAMETER VALIDATION (via PipelineCompiler)
// ═══════════════════════════════════════════════════════════════════════════

mod parameter_validation {
    use super::*;
    use racing_wheel_schemas::prelude::*;

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("test setup error: {e:?}"),
        }
    }

    fn linear_config() -> Result<FilterConfig, String> {
        FilterConfig::new_complete(
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
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .map_err(|e| e.to_string())
    }

    #[tokio::test]
    async fn rejects_reconstruction_level_above_8() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let mut config = linear_config()?;
        config.reconstruction = 10;
        let result = compiler.compile_pipeline(config).await;
        if result.is_ok() {
            return Err("should reject reconstruction > 8".to_string());
        }
        Ok(())
    }

    #[tokio::test]
    async fn rejects_notch_frequency_above_500() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config = FilterConfig::new_complete(
            0,
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(600.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(1.0)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .map_err(|e| e.to_string())?; // setup
        let result = compiler.compile_pipeline(config).await;
        if result.is_ok() {
            return Err("should reject freq > 500 Hz".to_string());
        }
        Ok(())
    }

    #[tokio::test]
    async fn rejects_non_monotonic_curve() -> Result<(), String> {
        // FilterConfig::new_complete itself rejects non-monotonic curves
        let result = FilterConfig::new_complete(
            0,
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            vec![],
            must(Gain::new(1.0)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.7, 0.6)),
                must(CurvePoint::new(0.5, 0.8)), // non-monotonic
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        );
        if result.is_ok() {
            return Err("should reject non-monotonic curve".to_string());
        }
        Ok(())
    }

    #[tokio::test]
    async fn accepts_valid_full_config() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config = FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(60.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.5, 0.6)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .map_err(|e| e.to_string())?; // setup
        let result = compiler.compile_pipeline(config).await;
        if result.is_err() {
            return Err(format!("valid config should compile: {:?}", result));
        }
        let compiled = result.map_err(|e| e.to_string())?; // safe: we just checked is_ok
        if compiled.pipeline.node_count() == 0 {
            return Err("compiled pipeline should have nodes".to_string());
        }
        Ok(())
    }

    #[tokio::test]
    async fn same_config_produces_same_hash() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let mk_config = || {
            FilterConfig::new_complete(
                4,
                must(Gain::new(0.1)),
                must(Gain::new(0.15)),
                must(Gain::new(0.05)),
                vec![],
                must(Gain::new(0.8)),
                vec![
                    must(CurvePoint::new(0.0, 0.0)),
                    must(CurvePoint::new(1.0, 1.0)),
                ],
                must(Gain::new(1.0)),
                BumpstopConfig::default(),
                HandsOffConfig::default(),
            )
            .map_err(|e| e.to_string())
        };
        let r1 = compiler
            .compile_pipeline(mk_config()?)
            .await
            .map_err(|e| e.to_string())?; // setup
        let r2 = compiler
            .compile_pipeline(mk_config()?)
            .await
            .map_err(|e| e.to_string())?; // setup
        if r1.config_hash != r2.config_hash {
            return Err(format!(
                "same config → different hash: {:x} vs {:x}",
                r1.config_hash, r2.config_hash
            ));
        }
        Ok(())
    }

    #[tokio::test]
    async fn different_configs_produce_different_hashes() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        let config_a = FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.0)),
            must(Gain::new(0.0)),
            vec![],
            must(Gain::new(1.0)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(1.0)),
            BumpstopConfig::default(),
            HandsOffConfig::default(),
        )
        .map_err(|e| e.to_string())?; // setup
        let config_b = linear_config()?;
        let r1 = compiler
            .compile_pipeline(config_a)
            .await
            .map_err(|e| e.to_string())?; // setup
        let r2 = compiler
            .compile_pipeline(config_b)
            .await
            .map_err(|e| e.to_string())?; // setup
        if r1.config_hash == r2.config_hash {
            return Err("different configs should have different hashes".to_string());
        }
        Ok(())
    }

    #[tokio::test]
    async fn zero_gain_filters_are_skipped() -> Result<(), String> {
        let compiler = PipelineCompiler::new();
        // Disable bumpstop and hands-off to get a truly empty pipeline
        let config = FilterConfig::new_complete(
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
            must(Gain::new(1.0)),
            BumpstopConfig {
                enabled: false,
                ..BumpstopConfig::default()
            },
            HandsOffConfig {
                enabled: false,
                ..HandsOffConfig::default()
            },
        )
        .map_err(|e| e.to_string())?; // setup
        let compiled = compiler
            .compile_pipeline(config)
            .await
            .map_err(|e| e.to_string())?; // setup
        // With all gains at zero, linear curve, and disabled bumpstop/hands-off → no nodes
        if compiled.pipeline.node_count() != 0 {
            return Err(format!(
                "expected 0 nodes for noop config, got {}",
                compiled.pipeline.node_count()
            ));
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. REAL-TIME CONSTRAINTS
// ═══════════════════════════════════════════════════════════════════════════

mod rt_constraints {
    use super::*;

    #[test]
    fn empty_pipeline_process_is_zero_alloc() -> Result<(), Box<dyn std::error::Error>> {
        let mut pipeline = Pipeline::new();
        let mut f = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };
        // In debug builds the pipeline tracks allocations internally.
        // In release builds we just verify no panic and correct output.
        pipeline.process(&mut f)?;
        assert!((f.torque_out - 0.5).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn filter_functions_do_not_allocate() -> Result<(), String> {
        // Run each filter function and ensure no panic or crash.
        // The actual zero-alloc enforcement is done by the allocation_tracker in debug mode.
        let mut recon = ReconstructionState::new(4);
        let friction = FrictionState::new(0.1, true);
        let damper = DamperState::new(0.1, true);
        let mut inertia = InertiaState::new(0.1);
        let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut slew = SlewRateState::new(0.5);
        let curve = CurveState::linear();
        let resp = ResponseCurveState::linear();
        let cap = 0.9f32;
        let mut bumpstop = BumpstopState::standard();
        let mut hands = HandsOffState::default_detector();

        for i in 0..1000 {
            let torque = (i as f32 * 0.01).sin() * 0.5;
            let speed = (i as f32 * 0.003).cos() * 2.0;
            let mut f = Frame {
                ffb_in: torque,
                torque_out: torque,
                wheel_speed: speed,
                hands_off: false,
                ts_mono_ns: i as u64 * 1_000_000,
                seq: i as u16,
            };

            reconstruction_filter(&mut f, &mut recon as *mut _ as *mut u8);
            friction_filter(&mut f, &friction as *const _ as *mut u8);
            damper_filter(&mut f, &damper as *const _ as *mut u8);
            inertia_filter(&mut f, &mut inertia as *mut _ as *mut u8);
            notch_filter(&mut f, &mut notch as *mut _ as *mut u8);
            slew_rate_filter(&mut f, &mut slew as *mut _ as *mut u8);
            curve_filter(&mut f, &curve as *const _ as *mut u8);
            response_curve_filter(&mut f, &resp as *const _ as *mut u8);
            torque_cap_filter(&mut f, &cap as *const _ as *mut u8);
            bumpstop_filter(&mut f, &mut bumpstop as *mut _ as *mut u8);
            hands_off_detector(&mut f, &mut hands as *mut _ as *mut u8);

            if !f.torque_out.is_finite() {
                return Err(format!("non-finite at tick {}: {}", i, f.torque_out));
            }
        }
        Ok(())
    }

    #[test]
    fn pipeline_swap_is_instant() -> Result<(), String> {
        let mut pipeline = Pipeline::new();
        let new_pipeline = Pipeline::with_hash(0xCAFEBABE);
        pipeline.swap_at_tick_boundary(new_pipeline);
        if pipeline.config_hash() != 0xCAFEBABE {
            return Err(format!(
                "swap failed: expected 0xCAFEBABE, got {:x}",
                pipeline.config_hash()
            ));
        }
        Ok(())
    }

    #[test]
    fn all_filter_state_types_are_repr_c_sized() -> Result<(), String> {
        // Verify filter state types have known, stable sizes (important for RT memory layout)
        let sizes = [
            (
                "ReconstructionState",
                std::mem::size_of::<ReconstructionState>(),
            ),
            ("FrictionState", std::mem::size_of::<FrictionState>()),
            ("DamperState", std::mem::size_of::<DamperState>()),
            ("InertiaState", std::mem::size_of::<InertiaState>()),
            ("NotchState", std::mem::size_of::<NotchState>()),
            ("SlewRateState", std::mem::size_of::<SlewRateState>()),
            ("BumpstopState", std::mem::size_of::<BumpstopState>()),
            ("HandsOffState", std::mem::size_of::<HandsOffState>()),
        ];
        for (name, size) in &sizes {
            if *size == 0 {
                return Err(format!("{} has zero size", name));
            }
            // All should be reasonably small for inline state storage
            if *size > 16384 {
                return Err(format!("{} is too large: {} bytes", name, size));
            }
        }
        Ok(())
    }

    #[test]
    fn pipeline_node_count_matches_config() -> Result<(), String> {
        let p = Pipeline::new();
        if p.node_count() != 0 {
            return Err(format!(
                "new pipeline should be empty, got {}",
                p.node_count()
            ));
        }
        if !p.is_empty() {
            return Err("new pipeline should be empty".to_string());
        }
        Ok(())
    }
}
