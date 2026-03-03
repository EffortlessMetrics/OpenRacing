//! Inertia Filter
//!
//! This module provides an inertia filter that simulates rotational inertia
//! by generating torque in response to angular acceleration.

use crate::Frame;

/// State for inertia filter.
///
/// This filter simulates rotational inertia by generating torque that
/// opposes angular acceleration. The coefficient determines the magnitude
/// of the inertia effect.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct InertiaState {
    /// Inertia coefficient (typical values 0.0 to 1.0)
    pub coefficient: f32,
    /// Previous wheel speed for acceleration calculation
    pub prev_wheel_speed: f32,
}

impl InertiaState {
    /// Create a new inertia filter state.
    ///
    /// # Arguments
    ///
    /// * `coefficient` - Inertia coefficient (typical values 0.0 to 1.0)
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::InertiaState;
    ///
    /// let state = InertiaState::new(0.1);
    /// assert_eq!(state.coefficient, 0.1);
    /// ```
    pub fn new(coefficient: f32) -> Self {
        Self {
            coefficient,
            prev_wheel_speed: 0.0,
        }
    }

    /// Create an inertia filter with light effect.
    pub fn light() -> Self {
        Self::new(0.05)
    }

    /// Create an inertia filter with medium effect.
    pub fn medium() -> Self {
        Self::new(0.1)
    }

    /// Create an inertia filter with heavy effect.
    pub fn heavy() -> Self {
        Self::new(0.2)
    }
}

impl Default for InertiaState {
    fn default() -> Self {
        Self::new(0.1)
    }
}

/// Inertia filter - simulates rotational inertia.
///
/// This filter generates torque that opposes angular acceleration,
/// simulating the effect of rotational inertia. The torque is proportional
/// to the rate of change of wheel speed.
///
/// # RT Safety
///
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
/// - No syscalls or I/O
///
/// # Arguments
///
/// * `frame` - The frame to process (modified in place)
/// * `state` - The filter state (updated with current wheel speed)
///
/// # Example
///
/// ```
/// use openracing_filters::prelude::*;
///
/// let mut state = InertiaState::new(0.1);
/// let mut frame = Frame::default();
/// frame.wheel_speed = 0.0;
/// frame.torque_out = 0.0;
///
/// inertia_filter(&mut frame, &mut state);
///
/// // Now accelerate
/// frame.wheel_speed = 5.0;
/// inertia_filter(&mut frame, &mut state);
/// assert!(frame.torque_out < 0.0); // Inertia opposes acceleration
/// ```
#[inline]
pub fn inertia_filter(frame: &mut Frame, state: &mut InertiaState) {
    // Calculate acceleration (change in wheel speed)
    let acceleration = frame.wheel_speed - state.prev_wheel_speed;
    let inertia_torque = -acceleration * state.coefficient;

    frame.torque_out += inertia_torque;
    state.prev_wheel_speed = frame.wheel_speed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inertia_filter_acceleration() {
        let mut state = InertiaState::new(0.1);

        let mut frame1 = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut frame1, &mut state);
        let initial_torque = frame1.torque_out;

        // Sudden acceleration
        let mut frame2 = Frame::from_ffb(0.0, 5.0);
        inertia_filter(&mut frame2, &mut state);

        // Should produce opposing torque due to inertia
        assert!(frame2.torque_out < initial_torque);
    }

    #[test]
    fn test_inertia_filter_constant_speed() {
        let mut state = InertiaState::new(0.1);

        // First tick sets prev_wheel_speed
        let mut frame1 = Frame::from_ffb(0.0, 5.0);
        inertia_filter(&mut frame1, &mut state);

        // Second tick at same speed should produce no inertia torque
        let mut frame2 = Frame::from_ffb(0.0, 5.0);
        frame2.torque_out = 0.0;
        inertia_filter(&mut frame2, &mut state);

        assert!((frame2.torque_out).abs() < 0.001);
    }

    #[test]
    fn test_inertia_filter_deceleration() {
        let mut state = InertiaState::new(0.1);

        // Start at high speed
        let mut frame1 = Frame::from_ffb(0.0, 10.0);
        inertia_filter(&mut frame1, &mut state);

        // Decelerate
        let mut frame2 = Frame::from_ffb(0.0, 5.0);
        inertia_filter(&mut frame2, &mut state);

        // Inertia should oppose deceleration (positive torque)
        assert!(frame2.torque_out > 0.0);
    }

    #[test]
    fn test_inertia_filter_opposes_acceleration() {
        let mut state = InertiaState::new(0.1);

        // Positive acceleration
        let mut frame_pos = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut frame_pos, &mut state);
        frame_pos.wheel_speed = 5.0;
        frame_pos.torque_out = 0.0;
        inertia_filter(&mut frame_pos, &mut state);
        assert!(frame_pos.torque_out < 0.0); // Opposes acceleration

        // Negative acceleration
        let mut state_neg = InertiaState::new(0.1);
        let mut frame_neg = Frame::from_ffb(0.0, 5.0);
        inertia_filter(&mut frame_neg, &mut state_neg);
        frame_neg.wheel_speed = 0.0;
        frame_neg.torque_out = 0.0;
        inertia_filter(&mut frame_neg, &mut state_neg);
        assert!(frame_neg.torque_out > 0.0); // Opposes deceleration
    }

    #[test]
    fn test_inertia_filter_proportional() {
        let state_high = InertiaState::new(0.2);
        let state_low = InertiaState::new(0.1);

        let mut frame_high = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut frame_high, &mut state_high.clone());
        frame_high.wheel_speed = 5.0;
        frame_high.torque_out = 0.0;
        let mut state_high_mut = state_high;
        inertia_filter(&mut frame_high, &mut state_high_mut);

        let mut frame_low = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut frame_low, &mut state_low.clone());
        frame_low.wheel_speed = 5.0;
        frame_low.torque_out = 0.0;
        let mut state_low_mut = state_low;
        inertia_filter(&mut frame_low, &mut state_low_mut);

        // Higher coefficient should produce more torque
        assert!(frame_high.torque_out.abs() > frame_low.torque_out.abs());
    }

    #[test]
    fn test_inertia_filter_stability() {
        let mut state = InertiaState::new(0.1);

        for i in 0..1000 {
            let speed = ((i as f32) * 0.01).sin() * 5.0;
            let mut frame = Frame::from_ffb(0.5, speed);
            inertia_filter(&mut frame, &mut state);

            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn test_inertia_filter_extreme_acceleration() {
        let mut state = InertiaState::new(0.1);

        let mut frame = Frame::from_ffb(0.0, 0.0);
        inertia_filter(&mut frame, &mut state);
        frame.wheel_speed = 1000.0;
        frame.torque_out = 0.0;
        inertia_filter(&mut frame, &mut state);

        assert!(frame.torque_out.is_finite());
    }
}
