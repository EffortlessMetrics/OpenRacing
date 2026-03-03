//! Damper Filter (Speed-Adaptive)
//!
//! This module provides a damper filter that applies velocity-proportional
//! resistance with optional speed-adaptive behavior.

use crate::Frame;

/// State for damper filter with speed adaptation.
///
/// This filter applies viscous damping that is proportional to wheel velocity.
/// When speed adaptation is enabled, the damping coefficient increases at
/// higher speeds.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct DamperState {
    /// Damping coefficient (typically 0.0 to 1.0)
    pub coefficient: f32,
    /// Whether to increase damping at higher speeds
    pub speed_adaptation: bool,
}

impl DamperState {
    /// Create a new damper filter state.
    ///
    /// # Arguments
    ///
    /// * `coefficient` - Damping coefficient (typically 0.0 to 1.0)
    /// * `speed_adaptive` - Whether to increase damping at higher speeds
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::DamperState;
    ///
    /// let state = DamperState::new(0.1, true);
    /// assert_eq!(state.coefficient, 0.1);
    /// assert!(state.speed_adaptation);
    /// ```
    pub fn new(coefficient: f32, speed_adaptive: bool) -> Self {
        Self {
            coefficient,
            speed_adaptation: speed_adaptive,
        }
    }

    /// Create a damper filter with fixed coefficient (no speed adaptation).
    pub fn fixed(coefficient: f32) -> Self {
        Self::new(coefficient, false)
    }

    /// Create a damper filter with speed-adaptive coefficient.
    pub fn adaptive(coefficient: f32) -> Self {
        Self::new(coefficient, true)
    }
}

impl Default for DamperState {
    fn default() -> Self {
        Self::new(0.1, false)
    }
}

/// Damper filter with speed adaptation - velocity-proportional resistance.
///
/// This filter applies viscous damping proportional to wheel speed.
/// When speed adaptation is enabled, the damping coefficient increases
/// at higher speeds for enhanced stability.
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
/// let mut state = DamperState::new(0.1, true);
/// let mut frame = Frame::default();
/// frame.wheel_speed = 1.0;
/// frame.torque_out = 0.0;
///
/// damper_filter(&mut frame, &mut state);
/// assert!(frame.torque_out.abs() > 0.0); // Damping applied
/// ```
#[inline]
pub fn damper_filter(frame: &mut Frame, state: &DamperState) {
    let damper_coeff = if state.speed_adaptation {
        // Increase damping at higher speeds (speed-adaptive)
        let speed_factor = 1.0 + (frame.wheel_speed.abs() * 0.2).min(0.5);
        state.coefficient * speed_factor
    } else {
        state.coefficient
    };

    let damper_torque = -frame.wheel_speed * damper_coeff;
    frame.torque_out += damper_torque;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damper_filter_speed_adaptive() {
        let state = DamperState::new(0.1, true);

        let mut frame_low = Frame::from_ffb(0.0, 1.0);
        damper_filter(&mut frame_low, &state);
        let damping_low = frame_low.torque_out.abs();

        let mut frame_high = Frame::from_ffb(0.0, 10.0);
        damper_filter(&mut frame_high, &state);
        let damping_high = frame_high.torque_out.abs();

        // Damping should be higher at higher speeds
        assert!(damping_high > damping_low);
    }

    #[test]
    fn test_damper_filter_non_adaptive() {
        let state = DamperState::new(0.1, false);

        let mut frame_low = Frame::from_ffb(0.0, 1.0);
        damper_filter(&mut frame_low, &state);
        let damping_low = frame_low.torque_out.abs();

        let mut frame_high = Frame::from_ffb(0.0, 10.0);
        damper_filter(&mut frame_high, &state);
        let damping_high = frame_high.torque_out.abs();

        // Damping should be proportional to speed (non-adaptive)
        assert!((damping_high - damping_low * 10.0).abs() < 0.01);
    }

    #[test]
    fn test_damper_filter_zero_speed() {
        let state = DamperState::new(0.1, true);

        let mut frame = Frame::from_ffb(0.5, 0.0);
        damper_filter(&mut frame, &state);

        // No damping at zero speed
        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_damper_filter_opposes_motion() {
        let state = DamperState::new(0.1, false);

        let mut frame_pos = Frame::from_ffb(0.0, 1.0);
        damper_filter(&mut frame_pos, &state);
        assert!(frame_pos.torque_out < 0.0); // Opposes positive speed

        let mut frame_neg = Frame::from_ffb(0.0, -1.0);
        damper_filter(&mut frame_neg, &state);
        assert!(frame_neg.torque_out > 0.0); // Opposes negative speed
    }

    #[test]
    fn test_damper_filter_proportional() {
        let state = DamperState::new(0.2, false);

        let mut frame = Frame::from_ffb(0.0, 1.0);
        damper_filter(&mut frame, &state);

        // Damping should be proportional to coefficient * speed
        assert!((frame.torque_out.abs() - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_damper_filter_stability() {
        let state = DamperState::new(0.1, true);

        for i in 0..1000 {
            let speed = ((i as f32) * 0.01 - 5.0).clamp(-10.0, 10.0);
            let mut frame = Frame::from_ffb(0.5, speed);
            damper_filter(&mut frame, &state);

            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn test_damper_filter_extreme_speed() {
        let state = DamperState::new(0.1, true);

        let mut frame = Frame::from_ffb(0.0, 100.0);
        damper_filter(&mut frame, &state);

        assert!(frame.torque_out.is_finite());
    }
}
