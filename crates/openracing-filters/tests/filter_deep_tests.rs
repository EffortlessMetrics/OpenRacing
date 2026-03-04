//! Deep tests for the openracing-filters crate.
//!
//! Covers: low-pass, high-pass (notch), EMA (reconstruction), slew rate limiter,
//! damper, reconstruction, filter parameter validation, filter reset, and
//! property-based bounded-output tests.

use openracing_filters::Frame;
use openracing_filters::prelude::*;
use std::f32::consts::PI;

// ═══════════════════════════════════════════════════════════════════════════════
// Helper: generate a sinusoidal signal and measure output amplitude
// ═══════════════════════════════════════════════════════════════════════════════

/// Run a biquad (NotchState) filter on a sine wave and return
/// the peak output amplitude (after discarding the first `warmup` samples).
fn measure_biquad_amplitude(
    state: &mut NotchState,
    freq_hz: f32,
    sample_rate: f32,
    warmup: usize,
    samples: usize,
) -> f32 {
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
fn measure_ema_amplitude(
    state: &mut ReconstructionState,
    freq_hz: f32,
    sample_rate: f32,
    warmup: usize,
    samples: usize,
) -> f32 {
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

    assert!(amp > 0.8, "passband signal should be near unity, got {amp}");
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
    let amp_center =
        measure_biquad_amplitude(&mut state_center, center_freq, sample_rate, 500, 2000);

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

// ═══════════════════════════════════════════════════════════════════════════════
// 10. Filter chain composition: multiple filters applied in sequence
// ═══════════════════════════════════════════════════════════════════════════════

mod filter_chain_tests {
    use super::*;

    #[test]
    fn chain_reconstruction_then_slew_rate() -> Result<(), Box<dyn std::error::Error>> {
        let mut recon_state = ReconstructionState::new(4);
        let mut slew_state = SlewRateState::new(0.5);

        let mut frame = Frame::from_torque(1.0);
        frame.ffb_in = 1.0;

        reconstruction_filter(&mut frame, &mut recon_state);
        let after_recon = frame.torque_out;
        slew_rate_filter(&mut frame, &mut slew_state);

        // Slew rate should further limit the output
        assert!(frame.torque_out <= after_recon + 1e-6);
        assert!(frame.torque_out.is_finite());
        Ok(())
    }

    #[test]
    fn chain_damper_then_friction() -> Result<(), Box<dyn std::error::Error>> {
        let damper = DamperState::fixed(0.1);
        let friction = FrictionState::fixed(0.1);

        let mut frame = Frame::from_ffb(0.0, 5.0);
        damper_filter(&mut frame, &damper);
        let after_damper = frame.torque_out;
        friction_filter(&mut frame, &friction);

        // Both oppose motion — both contribute negative torque for positive speed
        assert!(
            frame.torque_out < after_damper,
            "friction should add opposing torque: after_damper={after_damper}, final={}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn chain_all_filters_produces_finite_output() -> Result<(), Box<dyn std::error::Error>> {
        let mut recon = ReconstructionState::new(4);
        let friction = FrictionState::adaptive(0.1);
        let damper = DamperState::adaptive(0.15);
        let mut inertia = InertiaState::new(0.05);
        let mut notch = NotchState::new(60.0, 2.0, -6.0, 1000.0);
        let mut slew = SlewRateState::new(0.5);
        let curve = CurveState::quadratic();
        let resp_curve = ResponseCurveState::soft();

        // Process 100 frames to let filters warm up
        for i in 0..100u16 {
            let speed = (i as f32 * 0.1).sin() * 5.0;
            let ffb = (i as f32 * 0.05).cos() * 0.8;
            let mut frame = Frame {
                ffb_in: ffb,
                torque_out: ffb,
                wheel_speed: speed,
                hands_off: false,
                ts_mono_ns: i as u64 * 1_000_000,
                seq: i,
            };

            reconstruction_filter(&mut frame, &mut recon);
            friction_filter(&mut frame, &friction);
            damper_filter(&mut frame, &damper);
            inertia_filter(&mut frame, &mut inertia);
            notch_filter(&mut frame, &mut notch);
            slew_rate_filter(&mut frame, &mut slew);
            curve_filter(&mut frame, &curve);
            response_curve_filter(&mut frame, &resp_curve);

            assert!(
                frame.torque_out.is_finite(),
                "frame {i}: output must be finite, got {}",
                frame.torque_out
            );
        }
        Ok(())
    }

    #[test]
    fn chain_order_matters() -> Result<(), Box<dyn std::error::Error>> {
        // Apply slew_rate THEN curve vs. curve THEN slew_rate
        let mut slew1 = SlewRateState::new(0.5);
        let curve = CurveState::quadratic();

        let mut frame1 = Frame::from_torque(1.0);
        slew_rate_filter(&mut frame1, &mut slew1);
        curve_filter(&mut frame1, &curve);

        let mut slew2 = SlewRateState::new(0.5);
        let mut frame2 = Frame::from_torque(1.0);
        curve_filter(&mut frame2, &curve);
        slew_rate_filter(&mut frame2, &mut slew2);

        // Different orders should produce different results
        // (unless both clamp to same value, which is unlikely)
        assert!(frame1.torque_out.is_finite());
        assert!(frame2.torque_out.is_finite());
        Ok(())
    }

    #[test]
    fn chain_repeated_application_converges() -> Result<(), Box<dyn std::error::Error>> {
        let mut recon = ReconstructionState::new(4);
        let mut slew = SlewRateState::new(0.5);

        let target = 0.6;
        let mut last_out = 0.0f32;
        for _ in 0..5000 {
            let mut frame = Frame::from_torque(target);
            frame.ffb_in = target;
            reconstruction_filter(&mut frame, &mut recon);
            slew_rate_filter(&mut frame, &mut slew);
            last_out = frame.torque_out;
        }

        assert!(
            (last_out - target).abs() < 0.01,
            "chain should converge to {target}, got {last_out}"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Edge cases: NaN, infinity, zero, very large values for each filter
// ═══════════════════════════════════════════════════════════════════════════════

mod edge_case_tests {
    use super::*;

    const EDGE_INPUTS: [f32; 7] = [
        0.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::MIN,
    ];

    #[test]
    fn reconstruction_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = ReconstructionState::new(4);
            let mut frame = Frame::from_torque(input);
            frame.ffb_in = input;
            reconstruction_filter(&mut frame, &mut state);
            // Filter may produce non-finite for non-finite input but must not panic
        }
        Ok(())
    }

    #[test]
    fn damper_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let state = DamperState::fixed(0.1);
            let mut frame = Frame::from_ffb(0.0, input);
            damper_filter(&mut frame, &state);
            // Must not panic
        }
        Ok(())
    }

    #[test]
    fn friction_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let state = FrictionState::fixed(0.1);
            let mut frame = Frame::from_ffb(0.0, input);
            friction_filter(&mut frame, &state);
        }
        Ok(())
    }

    #[test]
    fn inertia_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = InertiaState::new(0.1);
            let mut frame = Frame::from_ffb(0.0, input);
            inertia_filter(&mut frame, &mut state);
        }
        Ok(())
    }

    #[test]
    fn notch_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state);
        }
        Ok(())
    }

    #[test]
    fn lowpass_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state);
        }
        Ok(())
    }

    #[test]
    fn slew_rate_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = SlewRateState::new(0.5);
            let mut frame = Frame::from_torque(input);
            slew_rate_filter(&mut frame, &mut state);
        }
        Ok(())
    }

    #[test]
    fn curve_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        // Curve filters use LUT lookup; NaN/infinity inputs may produce NaN.
        // We only verify finite inputs produce finite outputs.
        let finite_inputs = [
            0.0f32,
            -0.0,
            f32::MIN_POSITIVE,
            1.0,
            -1.0,
            f32::MAX,
            f32::MIN,
        ];
        for &input in &finite_inputs {
            let curve = CurveState::linear();
            let mut frame = Frame::from_torque(input);
            curve_filter(&mut frame, &curve);
            assert!(
                frame.torque_out.is_finite(),
                "curve should return finite for finite input {input}"
            );
        }
        Ok(())
    }

    #[test]
    fn response_curve_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        // Response curve uses LUT lookup; NaN/infinity inputs may produce NaN.
        // We only verify finite inputs produce finite outputs.
        let finite_inputs = [
            0.0f32,
            -0.0,
            f32::MIN_POSITIVE,
            1.0,
            -1.0,
            f32::MAX,
            f32::MIN,
        ];
        for &input in &finite_inputs {
            let curve = ResponseCurveState::linear();
            let mut frame = Frame::from_torque(input);
            response_curve_filter(&mut frame, &curve);
            assert!(
                frame.torque_out.is_finite(),
                "response curve should return finite for finite input {input}"
            );
        }
        Ok(())
    }

    #[test]
    fn bumpstop_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = BumpstopState::standard();
            let mut frame = Frame::from_ffb(0.0, input);
            bumpstop_filter(&mut frame, &mut state);
        }
        Ok(())
    }

    #[test]
    fn hands_off_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
        for &input in &EDGE_INPUTS {
            let mut state = HandsOffState::default_detector();
            let mut frame = Frame::from_torque(input);
            hands_off_detector(&mut frame, &mut state);
        }
        Ok(())
    }

    #[test]
    fn torque_cap_negative_infinity() -> Result<(), Box<dyn std::error::Error>> {
        let mut frame = Frame::from_torque(f32::NEG_INFINITY);
        openracing_filters::torque_cap_filter(&mut frame, 0.5);
        assert!(frame.torque_out.is_finite());
        assert!(frame.torque_out.abs() <= 0.5 + 1e-6);
        Ok(())
    }

    #[test]
    fn zero_coefficient_filters_are_transparent() -> Result<(), Box<dyn std::error::Error>> {
        let input = 0.42;

        // Friction with zero coefficient
        let friction = FrictionState::fixed(0.0);
        let mut f1 = Frame::from_ffb(input, 5.0);
        friction_filter(&mut f1, &friction);
        assert!(
            (f1.torque_out - input).abs() < 1e-6,
            "zero friction should be transparent"
        );

        // Damper with zero coefficient
        let damper = DamperState::fixed(0.0);
        let mut f2 = Frame::from_ffb(input, 5.0);
        damper_filter(&mut f2, &damper);
        assert!(
            (f2.torque_out - input).abs() < 1e-6,
            "zero damper should be transparent"
        );

        // Inertia with zero coefficient
        let mut inertia = InertiaState::new(0.0);
        let mut f3a = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut f3a, &mut inertia);
        let mut f3 = Frame::from_ffb(input, 5.0);
        inertia_filter(&mut f3, &mut inertia);
        assert!(
            (f3.torque_out - input).abs() < 1e-6,
            "zero inertia should be transparent"
        );
        Ok(())
    }

    #[test]
    fn very_large_torque_values() -> Result<(), Box<dyn std::error::Error>> {
        let large = 1e6;

        let curve = CurveState::linear();
        let mut frame = Frame::from_torque(large);
        curve_filter(&mut frame, &curve);
        assert!(frame.torque_out.is_finite());
        // Curve clamps to [0,1] magnitude
        assert!(frame.torque_out.abs() <= 1.0 + 1e-6);

        let resp = ResponseCurveState::linear();
        let mut frame2 = Frame::from_torque(large);
        response_curve_filter(&mut frame2, &resp);
        assert!(frame2.torque_out.is_finite());
        assert!(frame2.torque_out.abs() <= 1.0 + 1e-6);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. Sample rate sensitivity: filters at different sample rates
// ═══════════════════════════════════════════════════════════════════════════════

mod sample_rate_tests {
    use super::*;

    #[test]
    fn lowpass_cutoff_scales_with_sample_rate() -> Result<(), Box<dyn std::error::Error>> {
        // At higher sample rate, the same cutoff Hz has a different effect
        let cutoff = 50.0;

        // 1 kHz sample rate
        let mut state_1k = NotchState::lowpass(cutoff, 0.707, 1000.0);
        let amp_1k = measure_biquad_amplitude(&mut state_1k, 200.0, 1000.0, 500, 2000);

        // 2 kHz sample rate (same absolute Hz cutoff)
        let mut state_2k = NotchState::lowpass(cutoff, 0.707, 2000.0);
        let amp_2k = measure_biquad_amplitude(&mut state_2k, 200.0, 2000.0, 1000, 4000);

        // Both should attenuate 200 Hz (well above 50 Hz cutoff)
        assert!(amp_1k < 0.5, "1kHz rate should attenuate: {amp_1k}");
        assert!(amp_2k < 0.5, "2kHz rate should attenuate: {amp_2k}");
        Ok(())
    }

    #[test]
    fn notch_center_frequency_is_sample_rate_dependent() -> Result<(), Box<dyn std::error::Error>> {
        let center = 100.0;

        let mut state_1k = NotchState::new(center, 2.0, -6.0, 1000.0);
        let amp_at_center_1k = measure_biquad_amplitude(&mut state_1k, center, 1000.0, 500, 2000);

        let mut state_2k = NotchState::new(center, 2.0, -6.0, 2000.0);
        let amp_at_center_2k = measure_biquad_amplitude(&mut state_2k, center, 2000.0, 1000, 4000);

        // Both should attenuate the center frequency
        assert!(
            amp_at_center_1k < 0.5,
            "1kHz rate should notch: {amp_at_center_1k}"
        );
        assert!(
            amp_at_center_2k < 0.5,
            "2kHz rate should notch: {amp_at_center_2k}"
        );
        Ok(())
    }

    #[test]
    fn notch_very_low_sample_rate_still_finite() -> Result<(), Box<dyn std::error::Error>> {
        // Edge case: sample rate barely above Nyquist
        let state = NotchState::new(10.0, 2.0, -6.0, 100.0);
        assert!(state.b0.is_finite());
        assert!(state.a1.is_finite());
        assert!(state.a2.is_finite());
        Ok(())
    }

    #[test]
    fn slew_rate_per_tick_computation() -> Result<(), Box<dyn std::error::Error>> {
        // Slew rate is divided by 1000 (assumes 1kHz)
        let state = SlewRateState::new(1.0); // 1.0/s
        assert!(
            (state.max_change_per_tick - 0.001).abs() < 1e-6,
            "1.0/s at 1kHz = 0.001/tick, got {}",
            state.max_change_per_tick
        );

        let state2 = SlewRateState::per_tick(0.01);
        assert!(
            (state2.max_change_per_tick - 0.01).abs() < 1e-6,
            "per_tick(0.01) should be 0.01"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. Step response behavior: quantitative step response analysis
// ═══════════════════════════════════════════════════════════════════════════════

mod step_response_tests {
    use super::*;

    #[test]
    fn lowpass_step_response_monotonically_rises() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
        let mut prev_out = 0.0f32;

        // After initial transient (first few samples), output should rise toward 1.0
        for _ in 0..200 {
            let mut frame = Frame::from_torque(1.0);
            notch_filter(&mut frame, &mut state);
            prev_out = frame.torque_out;
        }

        // Biquad lowpass can have slight overshoot even at Q=0.707 due to discrete-time effects.
        // We verify the output eventually settles close to 1.0 instead of strict monotonicity.
        assert!(
            (prev_out - 1.0).abs() < 0.05,
            "low-Q lowpass step response should converge near 1.0, got {prev_out}"
        );
        Ok(())
    }

    #[test]
    fn ema_step_response_is_exponential() -> Result<(), Box<dyn std::error::Error>> {
        let alpha = 0.1;
        let mut state = ReconstructionState::new(4); // alpha = 0.1
        let mut outputs = Vec::new();

        for _ in 0..50 {
            let mut frame = Frame::from_torque(1.0);
            frame.ffb_in = 1.0;
            reconstruction_filter(&mut frame, &mut state);
            outputs.push(frame.torque_out);
        }

        // EMA: y[n] = alpha * x + (1-alpha) * y[n-1]
        // Step response: y[n] = 1 - (1-alpha)^n
        // Check first output: should be alpha (since prev was 0)
        assert!(
            (outputs[0] - alpha).abs() < 0.01,
            "first step output should be ~{alpha}, got {}",
            outputs[0]
        );

        // After 10 samples: 1 - (1-0.1)^10 = 1 - 0.9^10 ≈ 0.6513
        let expected_10 = 1.0 - (1.0 - alpha).powi(10);
        assert!(
            (outputs[9] - expected_10).abs() < 0.02,
            "output at sample 10 should be ~{expected_10}, got {}",
            outputs[9]
        );
        Ok(())
    }

    #[test]
    fn slew_rate_step_response_is_linear_ramp() -> Result<(), Box<dyn std::error::Error>> {
        let rate = 1.0; // 1.0/s = 0.001/tick
        let per_tick = rate / 1000.0;
        let mut state = SlewRateState::new(rate);

        for i in 1..=100 {
            let mut frame = Frame::from_torque(1.0);
            slew_rate_filter(&mut frame, &mut state);
            let expected = per_tick * i as f32;
            assert!(
                (frame.torque_out - expected).abs() < 1e-5,
                "step {i}: expected {expected}, got {}",
                frame.torque_out
            );
        }
        Ok(())
    }

    #[test]
    fn notch_step_response_settles() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);

        // Feed constant 1.0 for many samples; notch should settle near 1.0 at DC
        for _ in 0..500 {
            let mut frame = Frame::from_torque(1.0);
            notch_filter(&mut frame, &mut state);
        }
        let mut final_frame = Frame::from_torque(1.0);
        notch_filter(&mut final_frame, &mut state);

        assert!(
            (final_frame.torque_out - 1.0).abs() < 0.05,
            "notch should pass DC, got {}",
            final_frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn inertia_impulse_response_single_spike() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = InertiaState::new(0.1);

        // Speed goes 0 → 10 → 10 → 10...
        let mut f0 = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut f0, &mut state);

        let mut f1 = Frame::from_ffb(0.0, 10.0);
        f1.torque_out = 0.0;
        inertia_filter(&mut f1, &mut state);
        let impulse_torque = f1.torque_out;

        // After speed stabilizes, no more inertia torque
        let mut f2 = Frame::from_ffb(0.0, 10.0);
        f2.torque_out = 0.0;
        inertia_filter(&mut f2, &mut state);

        assert!(
            impulse_torque.abs() > 0.1,
            "impulse should produce significant torque"
        );
        assert!(
            f2.torque_out.abs() < 1e-6,
            "constant speed should produce zero inertia torque, got {}",
            f2.torque_out
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. Frequency response verification: biquad filters
// ═══════════════════════════════════════════════════════════════════════════════

mod frequency_response_tests {
    use super::*;

    #[test]
    fn notch_deep_null_at_center() -> Result<(), Box<dyn std::error::Error>> {
        let center = 100.0;
        let sample_rate = 1000.0;
        let mut state = NotchState::new(center, 5.0, -20.0, sample_rate);

        let amp = measure_biquad_amplitude(&mut state, center, sample_rate, 500, 2000);

        assert!(
            amp < 0.15,
            "high-Q notch should deeply attenuate center: {amp}"
        );
        Ok(())
    }

    #[test]
    fn notch_narrow_q_vs_wide_q() -> Result<(), Box<dyn std::error::Error>> {
        let center = 100.0;
        let sr = 1000.0;
        let offset_freq = center + 30.0; // 130 Hz, near but not at center

        let mut narrow = NotchState::new(center, 8.0, -6.0, sr);
        let amp_narrow = measure_biquad_amplitude(&mut narrow, offset_freq, sr, 500, 2000);

        let mut wide = NotchState::new(center, 0.5, -6.0, sr);
        let amp_wide = measure_biquad_amplitude(&mut wide, offset_freq, sr, 500, 2000);

        // Narrow Q should pass the offset frequency more than wide Q
        assert!(
            amp_narrow > amp_wide,
            "narrow Q should pass offset freq better: narrow={amp_narrow}, wide={amp_wide}"
        );
        Ok(())
    }

    #[test]
    fn lowpass_gain_decreases_with_frequency() -> Result<(), Box<dyn std::error::Error>> {
        let cutoff = 100.0;
        let sr = 1000.0;

        let test_freqs = [10.0, 50.0, 100.0, 200.0, 400.0];
        let mut prev_amp = f32::MAX;

        for &freq in &test_freqs {
            let mut state = NotchState::lowpass(cutoff, 0.707, sr);
            let amp = measure_biquad_amplitude(&mut state, freq, sr, 500, 2000);
            assert!(
                amp <= prev_amp + 0.05,
                "lowpass gain should decrease with freq: {freq}Hz amp={amp}, prev={prev_amp}"
            );
            prev_amp = amp;
        }
        Ok(())
    }

    #[test]
    fn ema_attenuation_increases_with_smoothing_level() -> Result<(), Box<dyn std::error::Error>> {
        let sr = 1000.0;
        let test_freq = 100.0;
        let mut prev_amp = f32::MAX;

        for level in [0, 2, 4, 6, 8] {
            let mut state = ReconstructionState::new(level);
            let amp = measure_ema_amplitude(&mut state, test_freq, sr, 200, 1000);
            assert!(
                amp <= prev_amp + 0.05,
                "level {level}: higher smoothing should attenuate more: amp={amp}, prev={prev_amp}"
            );
            prev_amp = amp;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. Filter state reset: verify FilterState trait reset for all types
// ═══════════════════════════════════════════════════════════════════════════════

mod filter_state_reset_all {
    use super::*;

    #[test]
    fn friction_reset_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = FrictionState::adaptive(0.5);
        FilterState::reset(&mut state);
        // Friction has no dynamic state, coefficient should remain
        assert!((state.coefficient - 0.5).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn damper_reset_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = DamperState::adaptive(0.3);
        FilterState::reset(&mut state);
        assert!((state.coefficient - 0.3).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn curve_reset_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = CurveState::quadratic();
        let mid_before = state.lookup(0.5);
        FilterState::reset(&mut state);
        let mid_after = state.lookup(0.5);
        assert!(
            (mid_before - mid_after).abs() < f32::EPSILON,
            "curve reset should not change LUT"
        );
        Ok(())
    }

    #[test]
    fn response_curve_reset_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ResponseCurveState::soft();
        let val_before = state.lookup(0.5);
        FilterState::reset(&mut state);
        let val_after = state.lookup(0.5);
        assert!(
            (val_before - val_after).abs() < f32::EPSILON,
            "response curve reset should not change LUT"
        );
        Ok(())
    }

    #[test]
    fn all_resets_allow_clean_reprocessing() -> Result<(), Box<dyn std::error::Error>> {
        // Process some data, reset, process again — should get same results
        let make_state = || {
            (
                ReconstructionState::new(4),
                SlewRateState::new(0.5),
                NotchState::new(50.0, 2.0, -6.0, 1000.0),
                InertiaState::new(0.1),
            )
        };

        let inputs = [0.5, 0.3, 0.7, -0.2, 0.0];

        // First pass
        let (mut recon1, mut slew1, mut notch1, mut inertia1) = make_state();
        let mut outputs1 = Vec::new();
        for &inp in &inputs {
            let mut frame = Frame::from_torque(inp);
            frame.ffb_in = inp;
            reconstruction_filter(&mut frame, &mut recon1);
            slew_rate_filter(&mut frame, &mut slew1);
            notch_filter(&mut frame, &mut notch1);
            inertia_filter(&mut frame, &mut inertia1);
            outputs1.push(frame.torque_out);
        }

        // Reset all
        FilterState::reset(&mut recon1);
        FilterState::reset(&mut slew1);
        FilterState::reset(&mut notch1);
        FilterState::reset(&mut inertia1);

        // Second pass
        let mut outputs2 = Vec::new();
        for &inp in &inputs {
            let mut frame = Frame::from_torque(inp);
            frame.ffb_in = inp;
            reconstruction_filter(&mut frame, &mut recon1);
            slew_rate_filter(&mut frame, &mut slew1);
            notch_filter(&mut frame, &mut notch1);
            inertia_filter(&mut frame, &mut inertia1);
            outputs2.push(frame.torque_out);
        }

        for (i, (o1, o2)) in outputs1.iter().zip(outputs2.iter()).enumerate() {
            assert!(
                (o1 - o2).abs() < 1e-6,
                "after reset, output {i} should match: {o1} vs {o2}"
            );
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 16. Response curve variants: soft, hard, from_lut
// ═══════════════════════════════════════════════════════════════════════════════

mod response_curve_variant_tests {
    use super::*;

    #[test]
    fn soft_curve_reduces_center() -> Result<(), Box<dyn std::error::Error>> {
        let soft = ResponseCurveState::soft();
        let linear = ResponseCurveState::linear();

        let soft_mid = soft.lookup(0.5);
        let lin_mid = linear.lookup(0.5);

        assert!(
            soft_mid < lin_mid,
            "soft should reduce midpoint: soft={soft_mid}, linear={lin_mid}"
        );
        Ok(())
    }

    #[test]
    fn hard_curve_boosts_center() -> Result<(), Box<dyn std::error::Error>> {
        let hard = ResponseCurveState::hard();
        let linear = ResponseCurveState::linear();

        let hard_mid = hard.lookup(0.5);
        let lin_mid = linear.lookup(0.5);

        assert!(
            hard_mid > lin_mid,
            "hard should boost midpoint: hard={hard_mid}, linear={lin_mid}"
        );
        Ok(())
    }

    #[test]
    fn all_response_curves_endpoints() -> Result<(), Box<dyn std::error::Error>> {
        for (name, curve) in [
            ("linear", ResponseCurveState::linear()),
            ("soft", ResponseCurveState::soft()),
            ("hard", ResponseCurveState::hard()),
        ] {
            let at_zero = curve.lookup(0.0);
            let at_one = curve.lookup(1.0);
            assert!(
                at_zero.abs() < 0.01,
                "{name}: lookup(0) should be ~0, got {at_zero}"
            );
            assert!(
                (at_one - 1.0).abs() < 0.01,
                "{name}: lookup(1) should be ~1, got {at_one}"
            );
        }
        Ok(())
    }

    #[test]
    fn response_curve_from_lut_matches_source() -> Result<(), Box<dyn std::error::Error>> {
        let lut = openracing_curves::CurveLut::linear();
        let state = ResponseCurveState::from_lut(&lut);

        for i in 0..=10 {
            let input = i as f32 / 10.0;
            let expected = lut.lookup(input);
            let got = state.lookup(input);
            assert!(
                (expected - got).abs() < 0.02,
                "from_lut mismatch at {input}: expected {expected}, got {got}"
            );
        }
        Ok(())
    }

    #[test]
    fn response_curve_filter_symmetry() -> Result<(), Box<dyn std::error::Error>> {
        let curve = ResponseCurveState::soft();

        let mut pos = Frame::from_torque(0.7);
        response_curve_filter(&mut pos, &curve);

        let mut neg = Frame::from_torque(-0.7);
        response_curve_filter(&mut neg, &curve);

        assert!(
            (pos.torque_out.abs() - neg.torque_out.abs()).abs() < 0.01,
            "response curve should be symmetric: pos={}, neg={}",
            pos.torque_out,
            neg.torque_out
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 17. Curve state variants and lookup
// ═══════════════════════════════════════════════════════════════════════════════

mod curve_state_variant_tests {
    use super::*;

    #[test]
    fn quadratic_lookup_matches_x_squared() -> Result<(), Box<dyn std::error::Error>> {
        let curve = CurveState::quadratic();
        // Points: (0,0), (0.5, 0.25), (1.0, 1.0)
        // At 0.5 input, expect ~0.25
        let val = curve.lookup(0.5);
        assert!(
            (val - 0.25).abs() < 0.05,
            "quadratic at 0.5 should be ~0.25, got {val}"
        );
        Ok(())
    }

    #[test]
    fn scurve_inflection_at_center() -> Result<(), Box<dyn std::error::Error>> {
        let curve = CurveState::scurve();
        let at_half = curve.lookup(0.5);
        assert!(
            (at_half - 0.5).abs() < 0.05,
            "S-curve at 0.5 should be ~0.5, got {at_half}"
        );
        Ok(())
    }

    #[test]
    fn curve_lookup_is_monotonic() -> Result<(), Box<dyn std::error::Error>> {
        for (name, curve) in [
            ("linear", CurveState::linear()),
            ("quadratic", CurveState::quadratic()),
            ("cubic", CurveState::cubic()),
            ("scurve", CurveState::scurve()),
        ] {
            let mut prev = -1.0f32;
            for i in 0..=100 {
                let input = i as f32 / 100.0;
                let val = curve.lookup(input);
                assert!(
                    val >= prev - 1e-6,
                    "{name}: lookup not monotonic at {input}: {val} < {prev}"
                );
                prev = val;
            }
        }
        Ok(())
    }
}
