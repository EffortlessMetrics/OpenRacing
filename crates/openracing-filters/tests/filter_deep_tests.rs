//! Deep tests for the openracing-filters crate.
//!
//! Covers: low-pass, high-pass (notch), EMA (reconstruction), slew rate limiter,
//! damper, reconstruction, filter parameter validation, filter reset, and
//! property-based bounded-output tests.

use openracing_filters::prelude::*;
use openracing_filters::Frame;
use std::f32::consts::PI;

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: generate a sinusoidal signal and measure output amplitude
// ═══════════════════════════════════════════════════════════════════════════════

/// Run a biquad (NotchState) filter on a sine wave and return
/// the peak output amplitude (after discarding the first `warmup` samples).
fn measure_biquad_amplitude(state: &mut NotchState, freq_hz: f32, sample_rate: f32, warmup: usize, samples: usize) -> f32 {
    let mut peak = 0.0f32;
    for i in 0..(warmup + samples) {
        let t = i as f32 / sample_rate;
        let input = (2.0 * PI * freq_hz * t).sin();
        let mut frame = Frame::from_torque(input);
        notch_filter(&mut frame, state);
        if i >= warmup {
            peak = peak.max(frame.torque_out.abs());
        }
    }
    peak
}

/// Run the reconstruction (EMA) filter on a sine wave and return
/// the peak output amplitude after warmup.
fn measure_ema_amplitude(state: &mut ReconstructionState, freq_hz: f32, sample_rate: f32, warmup: usize, samples: usize) -> f32 {
    let mut peak = 0.0f32;
    for i in 0..(warmup + samples) {
        let t = i as f32 / sample_rate;
        let input = (2.0 * PI * freq_hz * t).sin();
        let mut frame = Frame::from_torque(input);
        // reconstruction_filter uses ffb_in as input
        frame.ffb_in = input;
        reconstruction_filter(&mut frame, state);
        if i >= warmup {
            peak = peak.max(frame.torque_out.abs());
        }
    }
    peak
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Low-pass filter: frequency response (attenuates above cutoff)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn lowpass_passes_dc() {
    let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
    // Feed constant input (DC) for steady-state
    for _ in 0..500 {
        let mut frame = Frame::from_torque(1.0);
        notch_filter(&mut frame, &mut state);
    }
    let mut final_frame = Frame::from_torque(1.0);
    notch_filter(&mut final_frame, &mut state);
    // DC gain should be ~1.0
    assert!(
        (final_frame.torque_out - 1.0).abs() < 0.05,
        "DC gain should be ~1.0, got {}",
        final_frame.torque_out
    );
}

#[test]
fn lowpass_attenuates_above_cutoff() {
    let cutoff = 50.0;
    let sample_rate = 1000.0;

    // Measure amplitude below cutoff (10 Hz)
    let mut state_low = NotchState::lowpass(cutoff, 0.707, sample_rate);
    let amp_below = measure_biquad_amplitude(&mut state_low, 10.0, sample_rate, 500, 2000);

    // Measure amplitude well above cutoff (400 Hz)
    let mut state_high = NotchState::lowpass(cutoff, 0.707, sample_rate);
    let amp_above = measure_biquad_amplitude(&mut state_high, 400.0, sample_rate, 500, 2000);

    assert!(
        amp_above < amp_below * 0.3,
        "signal above cutoff should be attenuated: below={amp_below}, above={amp_above}"
    );
}

#[test]
fn lowpass_passband_near_unity() {
    let cutoff = 200.0;
    let sample_rate = 1000.0;

    let mut state = NotchState::lowpass(cutoff, 0.707, sample_rate);
    let amp = measure_biquad_amplitude(&mut state, 20.0, sample_rate, 500, 2000);

    assert!(
        amp > 0.8,
        "passband signal should be near unity, got {amp}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. High-pass / Notch: frequency response (attenuates at notch frequency)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn notch_attenuates_center_frequency() {
    let center_freq = 50.0;
    let sample_rate = 1000.0;

    // Measure amplitude at center frequency
    let mut state_center = NotchState::new(center_freq, 2.0, -6.0, sample_rate);
    let amp_center = measure_biquad_amplitude(&mut state_center, center_freq, sample_rate, 500, 2000);

    // Measure amplitude away from center (10 Hz)
    let mut state_off = NotchState::new(center_freq, 2.0, -6.0, sample_rate);
    let amp_off = measure_biquad_amplitude(&mut state_off, 10.0, sample_rate, 500, 2000);

    assert!(
        amp_center < amp_off * 0.5,
        "notch should attenuate center freq: center={amp_center}, off={amp_off}"
    );
}

#[test]
fn notch_passes_frequencies_away_from_center() {
    let center_freq = 200.0;
    let sample_rate = 1000.0;

    let mut state = NotchState::new(center_freq, 2.0, -6.0, sample_rate);
    let amp = measure_biquad_amplitude(&mut state, 20.0, sample_rate, 500, 2000);

    assert!(
        amp > 0.7,
        "notch should pass frequencies away from center, got {amp}"
    );
}

#[test]
fn notch_bypass_passes_all() {
    let mut state = NotchState::bypass();
    let mut frame = Frame::from_torque(0.75);
    notch_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.75).abs() < 0.001,
        "bypass should pass through: got {}",
        frame.torque_out
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. EMA (Reconstruction): convergence to constant input
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ema_converges_to_constant_input() {
    let target = 0.75;
    let mut state = ReconstructionState::new(4); // alpha = 0.1

    for _ in 0..500 {
        let mut frame = Frame::from_torque(target);
        frame.ffb_in = target;
        reconstruction_filter(&mut frame, &mut state);
    }

    assert!(
        (state.prev_output - target).abs() < 0.001,
        "EMA should converge to constant input {target}, got {}",
        state.prev_output
    );
}

#[test]
fn ema_convergence_speed_increases_with_alpha() {
    let target = 1.0;
    let iterations = 50;

    // High alpha (level 0, alpha=1.0) converges immediately
    let mut state_fast = ReconstructionState::new(0);
    for _ in 0..iterations {
        let mut frame = Frame::from_torque(target);
        frame.ffb_in = target;
        reconstruction_filter(&mut frame, &mut state_fast);
    }

    // Low alpha (level 6, alpha=0.03) converges slowly
    let mut state_slow = ReconstructionState::new(6);
    for _ in 0..iterations {
        let mut frame = Frame::from_torque(target);
        frame.ffb_in = target;
        reconstruction_filter(&mut frame, &mut state_slow);
    }

    let error_fast = (state_fast.prev_output - target).abs();
    let error_slow = (state_slow.prev_output - target).abs();

    assert!(
        error_fast <= error_slow,
        "higher alpha should converge faster: fast_err={error_fast}, slow_err={error_slow}"
    );
}

#[test]
fn ema_bypass_passes_through() {
    let mut state = ReconstructionState::bypass(); // alpha = 1.0
    let mut frame = Frame::from_torque(0.42);
    frame.ffb_in = 0.42;
    reconstruction_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.42).abs() < 0.001,
        "bypass EMA should pass through, got {}",
        frame.torque_out
    );
}

#[test]
fn ema_attenuates_high_frequency() {
    let sample_rate = 1000.0;

    // Low frequency: 5 Hz
    let mut state_low = ReconstructionState::new(4);
    let amp_low = measure_ema_amplitude(&mut state_low, 5.0, sample_rate, 200, 1000);

    // High frequency: 400 Hz
    let mut state_high = ReconstructionState::new(4);
    let amp_high = measure_ema_amplitude(&mut state_high, 400.0, sample_rate, 200, 1000);

    assert!(
        amp_high < amp_low,
        "EMA should attenuate high freq more: low={amp_low}, high={amp_high}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Slew rate limiter: output change rate bounded
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn slew_rate_limits_upward_step() {
    let mut state = SlewRateState::new(0.5); // 0.5/s → 0.0005 per tick
    let mut frame = Frame::from_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);

    let max_per_tick = 0.5 / 1000.0;
    assert!(
        frame.torque_out <= max_per_tick + 1e-6,
        "first step should be bounded: got {}",
        frame.torque_out
    );
}

#[test]
fn slew_rate_limits_downward_step() {
    let mut state = SlewRateState::new(0.5);
    state.prev_output = 1.0;
    let mut frame = Frame::from_torque(-1.0); // large downward step
    slew_rate_filter(&mut frame, &mut state);

    let max_per_tick = 0.5 / 1000.0;
    let change = (frame.torque_out - 1.0).abs();
    assert!(
        change <= max_per_tick + 1e-6,
        "downward step should be bounded: change={change}"
    );
}

#[test]
fn slew_rate_per_tick_change_never_exceeds_limit() {
    let slew_rate = 1.0; // 1.0/s
    let max_per_tick = slew_rate / 1000.0;
    let mut state = SlewRateState::new(slew_rate);

    let targets = [1.0, -1.0, 0.5, -0.5, 0.0, 1.0, -1.0];
    let mut prev_output = 0.0f32;

    for &target in &targets {
        for _ in 0..100 {
            let mut frame = Frame::from_torque(target);
            slew_rate_filter(&mut frame, &mut state);
            let change = (frame.torque_out - prev_output).abs();
            assert!(
                change <= max_per_tick + 1e-6,
                "change {change} exceeds max {max_per_tick}"
            );
            prev_output = frame.torque_out;
        }
    }
}

#[test]
fn slew_rate_unlimited_passes_through() {
    let mut state = SlewRateState::unlimited();
    let mut frame = Frame::from_torque(0.99);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.99).abs() < 0.001,
        "unlimited should pass through, got {}",
        frame.torque_out
    );
}

#[test]
fn slew_rate_zero_rate_freezes_output() {
    let mut state = SlewRateState::new(0.0);
    state.prev_output = 0.3;
    let mut frame = Frame::from_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.3).abs() < 0.001,
        "zero rate should freeze output at {}, got {}",
        0.3,
        frame.torque_out
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Damper: viscous damping behavior
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn damper_opposes_positive_motion() {
    let state = DamperState::fixed(0.2);
    let mut frame = Frame::from_ffb(0.0, 5.0); // positive speed
    damper_filter(&mut frame, &state);
    assert!(
        frame.torque_out < 0.0,
        "damper should oppose positive motion: got {}",
        frame.torque_out
    );
}

#[test]
fn damper_opposes_negative_motion() {
    let state = DamperState::fixed(0.2);
    let mut frame = Frame::from_ffb(0.0, -5.0);
    damper_filter(&mut frame, &state);
    assert!(
        frame.torque_out > 0.0,
        "damper should oppose negative motion: got {}",
        frame.torque_out
    );
}

#[test]
fn damper_proportional_to_speed() {
    let state = DamperState::fixed(0.1);

    let mut frame1 = Frame::from_ffb(0.0, 2.0);
    damper_filter(&mut frame1, &state);
    let mag1 = frame1.torque_out.abs();

    let mut frame2 = Frame::from_ffb(0.0, 4.0);
    damper_filter(&mut frame2, &state);
    let mag2 = frame2.torque_out.abs();

    // For non-adaptive damper, torque is proportional to speed
    assert!(
        (mag2 - mag1 * 2.0).abs() < 0.01,
        "non-adaptive damping should be linear: mag1={mag1}, mag2={mag2}"
    );
}

#[test]
fn damper_zero_speed_no_effect() {
    let state = DamperState::fixed(0.5);
    let mut frame = Frame::from_ffb(0.3, 0.0);
    damper_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 0.3).abs() < 0.001,
        "zero speed should produce no damping"
    );
}

#[test]
fn damper_adaptive_increases_with_speed() {
    let state = DamperState::adaptive(0.1);

    let mut frame_slow = Frame::from_ffb(0.0, 1.0);
    damper_filter(&mut frame_slow, &state);

    let mut frame_fast = Frame::from_ffb(0.0, 5.0);
    damper_filter(&mut frame_fast, &state);

    // Adaptive: damping coefficient increases with speed, so torque / speed ratio increases
    let ratio_slow = frame_slow.torque_out.abs() / 1.0;
    let ratio_fast = frame_fast.torque_out.abs() / 5.0;
    assert!(
        ratio_fast > ratio_slow,
        "adaptive damping ratio should increase with speed"
    );
}

#[test]
fn damper_coefficient_scales_torque() {
    let state_low = DamperState::fixed(0.1);
    let state_high = DamperState::fixed(0.5);

    let mut frame_low = Frame::from_ffb(0.0, 3.0);
    damper_filter(&mut frame_low, &state_low);

    let mut frame_high = Frame::from_ffb(0.0, 3.0);
    damper_filter(&mut frame_high, &state_high);

    assert!(
        frame_high.torque_out.abs() > frame_low.torque_out.abs(),
        "higher coefficient should produce more torque"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Reconstruction: signal smoothing quality
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn reconstruction_smooths_step_input() {
    let mut state = ReconstructionState::new(4);
    let mut frame = Frame::from_torque(1.0);
    frame.ffb_in = 1.0;
    reconstruction_filter(&mut frame, &mut state);

    // First sample after step should be < 1.0 (smoothed)
    assert!(
        frame.torque_out < 1.0,
        "step response should be smoothed: got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out > 0.0,
        "step response should be positive: got {}",
        frame.torque_out
    );
}

#[test]
fn reconstruction_heavier_smoothing_is_slower() {
    let target = 1.0;

    let mut state_light = ReconstructionState::light();
    let mut frame_light = Frame::from_torque(target);
    frame_light.ffb_in = target;
    reconstruction_filter(&mut frame_light, &mut state_light);
    let light_out = frame_light.torque_out;

    let mut state_heavy = ReconstructionState::heavy();
    let mut frame_heavy = Frame::from_torque(target);
    frame_heavy.ffb_in = target;
    reconstruction_filter(&mut frame_heavy, &mut state_heavy);
    let heavy_out = frame_heavy.torque_out;

    assert!(
        light_out > heavy_out,
        "lighter smoothing should respond faster: light={light_out}, heavy={heavy_out}"
    );
}

#[test]
fn reconstruction_preserves_steady_state() {
    let mut state = ReconstructionState::new(4);
    let target = 0.6;

    for _ in 0..1000 {
        let mut frame = Frame::from_torque(target);
        frame.ffb_in = target;
        reconstruction_filter(&mut frame, &mut state);
    }

    assert!(
        (state.prev_output - target).abs() < 0.001,
        "should converge to {target}, got {}",
        state.prev_output
    );
}

#[test]
fn reconstruction_reduces_noise() {
    let mut state = ReconstructionState::new(6); // heavy smoothing
    let mut max_output_variation = 0.0f32;

    // Feed noisy signal: alternating +1 and -1
    for i in 0..200 {
        let input = if i % 2 == 0 { 1.0 } else { -1.0 };
        let mut frame = Frame::from_torque(input);
        frame.ffb_in = input;
        reconstruction_filter(&mut frame, &mut state);
        if i > 20 {
            max_output_variation = max_output_variation.max(frame.torque_out.abs());
        }
    }

    // Heavy smoothing should significantly reduce the oscillation
    assert!(
        max_output_variation < 0.5,
        "heavy smoothing should reduce ±1 oscillation, got peak {max_output_variation}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Filter parameter validation: reject invalid parameters
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn notch_q_clamped_to_range() {
    // Q < 0.1 should be clamped
    let state_low = NotchState::new(50.0, 0.001, -6.0, 1000.0);
    assert!(state_low.b0.is_finite());
    assert!(state_low.a1.is_finite());

    // Q > 10 should be clamped
    let state_high = NotchState::new(50.0, 100.0, -6.0, 1000.0);
    assert!(state_high.b0.is_finite());
    assert!(state_high.a1.is_finite());
}

#[test]
fn reconstruction_level_above_max_still_works() {
    // Level > 8 should get same alpha as level 8 (0.01)
    let state_max = ReconstructionState::new(8);
    let state_over = ReconstructionState::new(100);
    assert!(
        (state_max.alpha - state_over.alpha).abs() < f32::EPSILON,
        "levels > 8 should use same alpha as level 8"
    );
}

#[test]
fn slew_rate_very_small_rate_nearly_freezes() {
    // Very small (but valid) slew rate should nearly freeze the output
    let mut state = SlewRateState::new(0.001); // 0.001/s → 0.000001 per tick
    state.prev_output = 0.5;
    let mut frame = Frame::from_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.01,
        "very small rate should nearly freeze output, got {}",
        frame.torque_out
    );
}

#[test]
fn torque_cap_handles_nan() {
    let mut frame = Frame::from_torque(f32::NAN);
    openracing_filters::torque_cap_filter(&mut frame, 0.8);
    assert!(
        frame.torque_out.is_finite(),
        "NaN input should produce finite output"
    );
}

#[test]
fn torque_cap_handles_infinity() {
    let mut frame = Frame::from_torque(f32::INFINITY);
    openracing_filters::torque_cap_filter(&mut frame, 0.8);
    assert!(
        frame.torque_out.is_finite(),
        "infinite input should produce finite output"
    );
    assert!(
        frame.torque_out <= 0.8 + 0.001,
        "should be clamped to max_torque"
    );
}

#[test]
fn damper_coefficient_zero_no_effect() {
    let state = DamperState::fixed(0.0);
    let mut frame = Frame::from_ffb(0.5, 10.0);
    damper_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.001,
        "zero coefficient should produce no damping"
    );
}

#[test]
fn friction_coefficient_zero_no_effect() {
    let state = FrictionState::fixed(0.0);
    let mut frame = Frame::from_ffb(0.5, 10.0);
    openracing_filters::friction_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.001,
        "zero friction coefficient should have no effect"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Filter reset: state cleared after reset
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn reconstruction_reset_clears_state() {
    let mut state = ReconstructionState::new(4);
    // Feed some data
    for _ in 0..50 {
        let mut frame = Frame::from_torque(1.0);
        frame.ffb_in = 1.0;
        reconstruction_filter(&mut frame, &mut state);
    }
    assert!(state.prev_output > 0.5);

    // Reset
    FilterState::reset(&mut state);
    assert!(
        state.prev_output.abs() < f32::EPSILON,
        "reset should clear prev_output"
    );

    // After reset, first sample should start from zero
    let mut frame = Frame::from_torque(1.0);
    frame.ffb_in = 1.0;
    reconstruction_filter(&mut frame, &mut state);
    assert!(
        frame.torque_out < 0.5,
        "output after reset should be small (starting from 0)"
    );
}

#[test]
fn slew_rate_reset_clears_state() {
    let mut state = SlewRateState::new(0.5);
    state.prev_output = 0.8;

    FilterState::reset(&mut state);
    assert!(
        state.prev_output.abs() < f32::EPSILON,
        "reset should clear prev_output"
    );
}

#[test]
fn notch_reset_clears_delay_lines() {
    let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    // Feed some data
    for _ in 0..100 {
        let mut frame = Frame::from_torque(0.5);
        notch_filter(&mut frame, &mut state);
    }
    assert!(state.y1.abs() > 0.0 || state.x1.abs() > 0.0);

    FilterState::reset(&mut state);
    assert!(state.x1.abs() < f32::EPSILON);
    assert!(state.x2.abs() < f32::EPSILON);
    assert!(state.y1.abs() < f32::EPSILON);
    assert!(state.y2.abs() < f32::EPSILON);
}

#[test]
fn inertia_reset_clears_prev_speed() {
    let mut state = InertiaState::new(0.1);
    let mut frame = Frame::from_ffb(0.0, 10.0);
    inertia_filter(&mut frame, &mut state);
    assert!((state.prev_wheel_speed - 10.0).abs() < 0.001);

    FilterState::reset(&mut state);
    assert!(
        state.prev_wheel_speed.abs() < f32::EPSILON,
        "reset should clear prev_wheel_speed"
    );
}

#[test]
fn bumpstop_reset_clears_angle() {
    let mut state = BumpstopState::standard();
    state.current_angle = 500.0;

    FilterState::reset(&mut state);
    assert!(
        state.current_angle.abs() < f32::EPSILON,
        "reset should clear current_angle"
    );
}

#[test]
fn hands_off_reset_clears_counter() {
    let mut state = HandsOffState::new(true, 0.05, 2.0);
    for _ in 0..100 {
        let mut frame = Frame::from_torque(0.01);
        hands_off_detector(&mut frame, &mut state);
    }
    assert!(state.counter > 0);

    FilterState::reset(&mut state);
    assert_eq!(state.counter, 0);
    assert!(state.last_torque.abs() < f32::EPSILON);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Property tests: all filters produce bounded output for bounded input
// ═══════════════════════════════════════════════════════════════════════════════

mod filter_property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        #[test]
        fn prop_reconstruction_bounded(
            level in 0u8..=8,
            inputs in proptest::collection::vec(-1.0f32..=1.0, 1..100),
        ) {
            let mut state = ReconstructionState::new(level);
            for &input in &inputs {
                let mut frame = Frame::from_torque(input);
                frame.ffb_in = input;
                reconstruction_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite());
                prop_assert!(frame.torque_out.abs() <= 1.0 + 1e-6,
                    "output {} out of bounds for input {}", frame.torque_out, input);
            }
        }

        #[test]
        fn prop_slew_rate_bounded(
            slew_rate in 0.01f32..10.0,
            inputs in proptest::collection::vec(-1.0f32..=1.0, 1..100),
        ) {
            let mut state = SlewRateState::new(slew_rate);
            for &input in &inputs {
                let mut frame = Frame::from_torque(input);
                slew_rate_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite());
                // Output can't exceed input range by much (bounded by history)
                prop_assert!(frame.torque_out.abs() <= 1.0 + 1e-6);
            }
        }

        #[test]
        fn prop_damper_finite(
            coeff in 0.0f32..1.0,
            speed in -10.0f32..10.0,
            ffb in -1.0f32..1.0,
        ) {
            let state = DamperState::fixed(coeff);
            let mut frame = Frame::from_ffb(ffb, speed);
            damper_filter(&mut frame, &state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn prop_damper_adaptive_finite(
            coeff in 0.0f32..1.0,
            speed in -10.0f32..10.0,
            ffb in -1.0f32..1.0,
        ) {
            let state = DamperState::adaptive(coeff);
            let mut frame = Frame::from_ffb(ffb, speed);
            damper_filter(&mut frame, &state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn prop_friction_finite(
            coeff in 0.0f32..1.0,
            speed in -10.0f32..10.0,
            ffb in -1.0f32..1.0,
        ) {
            let state = FrictionState::fixed(coeff);
            let mut frame = Frame::from_ffb(ffb, speed);
            openracing_filters::friction_filter(&mut frame, &state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn prop_inertia_finite(
            coeff in 0.0f32..1.0,
            speeds in proptest::collection::vec(-10.0f32..10.0, 2..50),
        ) {
            let mut state = InertiaState::new(coeff);
            for &speed in &speeds {
                let mut frame = Frame::from_ffb(0.0, speed);
                inertia_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite());
            }
        }

        #[test]
        fn prop_notch_finite(
            freq in 10.0f32..400.0,
            q in 0.1f32..10.0,
            inputs in proptest::collection::vec(-1.0f32..=1.0, 1..100),
        ) {
            let mut state = NotchState::new(freq, q, -6.0, 1000.0);
            for &input in &inputs {
                let mut frame = Frame::from_torque(input);
                notch_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite(),
                    "non-finite output for freq={freq}, q={q}, input={input}");
            }
        }

        #[test]
        fn prop_lowpass_finite(
            freq in 10.0f32..400.0,
            inputs in proptest::collection::vec(-1.0f32..=1.0, 1..100),
        ) {
            let mut state = NotchState::lowpass(freq, 0.707, 1000.0);
            for &input in &inputs {
                let mut frame = Frame::from_torque(input);
                notch_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite());
            }
        }

        #[test]
        fn prop_torque_cap_bounded(
            input in -10.0f32..10.0,
            max_torque in 0.01f32..2.0,
        ) {
            let mut frame = Frame::from_torque(input);
            openracing_filters::torque_cap_filter(&mut frame, max_torque);
            prop_assert!(frame.torque_out.is_finite());
            prop_assert!(frame.torque_out.abs() <= max_torque + 1e-6,
                "output {} exceeds cap {}", frame.torque_out, max_torque);
        }

        #[test]
        fn prop_bumpstop_finite(
            speed in -100.0f32..100.0,
            start_angle in 100.0f32..500.0,
        ) {
            let max_angle = start_angle + 50.0;
            let mut state = BumpstopState::new(true, start_angle, max_angle, 0.5, 0.2);
            let mut frame = Frame::from_ffb(0.0, speed);
            bumpstop_filter(&mut frame, &mut state);
            prop_assert!(frame.torque_out.is_finite());
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Additional filter-specific edge case tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn friction_opposes_motion_direction() {
    let state = FrictionState::fixed(0.1);

    let mut frame_pos = Frame::from_ffb(0.0, 1.0);
    openracing_filters::friction_filter(&mut frame_pos, &state);
    assert!(frame_pos.torque_out < 0.0, "should oppose positive motion");

    let mut frame_neg = Frame::from_ffb(0.0, -1.0);
    openracing_filters::friction_filter(&mut frame_neg, &state);
    assert!(frame_neg.torque_out > 0.0, "should oppose negative motion");
}

#[test]
fn friction_zero_speed_no_effect() {
    let state = FrictionState::fixed(0.2);
    let mut frame = Frame::from_ffb(0.5, 0.0);
    openracing_filters::friction_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.001,
        "zero speed → no friction"
    );
}

#[test]
fn inertia_opposes_acceleration() {
    let mut state = InertiaState::new(0.1);

    // First tick sets baseline
    let mut frame0 = Frame::from_ffb(0.0, 0.0);
    inertia_filter(&mut frame0, &mut state);

    // Accelerate
    let mut frame1 = Frame::from_ffb(0.0, 5.0);
    frame1.torque_out = 0.0;
    inertia_filter(&mut frame1, &mut state);
    assert!(
        frame1.torque_out < 0.0,
        "inertia should oppose positive acceleration"
    );
}

#[test]
fn inertia_constant_speed_no_effect() {
    let mut state = InertiaState::new(0.1);

    let mut frame0 = Frame::from_ffb(0.0, 5.0);
    inertia_filter(&mut frame0, &mut state);

    let mut frame1 = Frame::from_ffb(0.0, 5.0);
    frame1.torque_out = 0.0;
    inertia_filter(&mut frame1, &mut state);

    assert!(
        frame1.torque_out.abs() < 0.001,
        "constant speed should produce no inertia torque"
    );
}

#[test]
fn frame_from_ffb_initializes_correctly() {
    let frame = Frame::from_ffb(0.5, 1.2);
    assert!((frame.ffb_in - 0.5).abs() < f32::EPSILON);
    assert!((frame.torque_out - 0.5).abs() < f32::EPSILON);
    assert!((frame.wheel_speed - 1.2).abs() < f32::EPSILON);
    assert!(!frame.hands_off);
}

#[test]
fn frame_from_torque_initializes_correctly() {
    let frame = Frame::from_torque(0.7);
    assert!((frame.ffb_in - 0.7).abs() < f32::EPSILON);
    assert!((frame.torque_out - 0.7).abs() < f32::EPSILON);
    assert!((frame.wheel_speed).abs() < f32::EPSILON);
}

#[test]
fn hands_off_detects_after_timeout() {
    let mut state = HandsOffState::new(true, 0.05, 0.1); // 100ms timeout = 100 ticks
    let mut detected = false;

    for _ in 0..200 {
        let mut frame = Frame::from_torque(0.01); // below threshold
        hands_off_detector(&mut frame, &mut state);
        if frame.hands_off {
            detected = true;
            break;
        }
    }

    assert!(detected, "hands-off should be detected after timeout");
}

#[test]
fn hands_off_resets_on_torque_change() {
    let mut state = HandsOffState::new(true, 0.05, 0.5);

    // Build up counter
    for _ in 0..200 {
        let mut frame = Frame::from_torque(0.01);
        hands_off_detector(&mut frame, &mut state);
    }
    assert!(state.counter > 0);

    // Apply significant torque
    let mut frame = Frame::from_torque(0.5);
    hands_off_detector(&mut frame, &mut state);
    assert_eq!(state.counter, 0);
    assert!(!frame.hands_off);
}
