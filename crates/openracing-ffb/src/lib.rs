//! Force Feedback (FFB) effect types and calculations
//!
//! This crate provides standardized force feedback effect definitions
//! that can be used across different wheel protocols.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

pub mod constants;
pub mod effects;

pub use constants::*;
pub use effects::*;

use serde::{Deserialize, Serialize};

/// Overall FFB gain (0.0 to 1.0)
///
/// # Examples
///
/// ```
/// use openracing_ffb::FfbGain;
///
/// // Create gain with overall level, then customize sub-gains
/// let gain = FfbGain::new(0.8)
///     .with_torque(0.9)
///     .with_effects(0.5);
///
/// // Combined gain multiplies all three factors
/// let combined = gain.combined();
/// assert!((combined - 0.8 * 0.9 * 0.5).abs() < 0.001);
/// ```
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FfbGain {
    pub overall: f32,
    pub torque: f32,
    pub effects: f32,
}

impl FfbGain {
    pub fn new(overall: f32) -> Self {
        Self {
            overall: overall.clamp(0.0, 1.0),
            torque: 1.0,
            effects: 1.0,
        }
    }

    pub fn with_torque(mut self, torque: f32) -> Self {
        self.torque = torque.clamp(0.0, 1.0);
        self
    }

    pub fn with_effects(mut self, effects: f32) -> Self {
        self.effects = effects.clamp(0.0, 1.0);
        self
    }

    pub fn combined(&self) -> f32 {
        self.overall * self.torque * self.effects
    }
}

/// FFB direction in degrees (0-360)
///
/// # Examples
///
/// ```
/// use openracing_ffb::FfbDirection;
///
/// // Create direction, values wrap around 360°
/// let dir = FfbDirection::new(450.0);
/// assert!((dir.degrees - 90.0).abs() < f32::EPSILON);
///
/// // Negative angles also wrap correctly
/// let neg = FfbDirection::new(-90.0);
/// assert!((neg.degrees - 270.0).abs() < f32::EPSILON);
///
/// // Convert to/from radians
/// let dir = FfbDirection::from_radians(std::f32::consts::PI);
/// assert!((dir.degrees - 180.0).abs() < 0.001);
/// ```
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FfbDirection {
    pub degrees: f32,
}

impl FfbDirection {
    pub fn new(degrees: f32) -> Self {
        Self {
            degrees: degrees.rem_euclid(360.0),
        }
    }

    pub fn from_radians(rad: f32) -> Self {
        Self::new(rad.to_degrees())
    }

    pub fn to_radians(&self) -> f32 {
        self.degrees.to_radians()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffb_gain() {
        let gain = FfbGain::new(0.5);
        assert_eq!(gain.overall, 0.5);
        assert_eq!(gain.combined(), 0.5);
    }

    #[test]
    fn test_ffb_gain_clamping() {
        let gain = FfbGain::new(1.5);
        assert_eq!(gain.overall, 1.0);

        let gain = FfbGain::new(-0.5);
        assert_eq!(gain.overall, 0.0);
    }

    #[test]
    fn test_ffb_direction() {
        let dir = FfbDirection::new(90.0);
        assert_eq!(dir.degrees, 90.0);
    }

    #[test]
    fn test_ffb_direction_wrapping() {
        let dir = FfbDirection::new(450.0);
        assert_eq!(dir.degrees, 90.0);

        let dir = FfbDirection::new(-90.0);
        assert_eq!(dir.degrees, 270.0);
    }

    #[test]
    fn test_ffb_direction_radians() {
        let dir = FfbDirection::new(180.0);
        assert!((dir.to_radians() - std::f32::consts::PI).abs() < 0.001);
    }
}
