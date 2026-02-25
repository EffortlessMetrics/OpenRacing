//! Bumpstop Filter
//!
//! This module provides a bumpstop filter that simulates physical steering
//! stops at the end of the wheel's rotation range.

use crate::Frame;

/// State for bumpstop model.
///
/// This filter simulates physical steering stops by applying progressive
/// resistance when the wheel approaches the end of its rotation range.
///
/// # RT Safety
///
/// - `#[repr(C)]` for stable ABI
/// - No heap allocations
/// - O(1) time complexity
/// - Bounded execution time
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct BumpstopState {
    /// Whether the bumpstop is enabled
    pub enabled: bool,
    /// Angle in degrees where bumpstop resistance begins
    pub start_angle: f32,
    /// Maximum angle in degrees (hard stop)
    pub max_angle: f32,
    /// Spring stiffness coefficient
    pub stiffness: f32,
    /// Damping coefficient
    pub damping: f32,
    /// Current integrated angle (radians, from wheel speed)
    pub current_angle: f32,
}

impl BumpstopState {
    /// Create a new bumpstop state.
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether the bumpstop is active
    /// * `start_angle` - Angle in degrees where resistance begins
    /// * `max_angle` - Maximum angle in degrees (hard stop)
    /// * `stiffness` - Spring stiffness coefficient
    /// * `damping` - Damping coefficient
    ///
    /// # Example
    ///
    /// ```
    /// use openracing_filters::BumpstopState;
    ///
    /// let state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
    /// assert!(state.enabled);
    /// ```
    pub fn new(
        enabled: bool,
        start_angle: f32,
        max_angle: f32,
        stiffness: f32,
        damping: f32,
    ) -> Self {
        Self {
            enabled,
            start_angle,
            max_angle,
            stiffness,
            damping,
            current_angle: 0.0,
        }
    }

    /// Create a disabled bumpstop (no effect).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            start_angle: 0.0,
            max_angle: 0.0,
            stiffness: 0.0,
            damping: 0.0,
            current_angle: 0.0,
        }
    }

    /// Create a standard bumpstop (900 degree range, soft stops).
    pub fn standard() -> Self {
        Self::new(true, 400.0, 450.0, 0.5, 0.2)
    }

    /// Create a wide bumpstop (1080 degree range).
    pub fn wide() -> Self {
        Self::new(true, 500.0, 540.0, 0.6, 0.25)
    }

    /// Reset the current angle to center.
    pub fn reset(&mut self) {
        self.current_angle = 0.0;
    }
}

impl Default for BumpstopState {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Bumpstop model filter - simulates physical steering stops.
///
/// This filter applies progressive resistance when the wheel approaches
/// the end of its rotation range, simulating physical steering stops.
/// The resistance increases quadratically with penetration depth.
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
/// * `state` - The filter state (updated with current angle)
///
/// # Example
///
/// ```
/// use openracing_filters::prelude::*;
///
/// let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
/// let mut frame = Frame::default();
/// frame.wheel_speed = 1.0;
///
/// bumpstop_filter(&mut frame, &mut state);
/// ```
#[inline]
pub fn bumpstop_filter(frame: &mut Frame, state: &mut BumpstopState) {
    if !state.enabled {
        return;
    }

    // Integrate wheel speed to get current angle (simplified model)
    // wheel_speed is in rad/s, we want degrees
    let delta_angle = frame.wheel_speed.to_degrees() * 0.001; // 1ms integration step
    state.current_angle += delta_angle;

    let abs_angle = state.current_angle.abs();

    if abs_angle > state.start_angle {
        // Calculate how far into the bumpstop we are
        let bumpstop_penetration =
            (abs_angle - state.start_angle) / (state.max_angle - state.start_angle);
        let penetration_clamped = bumpstop_penetration.clamp(0.0, 1.0);

        // Apply progressive spring force (quadratic)
        let spring_force = penetration_clamped * penetration_clamped * state.stiffness;

        // Apply damping based on wheel speed
        let damping_force = frame.wheel_speed.to_degrees() * state.damping * 0.001;

        // Total bumpstop force opposes further rotation
        let bumpstop_torque = -(spring_force + damping_force) * state.current_angle.signum();

        frame.torque_out += bumpstop_torque;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(wheel_speed: f32) -> Frame {
        Frame {
            ffb_in: 0.0,
            torque_out: 0.0,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 0,
        }
    }

    #[test]
    fn test_bumpstop_disabled() {
        let mut state = BumpstopState::disabled();
        let mut frame = create_test_frame(1.0);
        bumpstop_filter(&mut frame, &mut state);

        assert!((frame.torque_out).abs() < 0.001);
    }

    #[test]
    fn test_bumpstop_within_range() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);

        // Slow wheel speed won't reach bumpstop in a few iterations
        for _ in 0..100 {
            let mut frame = create_test_frame(1.0);
            bumpstop_filter(&mut frame, &mut state);
        }

        // Should not have significant torque output yet
        assert!(state.current_angle.abs() < state.start_angle);
    }

    #[test]
    fn test_bumpstop_at_limit() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);

        // Simulate reaching bumpstop by using a large wheel speed
        let mut frame = create_test_frame(500_000.0);
        bumpstop_filter(&mut frame, &mut state);

        // Should have some torque output
        assert!(frame.torque_out.abs() > 0.0);
    }

    #[test]
    fn test_bumpstop_opposes_motion() {
        let mut state_pos = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        state_pos.current_angle = 500.0; // Past start_angle

        let mut frame_pos = create_test_frame(1.0);
        bumpstop_filter(&mut frame_pos, &mut state_pos);
        assert!(frame_pos.torque_out < 0.0); // Opposes positive rotation

        let mut state_neg = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        state_neg.current_angle = -500.0; // Past start_angle (negative side)

        let mut frame_neg = create_test_frame(-1.0);
        bumpstop_filter(&mut frame_neg, &mut state_neg);
        assert!(frame_neg.torque_out > 0.0); // Opposes negative rotation
    }

    #[test]
    fn test_bumpstop_progressive_resistance() {
        let mut state_light = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        state_light.current_angle = 460.0; // Light penetration

        let mut state_heavy = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        state_heavy.current_angle = 520.0; // Heavy penetration

        let mut frame_light = create_test_frame(0.0);
        bumpstop_filter(&mut frame_light, &mut state_light);

        let mut frame_heavy = create_test_frame(0.0);
        bumpstop_filter(&mut frame_heavy, &mut state_heavy);

        // Heavier penetration should produce more torque
        assert!(frame_heavy.torque_out.abs() > frame_light.torque_out.abs());
    }

    #[test]
    fn test_bumpstop_reset() {
        let mut state = BumpstopState::standard();
        state.current_angle = 500.0;

        state.reset();

        assert!((state.current_angle).abs() < 0.001);
    }

    #[test]
    fn test_bumpstop_stability() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);

        for i in 0..1000 {
            let speed = ((i as f32) * 0.01).sin() * 100.0;
            let mut frame = create_test_frame(speed);
            bumpstop_filter(&mut frame, &mut state);

            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 10.0); // Reasonable bound
        }
    }
}
