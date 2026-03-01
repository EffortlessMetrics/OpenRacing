//! Hands-Off Detection Filter
//!
//! This module provides a hands-off detector that identifies when the user
//! is not holding the wheel, based on torque patterns.

use crate::Frame;

/// State for hands-off detector.
///
/// This filter detects when the user is not holding the wheel by monitoring
/// torque patterns. When low torque is sustained for a configurable timeout,
/// the hands-off flag is set.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct HandsOffState {
    /// Whether hands-off detection is enabled
    pub enabled: bool,
    /// Torque threshold for detecting hands on wheel
    pub threshold: f32,
    /// Number of ticks at 1kHz before hands-off is detected
    pub timeout_ticks: u32,
    /// Counter for low-torque ticks
    pub counter: u32,
    /// Last torque value for change detection
    pub last_torque: f32,
}

impl HandsOffState {
    /// Create a new hands-off detector state.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether hands-off detection is active
    /// * `threshold` - Torque threshold for detecting hands on wheel
    /// * `timeout_seconds` - Seconds of low torque before hands-off is detected
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::HandsOffState;
    ///
    /// let state = HandsOffState::new(true, 0.05, 2.0);
    /// assert!(state.enabled);
    /// ```
    pub fn new(enabled: bool, threshold: f32, timeout_seconds: f32) -> Self {
        Self {
            enabled,
            threshold,
            timeout_ticks: (timeout_seconds * 1000.0) as u32, // Convert to ticks at 1kHz
            counter: 0,
            last_torque: 0.0,
        }
    }

    /// Create a disabled hands-off detector.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            threshold: 0.0,
            timeout_ticks: 0,
            counter: 0,
            last_torque: 0.0,
        }
    }

    /// Create a hands-off detector with default settings (2 second timeout).
    pub fn default_detector() -> Self {
        Self::new(true, 0.05, 2.0)
    }

    /// Create a hands-off detector with a short timeout (0.5 seconds).
    pub fn short_timeout() -> Self {
        Self::new(true, 0.05, 0.5)
    }

    /// Create a hands-off detector with a long timeout (5 seconds).
    pub fn long_timeout() -> Self {
        Self::new(true, 0.05, 5.0)
    }

    /// Reset the hands-off counter.
    pub fn reset(&mut self) {
        self.counter = 0;
        self.last_torque = 0.0;
    }
}

impl Default for HandsOffState {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Hands-off detector - detects when user is not holding the wheel.
///
/// This filter monitors torque patterns to detect when the user is not
/// holding the wheel. When low torque is sustained for the configured
/// timeout period, the hands_off flag is set in the frame.
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
/// * `state` - The filter state (updated with detection state)
///
/// # Example
///
/// ```
/// use openracing_filters::prelude::*;
///
/// let mut state = HandsOffState::new(true, 0.05, 2.0);
/// let mut frame = Frame::default();
/// frame.torque_out = 0.01; // Low torque
///
/// for _ in 0..2500 {
///     hands_off_detector(&mut frame, &mut state);
/// }
///
/// assert!(frame.hands_off); // Detected after timeout
/// ```
#[inline]
pub fn hands_off_detector(frame: &mut Frame, state: &mut HandsOffState) {
    if !state.enabled {
        frame.hands_off = false;
        return;
    }

    // Check if there's significant torque resistance (indicating hands on wheel)
    // Use both absolute torque and torque change as indicators
    let torque_change = (frame.torque_out - state.last_torque).abs();
    let absolute_torque = frame.torque_out.abs();
    let has_resistance = torque_change > state.threshold || absolute_torque > state.threshold;

    if has_resistance {
        // Reset counter - hands are on wheel
        state.counter = 0;
        frame.hands_off = false;
    } else {
        // Increment counter - no resistance detected
        state.counter = state.counter.saturating_add(1);
        // Only set hands_off if we've exceeded the timeout
        frame.hands_off = state.counter >= state.timeout_ticks;
    }

    state.last_torque = frame.torque_out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hands_off_disabled() {
        let mut state = HandsOffState::disabled();
        let mut frame = Frame::from_torque(0.0);

        for _ in 0..3000 {
            hands_off_detector(&mut frame, &mut state);
        }

        assert!(!frame.hands_off);
    }

    #[test]
    fn test_hands_off_with_resistance() {
        let mut state = HandsOffState::new(true, 0.05, 2.0);

        let mut frame = Frame::from_torque(0.1); // Significant torque
        hands_off_detector(&mut frame, &mut state);

        assert!(!frame.hands_off);
    }

    #[test]
    fn test_hands_off_no_resistance() {
        let mut state = HandsOffState::new(true, 0.05, 0.1); // 100ms timeout

        for _ in 0..200 {
            // 200ms at 1kHz
            let mut frame = Frame::from_torque(0.01); // Low torque
            hands_off_detector(&mut frame, &mut state);

            if frame.hands_off {
                break;
            }
        }

        let mut final_frame = Frame::from_torque(0.01);
        hands_off_detector(&mut final_frame, &mut state);
        assert!(final_frame.hands_off);
    }

    #[test]
    fn test_hands_off_timeout_exact() {
        let timeout_seconds = 0.5;
        let mut state = HandsOffState::new(true, 0.05, timeout_seconds);
        let expected_ticks = (timeout_seconds * 1000.0) as u32;

        // Run for just under the timeout
        for _ in 0..(expected_ticks - 1) {
            let mut frame = Frame::from_torque(0.01);
            hands_off_detector(&mut frame, &mut state);
            assert!(!frame.hands_off);
        }

        // One more tick should trigger hands-off
        let mut frame = Frame::from_torque(0.01);
        hands_off_detector(&mut frame, &mut state);
        assert!(frame.hands_off);
    }

    #[test]
    fn test_hands_off_resets_on_resistance() {
        let mut state = HandsOffState::new(true, 0.05, 0.1);

        // Build up counter
        for _ in 0..50 {
            let mut frame = Frame::from_torque(0.01);
            hands_off_detector(&mut frame, &mut state);
        }

        assert!(state.counter > 0);

        // Apply resistance
        let mut frame = Frame::from_torque(0.1);
        hands_off_detector(&mut frame, &mut state);

        assert_eq!(state.counter, 0);
        assert!(!frame.hands_off);
    }

    #[test]
    fn test_hands_off_torque_change_detection() {
        let mut state = HandsOffState::new(true, 0.05, 0.1);

        // Even low absolute torque with significant change should reset
        state.last_torque = 0.0;
        let mut frame = Frame::from_torque(0.1); // Change of 0.1 > threshold
        hands_off_detector(&mut frame, &mut state);

        assert_eq!(state.counter, 0);
    }

    #[test]
    fn test_hands_off_stability() {
        let mut state = HandsOffState::new(true, 0.05, 1.0);

        for i in 0..2000 {
            let torque = if i % 100 == 0 { 0.1 } else { 0.01 };
            let mut frame = Frame::from_torque(torque);
            hands_off_detector(&mut frame, &mut state);

            // Should not crash or overflow
            assert!(state.counter < u32::MAX);
        }
    }

    #[test]
    fn test_hands_off_reset() {
        let mut state = HandsOffState::new(true, 0.05, 1.0);

        // Build up counter
        for _ in 0..500 {
            let mut frame = Frame::from_torque(0.01);
            hands_off_detector(&mut frame, &mut state);
        }

        assert!(state.counter > 0);

        // Reset
        state.reset();

        assert_eq!(state.counter, 0);
        assert!((state.last_torque).abs() < 0.001);
    }
}
