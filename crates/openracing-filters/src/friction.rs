//! Friction Filter (Speed-Adaptive)
//!
//! This module provides a friction filter that simulates tire/road friction
//! with optional speed-adaptive behavior.

use crate::Frame;

/// State for friction filter with speed adaptation.
///
/// This filter simulates Coulomb friction with optional speed-dependent
/// coefficient reduction at higher speeds.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct FrictionState {
    /// Friction coefficient (0.0 to 1.0 typical)
    pub coefficient: f32,
    /// Whether to reduce friction at higher speeds
    pub speed_adaptation: bool,
}

impl FrictionState {
    /// Create a new friction filter state.
    ///
    /// # Arguments
    ///
    /// * `coefficient` - Friction coefficient (typically 0.0 to 1.0)
    /// * `speed_adaptive` - Whether to reduce friction at higher speeds
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::FrictionState;
    ///
    /// let state = FrictionState::new(0.1, true);
    /// assert_eq!(state.coefficient, 0.1);
    /// assert!(state.speed_adaptation);
    /// ```
    pub fn new(coefficient: f32, speed_adaptive: bool) -> Self {
        Self {
            coefficient,
            speed_adaptation: speed_adaptive,
        }
    }

    /// Create a friction filter with fixed coefficient (no speed adaptation).
    pub fn fixed(coefficient: f32) -> Self {
        Self::new(coefficient, false)
    }

    /// Create a friction filter with speed-adaptive coefficient.
    pub fn adaptive(coefficient: f32) -> Self {
        Self::new(coefficient, true)
    }
}

impl Default for FrictionState {
    fn default() -> Self {
        Self::new(0.1, false)
    }
}

/// Friction filter with speed adaptation - simulates tire/road friction.
///
/// This filter applies Coulomb friction that opposes wheel motion.
/// When speed adaptation is enabled, the friction coefficient decreases
/// at higher speeds to simulate reduced grip.
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
/// * `state` - The filter state
///
/// # Example
///
/// ```
/// use openracing_filters::prelude::*;
///
/// let mut state = FrictionState::new(0.1, true);
/// let mut frame = Frame::default();
/// frame.wheel_speed = 1.0;
/// frame.torque_out = 0.0;
///
/// friction_filter(&mut frame, &mut state);
/// assert!(frame.torque_out.abs() > 0.0); // Friction applied
/// ```
#[inline]
pub fn friction_filter(frame: &mut Frame, state: &FrictionState) {
    // Only apply friction if there's wheel movement
    if frame.wheel_speed.abs() < 1e-6 {
        return;
    }

    let friction_coeff = if state.speed_adaptation {
        // Reduce friction at higher speeds (speed-adaptive)
        let speed_factor = 1.0 - (frame.wheel_speed.abs() * 0.1).min(0.8);
        state.coefficient * speed_factor
    } else {
        state.coefficient
    };

    let friction_torque = -frame.wheel_speed.signum() * friction_coeff;
    frame.torque_out += friction_torque;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friction_filter_speed_adaptive() {
        let state = FrictionState::new(0.1, true);

        let mut frame_low = Frame::from_ffb(0.0, 1.0);
        friction_filter(&mut frame_low, &state);
        let friction_low = frame_low.torque_out.abs();

        let mut frame_high = Frame::from_ffb(0.0, 10.0);
        friction_filter(&mut frame_high, &state);
        let friction_high = frame_high.torque_out.abs();

        // Friction should be lower at higher speeds
        assert!(friction_high < friction_low);
    }

    #[test]
    fn test_friction_filter_non_adaptive() {
        let state = FrictionState::new(0.1, false);

        let mut frame_low = Frame::from_ffb(0.0, 1.0);
        friction_filter(&mut frame_low, &state);
        let friction_low = frame_low.torque_out.abs();

        let mut frame_high = Frame::from_ffb(0.0, 10.0);
        friction_filter(&mut frame_high, &state);
        let friction_high = frame_high.torque_out.abs();

        // Friction should be the same (non-adaptive)
        assert!((friction_high - friction_low).abs() < 0.001);
    }

    #[test]
    fn test_friction_filter_zero_speed() {
        let state = FrictionState::new(0.1, true);

        let mut frame = Frame::from_ffb(0.5, 0.0);
        friction_filter(&mut frame, &state);

        // No friction at zero speed
        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_friction_filter_opposes_motion() {
        let state = FrictionState::new(0.1, false);

        let mut frame_pos = Frame::from_ffb(0.0, 1.0);
        friction_filter(&mut frame_pos, &state);
        assert!(frame_pos.torque_out < 0.0); // Opposes positive speed

        let mut frame_neg = Frame::from_ffb(0.0, -1.0);
        friction_filter(&mut frame_neg, &state);
        assert!(frame_neg.torque_out > 0.0); // Opposes negative speed
    }

    #[test]
    fn test_friction_filter_proportional() {
        let state = FrictionState::new(0.2, false);

        let mut frame = Frame::from_ffb(0.0, 1.0);
        friction_filter(&mut frame, &state);

        // Friction should be proportional to coefficient
        assert!((frame.torque_out.abs() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_friction_filter_stability() {
        let state = FrictionState::new(0.1, true);

        for i in 0..1000 {
            let speed = ((i as f32) * 0.01 - 5.0).clamp(-10.0, 10.0);
            let mut frame = Frame::from_ffb(0.5, speed);
            friction_filter(&mut frame, &state);

            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0);
        }
    }

    #[test]
    fn test_friction_filter_extreme_coefficient() {
        let state_high = FrictionState::new(10.0, false);
        let mut frame = Frame::from_ffb(0.0, 1.0);
        friction_filter(&mut frame, &state_high);
        assert!(frame.torque_out.is_finite());
    }
}
