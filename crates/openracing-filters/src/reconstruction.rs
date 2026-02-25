//! Reconstruction Filter (Anti-Aliasing)
//!
//! This module provides a simple exponential moving average filter for smoothing
//! high-frequency content in the FFB signal.

use crate::Frame;

/// State for reconstruction filter (anti-aliasing).
///
/// This filter uses an exponential moving average to smooth the input signal.
/// The smoothing level determines the filter's alpha coefficient.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ReconstructionState {
    /// Smoothing level (0-8, higher = more smoothing)
    pub level: u8,
    /// Previous output value
    pub prev_output: f32,
    /// Smoothing coefficient (0.0 = full smoothing, 1.0 = no smoothing)
    pub alpha: f32,
}

impl ReconstructionState {
    /// Create a new reconstruction filter state with the given smoothing level.
    ///
    /// # Arguments
    ///
    /// * `level` - Smoothing level from 0 (no filtering) to 8 (heavy filtering)
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::ReconstructionState;
    ///
    /// let state = ReconstructionState::new(4);
    /// assert_eq!(state.level, 4);
    /// ```
    pub fn new(level: u8) -> Self {
        let alpha = match level {
            0 => 1.0,
            1 => 0.5,
            2 => 0.3,
            3 => 0.2,
            4 => 0.1,
            5 => 0.05,
            6 => 0.03,
            7 => 0.02,
            8 => 0.01,
            _ => 0.01,
        };

        Self {
            level,
            prev_output: 0.0,
            alpha,
        }
    }

    /// Create a reconstruction filter with no smoothing.
    pub fn bypass() -> Self {
        Self::new(0)
    }

    /// Create a reconstruction filter with light smoothing.
    pub fn light() -> Self {
        Self::new(2)
    }

    /// Create a reconstruction filter with medium smoothing.
    pub fn medium() -> Self {
        Self::new(4)
    }

    /// Create a reconstruction filter with heavy smoothing.
    pub fn heavy() -> Self {
        Self::new(6)
    }
}

impl Default for ReconstructionState {
    fn default() -> Self {
        Self::new(4)
    }
}

/// Reconstruction filter (anti-aliasing) - smooths high-frequency content.
///
/// This filter applies an exponential moving average to smooth the input signal.
/// The amount of smoothing is determined by the state's alpha coefficient.
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
/// let mut state = ReconstructionState::new(4);
/// let mut frame = Frame::default();
/// frame.ffb_in = 1.0;
/// frame.torque_out = 1.0;
///
/// reconstruction_filter(&mut frame, &mut state);
/// assert!(frame.torque_out < 1.0); // Smoothed output
/// ```
#[inline]
pub fn reconstruction_filter(frame: &mut Frame, state: &mut ReconstructionState) {
    let filtered = state.prev_output + state.alpha * (frame.ffb_in - state.prev_output);
    frame.torque_out = filtered;
    state.prev_output = filtered;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(ffb_in: f32) -> Frame {
        Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    #[test]
    fn test_reconstruction_filter_levels() {
        for level in 0..=8 {
            let state = ReconstructionState::new(level);
            assert_eq!(state.level, level);
            assert!(state.alpha > 0.0);
            assert!(state.alpha <= 1.0);
        }
    }

    #[test]
    fn test_reconstruction_filter_bypass() {
        let mut state = ReconstructionState::bypass();
        let mut frame = create_test_frame(0.5);
        reconstruction_filter(&mut frame, &mut state);
        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_reconstruction_filter_step_response() {
        let mut state = ReconstructionState::new(4);
        let mut frame = create_test_frame(1.0);

        reconstruction_filter(&mut frame, &mut state);

        // Output should be filtered (less than input)
        assert!(frame.torque_out < 1.0);
        assert!(frame.torque_out > 0.0);
    }

    #[test]
    fn test_reconstruction_filter_convergence() {
        let mut state = ReconstructionState::new(4);

        for _ in 0..100 {
            let mut frame = create_test_frame(1.0);
            reconstruction_filter(&mut frame, &mut state);
        }

        // Should converge close to input
        assert!((state.prev_output - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_reconstruction_filter_preserves_sign() {
        let mut state_pos = ReconstructionState::new(4);
        let mut frame_pos = create_test_frame(0.5);
        reconstruction_filter(&mut frame_pos, &mut state_pos);
        assert!(frame_pos.torque_out > 0.0);

        let mut state_neg = ReconstructionState::new(4);
        let mut frame_neg = create_test_frame(-0.5);
        reconstruction_filter(&mut frame_neg, &mut state_neg);
        assert!(frame_neg.torque_out < 0.0);
    }

    #[test]
    fn test_reconstruction_filter_stability() {
        let mut state = ReconstructionState::new(4);

        for i in 0..10000 {
            let input = ((i as f32) * 0.001).sin();
            let mut frame = create_test_frame(input);
            reconstruction_filter(&mut frame, &mut state);
            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0);
        }
    }

    #[test]
    fn test_reconstruction_filter_determinism() {
        let mut state1 = ReconstructionState::new(4);
        let mut state2 = ReconstructionState::new(4);

        let inputs = [0.0, 0.5, 1.0, -0.5, -1.0];

        for &input in &inputs {
            let mut frame1 = create_test_frame(input);
            let mut frame2 = create_test_frame(input);

            reconstruction_filter(&mut frame1, &mut state1);
            reconstruction_filter(&mut frame2, &mut state2);

            assert!(
                (frame1.torque_out - frame2.torque_out).abs() < 1e-6,
                "Filter not deterministic for input {}",
                input
            );
        }
    }

    #[test]
    fn test_reconstruction_filter_extreme_values() {
        let mut state = ReconstructionState::new(4);

        for &input in &[
            f32::MIN_POSITIVE,
            f32::MAX,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let mut frame = create_test_frame(input);
            reconstruction_filter(&mut frame, &mut state);

            if input.is_finite() {
                assert!(frame.torque_out.is_finite());
            }
        }
    }
}
