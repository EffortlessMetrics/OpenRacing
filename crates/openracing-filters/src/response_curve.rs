//! Response Curve Filter
//!
//! This module provides a response curve filter that applies a curve
//! transformation to torque outputs using a pre-computed lookup table.

use crate::Frame;
use openracing_curves::CurveLut;

/// State for response curve filter using CurveLut.
///
/// This filter applies a response curve transformation to torque outputs
/// using a pre-computed lookup table for RT-safe evaluation.
/// The LUT is computed at profile load time (not in RT path).
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations in filter function
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Clone, Debug)]
pub struct ResponseCurveState {
    /// Pre-computed lookup table for RT-safe curve evaluation.
    /// Uses 256 entries for good precision with minimal memory.
    lut: [f32; 256],
}

impl ResponseCurveState {
    /// Create a new response curve state from a CurveLut.
    ///
    /// This should be called at profile load time, not in the RT path.
    ///
    /// # Arguments
    ///
    /// * `curve_lut` - The curve lookup table to use
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_curves::CurveLut;
    /// use openracing_filters::ResponseCurveState;
    ///
    /// let curve_lut = CurveLut::linear();
    /// let state = ResponseCurveState::from_lut(&curve_lut);
    /// ```
    pub fn from_lut(curve_lut: &CurveLut) -> Self {
        let mut lut = [0.0f32; 256];

        for (i, entry) in lut.iter_mut().enumerate() {
            let input = i as f32 / 255.0;
            *entry = curve_lut.lookup(input);
        }

        Self { lut }
    }

    /// Create a linear (identity) response curve state.
    pub fn linear() -> Self {
        let mut lut = [0.0f32; 256];

        for (i, entry) in lut.iter_mut().enumerate() {
            *entry = i as f32 / 255.0;
        }

        Self { lut }
    }

    /// Create a soft response curve (reduced sensitivity near center).
    pub fn soft() -> Self {
        let mut lut = [0.0f32; 256];

        for (i, entry) in lut.iter_mut().enumerate() {
            let input = i as f32 / 255.0;
            // Soft curve: x^1.5 normalized
            *entry = input.powf(1.5);
        }

        Self { lut }
    }

    /// Create a hard response curve (increased sensitivity near limits).
    pub fn hard() -> Self {
        let mut lut = [0.0f32; 256];

        for (i, entry) in lut.iter_mut().enumerate() {
            let input = i as f32 / 255.0;
            // Hard curve: x^0.7 normalized
            *entry = input.powf(0.7);
        }

        Self { lut }
    }

    /// Fast lookup with linear interpolation (RT-safe).
    ///
    /// This method is designed for the RT path:
    /// - No heap allocations
    /// - O(1) time complexity
    /// - Bounded execution time
    ///
    /// # Arguments
    ///
    /// * `input` - Input value (will be clamped to [0.0, 1.0])
    #[inline]
    pub fn lookup(&self, input: f32) -> f32 {
        let input = input.clamp(0.0, 1.0);

        let scaled = input * 255.0;
        let index_low = (scaled as usize).min(254);
        let index_high = index_low + 1;
        let fraction = scaled - index_low as f32;

        let low_value = self.lut[index_low];
        let high_value = self.lut[index_high];

        low_value + fraction * (high_value - low_value)
    }
}

impl Default for ResponseCurveState {
    fn default() -> Self {
        Self::linear()
    }
}

impl Copy for ResponseCurveState {}

/// Response curve filter using CurveLut - applies response curve transformation.
///
/// This filter applies a response curve to the torque output using a pre-computed
/// lookup table. The curve transformation is applied to the absolute value of the
/// torque, preserving the sign.
///
/// **Property**: For any profile with a response curve and any torque output,
/// the final torque SHALL equal the curve-transformed value of the raw torque.
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
/// let state = ResponseCurveState::linear();
/// let mut frame = Frame::default();
/// frame.torque_out = 0.5;
///
/// response_curve_filter(&mut frame, &state);
/// assert!((frame.torque_out - 0.5).abs() < 0.02);
/// ```
#[inline]
pub fn response_curve_filter(frame: &mut Frame, state: &ResponseCurveState) {
    let input = frame.torque_out.abs().clamp(0.0, 1.0);

    let mapped_output = state.lookup(input);

    frame.torque_out = frame.torque_out.signum() * mapped_output;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(torque_out: f32) -> Frame {
        Frame {
            ffb_in: torque_out,
            torque_out,
            wheel_speed: 0.0,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    #[test]
    fn test_response_curve_state_linear() {
        let state = ResponseCurveState::linear();

        let tolerance = 0.01;
        assert!((state.lookup(0.0) - 0.0).abs() < tolerance);
        assert!((state.lookup(0.25) - 0.25).abs() < tolerance);
        assert!((state.lookup(0.5) - 0.5).abs() < tolerance);
        assert!((state.lookup(0.75) - 0.75).abs() < tolerance);
        assert!((state.lookup(1.0) - 1.0).abs() < tolerance);
    }

    #[test]
    fn test_response_curve_state_from_lut() {
        let curve_lut = CurveLut::linear();
        let state = ResponseCurveState::from_lut(&curve_lut);

        let tolerance = 0.01;
        assert!((state.lookup(0.0) - 0.0).abs() < tolerance);
        assert!((state.lookup(0.5) - 0.5).abs() < tolerance);
        assert!((state.lookup(1.0) - 1.0).abs() < tolerance);
    }

    #[test]
    fn test_response_curve_state_clamping() {
        let state = ResponseCurveState::linear();

        let below = state.lookup(-0.5);
        let above = state.lookup(1.5);

        assert!((below - 0.0).abs() < 0.01);
        assert!((above - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_response_curve_filter_linear() {
        let state = ResponseCurveState::linear();

        let test_cases = [
            (0.0f32, 0.0f32),
            (0.5, 0.5),
            (1.0, 1.0),
            (-0.5, -0.5),
            (-1.0, -1.0),
        ];

        for (input, expected) in test_cases {
            let mut frame = create_test_frame(input);
            response_curve_filter(&mut frame, &state);

            assert!(
                (frame.torque_out - expected).abs() < 0.02,
                "Linear curve failed: input={}, expected={}, got={}",
                input,
                expected,
                frame.torque_out
            );
        }
    }

    #[test]
    fn test_response_curve_filter_preserves_sign() {
        let state = ResponseCurveState::linear();

        let mut frame_pos = create_test_frame(0.5);
        response_curve_filter(&mut frame_pos, &state);
        assert!(frame_pos.torque_out > 0.0);

        let mut frame_neg = create_test_frame(-0.5);
        response_curve_filter(&mut frame_neg, &state);
        assert!(frame_neg.torque_out < 0.0);

        let mut frame_zero = create_test_frame(0.0);
        response_curve_filter(&mut frame_zero, &state);
        assert!((frame_zero.torque_out - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_response_curve_filter_output_bounded() {
        let state = ResponseCurveState::linear();

        let extreme_inputs = [-2.0, -1.0, 0.0, 1.0, 2.0];

        for input in extreme_inputs {
            let mut frame = create_test_frame(input);
            response_curve_filter(&mut frame, &state);

            assert!(
                frame.torque_out.is_finite(),
                "Output not finite for input {}",
                input
            );
            assert!(
                frame.torque_out.abs() <= 1.0,
                "Output {} out of bounds for input {}",
                frame.torque_out,
                input
            );
        }
    }

    #[test]
    fn test_response_curve_filter_rt_safe() {
        let state = ResponseCurveState::linear();

        for _ in 0..10000 {
            let mut frame = create_test_frame(0.5);
            response_curve_filter(&mut frame, &state);
            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn test_response_curve_state_soft() {
        let state = ResponseCurveState::soft();

        // Soft curve should have lower output at 0.5 input
        let output = state.lookup(0.5);
        assert!(output < 0.5);
        assert!(output > 0.0);
    }

    #[test]
    fn test_response_curve_state_hard() {
        let state = ResponseCurveState::hard();

        // Hard curve should have higher output at 0.5 input
        let output = state.lookup(0.5);
        assert!(output > 0.5);
        assert!(output < 1.0);
    }
}
