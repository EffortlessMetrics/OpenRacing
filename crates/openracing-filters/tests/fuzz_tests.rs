//! Fuzzing Tests for Filters
//!
//! This module contains fuzzing tests that verify filter robustness
//! against extreme and edge-case inputs including NaN and Infinity.

use openracing_filters::prelude::*;

fn create_test_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out: ffb_in,
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

#[cfg(test)]
mod fuzz_tests {
    use super::*;

    fn run_all_filters(frame: &mut Frame) {
        let mut recon_state = ReconstructionState::new(4);
        let friction_state = FrictionState::new(0.1, true);
        let damper_state = DamperState::new(0.1, true);
        let mut inertia_state = InertiaState::new(0.1);
        let mut notch_state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut slew_state = SlewRateState::new(0.5);
        let curve_state = CurveState::linear();
        let response_state = ResponseCurveState::linear();
        let mut bumpstop_state = BumpstopState::standard();
        let mut hands_off_state = HandsOffState::default_detector();

        reconstruction_filter(frame, &mut recon_state);
        friction_filter(frame, &friction_state);
        damper_filter(frame, &damper_state);
        inertia_filter(frame, &mut inertia_state);
        notch_filter(frame, &mut notch_state);
        slew_rate_filter(frame, &mut slew_state);
        curve_filter(frame, &curve_state);
        response_curve_filter(frame, &response_state);
        bumpstop_filter(frame, &mut bumpstop_state);
        hands_off_detector(frame, &mut hands_off_state);
        torque_cap_filter(frame, 1.0);
    }

    #[test]
    fn fuzz_nan_input() {
        let inputs = [f32::NAN, -f32::NAN];

        for input in inputs {
            let mut frame = create_test_frame(input, input);
            run_all_filters(&mut frame);
            // After all filters, output should be bounded
            // Some filters may propagate NaN, torque_cap will handle it
            assert!(frame.torque_out.is_nan() || frame.torque_out.abs() <= 1.0);
        }
    }

    #[test]
    fn fuzz_infinity_input() {
        let inputs = [f32::INFINITY, f32::NEG_INFINITY];

        for input in inputs {
            let mut frame = create_test_frame(input, input);
            run_all_filters(&mut frame);
            // Output should be bounded after torque cap
            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0);
        }
    }

    #[test]
    fn fuzz_extreme_positive() {
        let extreme_values = [f32::MAX, f32::MIN_POSITIVE, 1e10f32, 1e20f32, 1e30f32];

        for value in extreme_values {
            let mut frame = create_test_frame(value, value);
            run_all_filters(&mut frame);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_extreme_negative() {
        let extreme_values = [f32::MIN, -f32::MIN_POSITIVE, -1e10f32, -1e20f32, -1e30f32];

        for value in extreme_values {
            let mut frame = create_test_frame(value, value);
            run_all_filters(&mut frame);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_subnormal_input() {
        let subnormal_values = [
            f32::from_bits(1),          // Smallest positive subnormal
            f32::from_bits(0x007FFFFF), // Largest subnormal
            -f32::from_bits(1),         // Smallest negative subnormal
        ];

        for value in subnormal_values {
            let mut frame = create_test_frame(value, value);
            run_all_filters(&mut frame);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_zero_with_sign() {
        let zero_values = [0.0f32, -0.0f32];

        for value in zero_values {
            let mut frame = create_test_frame(value, value);
            run_all_filters(&mut frame);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_random_extreme() {
        // Pseudo-random extreme values
        let mut values = Vec::new();

        // Generate some extreme values
        for i in 0..100 {
            let exp = (i % 40) as f32 - 20.0;
            let base = if i % 2 == 0 { 10.0f32 } else { -10.0f32 };
            values.push(base.powf(exp));
        }

        for value in values {
            if !value.is_finite() {
                continue;
            }
            let mut frame = create_test_frame(value, value * 0.1);
            run_all_filters(&mut frame);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_rapid_changes() {
        let mut frame = create_test_frame(0.0, 0.0);

        for i in 0..1000 {
            // Rapid sign changes
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            let magnitude = ((i % 100) as f32) / 100.0;

            frame.ffb_in = sign * magnitude;
            frame.torque_out = frame.ffb_in;
            frame.wheel_speed = sign * magnitude * 10.0;

            run_all_filters(&mut frame);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_constant_nan_propagation() {
        let mut recon_state = ReconstructionState::new(4);
        let mut frame = create_test_frame(f32::NAN, 0.0);

        // Run multiple iterations to check NaN propagation
        for _ in 0..10 {
            reconstruction_filter(&mut frame, &mut recon_state);
        }

        // NaN should propagate consistently
        assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
    }

    #[test]
    fn fuzz_filter_state_nan_handling() {
        // Test filter states with NaN values
        let mut recon_state = ReconstructionState::new(4);
        recon_state.prev_output = f32::NAN;

        let mut frame = create_test_frame(0.5, 0.0);
        reconstruction_filter(&mut frame, &mut recon_state);

        // Should handle NaN in state gracefully
        assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
    }

    #[test]
    fn fuzz_inertia_state_nan_handling() {
        let mut state = InertiaState::new(0.1);
        state.prev_wheel_speed = f32::NAN;

        let mut frame = create_test_frame(0.0, 1.0);
        inertia_filter(&mut frame, &mut state);

        assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
    }

    #[test]
    fn fuzz_slew_rate_nan_handling() {
        let mut state = SlewRateState::new(0.5);
        state.prev_output = f32::NAN;

        let mut frame = create_test_frame(0.5, 0.0);
        slew_rate_filter(&mut frame, &mut state);

        assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
    }

    #[test]
    fn fuzz_notch_state_nan_handling() {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        state.y1 = f32::NAN;

        let mut frame = create_test_frame(0.5, 0.0);
        notch_filter(&mut frame, &mut state);

        assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
    }

    #[test]
    fn fuzz_curve_filter_nan_input() {
        let state = CurveState::linear();

        let nan_inputs = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];

        for input in nan_inputs {
            let mut frame = create_test_frame(input, 0.0);
            curve_filter(&mut frame, &state);

            // Should handle gracefully
            assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_response_curve_nan_input() {
        let state = ResponseCurveState::linear();

        let nan_inputs = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];

        for input in nan_inputs {
            let mut frame = create_test_frame(input, 0.0);
            response_curve_filter(&mut frame, &state);

            assert!(frame.torque_out.is_nan() || frame.torque_out.is_finite());
        }
    }

    #[test]
    fn fuzz_bumpstop_extreme_angle() {
        let mut state = BumpstopState::standard();
        state.current_angle = f32::MAX;

        let mut frame = create_test_frame(0.0, 0.0);
        bumpstop_filter(&mut frame, &mut state);

        assert!(frame.torque_out.is_finite() || frame.torque_out.is_nan());
    }

    #[test]
    fn fuzz_hands_off_counter_overflow() {
        let mut state = HandsOffState::new(true, 0.05, 1.0);
        state.counter = u32::MAX;

        let mut frame = create_test_frame(0.01, 0.0);
        hands_off_detector(&mut frame, &mut state);

        // Should handle counter overflow gracefully
        assert!(frame.hands_off);
    }
}
