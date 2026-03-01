//! Property-Based Tests for Filters
//!
//! This module contains property tests that verify filter behavior
//! across a wide range of inputs.

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
