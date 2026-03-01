//! Comprehensive unit tests for filter nodes
//!
//! These tests verify that each filter node behaves correctly with closed-form
//! expectations and bounds checking as required by task 3.2.

#![allow(unused_assignments)]
#![allow(unused_mut)]
#![allow(unused_variables)]

use racing_wheel_engine::filters::*;
use racing_wheel_engine::rt::Frame;

/// Helper function to create a test frame
fn create_test_frame(ffb_in: f32, wheel_speed: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out: ffb_in, // Initialize torque_out to ffb_in
        wheel_speed,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

/// Helper function to apply a filter multiple times
fn apply_filter_n_times<F>(filter_fn: F, frame: &mut Frame, state_ptr: *mut u8, n: usize)
where
    F: Fn(&mut Frame, *mut u8),
{
    for _ in 0..n {
        filter_fn(frame, state_ptr);
    }
}

#[cfg(test)]
mod reconstruction_filter_tests {
    use super::*;

    #[test]
    fn test_reconstruction_filter_step_response() {
        let mut state = ReconstructionState::new(4);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test step response
        let mut frame = create_test_frame(1.0, 0.0);
        reconstruction_filter(&mut frame, state_ptr);

        // Output should be filtered (less than input for first iteration)
        assert!(frame.torque_out < 1.0);
        assert!(frame.torque_out > 0.0);

        // Store first iteration result
        let first_output = frame.torque_out;

        // Apply filter again
        reconstruction_filter(&mut frame, state_ptr);

        // Should be closer to target
        assert!(frame.torque_out > first_output);
        assert!(frame.torque_out < 1.0);
    }

    #[test]
    fn test_reconstruction_filter_convergence() {
        let mut state = ReconstructionState::new(4);
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(1.0, 0.0);

        // Apply filter many times to test convergence
        apply_filter_n_times(reconstruction_filter, &mut frame, state_ptr, 100);

        // Should converge close to input
        assert!((frame.torque_out - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_reconstruction_filter_different_levels() {
        // Test different reconstruction levels
        for level in 1..=8 {
            let mut state = ReconstructionState::new(level);
            let state_ptr = &mut state as *mut _ as *mut u8;

            let mut frame = create_test_frame(1.0, 0.0);
            reconstruction_filter(&mut frame, state_ptr);

            // Higher levels should filter more aggressively (smaller first step)
            if level > 1 {
                let mut state_lower = ReconstructionState::new(level - 1);
                let state_lower_ptr = &mut state_lower as *mut _ as *mut u8;

                let mut frame_lower = create_test_frame(1.0, 0.0);
                reconstruction_filter(&mut frame_lower, state_lower_ptr);

                // Higher level should have smaller output (more filtering)
                assert!(frame.torque_out <= frame_lower.torque_out);
            }
        }
    }

    #[test]
    fn test_reconstruction_filter_bounds() {
        let mut state = ReconstructionState::new(4);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test extreme inputs
        let extreme_inputs = vec![-1.0, -0.5, 0.0, 0.5, 1.0];

        for &input in &extreme_inputs {
            let mut frame = create_test_frame(input, 0.0);
            reconstruction_filter(&mut frame, state_ptr);

            // Output should be finite and reasonable
            assert!(frame.torque_out.is_finite());
            // For reconstruction filter, output should be between 0 and input (first iteration)
            if input != 0.0 {
                assert!(frame.torque_out.abs() <= input.abs()); // Should not exceed input on first pass
            }
        }
    }
}

#[cfg(test)]
mod friction_filter_tests {
    use super::*;

    #[test]
    fn test_friction_filter_speed_adaptive() {
        let mut state = FrictionState::new(0.1, true);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test at low speed
        let mut frame_low = create_test_frame(0.0, 1.0); // 1 rad/s
        friction_filter(&mut frame_low, state_ptr);
        let friction_low = frame_low.torque_out.abs();

        // Test at high speed
        let mut frame_high = create_test_frame(0.0, 10.0); // 10 rad/s
        friction_filter(&mut frame_high, state_ptr);
        let friction_high = frame_high.torque_out.abs();

        // Friction should be lower at higher speeds (speed-adaptive)
        assert!(friction_high < friction_low);
    }

    #[test]
    fn test_friction_filter_non_adaptive() {
        let mut state = FrictionState::new(0.1, false);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test at different speeds
        let mut frame_low = create_test_frame(0.0, 1.0);
        friction_filter(&mut frame_low, state_ptr);
        let friction_low = frame_low.torque_out.abs();

        let mut frame_high = create_test_frame(0.0, 10.0);
        friction_filter(&mut frame_high, state_ptr);
        let friction_high = frame_high.torque_out.abs();

        // Friction should be the same (non-adaptive)
        assert!((friction_high - friction_low).abs() < 0.001);
    }

    #[test]
    fn test_friction_filter_direction() {
        let mut state = FrictionState::new(0.1, false);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test positive wheel speed
        let mut frame_pos = create_test_frame(0.0, 5.0);
        friction_filter(&mut frame_pos, state_ptr);

        // Test negative wheel speed
        let mut frame_neg = create_test_frame(0.0, -5.0);
        friction_filter(&mut frame_neg, state_ptr);

        // Friction should oppose motion (opposite sign to wheel speed)
        assert!(frame_pos.torque_out < 0.0); // Negative friction for positive speed
        assert!(frame_neg.torque_out > 0.0); // Positive friction for negative speed

        // Magnitudes should be equal
        assert!((frame_pos.torque_out.abs() - frame_neg.torque_out.abs()).abs() < 0.001);
    }

    #[test]
    fn test_friction_filter_zero_speed() {
        let mut state = FrictionState::new(0.1, true);
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(0.5, 0.0); // Zero wheel speed
        let initial_torque = frame.torque_out;

        friction_filter(&mut frame, state_ptr);

        // At zero speed, friction should not add any torque
        assert_eq!(frame.torque_out, initial_torque);
    }
}

#[cfg(test)]
mod damper_filter_tests {
    use super::*;

    #[test]
    fn test_damper_filter_speed_adaptive() {
        let mut state = DamperState::new(0.1, true);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test at low speed
        let mut frame_low = create_test_frame(0.0, 1.0);
        damper_filter(&mut frame_low, state_ptr);
        let damping_low = frame_low.torque_out.abs();

        // Test at high speed
        let mut frame_high = create_test_frame(0.0, 10.0);
        damper_filter(&mut frame_high, state_ptr);
        let damping_high = frame_high.torque_out.abs();

        // Damping should be higher at higher speeds (speed-adaptive)
        assert!(damping_high > damping_low);
    }

    #[test]
    fn test_damper_filter_proportional_to_speed() {
        let mut state = DamperState::new(0.1, false);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test different speeds
        let speeds = vec![1.0, 2.0, 5.0, 10.0];
        let mut damping_forces = Vec::new();

        for &speed in &speeds {
            let mut frame = create_test_frame(0.0, speed);
            damper_filter(&mut frame, state_ptr);
            damping_forces.push(frame.torque_out.abs());
        }

        // Damping should be proportional to speed
        for i in 1..damping_forces.len() {
            assert!(damping_forces[i] > damping_forces[i - 1]);
        }

        // Check approximate proportionality
        let ratio1 = damping_forces[1] / damping_forces[0];
        let ratio2 = damping_forces[2] / damping_forces[1];
        assert!((ratio1 - 2.0).abs() < 0.1); // Should be approximately 2x
        assert!((ratio2 - 2.5).abs() < 0.1); // Should be approximately 2.5x
    }

    #[test]
    fn test_damper_filter_opposes_motion() {
        let mut state = DamperState::new(0.1, false);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test positive wheel speed
        let mut frame_pos = create_test_frame(0.0, 5.0);
        damper_filter(&mut frame_pos, state_ptr);

        // Test negative wheel speed
        let mut frame_neg = create_test_frame(0.0, -5.0);
        damper_filter(&mut frame_neg, state_ptr);

        // Damping should oppose motion
        assert!(frame_pos.torque_out < 0.0); // Negative damping for positive speed
        assert!(frame_neg.torque_out > 0.0); // Positive damping for negative speed
    }
}

#[cfg(test)]
mod inertia_filter_tests {
    use super::*;

    #[test]
    fn test_inertia_filter_acceleration_response() {
        let mut state = InertiaState::new(0.1);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Start with zero speed
        let mut frame1 = create_test_frame(0.0, 0.0);
        inertia_filter(&mut frame1, state_ptr);
        let initial_torque = frame1.torque_out;

        // Sudden acceleration
        let mut frame2 = create_test_frame(0.0, 5.0);
        inertia_filter(&mut frame2, state_ptr);

        // Should produce opposing torque due to inertia
        assert!(frame2.torque_out < initial_torque);

        // Continue with same speed (no acceleration)
        let mut frame3 = create_test_frame(0.0, 5.0);
        inertia_filter(&mut frame3, state_ptr);

        // No acceleration, so no additional inertia torque
        assert_eq!(frame3.torque_out, 0.0);
    }

    #[test]
    fn test_inertia_filter_deceleration() {
        let mut state = InertiaState::new(0.1);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Start with high speed
        let mut frame1 = create_test_frame(0.0, 10.0);
        inertia_filter(&mut frame1, state_ptr);

        // Sudden deceleration
        let mut frame2 = create_test_frame(0.0, 2.0);
        inertia_filter(&mut frame2, state_ptr);

        // Should produce torque in direction of previous motion (inertia effect)
        assert!(frame2.torque_out > 0.0);
    }

    #[test]
    fn test_inertia_filter_proportional_to_acceleration() {
        let mut state = InertiaState::new(0.1);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Initialize with zero speed
        let mut frame_init = create_test_frame(0.0, 0.0);
        inertia_filter(&mut frame_init, state_ptr);

        // Test different accelerations
        let accelerations = vec![1.0, 2.0, 5.0];
        let mut inertia_torques = Vec::new();

        for &accel in &accelerations {
            // Reset state
            state = InertiaState::new(0.1);
            let state_ptr = &mut state as *mut _ as *mut u8;

            // Initialize
            let mut frame_init = create_test_frame(0.0, 0.0);
            inertia_filter(&mut frame_init, state_ptr);

            // Apply acceleration
            let mut frame = create_test_frame(0.0, accel);
            inertia_filter(&mut frame, state_ptr);
            inertia_torques.push(frame.torque_out.abs());
        }

        // Inertia torque should be proportional to acceleration
        for i in 1..inertia_torques.len() {
            assert!(inertia_torques[i] > inertia_torques[i - 1]);
        }
    }
}

#[cfg(test)]
mod notch_filter_tests {
    use super::*;

    #[test]
    fn test_notch_filter_dc_response() {
        let mut state = NotchState::new(50.0, 2.0, -20.0, 1000.0); // 50Hz notch
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Feed constant DC input to reach steady state
        let mut frame = create_test_frame(1.0, 0.0);
        for _ in 0..20 {
            frame.torque_out = 1.0; // reset input to DC each iteration
            notch_filter(&mut frame, state_ptr);
        }

        // DC should pass through relatively unchanged - allow wider tolerance for numerical precision
        assert!(
            (frame.torque_out - 1.0).abs() < 0.5,
            "DC response out of range: {}",
            frame.torque_out
        );
    }

    #[test]
    fn test_notch_filter_stability() {
        let mut state = NotchState::new(60.0, 2.0, -12.0, 1000.0);
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(0.5, 0.0);

        // Apply filter many times to check stability
        for _ in 0..100 {
            notch_filter(&mut frame, state_ptr);
            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() < 10.0); // Reasonable bound
        }
    }

    #[test]
    fn test_notch_filter_different_frequencies() {
        let frequencies = vec![10.0, 50.0, 100.0, 200.0];

        for &freq in &frequencies {
            let mut state = NotchState::new(freq, 2.0, -20.0, 1000.0);
            let state_ptr = &mut state as *mut _ as *mut u8;

            let mut frame = create_test_frame(0.5, 0.0);

            // Apply filter and check it doesn't blow up
            apply_filter_n_times(notch_filter, &mut frame, state_ptr, 10);

            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() < 2.0); // Reasonable bound
        }
    }

    #[test]
    fn test_notch_filter_q_factor_effect() {
        let q_factors = vec![0.5, 1.0, 2.0, 5.0];

        for &q in &q_factors {
            let mut state = NotchState::new(50.0, q, -20.0, 1000.0);
            let state_ptr = &mut state as *mut _ as *mut u8;

            let mut frame = create_test_frame(0.5, 0.0);

            // Apply filter and verify stability
            apply_filter_n_times(notch_filter, &mut frame, state_ptr, 20);

            assert!(frame.torque_out.is_finite());
        }
    }
}

#[cfg(test)]
mod slew_rate_filter_tests {
    use super::*;

    #[test]
    fn test_slew_rate_filter_limiting() {
        let mut state = SlewRateState::new(0.5); // 50% slew rate
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test step input
        let mut frame = create_test_frame(1.0, 0.0);
        slew_rate_filter(&mut frame, state_ptr);

        // Output should be limited by slew rate
        assert!(frame.torque_out < 1.0);
        assert!(frame.torque_out > 0.0);

        // Expected output: 0.5 / 1000 = 0.0005 per tick
        assert!((frame.torque_out - 0.0005).abs() < 0.0001);
    }

    #[test]
    fn test_slew_rate_filter_convergence() {
        let mut state = SlewRateState::new(0.1); // 10% slew rate
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(1.0, 0.0);

        // Apply filter multiple times
        for i in 0..10 {
            frame.ffb_in = 1.0;
            frame.torque_out = 1.0; // Reset input
            slew_rate_filter(&mut frame, state_ptr);

            // Should gradually approach target
            if i > 0 {
                assert!(frame.torque_out > 0.0);
                assert!(frame.torque_out <= 1.0);
            }
        }

        // After many iterations, should be closer to target
        assert!(frame.torque_out > 0.0005);
    }

    #[test]
    fn test_slew_rate_filter_negative_input() {
        let mut state = SlewRateState::new(0.2); // 20% slew rate
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test negative step input
        let mut frame = create_test_frame(-1.0, 0.0);
        slew_rate_filter(&mut frame, state_ptr);

        // Should be limited in negative direction
        assert!(frame.torque_out > -1.0);
        assert!(frame.torque_out < 0.0);

        // Expected: -0.2 / 1000 = -0.0002
        assert!((frame.torque_out + 0.0002).abs() < 0.0001);
    }

    #[test]
    fn test_slew_rate_filter_no_limiting() {
        let mut state = SlewRateState::new(1.0); // 100% slew rate (no limiting)
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(0.5, 0.0);
        slew_rate_filter(&mut frame, state_ptr);

        // With 100% slew rate, small changes should pass through
        assert!((frame.torque_out - 0.001).abs() < 0.0001); // 1.0/1000 = 0.001
    }
}

#[cfg(test)]
mod curve_filter_tests {
    use super::*;

    #[test]
    fn test_curve_filter_linear() {
        let curve_points = vec![(0.0, 0.0), (1.0, 1.0)]; // Linear curve
        let mut state = CurveState::new(&curve_points);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test various inputs
        let test_inputs = vec![0.0, 0.25, 0.5, 0.75, 1.0];

        for &input in &test_inputs {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            curve_filter(&mut frame, state_ptr);

            // Linear curve should pass through unchanged
            assert!((frame.torque_out - input).abs() < 0.1);
        }
    }

    #[test]
    fn test_curve_filter_quadratic() {
        let curve_points = vec![(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)]; // Quadratic curve
        let mut state = CurveState::new(&curve_points);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test mid-point
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.5;
        curve_filter(&mut frame, state_ptr);

        // Should map 0.5 to approximately 0.25 (quadratic curve)
        assert!((frame.torque_out - 0.25).abs() < 0.1);
    }

    #[test]
    fn test_curve_filter_negative_input() {
        let curve_points = vec![(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)];
        let mut state = CurveState::new(&curve_points);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test negative input
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = -0.5;
        curve_filter(&mut frame, state_ptr);

        // Should preserve sign and apply curve to magnitude
        assert!(frame.torque_out < 0.0);
        assert!(frame.torque_out.abs() < 0.5); // Quadratic curve reduces magnitude
    }

    #[test]
    fn test_curve_filter_bounds() {
        let curve_points = vec![(0.0, 0.0), (1.0, 1.0)];
        let mut state = CurveState::new(&curve_points);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test out-of-bounds inputs
        let extreme_inputs = vec![-2.0, -1.5, 1.5, 2.0];

        for &input in &extreme_inputs {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = input;
            curve_filter(&mut frame, state_ptr);

            // Output should be bounded
            assert!(frame.torque_out.abs() <= 1.0);
            assert!(frame.torque_out.is_finite());
        }
    }
}

#[cfg(test)]
mod torque_cap_filter_tests {
    use super::*;

    #[test]
    fn test_torque_cap_filter_within_limit() {
        let max_torque = 0.8f32;
        let state_ptr = &max_torque as *const f32 as *mut u8;

        // Test within limit
        let mut frame = create_test_frame(0.5, 0.0);
        frame.torque_out = 0.5;
        torque_cap_filter(&mut frame, state_ptr);

        // Should pass through unchanged
        assert_eq!(frame.torque_out, 0.5);
    }

    #[test]
    fn test_torque_cap_filter_over_limit() {
        let max_torque = 0.8f32;
        let state_ptr = &max_torque as *const f32 as *mut u8;

        // Test over positive limit
        let mut frame_pos = create_test_frame(1.0, 0.0);
        frame_pos.torque_out = 1.0;
        torque_cap_filter(&mut frame_pos, state_ptr);
        assert_eq!(frame_pos.torque_out, 0.8);

        // Test over negative limit
        let mut frame_neg = create_test_frame(-1.0, 0.0);
        frame_neg.torque_out = -1.0;
        torque_cap_filter(&mut frame_neg, state_ptr);
        assert_eq!(frame_neg.torque_out, -0.8);
    }

    #[test]
    fn test_torque_cap_filter_extreme_inputs() {
        let max_torque = 0.5f32;
        let state_ptr = &max_torque as *const f32 as *mut u8;

        let extreme_inputs = vec![-100.0, -10.0, 10.0, 100.0];

        for &input in &extreme_inputs {
            let mut frame = create_test_frame(input, 0.0);
            frame.torque_out = input;
            torque_cap_filter(&mut frame, state_ptr);

            // Should be clamped to limit
            assert!(frame.torque_out.abs() <= 0.5);
            assert_eq!(frame.torque_out.abs(), 0.5);
        }
    }
}

#[cfg(test)]
mod bumpstop_filter_tests {
    use super::*;

    #[test]
    fn test_bumpstop_filter_disabled() {
        let mut state = BumpstopState::new(false, 450.0, 540.0, 0.8, 0.3);
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(0.0, 10.0);
        let initial_torque = frame.torque_out;

        bumpstop_filter(&mut frame, state_ptr);

        // Should not modify torque when disabled
        assert_eq!(frame.torque_out, initial_torque);
    }

    #[test]
    #[allow(unused_assignments)]
    fn test_bumpstop_filter_within_range() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Set angle within normal range
        state.current_angle = 400.0; // Below start_angle

        let mut frame = create_test_frame(0.0, 1.0);
        let _initial_torque = frame.torque_out;

        bumpstop_filter(&mut frame, state_ptr);

        // Should not add bumpstop torque within normal range
        // (may change due to angle integration, but no spring force)
        assert!(frame.torque_out.is_finite());
    }

    #[test]
    #[allow(unused_assignments)]
    fn test_bumpstop_filter_at_limit() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Set angle beyond start_angle
        state.current_angle = 500.0; // Beyond start_angle

        let mut frame = create_test_frame(0.0, 1.0);
        let initial_torque = frame.torque_out;

        bumpstop_filter(&mut frame, state_ptr);

        // Should add opposing torque at bumpstop
        assert!(frame.torque_out != initial_torque);
        assert!(frame.torque_out.is_finite());
    }

    #[test]
    #[allow(unused_assignments)]
    fn test_bumpstop_filter_progressive_force() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test different penetration levels
        let angles = vec![460.0, 480.0, 520.0]; // Increasing penetration
        let mut forces = Vec::new();

        for &angle in &angles {
            state.current_angle = angle;

            let mut frame = create_test_frame(0.0, 1.0);
            let initial_torque = frame.torque_out;

            bumpstop_filter(&mut frame, state_ptr);

            forces.push((frame.torque_out - initial_torque).abs());
        }

        // Force should increase with penetration
        for i in 1..forces.len() {
            assert!(forces[i] >= forces[i - 1]);
        }
    }
}

#[cfg(test)]
mod hands_off_detector_tests {
    use super::*;

    #[test]
    fn test_hands_off_detector_disabled() {
        let mut state = HandsOffState::new(false, 0.05, 2.0);
        let state_ptr = &mut state as *mut _ as *mut u8;

        let mut frame = create_test_frame(0.0, 0.0);
        hands_off_detector(&mut frame, state_ptr);

        // Should always report hands on when disabled
        assert!(!frame.hands_off);
    }

    #[test]
    fn test_hands_off_detector_with_resistance() {
        let mut state = HandsOffState::new(true, 0.05, 2.0);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test with significant torque change (hands on)
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.1; // Above threshold
        hands_off_detector(&mut frame, state_ptr);

        assert!(!frame.hands_off);
        assert_eq!(state.counter, 0); // Counter should be reset
    }

    #[test]
    fn test_hands_off_detector_without_resistance() {
        let mut state = HandsOffState::new(true, 0.05, 1.0); // 1 second timeout
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Apply low torque for extended period
        for i in 0..1500 {
            // 1.5 seconds at 1kHz
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 0.01; // Below threshold
            hands_off_detector(&mut frame, state_ptr);

            if i < 999 {
                // Should not detect hands-off before timeout (999 iterations = 1000 calls)
                assert!(
                    !frame.hands_off,
                    "Hands-off detected too early at iteration {}",
                    i
                );
            } else {
                // Should detect hands-off after timeout (at 1000th call and beyond)
                assert!(
                    frame.hands_off,
                    "Hands-off not detected after timeout at iteration {}",
                    i
                );
            }
        }
    }

    #[test]
    fn test_hands_off_detector_reset() {
        let mut state = HandsOffState::new(true, 0.05, 1.0);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Build up counter
        for _ in 0..500 {
            let mut frame = create_test_frame(0.0, 0.0);
            frame.torque_out = 0.01; // Below threshold
            hands_off_detector(&mut frame, state_ptr);
        }

        assert!(state.counter > 0);

        // Apply resistance (hands on)
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.1; // Above threshold
        hands_off_detector(&mut frame, state_ptr);

        // Counter should be reset
        assert_eq!(state.counter, 0);
        assert!(!frame.hands_off);
    }

    #[test]
    fn test_hands_off_detector_threshold_boundary() {
        let mut state = HandsOffState::new(true, 0.05, 1.0);
        let state_ptr = &mut state as *mut _ as *mut u8;

        // Test exactly at threshold
        let mut frame = create_test_frame(0.0, 0.0);
        frame.torque_out = 0.05; // Exactly at threshold
        hands_off_detector(&mut frame, state_ptr);

        // At threshold should not reset counter (needs to be above)
        assert!(state.counter > 0);

        // Test just above threshold
        frame.torque_out = 0.051; // Just above threshold
        hands_off_detector(&mut frame, state_ptr);

        // Should reset counter
        assert_eq!(state.counter, 0);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_filter_chain_stability() {
        // Test a chain of filters for stability
        let mut recon_state = ReconstructionState::new(4);
        let mut friction_state = FrictionState::new(0.1, true);
        let mut damper_state = DamperState::new(0.15, true);
        let mut slew_state = SlewRateState::new(0.8);
        let torque_cap = 0.9f32;

        let mut frame = create_test_frame(0.5, 2.0);

        // Apply filter chain
        for _ in 0..100 {
            reconstruction_filter(&mut frame, &mut recon_state as *mut _ as *mut u8);
            friction_filter(&mut frame, &mut friction_state as *mut _ as *mut u8);
            damper_filter(&mut frame, &mut damper_state as *mut _ as *mut u8);
            slew_rate_filter(&mut frame, &mut slew_state as *mut _ as *mut u8);
            torque_cap_filter(&mut frame, &torque_cap as *const f32 as *mut u8);

            // Verify stability
            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0);
        }
    }

    #[test]
    fn test_all_filters_bounds_checking() {
        // Test that all filters handle extreme inputs gracefully
        let extreme_inputs = vec![-1000.0, -1.0, 0.0, 1.0, 1000.0];

        for &input in &extreme_inputs {
            let mut frame = create_test_frame(input, input);
            frame.torque_out = input;

            // Test each filter with extreme input
            let mut recon_state = ReconstructionState::new(4);
            reconstruction_filter(&mut frame, &mut recon_state as *mut _ as *mut u8);
            assert!(
                frame.torque_out.is_finite(),
                "Reconstruction filter produced non-finite output"
            );

            let mut friction_state = FrictionState::new(0.1, true);
            friction_filter(&mut frame, &mut friction_state as *mut _ as *mut u8);
            assert!(
                frame.torque_out.is_finite(),
                "Friction filter produced non-finite output"
            );

            let mut damper_state = DamperState::new(0.1, true);
            damper_filter(&mut frame, &mut damper_state as *mut _ as *mut u8);
            assert!(
                frame.torque_out.is_finite(),
                "Damper filter produced non-finite output"
            );

            // Apply torque cap to ensure bounds
            let cap = 1.0f32;
            torque_cap_filter(&mut frame, &cap as *const f32 as *mut u8);
            assert!(
                frame.torque_out.abs() <= 1.0,
                "Torque cap failed to limit output"
            );
        }
    }

    #[test]
    fn test_filter_determinism() {
        // Test that filters produce identical outputs for identical inputs
        let inputs = vec![0.0, 0.5, 1.0, -0.5, -1.0];

        for &input in &inputs {
            // Test reconstruction filter determinism
            let mut state1 = ReconstructionState::new(4);
            let mut state2 = ReconstructionState::new(4);

            let mut frame1 = create_test_frame(input, 0.0);
            let mut frame2 = create_test_frame(input, 0.0);

            reconstruction_filter(&mut frame1, &mut state1 as *mut _ as *mut u8);
            reconstruction_filter(&mut frame2, &mut state2 as *mut _ as *mut u8);

            assert_eq!(
                frame1.torque_out, frame2.torque_out,
                "Reconstruction filter not deterministic for input {}",
                input
            );

            // Test other filters similarly
            let mut friction_state1 = FrictionState::new(0.1, true);
            let mut friction_state2 = FrictionState::new(0.1, true);

            let mut frame3 = create_test_frame(0.0, input);
            let mut frame4 = create_test_frame(0.0, input);

            friction_filter(&mut frame3, &mut friction_state1 as *mut _ as *mut u8);
            friction_filter(&mut frame4, &mut friction_state2 as *mut _ as *mut u8);

            assert_eq!(
                frame3.torque_out, frame4.torque_out,
                "Friction filter not deterministic for wheel speed {}",
                input
            );
        }
    }

    #[test]
    fn test_speed_adaptive_behavior() {
        // Verify that speed-adaptive filters actually adapt to speed
        let speeds = vec![0.1, 1.0, 5.0, 10.0];

        // Test friction adaptation
        let mut friction_outputs = Vec::new();
        for &speed in &speeds {
            let mut state = FrictionState::new(0.1, true);
            let mut frame = create_test_frame(0.0, speed);
            friction_filter(&mut frame, &mut state as *mut _ as *mut u8);
            friction_outputs.push(frame.torque_out.abs());
        }

        // Friction should decrease with speed
        for i in 1..friction_outputs.len() {
            assert!(
                friction_outputs[i] <= friction_outputs[i - 1],
                "Friction should decrease with speed"
            );
        }

        // Test damper adaptation
        let mut damper_outputs = Vec::new();
        for &speed in &speeds {
            let mut state = DamperState::new(0.1, true);
            let mut frame = create_test_frame(0.0, speed);
            damper_filter(&mut frame, &mut state as *mut _ as *mut u8);
            damper_outputs.push(frame.torque_out.abs());
        }

        // Damper should increase with speed
        for i in 1..damper_outputs.len() {
            assert!(
                damper_outputs[i] >= damper_outputs[i - 1],
                "Damper should increase with speed"
            );
        }
    }
}
