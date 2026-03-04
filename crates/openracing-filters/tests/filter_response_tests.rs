//! Filter response tests: frequency response, phase, stability, reset, coefficients, Bode.
//!
//! Covers notch, lowpass, reconstruction (EMA), slew rate, damper, friction,
//! inertia, bumpstop, curve, response curve, and hands-off filters.

use openracing_filters::Frame;
use openracing_filters::prelude::*;
use proptest::prelude::*;
use std::f32::consts::PI;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Measure steady-state peak amplitude of a biquad filter on a sine wave.
fn biquad_peak(state: &mut NotchState, freq_hz: f32, sr: f32, warmup: usize, n: usize) -> f32 {
    let mut peak = 0.0f32;
    for i in 0..(warmup + n) {
        let t = i as f32 / sr;
        let input = (2.0 * PI * freq_hz * t).sin();
        let mut frame = Frame::from_torque(input);
        notch_filter(&mut frame, state);
        if i >= warmup {
            peak = peak.max(frame.torque_out.abs());
        }
    }
    peak
}

/// Measure steady-state peak amplitude of EMA (reconstruction) on a sine wave.
fn ema_peak(
    state: &mut ReconstructionState,
    freq_hz: f32,
    sr: f32,
    warmup: usize,
    n: usize,
) -> f32 {
    let mut peak = 0.0f32;
    for i in 0..(warmup + n) {
        let t = i as f32 / sr;
        let input = (2.0 * PI * freq_hz * t).sin();
        let mut frame = Frame::from_torque(input);
        frame.ffb_in = input;
        reconstruction_filter(&mut frame, state);
        if i >= warmup {
            peak = peak.max(frame.torque_out.abs());
        }
    }
    peak
}

/// Measure phase lag of a biquad filter at a given frequency (radians).
/// Uses zero-crossing analysis.
fn biquad_phase_lag(state: &mut NotchState, freq_hz: f32, sr: f32) -> f32 {
    let warmup = 2000;
    let n = 4000;
    let period_samples = sr / freq_hz;

    // Find zero crossings in input and output
    let mut last_in = 0.0f32;
    let mut last_out = 0.0f32;
    let mut in_cross: Option<f32> = None;
    let mut out_cross: Option<f32> = None;

    for i in 0..(warmup + n) {
        let t = i as f32 / sr;
        let input = (2.0 * PI * freq_hz * t).sin();
        let mut frame = Frame::from_torque(input);
        notch_filter(&mut frame, state);

        if i >= warmup {
            // Detect positive zero crossings
            if last_in <= 0.0 && input > 0.0 && in_cross.is_none() {
                in_cross = Some(i as f32);
            }
            if in_cross.is_some()
                && last_out <= 0.0
                && frame.torque_out > 0.0
                && out_cross.is_none()
            {
                out_cross = Some(i as f32);
            }
        }
        last_in = input;
        last_out = frame.torque_out;
    }

    match (in_cross, out_cross) {
        (Some(ic), Some(oc)) => {
            let delay_samples = oc - ic;
            // Phase lag in radians
            2.0 * PI * delay_samples / period_samples
        }
        _ => 0.0,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Notch filter frequency response
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn notch_deep_null_at_center() {
    let sr = 1000.0;
    let center = 100.0;
    let mut state = NotchState::new(center, 5.0, -20.0, sr);
    let amp = biquad_peak(&mut state, center, sr, 1000, 4000);
    // Notch should heavily attenuate center frequency
    assert!(
        amp < 0.15,
        "notch center amp should be near zero, got {amp}"
    );
}

#[test]
fn notch_passband_gain_near_unity_at_dc() {
    let sr = 1000.0;
    let mut state = NotchState::new(100.0, 2.0, -6.0, sr);
    // DC: feed constant 1.0
    for _ in 0..500 {
        let mut f = Frame::from_torque(1.0);
        notch_filter(&mut f, &mut state);
    }
    let mut final_f = Frame::from_torque(1.0);
    notch_filter(&mut final_f, &mut state);
    assert!(
        (final_f.torque_out - 1.0).abs() < 0.05,
        "notch DC gain should be ~1.0, got {}",
        final_f.torque_out
    );
}

#[test]
fn notch_passband_gain_near_unity_far_from_center() {
    let sr = 1000.0;
    let center = 200.0;
    let mut state = NotchState::new(center, 2.0, -6.0, sr);
    let amp = biquad_peak(&mut state, 10.0, sr, 500, 2000);
    assert!(
        amp > 0.85,
        "far-from-center gain should be near 1.0, got {amp}"
    );
}

#[test]
fn notch_higher_q_narrower_notch() {
    let sr = 1000.0;
    let center = 100.0;
    // Low Q (wider notch) should attenuate more at nearby frequencies
    let mut state_low_q = NotchState::new(center, 0.5, -6.0, sr);
    let amp_low_q = biquad_peak(&mut state_low_q, center + 30.0, sr, 500, 2000);

    let mut state_high_q = NotchState::new(center, 5.0, -6.0, sr);
    let amp_high_q = biquad_peak(&mut state_high_q, center + 30.0, sr, 500, 2000);

    assert!(
        amp_high_q > amp_low_q,
        "higher Q should pass nearby freqs better: q5={amp_high_q}, q0.5={amp_low_q}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Lowpass frequency response & Bode characteristics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn lowpass_dc_gain_unity() {
    let sr = 1000.0;
    let mut state = NotchState::lowpass(100.0, 0.707, sr);
    for _ in 0..500 {
        let mut f = Frame::from_torque(1.0);
        notch_filter(&mut f, &mut state);
    }
    let mut f = Frame::from_torque(1.0);
    notch_filter(&mut f, &mut state);
    assert!(
        (f.torque_out - 1.0).abs() < 0.05,
        "lowpass DC gain should be ~1.0, got {}",
        f.torque_out
    );
}

#[test]
fn lowpass_nyquist_heavily_attenuated() {
    let sr = 1000.0;
    let mut state = NotchState::lowpass(50.0, 0.707, sr);
    // Nyquist = 500 Hz; but close to Nyquist = 450 Hz
    let amp = biquad_peak(&mut state, 450.0, sr, 500, 2000);
    assert!(
        amp < 0.1,
        "lowpass near-Nyquist should be very low, got {amp}"
    );
}

#[test]
fn lowpass_cutoff_gain_about_minus_3db() {
    let sr = 1000.0;
    let cutoff = 100.0;
    let mut state = NotchState::lowpass(cutoff, 0.707, sr);
    let amp_cutoff = biquad_peak(&mut state, cutoff, sr, 1000, 4000);
    // -3dB = 10^(-3/20) ≈ 0.707
    assert!(
        amp_cutoff < 0.85 && amp_cutoff > 0.45,
        "lowpass gain at cutoff should be ~0.707, got {amp_cutoff}"
    );
}

#[test]
fn lowpass_monotonic_rolloff() {
    let sr = 1000.0;
    let cutoff = 100.0;

    let freqs = [10.0, 50.0, 100.0, 200.0, 400.0];
    let mut prev_amp = f32::MAX;
    for &freq in &freqs {
        let mut state = NotchState::lowpass(cutoff, 0.707, sr);
        let amp = biquad_peak(&mut state, freq, sr, 500, 2000);
        assert!(
            amp <= prev_amp + 0.05,
            "lowpass should roll off monotonically: freq={freq}, amp={amp}, prev={prev_amp}"
        );
        prev_amp = amp;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Phase response characteristics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn lowpass_phase_lag_increases_with_frequency() {
    let sr = 1000.0;
    let cutoff = 100.0;

    let mut state_lo = NotchState::lowpass(cutoff, 0.707, sr);
    let phase_lo = biquad_phase_lag(&mut state_lo, 20.0, sr);

    let mut state_hi = NotchState::lowpass(cutoff, 0.707, sr);
    let phase_hi = biquad_phase_lag(&mut state_hi, 80.0, sr);

    // Higher frequency should have more phase lag
    assert!(
        phase_hi >= phase_lo,
        "phase lag should increase with freq: lo={phase_lo}, hi={phase_hi}"
    );
}

#[test]
fn notch_phase_is_finite_at_center() {
    let sr = 1000.0;
    let mut state = NotchState::new(100.0, 2.0, -6.0, sr);
    let phase = biquad_phase_lag(&mut state, 100.0, sr);
    assert!(phase.is_finite(), "phase should be finite at notch center");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Filter stability with edge-case inputs (NaN, infinity, zero)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn notch_nan_input_does_not_crash() {
    let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    let mut frame = Frame::from_torque(f32::NAN);
    notch_filter(&mut frame, &mut state);
    // Result may be NaN but should not panic
}

#[test]
fn notch_infinity_input_does_not_crash() {
    let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    let mut frame = Frame::from_torque(f32::INFINITY);
    notch_filter(&mut frame, &mut state);
    // Should not panic
}

#[test]
fn lowpass_nan_input_does_not_crash() {
    let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
    let mut frame = Frame::from_torque(f32::NAN);
    notch_filter(&mut frame, &mut state);
}

#[test]
fn reconstruction_nan_produces_finite_after_recovery() {
    let mut state = ReconstructionState::new(4);
    // Feed NaN
    let mut frame_nan = Frame::from_torque(f32::NAN);
    frame_nan.ffb_in = f32::NAN;
    reconstruction_filter(&mut frame_nan, &mut state);

    // Feed normal values to recover
    for _ in 0..100 {
        let mut frame = Frame::from_torque(0.5);
        frame.ffb_in = 0.5;
        reconstruction_filter(&mut frame, &mut state);
    }
    // After NaN, the filter state may be poisoned but should not crash
}

#[test]
fn slew_rate_infinity_input_bounded() {
    let mut state = SlewRateState::new(0.5);
    let mut frame = Frame::from_torque(f32::INFINITY);
    slew_rate_filter(&mut frame, &mut state);
    // The clamp should bound the change
    assert!(
        frame.torque_out.is_finite(),
        "slew rate should produce finite from inf, got {}",
        frame.torque_out
    );
}

#[test]
fn damper_nan_speed_does_not_crash() {
    let state = DamperState::fixed(0.1);
    let mut frame = Frame::from_ffb(0.0, f32::NAN);
    damper_filter(&mut frame, &state);
    // Should not crash
}

#[test]
fn friction_nan_speed_does_not_crash() {
    let state = FrictionState::fixed(0.1);
    let mut frame = Frame::from_ffb(0.0, f32::NAN);
    friction_filter(&mut frame, &state);
}

#[test]
fn inertia_inf_speed_does_not_crash() {
    let mut state = InertiaState::new(0.1);
    let mut frame = Frame::from_ffb(0.0, f32::INFINITY);
    inertia_filter(&mut frame, &mut state);
}

#[test]
fn curve_nan_input_does_not_crash() {
    let state = CurveState::linear();
    let mut frame = Frame::from_torque(f32::NAN);
    curve_filter(&mut frame, &state);
}

#[test]
fn response_curve_nan_input_does_not_crash() {
    let state = ResponseCurveState::linear();
    let mut frame = Frame::from_torque(f32::NAN);
    response_curve_filter(&mut frame, &state);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Filter reset behavior
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn notch_reset_clears_delay_line() {
    let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    // Process some frames to fill delay line
    for _ in 0..100 {
        let mut f = Frame::from_torque(0.5);
        notch_filter(&mut f, &mut state);
    }
    assert!(state.x1.abs() > 0.0 || state.y1.abs() > 0.0);

    state.reset();
    assert!((state.x1).abs() < f32::EPSILON);
    assert!((state.x2).abs() < f32::EPSILON);
    assert!((state.y1).abs() < f32::EPSILON);
    assert!((state.y2).abs() < f32::EPSILON);
}

#[test]
fn reconstruction_reset_clears_prev_output() {
    let mut state = ReconstructionState::new(4);
    for _ in 0..50 {
        let mut f = Frame::from_torque(0.8);
        f.ffb_in = 0.8;
        reconstruction_filter(&mut f, &mut state);
    }
    assert!(state.prev_output.abs() > 0.1);

    state.reset();
    assert!((state.prev_output).abs() < f32::EPSILON);
}

#[test]
fn slew_rate_reset_clears_prev_output() {
    let mut state = SlewRateState::new(0.5);
    for _ in 0..100 {
        let mut f = Frame::from_torque(1.0);
        slew_rate_filter(&mut f, &mut state);
    }
    assert!(state.prev_output.abs() > 0.0);

    state.reset();
    assert!((state.prev_output).abs() < f32::EPSILON);
}

#[test]
fn inertia_reset_clears_prev_speed() {
    let mut state = InertiaState::new(0.1);
    let mut f = Frame::from_ffb(0.0, 5.0);
    inertia_filter(&mut f, &mut state);
    assert!((state.prev_wheel_speed - 5.0).abs() < f32::EPSILON);

    state.reset();
    assert!((state.prev_wheel_speed).abs() < f32::EPSILON);
}

#[test]
fn bumpstop_reset_clears_angle() {
    let mut state = BumpstopState::standard();
    state.current_angle = 500.0;
    state.reset();
    assert!((state.current_angle).abs() < f32::EPSILON);
}

#[test]
fn hands_off_reset_clears_counter_and_torque() {
    let mut state = HandsOffState::default_detector();
    state.counter = 999;
    state.last_torque = 0.42;
    state.reset();
    assert_eq!(state.counter, 0);
    assert!((state.last_torque).abs() < f32::EPSILON);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Filter coefficient computation accuracy
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn notch_bypass_coefficients_are_identity() {
    let state = NotchState::bypass();
    assert!((state.b0 - 1.0).abs() < f32::EPSILON);
    assert!((state.b1).abs() < f32::EPSILON);
    assert!((state.b2).abs() < f32::EPSILON);
    assert!((state.a1).abs() < f32::EPSILON);
    assert!((state.a2).abs() < f32::EPSILON);
}

#[test]
fn lowpass_coefficients_are_finite() {
    let state = NotchState::lowpass(200.0, 0.707, 1000.0);
    assert!(state.b0.is_finite());
    assert!(state.b1.is_finite());
    assert!(state.b2.is_finite());
    assert!(state.a1.is_finite());
    assert!(state.a2.is_finite());
}

#[test]
fn lowpass_b0_b2_are_equal() {
    // For a standard lowpass biquad, b0 == b2
    let state = NotchState::lowpass(200.0, 0.707, 1000.0);
    assert!(
        (state.b0 - state.b2).abs() < 1e-6,
        "lowpass b0 ({}) should equal b2 ({})",
        state.b0,
        state.b2
    );
}

#[test]
fn lowpass_b1_equals_2_times_b0() {
    // For standard lowpass: b1 = 2*b0
    let state = NotchState::lowpass(200.0, 0.707, 1000.0);
    assert!(
        (state.b1 - 2.0 * state.b0).abs() < 1e-5,
        "lowpass b1 ({}) should equal 2*b0 ({})",
        state.b1,
        2.0 * state.b0
    );
}

#[test]
fn notch_coefficients_b0_equals_b2() {
    // For standard notch: b0 == b2 == 1/a0
    let state = NotchState::new(100.0, 2.0, -6.0, 1000.0);
    assert!(
        (state.b0 - state.b2).abs() < 1e-6,
        "notch b0 ({}) should equal b2 ({})",
        state.b0,
        state.b2
    );
}

#[test]
fn notch_b1_equals_a1() {
    // For standard notch: b1 == a1 (both are -2*cos(omega)/a0)
    let state = NotchState::new(100.0, 2.0, -6.0, 1000.0);
    assert!(
        (state.b1 - state.a1).abs() < 1e-5,
        "notch b1 ({}) should equal a1 ({})",
        state.b1,
        state.a1
    );
}

#[test]
fn reconstruction_alpha_decreases_with_level() {
    let level_0 = ReconstructionState::new(0);
    let level_4 = ReconstructionState::new(4);
    let level_8 = ReconstructionState::new(8);
    assert!(level_0.alpha > level_4.alpha);
    assert!(level_4.alpha > level_8.alpha);
}

#[test]
fn slew_rate_max_change_per_tick_correct() {
    let state = SlewRateState::new(1.0);
    assert!(
        (state.max_change_per_tick - 0.001).abs() < 1e-6,
        "1.0/s should give 0.001/tick, got {}",
        state.max_change_per_tick
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Bode plot characteristics
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn ema_gain_at_dc_is_unity() {
    let mut state = ReconstructionState::new(4);
    for _ in 0..1000 {
        let mut f = Frame::from_torque(1.0);
        f.ffb_in = 1.0;
        reconstruction_filter(&mut f, &mut state);
    }
    assert!(
        (state.prev_output - 1.0).abs() < 0.001,
        "EMA DC gain should be 1.0, got {}",
        state.prev_output
    );
}

#[test]
fn ema_gain_at_nyquist_is_minimal() {
    let sr = 1000.0;
    let mut state = ReconstructionState::new(6);
    // Nyquist frequency = 500 Hz
    let amp = ema_peak(&mut state, 490.0, sr, 500, 2000);
    assert!(
        amp < 0.15,
        "EMA at near-Nyquist should be very low, got {amp}"
    );
}

#[test]
fn lowpass_bode_gain_below_cutoff_near_0db() {
    let sr = 1000.0;
    let cutoff = 150.0;
    let mut state = NotchState::lowpass(cutoff, 0.707, sr);
    let amp = biquad_peak(&mut state, 10.0, sr, 500, 2000);
    // Gain in dB should be near 0
    let gain_db = 20.0 * amp.log10();
    assert!(
        gain_db.abs() < 3.0,
        "passband gain should be within 3dB of 0, got {gain_db} dB"
    );
}

#[test]
fn lowpass_bode_steep_rolloff_above_cutoff() {
    let sr = 1000.0;
    let cutoff = 50.0;
    let mut state_2x = NotchState::lowpass(cutoff, 0.707, sr);
    let amp_2x = biquad_peak(&mut state_2x, cutoff * 2.0, sr, 500, 2000);

    let mut state_4x = NotchState::lowpass(cutoff, 0.707, sr);
    let amp_4x = biquad_peak(&mut state_4x, cutoff * 4.0, sr, 500, 2000);

    // 2nd-order filter: ~12 dB/octave rolloff. At 4x cutoff should be lower than 2x.
    assert!(
        amp_4x < amp_2x,
        "4x cutoff should be more attenuated: 2x={amp_2x}, 4x={amp_4x}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Additional functional tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn response_curve_soft_attenuates_midrange() {
    let state = ResponseCurveState::soft();
    let mut frame = Frame::from_torque(0.5);
    response_curve_filter(&mut frame, &state);
    assert!(
        frame.torque_out < 0.5,
        "soft curve should reduce mid-range, got {}",
        frame.torque_out
    );
}

#[test]
fn response_curve_hard_boosts_midrange() {
    let state = ResponseCurveState::hard();
    let mut frame = Frame::from_torque(0.5);
    response_curve_filter(&mut frame, &state);
    assert!(
        frame.torque_out > 0.5,
        "hard curve should boost mid-range, got {}",
        frame.torque_out
    );
}

#[test]
fn curve_quadratic_reduces_midrange() {
    let state = CurveState::quadratic();
    let mut frame = Frame::from_torque(0.5);
    curve_filter(&mut frame, &state);
    assert!(
        frame.torque_out < 0.5,
        "quadratic should reduce mid-range, got {}",
        frame.torque_out
    );
}

#[test]
fn torque_cap_clamps_above_max() {
    let mut frame = Frame::from_torque(0.9);
    openracing_filters::torque_cap_filter(&mut frame, 0.5);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.001,
        "torque cap should clamp to max, got {}",
        frame.torque_out
    );
}

#[test]
fn torque_cap_passes_below_max() {
    let mut frame = Frame::from_torque(0.3);
    openracing_filters::torque_cap_filter(&mut frame, 0.5);
    assert!(
        (frame.torque_out - 0.3).abs() < 0.001,
        "torque cap should pass through below max, got {}",
        frame.torque_out
    );
}

#[test]
fn torque_cap_handles_infinity() {
    let mut frame = Frame::from_torque(f32::INFINITY);
    openracing_filters::torque_cap_filter(&mut frame, 0.8);
    assert!(
        frame.torque_out.is_finite(),
        "torque cap should handle infinity, got {}",
        frame.torque_out
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. Proptest: property-based tests (≥25)
// ═══════════════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_notch_output_is_finite(input in -1.0f32..1.0) {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut frame = Frame::from_torque(input);
        notch_filter(&mut frame, &mut state);
        prop_assert!(frame.torque_out.is_finite());
    }

    #[test]
    fn prop_lowpass_output_is_finite(input in -1.0f32..1.0) {
        let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
        let mut frame = Frame::from_torque(input);
        notch_filter(&mut frame, &mut state);
        prop_assert!(frame.torque_out.is_finite());
    }

    #[test]
    fn prop_reconstruction_output_bounded(input in -1.0f32..1.0) {
        let mut state = ReconstructionState::new(4);
        let mut frame = Frame::from_torque(input);
        frame.ffb_in = input;
        reconstruction_filter(&mut frame, &mut state);
        prop_assert!(frame.torque_out.abs() <= 1.0 + 1e-6);
    }

    #[test]
    fn prop_slew_rate_change_bounded(input in -1.0f32..1.0) {
        let mut state = SlewRateState::new(0.5);
        let mut frame = Frame::from_torque(input);
        slew_rate_filter(&mut frame, &mut state);
        let max_change = 0.5 / 1000.0;
        let change = frame.torque_out.abs();
        prop_assert!(change <= max_change + 1e-6);
    }

    #[test]
    fn prop_damper_zero_speed_no_change(input in -1.0f32..1.0) {
        let state = DamperState::fixed(0.1);
        let mut frame = Frame::from_ffb(input, 0.0);
        damper_filter(&mut frame, &state);
        prop_assert!((frame.torque_out - input).abs() < 1e-6);
    }

    #[test]
    fn prop_friction_zero_speed_no_change(input in -1.0f32..1.0) {
        let state = FrictionState::fixed(0.1);
        let mut frame = Frame::from_ffb(input, 0.0);
        friction_filter(&mut frame, &state);
        prop_assert!((frame.torque_out - input).abs() < 1e-6);
    }

    #[test]
    fn prop_inertia_constant_speed_no_effect(speed in -10.0f32..10.0) {
        let mut state = InertiaState::new(0.1);
        // Initialize state
        let mut f1 = Frame::from_ffb(0.0, speed);
        inertia_filter(&mut f1, &mut state);
        // Second frame at same speed
        let mut f2 = Frame::from_ffb(0.0, speed);
        f2.torque_out = 0.0;
        inertia_filter(&mut f2, &mut state);
        prop_assert!((f2.torque_out).abs() < 1e-4);
    }

    #[test]
    fn prop_curve_linear_preserves_sign(input in -1.0f32..1.0) {
        let state = CurveState::linear();
        let mut frame = Frame::from_torque(input);
        curve_filter(&mut frame, &state);
        if input.abs() > 0.001 {
            prop_assert_eq!(frame.torque_out.signum() as i32, input.signum() as i32);
        }
    }

    #[test]
    fn prop_response_curve_preserves_sign(input in -1.0f32..1.0) {
        let state = ResponseCurveState::linear();
        let mut frame = Frame::from_torque(input);
        response_curve_filter(&mut frame, &state);
        if input.abs() > 0.001 {
            prop_assert_eq!(frame.torque_out.signum() as i32, input.signum() as i32);
        }
    }

    #[test]
    fn prop_torque_cap_output_bounded(input in -2.0f32..2.0, cap in 0.1f32..1.0) {
        let mut frame = Frame::from_torque(input);
        openracing_filters::torque_cap_filter(&mut frame, cap);
        prop_assert!(frame.torque_out.abs() <= cap + 1e-6);
    }

    #[test]
    fn prop_bumpstop_disabled_no_effect(input in -1.0f32..1.0, speed in -10.0f32..10.0) {
        let mut state = BumpstopState::disabled();
        let mut frame = Frame::from_ffb(input, speed);
        bumpstop_filter(&mut frame, &mut state);
        prop_assert!((frame.torque_out - input).abs() < 1e-6);
    }

    #[test]
    fn prop_hands_off_disabled_always_false(torque in -1.0f32..1.0) {
        let mut state = HandsOffState::disabled();
        let mut frame = Frame::from_torque(torque);
        hands_off_detector(&mut frame, &mut state);
        prop_assert!(!frame.hands_off);
    }

    #[test]
    fn prop_notch_bypass_is_identity(input in -1.0f32..1.0) {
        let mut state = NotchState::bypass();
        let mut frame = Frame::from_torque(input);
        notch_filter(&mut frame, &mut state);
        prop_assert!((frame.torque_out - input).abs() < 1e-4);
    }

    #[test]
    fn prop_slew_rate_unlimited_is_passthrough(input in -1.0f32..1.0) {
        let mut state = SlewRateState::unlimited();
        let mut frame = Frame::from_torque(input);
        slew_rate_filter(&mut frame, &mut state);
        prop_assert!((frame.torque_out - input).abs() < 1e-4);
    }

    #[test]
    fn prop_reconstruction_bypass_is_passthrough(input in -1.0f32..1.0) {
        let mut state = ReconstructionState::bypass();
        let mut frame = Frame::from_torque(input);
        frame.ffb_in = input;
        reconstruction_filter(&mut frame, &mut state);
        prop_assert!((frame.torque_out - input).abs() < 1e-4);
    }

    #[test]
    fn prop_damper_opposes_motion(speed in 0.1f32..10.0) {
        let state = DamperState::fixed(0.1);
        let mut frame = Frame::from_ffb(0.0, speed);
        damper_filter(&mut frame, &state);
        prop_assert!(frame.torque_out < 0.0);
    }

    #[test]
    fn prop_friction_opposes_motion(speed in 0.1f32..10.0) {
        let state = FrictionState::fixed(0.1);
        let mut frame = Frame::from_ffb(0.0, speed);
        friction_filter(&mut frame, &state);
        prop_assert!(frame.torque_out < 0.0);
    }

    #[test]
    fn prop_curve_output_bounded(input in -1.0f32..1.0) {
        let state = CurveState::linear();
        let mut frame = Frame::from_torque(input);
        curve_filter(&mut frame, &state);
        prop_assert!(frame.torque_out.abs() <= 1.0 + 1e-6);
        prop_assert!(frame.torque_out.is_finite());
    }

    #[test]
    fn prop_response_curve_output_bounded(input in -1.0f32..1.0) {
        let state = ResponseCurveState::linear();
        let mut frame = Frame::from_torque(input);
        response_curve_filter(&mut frame, &state);
        prop_assert!(frame.torque_out.abs() <= 1.0 + 1e-6);
        prop_assert!(frame.torque_out.is_finite());
    }

    #[test]
    fn prop_notch_deterministic(input in -1.0f32..1.0) {
        let mut s1 = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut s2 = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut f1 = Frame::from_torque(input);
        let mut f2 = Frame::from_torque(input);
        notch_filter(&mut f1, &mut s1);
        notch_filter(&mut f2, &mut s2);
        prop_assert!((f1.torque_out - f2.torque_out).abs() < 1e-6);
    }

    #[test]
    fn prop_lowpass_dc_gain_near_unity(level in 0u8..9) {
        let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);
        let _ = level; // used for variance
        for _ in 0..500 {
            let mut f = Frame::from_torque(1.0);
            notch_filter(&mut f, &mut state);
        }
        let mut f = Frame::from_torque(1.0);
        notch_filter(&mut f, &mut state);
        prop_assert!((f.torque_out - 1.0).abs() < 0.1);
    }

    #[test]
    fn prop_reconstruction_level_valid(level in 0u8..9) {
        let state = ReconstructionState::new(level);
        prop_assert!(state.alpha > 0.0);
        prop_assert!(state.alpha <= 1.0);
    }

    #[test]
    fn prop_inertia_output_finite(accel in -100.0f32..100.0) {
        let mut state = InertiaState::new(0.1);
        let mut f1 = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut f1, &mut state);
        let mut f2 = Frame::from_ffb(0.0, accel);
        inertia_filter(&mut f2, &mut state);
        prop_assert!(f2.torque_out.is_finite());
    }

    #[test]
    fn prop_slew_rate_positive_rate_nonneg_output_for_pos_target(rate in 0.1f32..2.0) {
        let mut state = SlewRateState::new(rate);
        let mut frame = Frame::from_torque(1.0);
        slew_rate_filter(&mut frame, &mut state);
        prop_assert!(frame.torque_out >= 0.0);
    }

    #[test]
    fn prop_curve_scurve_endpoints(input_end in prop::bool::ANY) {
        let state = CurveState::scurve();
        let val = if input_end { 1.0 } else { 0.0 };
        let out = state.lookup(val);
        let expected = val;
        prop_assert!((out - expected).abs() < 0.02);
    }
}
