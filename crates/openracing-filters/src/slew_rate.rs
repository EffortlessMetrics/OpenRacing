//! Slew Rate Limiter
//!
//! This module provides a slew rate limiter that restricts the rate of change
//! of the output signal, preventing sudden torque spikes.

use crate::Frame;

/// State for slew rate limiter.
///
/// This filter limits the maximum rate of change of the output signal.
/// It's useful for preventing sudden torque changes that could be
/// jarring or damaging to hardware.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SlewRateState {
    /// Maximum change per tick (slew_rate / sample_rate)
    pub max_change_per_tick: f32,
    /// Previous output value
    pub prev_output: f32,
}

impl SlewRateState {
    /// Create a new slew rate limiter state.
    ///
    /// # Arguments
    ///
    /// * `slew_rate` - Maximum rate of change per second (e.g., 0.5 = 50% per second)
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::SlewRateState;
    ///
    /// let state = SlewRateState::new(0.5);
    /// assert!(state.max_change_per_tick > 0.0);
    /// ```
    pub fn new(slew_rate: f32) -> Self {
        Self {
            max_change_per_tick: slew_rate / 1000.0, // Per 1ms tick at 1kHz
            prev_output: 0.0,
        }
    }

    /// Create a slew rate limiter with a per-tick limit directly.
    ///
    /// # Arguments
    ///
    /// * `max_change` - Maximum change per sample tick
    pub fn per_tick(max_change: f32) -> Self {
        Self {
            max_change_per_tick: max_change,
            prev_output: 0.0,
        }
    }

    /// Create a slow slew rate limiter (conservative).
    pub fn slow() -> Self {
        Self::new(0.2)
    }

    /// Create a medium slew rate limiter.
    pub fn medium() -> Self {
        Self::new(0.5)
    }

    /// Create a fast slew rate limiter (aggressive).
    pub fn fast() -> Self {
        Self::new(1.0)
    }

    /// Create an unlimited slew rate (bypass).
    pub fn unlimited() -> Self {
        Self {
            max_change_per_tick: f32::MAX,
            prev_output: 0.0,
        }
    }
}

impl Default for SlewRateState {
    fn default() -> Self {
        Self::medium()
    }
}

/// Slew rate limiter - limits rate of change.
///
/// This filter restricts the rate at which the output can change,
/// providing a smooth transition between torque values.
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
/// let mut state = SlewRateState::new(0.5);
/// let mut frame = Frame::default();
/// frame.torque_out = 1.0;
///
/// slew_rate_filter(&mut frame, &mut state);
/// assert!(frame.torque_out < 1.0); // Limited by slew rate
/// ```
#[inline]
pub fn slew_rate_filter(frame: &mut Frame, state: &mut SlewRateState) {
    let desired_output = frame.torque_out;
    let max_change = state.max_change_per_tick;
    let change = desired_output - state.prev_output;

    let limited_change = change.clamp(-max_change, max_change);
    let limited_output = state.prev_output + limited_change;

    frame.torque_out = limited_output;
    state.prev_output = limited_output;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slew_rate_filter_step_response() {
        let mut state = SlewRateState::new(0.5);
        let mut frame = Frame::from_torque(1.0);

        slew_rate_filter(&mut frame, &mut state);

        // Output should be limited by slew rate (0.5/1000 = 0.0005 per tick)
        assert!(frame.torque_out < 1.0);
        assert!(frame.torque_out > 0.0);
        assert!((frame.torque_out - 0.0005).abs() < 0.0001);
    }

    #[test]
    fn test_slew_rate_filter_convergence() {
        let mut state = SlewRateState::new(0.5);

        for _ in 0..1000 {
            let mut frame = Frame::from_torque(1.0);
            slew_rate_filter(&mut frame, &mut state);
        }

        // After 1000 ticks, should approach target (1000 * 0.0005 = 0.5)
        assert!(state.prev_output > 0.4);
    }

    #[test]
    fn test_slew_rate_filter_negative() {
        let mut state = SlewRateState::new(0.5);
        let mut frame = Frame::from_torque(-1.0);

        slew_rate_filter(&mut frame, &mut state);

        // Should limit negative slew rate
        assert!(frame.torque_out > -1.0);
        assert!(frame.torque_out < 0.0);
    }

    #[test]
    fn test_slew_rate_filter_no_change_needed() {
        let mut state = SlewRateState::new(0.5);
        state.prev_output = 0.5;

        let mut frame = Frame::from_torque(0.5);
        slew_rate_filter(&mut frame, &mut state);

        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_slew_rate_filter_unlimited() {
        let mut state = SlewRateState::unlimited();
        let mut frame = Frame::from_torque(1.0);

        slew_rate_filter(&mut frame, &mut state);

        assert!((frame.torque_out - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_slew_rate_filter_direction_change() {
        let mut state = SlewRateState::new(0.5);
        state.prev_output = 0.5;

        // Step down
        let mut frame = Frame::from_torque(-0.5);
        slew_rate_filter(&mut frame, &mut state);

        // Should be limited in the negative direction
        assert!(frame.torque_out < 0.5);
        assert!(frame.torque_out > -0.5);
    }

    #[test]
    fn test_slew_rate_filter_stability() {
        let mut state = SlewRateState::new(0.5);

        for i in 0..1000 {
            let target = ((i as f32) * 0.01).sin();
            let mut frame = Frame::from_torque(target);
            slew_rate_filter(&mut frame, &mut state);

            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0 + 0.001);
        }
    }

    #[test]
    fn test_slew_rate_filter_symmetry() {
        let mut state_pos = SlewRateState::new(0.5);
        let mut state_neg = SlewRateState::new(0.5);

        let mut frame_pos = Frame::from_torque(1.0);
        let mut frame_neg = Frame::from_torque(-1.0);

        slew_rate_filter(&mut frame_pos, &mut state_pos);
        slew_rate_filter(&mut frame_neg, &mut state_neg);

        // Magnitude should be the same (symmetric)
        assert!((frame_pos.torque_out.abs() - frame_neg.torque_out.abs()).abs() < 0.0001);
    }

    #[test]
    fn test_slew_rate_filter_zero_rate() {
        let mut state = SlewRateState::new(0.0);
        state.prev_output = 0.5;

        let mut frame = Frame::from_torque(1.0);
        slew_rate_filter(&mut frame, &mut state);

        // Zero rate means output should not change
        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }
}
