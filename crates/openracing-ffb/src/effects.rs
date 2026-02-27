//! FFB effect types

use serde::{Deserialize, Serialize};

/// Types of force feedback effects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectType {
    None,
    Constant,
    Ramp,
    Square,
    Sine,
    Triangle,
    SawtoothUp,
    SawtoothDown,
    Spring,
    Damper,
    Friction,
    Custom,
}

impl Default for EffectType {
    fn default() -> Self {
        Self::None
    }
}

/// Base parameters for all FFB effects
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EffectParams {
    pub effect_type: EffectType,
    pub duration_ms: u32,
    pub gain: u8,
    pub direction: u16,
}

impl EffectParams {
    pub fn new(effect_type: EffectType, duration_ms: u32) -> Self {
        Self {
            effect_type,
            duration_ms,
            gain: 255,
            direction: 0,
        }
    }

    pub fn with_gain(mut self, gain: u8) -> Self {
        self.gain = gain.min(255);
        self
    }

    pub fn with_direction(mut self, direction: u16) -> Self {
        self.direction = direction;
        self
    }
}

/// Constant force effect
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ConstantEffect {
    pub params: EffectParams,
    pub magnitude: i16,
}

impl ConstantEffect {
    pub fn new(magnitude: i16) -> Self {
        Self {
            params: EffectParams::new(EffectType::Constant, 0),
            magnitude,
        }
    }

    pub fn apply_gain(&self, global_gain: f32) -> i16 {
        ((self.magnitude as f32) * global_gain) as i16
    }
}

/// Spring effect
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SpringEffect {
    pub params: EffectParams,
    pub coefficient: i16,
    pub offset: i16,
    pub deadband: i16,
}

impl SpringEffect {
    pub fn new(coefficient: i16) -> Self {
        Self {
            params: EffectParams::new(EffectType::Spring, 0),
            coefficient,
            offset: 0,
            deadband: 0,
        }
    }

    pub fn calculate(&self, position: i16) -> i16 {
        let diff = position - self.offset;
        if diff.abs() < self.deadband {
            return 0;
        }
        ((diff as f32) * (self.coefficient as f32 / 1000.0)) as i16
    }
}

/// Damper effect
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DamperEffect {
    pub params: EffectParams,
    pub coefficient: i16,
}

impl DamperEffect {
    pub fn new(coefficient: i16) -> Self {
        Self {
            params: EffectParams::new(EffectType::Damper, 0),
            coefficient,
        }
    }

    pub fn calculate(&self, velocity: i16) -> i16 {
        ((velocity as f32) * (self.coefficient as f32 / 1000.0)) as i16
    }
}

/// Friction effect
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct FrictionEffect {
    pub params: EffectParams,
    pub coefficient: i16,
    pub offset: i16,
}

impl FrictionEffect {
    pub fn new(coefficient: i16) -> Self {
        Self {
            params: EffectParams::new(EffectType::Friction, 0),
            coefficient,
            offset: 0,
        }
    }

    pub fn calculate(&self, velocity: i16) -> i16 {
        let sign = velocity.signum();
        let abs_vel = velocity.abs();
        let friction = self.coefficient.saturating_add(abs_vel / 100);
        -(sign * friction.min(i16::MAX))
    }
}

/// Sine wave effect for vibration
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SineEffect {
    pub params: EffectParams,
    pub frequency_hz: f32,
    pub phase: f32,
}

impl SineEffect {
    pub fn new(frequency_hz: f32, duration_ms: u32) -> Self {
        Self {
            params: EffectParams::new(EffectType::Sine, duration_ms),
            frequency_hz,
            phase: 0.0,
        }
    }

    pub fn calculate(&self, time_ms: u32) -> i16 {
        let t = time_ms as f32 / 1000.0;
        let angle = 2.0 * std::f32::consts::PI * self.frequency_hz * t + self.phase;
        (angle.sin() * (self.params.gain as f32 / 255.0) * i16::MAX as f32) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_effect() {
        let effect = ConstantEffect::new(1000);
        assert_eq!(effect.magnitude, 1000);

        let applied = effect.apply_gain(0.5);
        assert_eq!(applied, 500);
    }

    #[test]
    fn test_spring_effect() {
        let spring = SpringEffect::new(1000);

        assert_eq!(spring.calculate(0), 0);
        assert_eq!(spring.calculate(100), 100);
    }

    #[test]
    fn test_spring_with_deadband() {
        let mut spring = SpringEffect::new(1000);
        spring.deadband = 50;

        assert_eq!(spring.calculate(25), 0);
        assert!(spring.calculate(100) > 0);
    }

    #[test]
    fn test_damper_effect() {
        let damper = DamperEffect::new(500);

        let result = damper.calculate(100);
        assert_eq!(result, 50);
    }

    #[test]
    fn test_friction_effect() {
        let friction = FrictionEffect::new(100);

        assert!(friction.calculate(100) < 0);
        assert!(friction.calculate(-100) > 0);
        assert_eq!(friction.calculate(0), 0);
    }

    #[test]
    fn test_sine_effect() {
        let sine = SineEffect::new(1.0, 1000);

        let sample1 = sine.calculate(125);
        let _sample250 = sine.calculate(250);
        let _sample500 = sine.calculate(500);

        // Sine wave should produce non-zero values
        assert!(sample1 != 0);
    }
}
