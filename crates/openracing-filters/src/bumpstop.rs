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

    /// Kill mutant: replace standard() with Default::default().
    /// Standard must have non-default (non-zero) parameters.
    #[test]
    fn test_bumpstop_standard_differs_from_default() {
        let standard = BumpstopState::standard();
        let default = BumpstopState::default();
        assert!(standard.enabled, "standard must be enabled");
        assert!(!default.enabled, "default must be disabled");
        assert!(standard.start_angle > 0.0, "standard start_angle must be positive");
        assert!(standard.stiffness > 0.0, "standard stiffness must be positive");
        // Verify specific values
        assert!((standard.start_angle - 400.0).abs() < 0.01);
        assert!((standard.max_angle - 450.0).abs() < 0.01);
    }

    /// Kill mutant: replace wide() with Default::default().
    #[test]
    fn test_bumpstop_wide_differs_from_default() {
        let wide = BumpstopState::wide();
        let default = BumpstopState::default();
        assert!(wide.enabled, "wide must be enabled");
        assert!(!default.enabled, "default must be disabled");
        assert!(wide.start_angle > 0.0, "wide start_angle must be positive");
        // Verify specific values
        assert!((wide.start_angle - 500.0).abs() < 0.01);
        assert!((wide.max_angle - 540.0).abs() < 0.01);
    }

    /// Kill mutants in bumpstop_filter arithmetic:
    /// - += → -= (angle integration sign)
    /// - > → >= (threshold comparison)
    /// - * → + or / (spring force quadratic)
    /// - + → - (torque sum)
    #[test]
    fn test_bumpstop_angle_integration_direction() {
        // Positive wheel speed must increase current_angle
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        let mut frame = create_test_frame(100.0); // positive speed
        bumpstop_filter(&mut frame, &mut state);
        assert!(
            state.current_angle > 0.0,
            "positive wheel speed must increase angle, got {}",
            state.current_angle
        );

        // Negative wheel speed must decrease current_angle from zero
        let mut state_neg = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        let mut frame_neg = create_test_frame(-100.0);
        bumpstop_filter(&mut frame_neg, &mut state_neg);
        assert!(
            state_neg.current_angle < 0.0,
            "negative wheel speed must decrease angle, got {}",
            state_neg.current_angle
        );
    }

    /// Kill mutant: `> start_angle` → `>= start_angle`.
    /// At exactly start_angle, no bumpstop force should be applied (we're at the boundary).
    #[test]
    fn test_bumpstop_at_exact_start_angle_no_spring() {
        let mut state = BumpstopState::new(true, 450.0, 540.0, 0.8, 0.3);
        state.current_angle = 450.0; // exactly at start_angle

        let mut frame = create_test_frame(0.0); // zero speed → no damping
        bumpstop_filter(&mut frame, &mut state);

        // At exactly start_angle, abs_angle (450.0) is NOT > start_angle (450.0),
        // so no bumpstop force should be applied.
        assert!(
            frame.torque_out.abs() < 1e-6,
            "at exact start_angle with zero speed, torque must be ~0, got {}",
            frame.torque_out
        );
    }

    /// Kill mutants: verify quadratic spring force formula.
    /// The spring force is penetration^2 * stiffness, which differs from
    /// penetration + stiffness (if * → +) or penetration / stiffness (if * → /).
    #[test]
    fn test_bumpstop_quadratic_spring_force() {
        let stiffness = 1.0;
        let start = 400.0;
        let max = 500.0;
        let range = max - start; // 100.0

        // Light penetration: 10% into bumpstop zone
        let mut state_light = BumpstopState::new(true, start, max, stiffness, 0.0);
        state_light.current_angle = start + range * 0.1; // 410°
        let mut frame_light = create_test_frame(0.0);
        bumpstop_filter(&mut frame_light, &mut state_light);

        // Heavy penetration: 50% into bumpstop zone
        let mut state_heavy = BumpstopState::new(true, start, max, stiffness, 0.0);
        state_heavy.current_angle = start + range * 0.5; // 450°
        let mut frame_heavy = create_test_frame(0.0);
        bumpstop_filter(&mut frame_heavy, &mut state_heavy);

        // For quadratic: ratio should be (0.5^2)/(0.1^2) = 25.0
        // For linear (if * → +): ratio would be ~0.5/0.1 = 5.0
        let ratio = frame_heavy.torque_out.abs() / frame_light.torque_out.abs();
        assert!(
            ratio > 10.0,
            "force must increase quadratically; ratio of heavy/light should be ~25, got {}",
            ratio
        );
    }

    /// Kill mutant: `spring_force + damping_force` → `spring_force - damping_force`.
    /// With positive spring and positive damping (same direction), total force
    /// must be larger than spring alone.
    #[test]
    fn test_bumpstop_damping_adds_to_spring() {
        let start = 400.0;
        let max = 500.0;
        let stiffness = 0.5;

        // With damping=0: only spring force
        let mut state_no_damp = BumpstopState::new(true, start, max, stiffness, 0.0);
        state_no_damp.current_angle = 450.0;
        let mut frame_no_damp = create_test_frame(1000.0); // positive speed into the stop
        bumpstop_filter(&mut frame_no_damp, &mut state_no_damp);

        // With damping > 0: spring + damping
        let mut state_damp = BumpstopState::new(true, start, max, stiffness, 0.5);
        state_damp.current_angle = 450.0;
        let mut frame_damp = create_test_frame(1000.0);
        bumpstop_filter(&mut frame_damp, &mut state_damp);

        // Both should oppose positive rotation (negative torque)
        assert!(frame_no_damp.torque_out < 0.0, "spring force must oppose");
        assert!(frame_damp.torque_out < 0.0, "spring+damping must oppose");
        // Damped version should have greater magnitude
        assert!(
            frame_damp.torque_out.abs() > frame_no_damp.torque_out.abs(),
            "damping must add to spring force magnitude: no_damp={}, damp={}",
            frame_no_damp.torque_out.abs(),
            frame_damp.torque_out.abs()
        );
    }
}
