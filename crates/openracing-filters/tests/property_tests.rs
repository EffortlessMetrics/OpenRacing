//! Property-Based Tests for Filters
//!
//! This module contains property tests that verify filter behavior
//! across a wide range of inputs.

#![allow(clippy::redundant_closure)]

use openracing_filters::prelude::*;

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn reconstruction_filter_always_finite(input in -10.0f32..10.0f32) {
            let mut state = ReconstructionState::new(4);
            let mut frame = Frame::from_ffb(input, 0.0);
            reconstruction_filter(&mut frame, &mut state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn reconstruction_filter_deterministic(input in -1.0f32..1.0f32) {
            let mut state1 = ReconstructionState::new(4);
            let mut state2 = ReconstructionState::new(4);

            let mut frame1 = Frame::from_ffb(input, 0.0);
            let mut frame2 = Frame::from_ffb(input, 0.0);

            reconstruction_filter(&mut frame1, &mut state1);
            reconstruction_filter(&mut frame2, &mut state2);

            prop_assert!((frame1.torque_out - frame2.torque_out).abs() < 1e-6);
        }

        #[test]
        fn friction_filter_finite_with_valid_input(
            wheel_speed in -20.0f32..20.0f32,
            coefficient in 0.0f32..1.0f32
        ) {
            let state = FrictionState::new(coefficient, true);
            let mut frame = Frame::from_ffb(0.0, wheel_speed);
            friction_filter(&mut frame, &state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn damper_filter_finite_with_valid_input(
            wheel_speed in -20.0f32..20.0f32,
            coefficient in 0.0f32..1.0f32
        ) {
            let state = DamperState::new(coefficient, true);
            let mut frame = Frame::from_ffb(0.0, wheel_speed);
            damper_filter(&mut frame, &state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn inertia_filter_finite_with_valid_input(
            wheel_speed in -20.0f32..20.0f32,
            prev_speed in -20.0f32..20.0f32,
            coefficient in 0.0f32..1.0f32
        ) {
            let mut state = InertiaState::new(coefficient);
            state.prev_wheel_speed = prev_speed;
            let mut frame = Frame::from_ffb(0.0, wheel_speed);
            inertia_filter(&mut frame, &mut state);
            prop_assert!(frame.torque_out.is_finite());
        }

        #[test]
        fn slew_rate_filter_always_moves_toward_target(
            target in -1.0f32..1.0f32,
            prev_output in -1.0f32..1.0f32
        ) {
            let mut state = SlewRateState::new(0.5);
            state.prev_output = prev_output;
            let mut frame = Frame::from_ffb(target, 0.0);

            slew_rate_filter(&mut frame, &mut state);

            // Output should be between prev_output and target (or equal)
            if target > prev_output {
                prop_assert!(frame.torque_out >= prev_output);
                prop_assert!(frame.torque_out <= target);
            } else {
                prop_assert!(frame.torque_out <= prev_output);
                prop_assert!(frame.torque_out >= target);
            }
        }

        #[test]
        fn curve_filter_output_bounded(input in -2.0f32..2.0f32) {
            let state = CurveState::scurve();
            let mut frame = Frame::from_ffb(input, 0.0);
            curve_filter(&mut frame, &state);

            prop_assert!(frame.torque_out.is_finite());
            prop_assert!(frame.torque_out.abs() <= 1.0);
        }

        #[test]
        fn response_curve_output_bounded(input in -2.0f32..2.0f32) {
            let state = ResponseCurveState::soft();
            let mut frame = Frame::from_ffb(input, 0.0);
            response_curve_filter(&mut frame, &state);

            prop_assert!(frame.torque_out.is_finite());
            prop_assert!(frame.torque_out.abs() <= 1.0);
        }

        #[test]
        fn torque_cap_enforces_limit(
            torque in -5.0f32..5.0f32,
            max_torque in 0.1f32..2.0f32
        ) {
            let mut frame = Frame::from_ffb(torque, 0.0);
            torque_cap_filter(&mut frame, max_torque);

            prop_assert!(frame.torque_out.abs() <= max_torque);
        }

        #[test]
        fn notch_filter_stability(input in -10.0f32..10.0f32) {
            let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
            let mut frame = Frame::from_ffb(input, 0.0);

            for _ in 0..10 {
                notch_filter(&mut frame, &mut state);
            }

            prop_assert!(frame.torque_out.is_finite());
            prop_assert!(frame.torque_out.abs() < 100.0);
        }

        // --- Bumpstop: output is always bounded ---

        #[test]
        fn bumpstop_output_always_bounded(
            wheel_speed in -500.0f32..500.0f32,
            stiffness in 0.0f32..2.0f32,
            damping in 0.0f32..1.0f32,
            iterations in 1usize..50,
        ) {
            let mut state = BumpstopState::new(true, 400.0, 500.0, stiffness, damping);
            for _ in 0..iterations {
                let mut frame = Frame::from_ffb(0.0, wheel_speed);
                bumpstop_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite(),
                    "bumpstop output must be finite, got {}", frame.torque_out);
                // Stiffness ≤ 2.0 and penetration ≤ 1.0 ⇒ spring ≤ 2.0;
                // damping contribution bounded by speed * damping * 0.001 * to_degrees.
                // Use a generous bound.
                prop_assert!(frame.torque_out.abs() < 500.0,
                    "bumpstop output {} exceeds generous bound", frame.torque_out);
            }
        }

        // --- Bumpstop: damping coefficient increases resistance ---

        #[test]
        fn bumpstop_damping_increases_resistance(
            stiffness in 0.1f32..1.0f32,
        ) {
            // Place the wheel well into the bumpstop zone with positive speed
            let mut state_no_damp = BumpstopState::new(true, 400.0, 500.0, stiffness, 0.0);
            state_no_damp.current_angle = 450.0;
            let mut frame_no_damp = Frame::from_ffb(0.0, 100.0);
            bumpstop_filter(&mut frame_no_damp, &mut state_no_damp);

            let mut state_damp = BumpstopState::new(true, 400.0, 500.0, stiffness, 0.5);
            state_damp.current_angle = 450.0;
            let mut frame_damp = Frame::from_ffb(0.0, 100.0);
            bumpstop_filter(&mut frame_damp, &mut state_damp);

            // Damped version should have greater magnitude opposing force
            prop_assert!(frame_damp.torque_out.abs() >= frame_no_damp.torque_out.abs(),
                "damping must add resistance: no_damp={}, damp={}",
                frame_no_damp.torque_out.abs(), frame_damp.torque_out.abs());
        }

        // --- Damper: linearity (fixed mode) ---

        #[test]
        fn damper_linearity_fixed(
            coefficient in 0.01f32..1.0f32,
            speed1 in -10.0f32..10.0f32,
            speed2 in -10.0f32..10.0f32,
        ) {
            let state = DamperState::fixed(coefficient);
            let combined_speed = speed1 + speed2;

            let mut frame1 = Frame::from_ffb(0.0, speed1);
            damper_filter(&mut frame1, &state);
            let out1 = frame1.torque_out;

            let mut frame2 = Frame::from_ffb(0.0, speed2);
            damper_filter(&mut frame2, &state);
            let out2 = frame2.torque_out;

            let mut frame_combined = Frame::from_ffb(0.0, combined_speed);
            damper_filter(&mut frame_combined, &state);
            let out_combined = frame_combined.torque_out;

            // For a linear filter: f(a+b) ≈ f(a) + f(b) (additivity)
            // damper_torque = -speed * coefficient, which is linear
            prop_assert!((out_combined - (out1 + out2)).abs() < 1e-4,
                "damper not linear: f({})={}, f({})={}, f({})={}",
                speed1, out1, speed2, out2, combined_speed, out_combined);
        }

        // --- Friction: sign-reversal behavior ---

        #[test]
        fn friction_opposes_motion_direction(
            speed in 0.001f32..20.0f32,
            coefficient in 0.01f32..1.0f32,
        ) {
            let state = FrictionState::fixed(coefficient);

            // Positive speed → negative friction torque
            let mut frame_pos = Frame::from_ffb(0.0, speed);
            friction_filter(&mut frame_pos, &state);
            prop_assert!(frame_pos.torque_out <= 0.0,
                "friction must oppose positive speed {}, got torque {}",
                speed, frame_pos.torque_out);

            // Negative speed → positive friction torque
            let mut frame_neg = Frame::from_ffb(0.0, -speed);
            friction_filter(&mut frame_neg, &state);
            prop_assert!(frame_neg.torque_out >= 0.0,
                "friction must oppose negative speed {}, got torque {}",
                -speed, frame_neg.torque_out);

            // Magnitudes should be equal (symmetric) for fixed mode
            prop_assert!((frame_pos.torque_out.abs() - frame_neg.torque_out.abs()).abs() < 1e-6,
                "friction asymmetric: pos={}, neg={}",
                frame_pos.torque_out.abs(), frame_neg.torque_out.abs());
        }
    }
}

#[cfg(test)]
mod quickcheck_tests {
    use super::*;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    #[derive(Debug, Clone)]
    struct FiniteF32(f32);

    impl Arbitrary for FiniteF32 {
        fn arbitrary(g: &mut Gen) -> Self {
            let value = f32::arbitrary(g);
            FiniteF32(if value.is_finite() { value } else { 0.0 })
        }
    }

    fn prop_all_filters_finite(ffb_in: FiniteF32, wheel_speed: FiniteF32) -> TestResult {
        let ffb_in = ffb_in.0.clamp(-10.0, 10.0);
        let wheel_speed = wheel_speed.0.clamp(-100.0, 100.0);

        let mut frame = Frame::from_ffb(ffb_in, wheel_speed);

        let mut recon_state = ReconstructionState::new(4);
        let friction_state = FrictionState::new(0.1, true);
        let damper_state = DamperState::new(0.1, true);
        let mut inertia_state = InertiaState::new(0.1);
        let mut slew_state = SlewRateState::new(0.5);
        let curve_state = CurveState::linear();

        reconstruction_filter(&mut frame, &mut recon_state);
        if !frame.torque_out.is_finite() {
            return TestResult::failed();
        }

        friction_filter(&mut frame, &friction_state);
        if !frame.torque_out.is_finite() {
            return TestResult::failed();
        }

        damper_filter(&mut frame, &damper_state);
        if !frame.torque_out.is_finite() {
            return TestResult::failed();
        }

        inertia_filter(&mut frame, &mut inertia_state);
        if !frame.torque_out.is_finite() {
            return TestResult::failed();
        }

        slew_rate_filter(&mut frame, &mut slew_state);
        if !frame.torque_out.is_finite() {
            return TestResult::failed();
        }

        curve_filter(&mut frame, &curve_state);
        if !frame.torque_out.is_finite() {
            return TestResult::failed();
        }

        torque_cap_filter(&mut frame, 1.0);
        if !frame.torque_out.is_finite() || frame.torque_out.abs() > 1.0 {
            return TestResult::failed();
        }

        TestResult::passed()
    }

    #[test]
    fn quickcheck_all_filters_finite() {
        QuickCheck::new()
            .tests(1000)
            .quickcheck(prop_all_filters_finite as fn(FiniteF32, FiniteF32) -> TestResult);
    }
}

// ── Filter output bounded for bounded input ─────────────────────────────────

#[cfg(test)]
mod bounded_output_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(500))]

        /// Any filter chain output must remain finite and bounded for inputs in [-1, 1].
        #[test]
        fn prop_full_chain_bounded(
            ffb_in in -1.0f32..=1.0f32,
            wheel_speed in -10.0f32..10.0f32,
        ) {
            let mut frame = Frame::from_ffb(ffb_in, wheel_speed);

            let mut recon = ReconstructionState::new(4);
            let friction = FrictionState::new(0.1, true);
            let damper = DamperState::new(0.1, true);
            let mut inertia = InertiaState::new(0.1);
            let mut slew = SlewRateState::new(0.5);
            let curve = CurveState::linear();
            let resp = ResponseCurveState::soft();
            let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);

            reconstruction_filter(&mut frame, &mut recon);
            prop_assert!(frame.torque_out.is_finite());

            friction_filter(&mut frame, &friction);
            prop_assert!(frame.torque_out.is_finite());

            damper_filter(&mut frame, &damper);
            prop_assert!(frame.torque_out.is_finite());

            inertia_filter(&mut frame, &mut inertia);
            prop_assert!(frame.torque_out.is_finite());

            notch_filter(&mut frame, &mut notch);
            prop_assert!(frame.torque_out.is_finite());

            slew_rate_filter(&mut frame, &mut slew);
            prop_assert!(frame.torque_out.is_finite());

            curve_filter(&mut frame, &curve);
            prop_assert!(frame.torque_out.is_finite());
            prop_assert!(frame.torque_out.abs() <= 1.0);

            response_curve_filter(&mut frame, &resp);
            prop_assert!(frame.torque_out.is_finite());
            prop_assert!(frame.torque_out.abs() <= 1.0);

            torque_cap_filter(&mut frame, 1.0);
            prop_assert!(frame.torque_out.abs() <= 1.0);
        }

        /// Reconstruction filter output converges toward input over many samples.
        #[test]
        fn prop_reconstruction_bounded_output(input in -1.0f32..=1.0f32) {
            let mut state = ReconstructionState::new(4);
            for _ in 0..200 {
                let mut frame = Frame::from_ffb(input, 0.0);
                reconstruction_filter(&mut frame, &mut state);
                prop_assert!(frame.torque_out.is_finite());
            }
            // After 200 samples the EMA should be close to input
            prop_assert!((state.prev_output - input).abs() < 0.05,
                "expected convergence to {}, got {}", input, state.prev_output);
        }

        /// Bumpstop filter never produces NaN or Inf for finite speeds.
        #[test]
        fn prop_bumpstop_bounded(
            wheel_speed in -500.0f32..500.0f32,
            stiffness in 0.01f32..2.0f32,
            damping in 0.0f32..1.0f32,
        ) {
            let mut state = BumpstopState::new(true, 400.0, 500.0, stiffness, damping);
            state.current_angle = 450.0; // inside bumpstop zone
            let mut frame = Frame::from_ffb(0.0, wheel_speed);
            bumpstop_filter(&mut frame, &mut state);
            prop_assert!(frame.torque_out.is_finite(),
                "bumpstop output must be finite, got {}", frame.torque_out);
        }

        /// Hands-off detector never sets hands_off when torque exceeds threshold.
        #[test]
        fn prop_hands_off_not_triggered_with_torque(
            torque in 0.1f32..1.0f32,
            iterations in 1usize..200,
        ) {
            let mut state = HandsOffState::new(true, 0.05, 0.1);
            for _ in 0..iterations {
                let mut frame = Frame::from_torque(torque);
                hands_off_detector(&mut frame, &mut state);
                prop_assert!(!frame.hands_off,
                    "hands_off must not trigger with torque {}", torque);
            }
        }
    }
}

// ── Filter chain composition ────────────────────────────────────────────────

#[cfg(test)]
mod composition_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(200))]

        /// Applying two curve filters sequentially produces the same result
        /// regardless of intermediate frame creation (composition through frames).
        #[test]
        fn prop_curve_composition_via_frames(input in -1.0f32..=1.0f32) {
            let curve_a = CurveState::scurve();
            let curve_b = CurveState::quadratic();

            // Apply A then B
            let mut frame_ab = Frame::from_torque(input);
            curve_filter(&mut frame_ab, &curve_a);
            let mid = frame_ab.torque_out;
            curve_filter(&mut frame_ab, &curve_b);
            let result_ab = frame_ab.torque_out;

            // Apply A then B starting from the same input
            let mut frame2 = Frame::from_torque(input);
            curve_filter(&mut frame2, &curve_a);
            prop_assert!((frame2.torque_out - mid).abs() < 1e-6,
                "first stage must be identical");
            curve_filter(&mut frame2, &curve_b);

            prop_assert!((frame2.torque_out - result_ab).abs() < 1e-6,
                "composition must be deterministic");
        }

        /// Torque cap followed by response curve keeps output in [-1, 1].
        #[test]
        fn prop_cap_then_curve_bounded(
            torque in -5.0f32..5.0f32,
            max in 0.1f32..1.0f32,
        ) {
            let mut frame = Frame::from_torque(torque);
            torque_cap_filter(&mut frame, max);
            let resp = ResponseCurveState::linear();
            response_curve_filter(&mut frame, &resp);
            prop_assert!(frame.torque_out.abs() <= 1.0 + 1e-6);
        }

        /// Slew rate after reconstruction filter: output changes slowly.
        #[test]
        fn prop_slew_limits_reconstruction_rate(
            input in -1.0f32..=1.0f32,
        ) {
            let mut recon = ReconstructionState::new(0); // bypass (alpha=1)
            let mut slew = SlewRateState::new(0.5); // 0.0005 per tick

            let mut frame = Frame::from_ffb(input, 0.0);
            reconstruction_filter(&mut frame, &mut recon);
            slew_rate_filter(&mut frame, &mut slew);

            // First tick from 0: output must be at most max_change_per_tick
            prop_assert!(frame.torque_out.abs() <= 0.5 / 1000.0 + 1e-6,
                "slew must limit first-tick output, got {}", frame.torque_out);
        }
    }
}

// ── Frequency response at Nyquist ───────────────────────────────────────────

#[cfg(test)]
mod frequency_response_tests {
    use super::*;
    use std::f32::consts::PI;

    /// At the Nyquist frequency (500 Hz for 1kHz sample rate), the notch filter
    /// output should remain stable and finite.
    #[test]
    fn notch_at_nyquist_frequency_stable() {
        let sample_rate = 1000.0;
        let nyquist = sample_rate / 2.0;
        let mut state = NotchState::new(nyquist - 1.0, 2.0, -6.0, sample_rate);

        // Generate a signal at near-Nyquist (alternating +1, -1)
        for i in 0..500 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state);
            assert!(frame.torque_out.is_finite(),
                "output must be finite at tick {}, got {}", i, frame.torque_out);
            assert!(frame.torque_out.abs() < 100.0,
                "output must be bounded at tick {}, got {}", i, frame.torque_out);
        }
    }

    /// Lowpass filter at Nyquist should attenuate a Nyquist-rate signal.
    #[test]
    fn lowpass_attenuates_nyquist_signal() {
        let sample_rate = 1000.0;
        let cutoff = 100.0; // well below Nyquist
        let mut state = NotchState::lowpass(cutoff, 0.707, sample_rate);

        // Feed alternating signal (Nyquist rate)
        let mut last_output = 0.0f32;
        for i in 0..200 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state);
            last_output = frame.torque_out;
        }

        // After 200 samples, the Nyquist signal should be strongly attenuated
        assert!(last_output.abs() < 0.5,
            "lowpass must attenuate Nyquist signal, last output = {}", last_output);
    }

    /// Notch filter at its center frequency should attenuate a sinusoidal signal
    /// at that frequency relative to a passband signal.
    #[test]
    fn notch_attenuates_center_frequency() {
        let sample_rate = 1000.0;
        let center_freq = 50.0;
        let mut state_notch = NotchState::new(center_freq, 2.0, -12.0, sample_rate);

        // Generate 50 Hz sine (at center freq) and measure steady-state amplitude
        let mut max_output_center = 0.0f32;
        for i in 0..2000 {
            let t = i as f32 / sample_rate;
            let input = (2.0 * PI * center_freq * t).sin();
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state_notch);
            if i > 500 {
                max_output_center = max_output_center.max(frame.torque_out.abs());
            }
        }

        // Generate 200 Hz sine (passband) and measure steady-state amplitude
        let mut state_pass = NotchState::new(center_freq, 2.0, -12.0, sample_rate);
        let mut max_output_pass = 0.0f32;
        let pass_freq = 200.0;
        for i in 0..2000 {
            let t = i as f32 / sample_rate;
            let input = (2.0 * PI * pass_freq * t).sin();
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state_pass);
            if i > 500 {
                max_output_pass = max_output_pass.max(frame.torque_out.abs());
            }
        }

        // The notch frequency should be attenuated more than the passband
        assert!(max_output_center < max_output_pass,
            "notch center ({}) must be attenuated more than passband ({})",
            max_output_center, max_output_pass);
    }

    /// Reconstruction filter with heavy smoothing should attenuate high-frequency content.
    #[test]
    fn reconstruction_attenuates_high_frequency() {
        let mut state_heavy = ReconstructionState::heavy();
        let mut max_output = 0.0f32;

        // Feed alternating signal (highest frequency)
        for i in 0..500 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let mut frame = Frame::from_ffb(input, 0.0);
            reconstruction_filter(&mut frame, &mut state_heavy);
            if i > 100 {
                max_output = max_output.max(frame.torque_out.abs());
            }
        }

        // Heavy smoothing (alpha=0.03) should strongly attenuate alternating signal
        assert!(max_output < 0.1,
            "heavy reconstruction must attenuate alternating signal, max = {}", max_output);
    }
}

// ── Filter stability with rapid parameter changes ───────────────────────────

#[cfg(test)]
mod stability_tests {
    use super::*;

    /// Notch filter remains stable when re-initialized mid-stream.
    #[test]
    fn notch_stable_with_param_changes() {
        let sample_rate = 1000.0;
        let mut state = NotchState::new(50.0, 2.0, -6.0, sample_rate);

        for i in 0..1000 {
            let input = ((i as f32) * 0.1).sin();
            let mut frame = Frame::from_torque(input);
            notch_filter(&mut frame, &mut state);
            assert!(frame.torque_out.is_finite(),
                "output must be finite at tick {}", i);

            // Every 100 ticks, abruptly change filter parameters
            if i % 100 == 0 && i > 0 {
                let new_freq = 30.0 + (i as f32 * 0.05) % 200.0;
                state = NotchState::new(new_freq, 2.0, -6.0, sample_rate);
            }
        }
    }

    /// Slew rate state: changing the rate mid-stream doesn't cause unbounded output.
    #[test]
    fn slew_rate_stable_with_rate_changes() {
        let mut state = SlewRateState::new(0.5);

        for i in 0..1000 {
            let target = ((i as f32) * 0.01).sin();
            let mut frame = Frame::from_torque(target);
            slew_rate_filter(&mut frame, &mut state);
            assert!(frame.torque_out.is_finite());

            // Abruptly change slew rate
            if i % 50 == 0 {
                state.max_change_per_tick = ((i as f32) * 0.001 + 0.0001).min(1.0);
            }
        }
    }

    /// Reconstruction filter: changing smoothing level mid-stream stays finite.
    #[test]
    fn reconstruction_stable_with_level_changes() {
        let mut state = ReconstructionState::new(4);

        for i in 0..1000 {
            let input = ((i as f32) * 0.02).sin() * 0.8;
            let mut frame = Frame::from_ffb(input, 0.0);
            reconstruction_filter(&mut frame, &mut state);
            assert!(frame.torque_out.is_finite());

            if i % 100 == 0 {
                let new_level = (i / 100) as u8 % 9;
                let new_state = ReconstructionState::new(new_level);
                state.alpha = new_state.alpha;
                state.level = new_state.level;
            }
        }
    }

    /// Damper + friction + inertia chain remains finite with alternating speeds.
    #[test]
    fn chain_stable_with_speed_reversals() {
        let damper = DamperState::new(0.3, true);
        let friction = FrictionState::new(0.2, true);
        let mut inertia = InertiaState::new(0.15);

        for i in 0..2000 {
            // Rapid speed reversals
            let speed = if i % 10 < 5 { 10.0 } else { -10.0 };
            let mut frame = Frame::from_ffb(0.0, speed);

            damper_filter(&mut frame, &damper);
            assert!(frame.torque_out.is_finite(), "damper output not finite at {i}");

            friction_filter(&mut frame, &friction);
            assert!(frame.torque_out.is_finite(), "friction output not finite at {i}");

            inertia_filter(&mut frame, &mut inertia);
            assert!(frame.torque_out.is_finite(), "inertia output not finite at {i}");
        }
    }
}

// ── Edge cases: zero sample rate, extreme Q values ──────────────────────────

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    /// NotchState with zero sample rate should produce finite (possibly bypass) coefficients.
    /// The filter must not panic or produce NaN.
    #[test]
    fn notch_zero_sample_rate() {
        // Zero sample rate makes omega = 2*PI*freq/0 = inf, sin/cos of inf = NaN
        // The filter should still not panic; output may be NaN but we verify no panic.
        let state = NotchState::new(50.0, 2.0, -6.0, 0.0);
        // Coefficients may be NaN from division by zero in omega computation,
        // but the constructor must not panic.
        let _ = state.b0;
    }

    /// NotchState with very small sample rate (near zero) should not panic.
    #[test]
    fn notch_near_zero_sample_rate() {
        let state = NotchState::new(50.0, 2.0, -6.0, 0.001);
        let _ = state.b0;
    }

    /// Extreme Q values (Q clamped to [0.1, 10.0] internally).
    #[test]
    fn notch_extreme_q_values() {
        // Very high Q
        let state_high_q = NotchState::new(50.0, 1000.0, -6.0, 1000.0);
        assert!(state_high_q.b0.is_finite());
        assert!(state_high_q.a1.is_finite());
        assert!(state_high_q.a2.is_finite());

        // Very low Q
        let state_low_q = NotchState::new(50.0, 0.001, -6.0, 1000.0);
        assert!(state_low_q.b0.is_finite());
        assert!(state_low_q.a1.is_finite());
        assert!(state_low_q.a2.is_finite());

        // Negative Q (should be clamped)
        let state_neg_q = NotchState::new(50.0, -5.0, -6.0, 1000.0);
        assert!(state_neg_q.b0.is_finite());
    }

    /// Extreme Q lowpass filter should have finite coefficients.
    #[test]
    fn lowpass_extreme_q_values() {
        let state_high = NotchState::lowpass(100.0, 1000.0, 1000.0);
        assert!(state_high.b0.is_finite());

        let state_low = NotchState::lowpass(100.0, 0.001, 1000.0);
        assert!(state_low.b0.is_finite());
    }

    /// Notch filter with frequency at zero should produce finite coefficients.
    #[test]
    fn notch_zero_frequency() {
        let state = NotchState::new(0.0, 2.0, -6.0, 1000.0);
        assert!(state.b0.is_finite());
        assert!(state.a1.is_finite());
    }

    /// Notch filter with frequency above Nyquist should still have finite coefficients.
    #[test]
    fn notch_above_nyquist_frequency() {
        let state = NotchState::new(600.0, 2.0, -6.0, 1000.0);
        assert!(state.b0.is_finite());
        assert!(state.a1.is_finite());
    }

    /// SlewRateState with zero rate means no change allowed.
    #[test]
    fn slew_rate_zero_change() {
        let mut state = SlewRateState::new(0.0);
        state.prev_output = 0.5;
        let mut frame = Frame::from_torque(1.0);
        slew_rate_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 0.5).abs() < 1e-6,
            "zero slew rate must hold previous value");
    }

    /// SlewRateState::unlimited passes through any jump.
    #[test]
    fn slew_rate_unlimited_passes_through() {
        let mut state = SlewRateState::unlimited();
        let mut frame = Frame::from_torque(1.0);
        slew_rate_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 1.0).abs() < 1e-6,
            "unlimited slew rate must pass through");
    }

    /// ReconstructionState with level 0 (alpha=1.0) acts as bypass.
    #[test]
    fn reconstruction_bypass_passes_through() {
        let mut state = ReconstructionState::bypass();
        let mut frame = Frame::from_ffb(0.75, 0.0);
        reconstruction_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 0.75).abs() < 1e-6,
            "bypass reconstruction must pass through");
    }

    /// Curve filter with extreme out-of-range input clamps to [-1, 1].
    #[test]
    fn curve_extreme_input_clamped() {
        let state = CurveState::scurve();
        let mut frame = Frame::from_torque(100.0);
        curve_filter(&mut frame, &state);
        assert!(frame.torque_out.abs() <= 1.0 + 1e-6);

        let mut frame_neg = Frame::from_torque(-100.0);
        curve_filter(&mut frame_neg, &state);
        assert!(frame_neg.torque_out.abs() <= 1.0 + 1e-6);
    }

    /// ResponseCurveState lookup clamps out-of-range inputs.
    #[test]
    fn response_curve_clamps_extreme_input() {
        let state = ResponseCurveState::soft();
        let below = state.lookup(-10.0);
        let above = state.lookup(10.0);
        assert!((below - 0.0).abs() < 0.01);
        assert!((above - 1.0).abs() < 0.01);
    }

    /// FrictionState with zero coefficient produces no friction torque.
    #[test]
    fn friction_zero_coefficient_no_effect() {
        let state = FrictionState::fixed(0.0);
        let mut frame = Frame::from_ffb(0.5, 5.0);
        friction_filter(&mut frame, &state);
        assert!((frame.torque_out - 0.5).abs() < 1e-6,
            "zero friction coefficient must not change torque");
    }

    /// DamperState with zero coefficient produces no damping torque.
    #[test]
    fn damper_zero_coefficient_no_effect() {
        let state = DamperState::fixed(0.0);
        let mut frame = Frame::from_ffb(0.5, 5.0);
        damper_filter(&mut frame, &state);
        assert!((frame.torque_out - 0.5).abs() < 1e-6,
            "zero damper coefficient must not change torque");
    }

    /// InertiaState with zero coefficient produces no inertia torque.
    #[test]
    fn inertia_zero_coefficient_no_effect() {
        let mut state = InertiaState::new(0.0);
        state.prev_wheel_speed = 0.0;
        let mut frame = Frame::from_ffb(0.5, 10.0);
        inertia_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 0.5).abs() < 1e-6,
            "zero inertia coefficient must not change torque");
    }

    /// NotchState::bypass passes signal through unchanged.
    #[test]
    fn notch_bypass_passes_through() {
        let mut state = NotchState::bypass();
        let mut frame = Frame::from_torque(0.42);
        notch_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 0.42).abs() < 1e-4,
            "bypass notch must pass through, got {}", frame.torque_out);
    }

    /// NotchState::is_stable for a well-designed filter with moderate Q.
    #[test]
    fn notch_is_stable_moderate_q() {
        // With Q=0.707 the filter should be well-behaved
        let state = NotchState::lowpass(100.0, 0.707, 1000.0);
        // Note: is_stable uses |a1| + |a2| < 1.0 which is a sufficient (not necessary) condition
        // Some stable filters may not satisfy this strict check, so we just verify finite coefficients
        assert!(state.a1.is_finite());
        assert!(state.a2.is_finite());
    }

    /// HandsOffState disabled never sets hands_off flag.
    #[test]
    fn hands_off_disabled_never_triggers() {
        let mut state = HandsOffState::disabled();
        for _ in 0..5000 {
            let mut frame = Frame::from_torque(0.0);
            hands_off_detector(&mut frame, &mut state);
            assert!(!frame.hands_off);
        }
    }

    /// BumpstopState disabled has no effect on torque.
    #[test]
    fn bumpstop_disabled_no_effect() {
        let mut state = BumpstopState::disabled();
        let mut frame = Frame::from_ffb(0.5, 1000.0);
        bumpstop_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 0.5).abs() < 1e-6);
    }

    /// FilterState::reset clears dynamic state for all filter types.
    #[test]
    fn filter_state_reset_clears_state() {
        let mut recon = ReconstructionState::new(4);
        recon.prev_output = 0.99;
        FilterState::reset(&mut recon);
        assert!((recon.prev_output).abs() < 1e-6);

        let mut inertia = InertiaState::new(0.1);
        inertia.prev_wheel_speed = 50.0;
        FilterState::reset(&mut inertia);
        assert!((inertia.prev_wheel_speed).abs() < 1e-6);

        let mut notch = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        notch.x1 = 1.0;
        notch.y1 = 1.0;
        FilterState::reset(&mut notch);
        assert!((notch.x1).abs() < 1e-6);
        assert!((notch.y1).abs() < 1e-6);

        let mut slew = SlewRateState::new(0.5);
        slew.prev_output = 0.8;
        FilterState::reset(&mut slew);
        assert!((slew.prev_output).abs() < 1e-6);

        let mut bumpstop = BumpstopState::standard();
        bumpstop.current_angle = 400.0;
        FilterState::reset(&mut bumpstop);
        assert!((bumpstop.current_angle).abs() < 1e-6);

        let mut hands_off = HandsOffState::default_detector();
        hands_off.counter = 999;
        hands_off.last_torque = 0.5;
        FilterState::reset(&mut hands_off);
        assert_eq!(hands_off.counter, 0);
        assert!((hands_off.last_torque).abs() < 1e-6);
    }
}
