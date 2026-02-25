//! Curve Mapping Filter
//!
//! This module provides a lookup table-based curve mapping filter
//! for applying custom force curves to the FFB signal.

use crate::Frame;

/// State for curve mapping (lookup table).
///
/// This filter applies a user-defined curve to the torque signal using
/// a pre-computed lookup table for RT-safe evaluation.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations in filter function
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CurveState {
    /// Pre-computed lookup table
    pub lut: [f32; 1024],
    /// Size of the lookup table
    pub lut_size: usize,
}

impl CurveState {
    /// Lookup table size
    pub const LUT_SIZE: usize = 1024;

    /// Create a new curve state from control points.
    ///
    /// # Arguments
    ///
    /// * `curve_points` - Slice of (input, output) control points, must be sorted by input
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::CurveState;
    ///
    /// let points = [(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)];
    /// let state = CurveState::new(&points);
    /// ```
    pub fn new(curve_points: &[(f32, f32)]) -> Self {
        let mut lut = [0.0f32; Self::LUT_SIZE];

        #[allow(clippy::needless_range_loop)]
        for i in 0..Self::LUT_SIZE {
            let input = i as f32 / (Self::LUT_SIZE - 1) as f32;
            lut[i] = Self::interpolate_curve(input, curve_points);
        }

        Self {
            lut,
            lut_size: Self::LUT_SIZE,
        }
    }

    /// Create a linear (identity) curve.
    pub fn linear() -> Self {
        let mut lut = [0.0f32; Self::LUT_SIZE];

        #[allow(clippy::needless_range_loop)]
        for i in 0..Self::LUT_SIZE {
            lut[i] = i as f32 / (Self::LUT_SIZE - 1) as f32;
        }

        Self {
            lut,
            lut_size: Self::LUT_SIZE,
        }
    }

    /// Create a quadratic curve (softer near center).
    pub fn quadratic() -> Self {
        let points = [(0.0f32, 0.0f32), (0.5f32, 0.25f32), (1.0f32, 1.0f32)];
        Self::new(&points)
    }

    /// Create a cubic curve (even softer near center).
    pub fn cubic() -> Self {
        let points = [
            (0.0f32, 0.0f32),
            (0.33f32, 0.037f32),
            (0.67f32, 0.296f32),
            (1.0f32, 1.0f32),
        ];
        Self::new(&points)
    }

    /// Create an S-curve (soft knee).
    pub fn scurve() -> Self {
        let points = [
            (0.0f32, 0.0f32),
            (0.25f32, 0.1f32),
            (0.5f32, 0.5f32),
            (0.75f32, 0.9f32),
            (1.0f32, 1.0f32),
        ];
        Self::new(&points)
    }

    /// Interpolate the curve at a given input value.
    fn interpolate_curve(input: f32, curve_points: &[(f32, f32)]) -> f32 {
        let clamped_input = input.clamp(0.0, 1.0);

        // Handle edge cases
        if curve_points.is_empty() {
            return clamped_input;
        }
        if curve_points.len() == 1 {
            return curve_points[0].1;
        }

        // Find the two points to interpolate between
        for window in curve_points.windows(2) {
            if clamped_input >= window[0].0 && clamped_input <= window[1].0 {
                let t = if (window[1].0 - window[0].0).abs() > 1e-6 {
                    (clamped_input - window[0].0) / (window[1].0 - window[0].0)
                } else {
                    0.5
                };
                return window[0].1 + t * (window[1].1 - window[0].1);
            }
        }

        // Fallback (extrapolation or edge case)
        if clamped_input <= curve_points[0].0 {
            return curve_points[0].1;
        }
        curve_points[curve_points.len() - 1].1
    }

    /// Lookup a value in the curve with linear interpolation.
    ///
    /// # RT Safety
    ///
    /// - No heap allocations
    /// - O(1) time complexity
    /// - Bounded execution time
    #[inline]
    pub fn lookup(&self, input: f32) -> f32 {
        let clamped = input.clamp(0.0, 1.0);
        let scaled = clamped * (self.lut_size - 1) as f32;
        let index_low = (scaled as usize).min(self.lut_size - 2);
        let index_high = index_low + 1;
        let fraction = scaled - index_low as f32;

        let low = self.lut[index_low];
        let high = self.lut[index_high];

        low + fraction * (high - low)
    }
}

impl Default for CurveState {
    fn default() -> Self {
        Self::linear()
    }
}

/// Curve mapping filter using lookup table - applies force curve.
///
/// This filter applies a pre-computed curve transformation to the torque
/// output. The curve is applied to the absolute value of the torque,
/// preserving the sign.
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
/// let points = [(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)];
/// let state = CurveState::new(&points);
/// let mut frame = Frame::default();
/// frame.torque_out = 0.5;
///
/// curve_filter(&mut frame, &state);
/// assert!((frame.torque_out - 0.25).abs() < 0.1);
/// ```
#[inline]
pub fn curve_filter(frame: &mut Frame, state: &CurveState) {
    let input = frame.torque_out.abs().clamp(0.0, 1.0);
    let index = (input * (state.lut_size - 1) as f32) as usize;
    let index = index.min(state.lut_size - 1);

    let mapped_output = state.lut[index];
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
    fn test_curve_state_linear() {
        let state = CurveState::linear();

        assert!((state.lookup(0.0) - 0.0).abs() < 0.01);
        assert!((state.lookup(0.5) - 0.5).abs() < 0.01);
        assert!((state.lookup(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_state_quadratic() {
        let state = CurveState::quadratic();

        // Quadratic curve: 0.5 input should give ~0.25 output
        assert!((state.lookup(0.5) - 0.25).abs() < 0.1);
    }

    #[test]
    fn test_curve_state_clamping() {
        let state = CurveState::linear();

        // Out of range inputs should be clamped
        assert!((state.lookup(-0.5) - 0.0).abs() < 0.01);
        assert!((state.lookup(1.5) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_curve_filter_midpoint() {
        let points = [(0.0f32, 0.0f32), (0.5f32, 0.25f32), (1.0f32, 1.0f32)];
        let state = CurveState::new(&points);

        let mut frame = create_test_frame(0.5);
        curve_filter(&mut frame, &state);

        // Should map 0.5 to approximately 0.25 (quadratic curve)
        assert!((frame.torque_out - 0.25).abs() < 0.1);
    }

    #[test]
    fn test_curve_filter_preserves_sign() {
        let state = CurveState::linear();

        let mut frame_pos = create_test_frame(0.5);
        curve_filter(&mut frame_pos, &state);
        assert!(frame_pos.torque_out > 0.0);

        let mut frame_neg = create_test_frame(-0.5);
        curve_filter(&mut frame_neg, &state);
        assert!(frame_neg.torque_out < 0.0);
    }

    #[test]
    fn test_curve_filter_endpoints() {
        let state = CurveState::linear();

        let mut frame_0 = create_test_frame(0.0);
        curve_filter(&mut frame_0, &state);
        assert!((frame_0.torque_out).abs() < 0.01);

        let mut frame_1 = create_test_frame(1.0);
        curve_filter(&mut frame_1, &state);
        assert!((frame_1.torque_out - 1.0).abs() < 0.01);

        let mut frame_neg1 = create_test_frame(-1.0);
        curve_filter(&mut frame_neg1, &state);
        assert!((frame_neg1.torque_out - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_curve_filter_stability() {
        let state = CurveState::scurve();

        for i in 0..1000 {
            let input = ((i as f32) * 0.001 - 0.5) * 2.0; // -1 to 1
            let mut frame = create_test_frame(input);
            curve_filter(&mut frame, &state);

            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0);
        }
    }

    #[test]
    fn test_curve_filter_extreme_input() {
        let state = CurveState::linear();

        let mut frame_high = create_test_frame(100.0);
        curve_filter(&mut frame_high, &state);
        assert!(frame_high.torque_out.is_finite());

        let mut frame_low = create_test_frame(-100.0);
        curve_filter(&mut frame_low, &state);
        assert!(frame_low.torque_out.is_finite());
    }

    #[test]
    fn test_curve_state_empty_points() {
        let state = CurveState::new(&[]);

        // Should return clamped input for empty curve
        assert!((state.lookup(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_curve_state_single_point() {
        let state = CurveState::new(&[(0.5, 0.75)]);

        // Should return the single point's output
        assert!((state.lookup(0.5) - 0.75).abs() < 0.01);
    }
}
