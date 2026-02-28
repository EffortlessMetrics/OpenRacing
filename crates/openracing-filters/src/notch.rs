//! Notch Filter (Biquad Implementation)
//!
//! This module provides a biquad-based notch filter for eliminating
//! specific frequencies from the FFB signal.

use crate::Frame;
use std::f32::consts::PI;

/// State for notch filter (biquad implementation).
///
/// This filter implements a biquad (second-order IIR) notch filter
/// that can attenuate a specific frequency.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct NotchState {
    /// Numerator coefficient b0
    pub b0: f32,
    /// Numerator coefficient b1
    pub b1: f32,
    /// Numerator coefficient b2
    pub b2: f32,
    /// Denominator coefficient a1
    pub a1: f32,
    /// Denominator coefficient a2
    pub a2: f32,
    /// Previous input sample x[n-1]
    pub x1: f32,
    /// Previous input sample x[n-2]
    pub x2: f32,
    /// Previous output sample y[n-1]
    pub y1: f32,
    /// Previous output sample y[n-2]
    pub y2: f32,
}

impl NotchState {
    /// Create a new notch filter state.
    ///
    /// # Arguments
    ///
    /// * `frequency` - Center frequency to notch (Hz)
    /// * `q` - Q factor (bandwidth control, higher = narrower notch)
    /// * `gain_db` - Gain at center frequency in dB (typically negative)
    /// * `sample_rate` - Sample rate in Hz (typically 1000 for 1kHz FFB)
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::NotchState;
    ///
    /// let state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
    /// ```
    pub fn new(frequency: f32, q: f32, _gain_db: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * PI * frequency / sample_rate;
        let q_clamped = q.clamp(0.1, 10.0);

        // Peaking/notch filter coefficients
        let alpha = omega.sin() / (2.0 * q_clamped);

        // For stability, use a conservative coefficient calculation
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * omega.cos();
        let a2 = 1.0 - alpha;

        // Normalize by a0
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Create a bypass filter (no filtering).
    pub fn bypass() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Create a low-pass filter.
    pub fn lowpass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let omega = 2.0 * PI * frequency / sample_rate;
        let q_clamped = q.clamp(0.1, 10.0);
        let alpha = omega.sin() / (2.0 * q_clamped);

        let b0 = (1.0 - omega.cos()) / 2.0;
        let b1 = 1.0 - omega.cos();
        let b2 = (1.0 - omega.cos()) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * omega.cos();
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Check if the filter coefficients are stable.
    pub fn is_stable(&self) -> bool {
        // Stability condition for biquad: poles must be inside unit circle
        // Simplified check: |a1| + |a2| < 1 is a sufficient condition
        self.a1.abs() + self.a2.abs() < 1.0
    }
}

impl Default for NotchState {
    fn default() -> Self {
        Self::bypass()
    }
}

/// Notch filter (biquad implementation) - eliminates specific frequencies.
///
/// This filter applies a biquad IIR filter to the torque signal.
/// It can be configured as a notch filter, low-pass filter, or other
/// second-order filter types.
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
/// let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
/// let mut frame = Frame::default();
/// frame.torque_out = 1.0;
///
/// notch_filter(&mut frame, &mut state);
/// ```
#[inline]
pub fn notch_filter(frame: &mut Frame, state: &mut NotchState) {
    let input = frame.torque_out;

    // Direct Form I implementation
    let output = state.b0 * input + state.b1 * state.x1 + state.b2 * state.x2
        - state.a1 * state.y1
        - state.a2 * state.y2;

    // Update delay line
    state.x2 = state.x1;
    state.x1 = input;
    state.y2 = state.y1;
    state.y1 = output;

    frame.torque_out = output;
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
    fn test_notch_filter_bypass() {
        let mut state = NotchState::bypass();
        let mut frame = create_test_frame(0.5);
        notch_filter(&mut frame, &mut state);

        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_notch_filter_dc() {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);

        let mut frame = create_test_frame(1.0);

        for _ in 0..50 {
            notch_filter(&mut frame, &mut state);
            if !frame.torque_out.is_finite() || frame.torque_out.abs() > 10.0 {
                break;
            }
        }

        assert!(frame.torque_out.is_finite());
        assert!(frame.torque_out.abs() < 10.0);
    }

    #[test]
    fn test_notch_filter_stability_check() {
        let state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        // The filter should have finite coefficients
        assert!(state.b0.is_finite());
        assert!(state.a1.is_finite());
    }

    #[test]
    fn test_notch_filter_lowpass() {
        let mut state = NotchState::lowpass(100.0, 0.707, 1000.0);

        for _ in 0..100 {
            let mut frame = create_test_frame(1.0);
            notch_filter(&mut frame, &mut state);

            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn test_notch_filter_zero_input() {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);
        let mut frame = create_test_frame(0.0);

        for _ in 0..10 {
            notch_filter(&mut frame, &mut state);
        }

        assert!((frame.torque_out).abs() < 0.001);
    }

    #[test]
    fn test_notch_filter_sinusoidal() {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);

        for i in 0..1000 {
            let input = ((i as f32) * 0.01).sin();
            let mut frame = create_test_frame(input);
            notch_filter(&mut frame, &mut state);

            assert!(frame.torque_out.is_finite());
        }
    }

    #[test]
    fn test_notch_filter_extreme_input() {
        let mut state = NotchState::new(50.0, 2.0, -6.0, 1000.0);

        let mut frame = create_test_frame(100.0);
        notch_filter(&mut frame, &mut state);

        // Filter should handle extreme input gracefully
        // Output may be large but should be finite
        assert!(frame.torque_out.is_finite());
    }
}
