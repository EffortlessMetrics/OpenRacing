//! FFB effect types

use serde::{Deserialize, Serialize};

/// Types of force feedback effects
///
/// # Examples
///
/// ```
/// use openracing_ffb::EffectType;
///
/// let effect = EffectType::Spring;
/// assert_ne!(effect, EffectType::None);
///
/// // Default is None (no effect)
/// assert_eq!(EffectType::default(), EffectType::None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EffectType {
    #[default]
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

/// Base parameters for all FFB effects
///
/// # Examples
///
/// ```
/// use openracing_ffb::{EffectParams, EffectType};
///
/// let params = EffectParams::new(EffectType::Sine, 1000)
///     .with_gain(128)
///     .with_direction(90);
///
/// assert_eq!(params.effect_type, EffectType::Sine);
/// assert_eq!(params.duration_ms, 1000);
/// assert_eq!(params.gain, 128);
/// assert_eq!(params.direction, 90);
/// ```
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
        self.gain = gain;
        self
    }

    pub fn with_direction(mut self, direction: u16) -> Self {
        self.direction = direction;
        self
    }
}

/// Constant force effect
///
/// # Examples
///
/// ```
/// use openracing_ffb::ConstantEffect;
///
/// let effect = ConstantEffect::new(1000);
/// assert_eq!(effect.magnitude, 1000);
///
/// // apply_gain scales the magnitude by a global gain factor
/// let scaled = effect.apply_gain(0.5);
/// assert_eq!(scaled, 500);
/// ```
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
        ((self.magnitude as f32) * global_gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16
    }
}

/// Spring effect
///
/// Produces a force proportional to displacement from a center position,
/// with an optional deadband.
///
/// # Examples
///
/// ```
/// use openracing_ffb::SpringEffect;
///
/// let spring = SpringEffect::new(1000);
///
/// // Force is proportional to position
/// assert_eq!(spring.calculate(0), 0);
/// assert_eq!(spring.calculate(100), 100);
/// assert_eq!(spring.calculate(-100), -100);
/// ```
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
        let diff = (position as i32) - (self.offset as i32);
        if diff.abs() < self.deadband as i32 {
            return 0;
        }
        (diff as f32 * (self.coefficient as f32 / 1000.0)).clamp(i16::MIN as f32, i16::MAX as f32)
            as i16
    }
}

/// Damper effect
///
/// Produces a force proportional to velocity, simulating viscous damping.
///
/// # Examples
///
/// ```
/// use openracing_ffb::DamperEffect;
///
/// let damper = DamperEffect::new(500);
///
/// // Damping force scales with velocity
/// assert_eq!(damper.calculate(100), 50);
/// assert_eq!(damper.calculate(200), 100);
/// ```
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
        ((velocity as f32) * (self.coefficient as f32 / 1000.0))
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16
    }
}

/// Friction effect
///
/// Produces a force opposing the direction of motion.
///
/// # Examples
///
/// ```
/// use openracing_ffb::FrictionEffect;
///
/// let friction = FrictionEffect::new(100);
///
/// // Friction opposes direction of movement
/// assert!(friction.calculate(100) < 0);  // moving right → force left
/// assert!(friction.calculate(-100) > 0); // moving left → force right
/// assert_eq!(friction.calculate(0), 0);  // no movement → no friction
/// ```
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
        let abs_vel = velocity.saturating_abs();
        let friction = self.coefficient.saturating_add(abs_vel / 100);
        sign.saturating_mul(friction).saturating_neg()
    }
}

/// Sine wave effect for vibration
///
/// # Examples
///
/// ```
/// use openracing_ffb::SineEffect;
///
/// // 10 Hz vibration for 1 second
/// let sine = SineEffect::new(10.0, 1000);
///
/// // The waveform produces non-zero values at non-zero times
/// let sample = sine.calculate(25);
/// assert_ne!(sample, 0);
/// ```
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
        (angle.sin() * (self.params.gain as f32 / 255.0) * i16::MAX as f32)
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16
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

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        // --- SpringEffect ---

        #[test]
        fn prop_spring_output_bounded(
            coefficient in i16::MIN..=i16::MAX,
            position in i16::MIN..=i16::MAX,
        ) {
            let spring = SpringEffect::new(coefficient);
            let output = spring.calculate(position) as i32;
            prop_assert!(
                output >= i16::MIN as i32 && output <= i16::MAX as i32,
                "spring output {} out of i16 range", output
            );
        }

        #[test]
        fn prop_spring_zero_at_center(coefficient in i16::MIN..=i16::MAX) {
            let spring = SpringEffect::new(coefficient);
            prop_assert_eq!(spring.calculate(0), 0, "spring must be zero at center");
        }

        #[test]
        fn prop_spring_monotonic_near_center(coefficient in 1i16..=i16::MAX) {
            let spring = SpringEffect::new(coefficient);
            let out_neg = spring.calculate(-1);
            let out_zero = spring.calculate(0);
            let out_pos = spring.calculate(1);
            // For positive coefficient: output(-1) <= output(0) <= output(1)
            prop_assert!(
                out_neg <= out_zero && out_zero <= out_pos,
                "spring not monotonic near center: f(-1)={}, f(0)={}, f(1)={}",
                out_neg, out_zero, out_pos
            );
        }

        // --- DamperEffect ---

        #[test]
        fn prop_damper_output_bounded(
            coefficient in i16::MIN..=i16::MAX,
            velocity in i16::MIN..=i16::MAX,
        ) {
            let damper = DamperEffect::new(coefficient);
            let output = damper.calculate(velocity) as i32;
            prop_assert!(
                output >= i16::MIN as i32 && output <= i16::MAX as i32,
                "damper output {} out of i16 range", output
            );
        }

        #[test]
        fn prop_damper_proportional_to_velocity(coefficient in 1i16..=1000i16) {
            let damper = DamperEffect::new(coefficient);
            let out_low = damper.calculate(100);
            let out_high = damper.calculate(200);
            // For positive coefficient: higher velocity → higher magnitude output
            prop_assert!(
                out_high.abs() >= out_low.abs(),
                "damper not proportional: f(100)={}, f(200)={}", out_low, out_high
            );
        }

        // --- FrictionEffect ---

        #[test]
        fn prop_friction_output_bounded(
            coefficient in i16::MIN..=i16::MAX,
            velocity in i16::MIN..=i16::MAX,
        ) {
            let friction = FrictionEffect::new(coefficient);
            let output = friction.calculate(velocity) as i32;
            prop_assert!(
                output >= i16::MIN as i32 && output <= i16::MAX as i32,
                "friction output {} out of i16 range", output
            );
        }

        #[test]
        fn prop_friction_opposes_velocity(coefficient in 0i16..=i16::MAX) {
            let friction = FrictionEffect::new(coefficient);
            let out_pos = friction.calculate(100);
            let out_neg = friction.calculate(-100);
            // Friction opposes movement direction
            prop_assert!(out_pos <= 0, "friction should oppose positive velocity, got {}", out_pos);
            prop_assert!(out_neg >= 0, "friction should oppose negative velocity, got {}", out_neg);
        }

        #[test]
        fn prop_friction_zero_at_rest(coefficient in i16::MIN..=i16::MAX) {
            let friction = FrictionEffect::new(coefficient);
            prop_assert_eq!(friction.calculate(0), 0, "friction must be zero at rest");
        }

        // --- ConstantForceEffect ---

        #[test]
        fn prop_constant_effect_apply_gain_bounded(
            magnitude in i16::MIN..=i16::MAX,
            gain in 0.0f32..=1.0,
        ) {
            let effect = ConstantEffect::new(magnitude);
            let output = effect.apply_gain(gain) as i32;
            prop_assert!(
                output >= i16::MIN as i32 && output <= i16::MAX as i32,
                "constant effect output {} out of i16 range", output
            );
        }

        #[test]
        fn prop_constant_effect_unity_gain(magnitude in i16::MIN..=i16::MAX) {
            let effect = ConstantEffect::new(magnitude);
            let output = effect.apply_gain(1.0);
            prop_assert_eq!(
                output, magnitude,
                "constant effect with gain=1.0 should equal magnitude"
            );
        }

        #[test]
        fn prop_constant_effect_zero_gain(magnitude in i16::MIN..=i16::MAX) {
            let effect = ConstantEffect::new(magnitude);
            let output = effect.apply_gain(0.0);
            prop_assert_eq!(output, 0, "constant effect with gain=0.0 should be zero");
        }

        // --- SineEffect ---

        #[test]
        fn prop_sine_output_bounded(
            freq in 0.1f32..=1000.0,
            time_ms in 0u32..=10_000,
        ) {
            let sine = SineEffect::new(freq, 10_000);
            let output = sine.calculate(time_ms) as i32;
            prop_assert!(
                output >= i16::MIN as i32 && output <= i16::MAX as i32,
                "sine output {} out of i16 range", output
            );
        }

        #[test]
        fn prop_sine_periodic(
            // Use integer frequencies so period_ms is exact
            freq_int in 1u32..=100,
        ) {
            let freq = freq_int as f32;
            let sine = SineEffect::new(freq, 10_000);
            // Only test when 1000 is exactly divisible by freq (exact integer period)
            if 1000 % freq_int == 0 {
                let period_ms = 1000 / freq_int;
                let s1 = sine.calculate(0);
                let s2 = sine.calculate(period_ms);
                let diff = (s1 as i32 - s2 as i32).abs();
                prop_assert!(
                    diff <= 1,
                    "sine not periodic: f(0)={}, f({})={}, diff={}", s1, period_ms, s2, diff
                );
            }
        }

        // --- Extreme inputs: all effects must not panic ---

        #[test]
        fn prop_spring_extreme_inputs(coefficient in i16::MIN..=i16::MAX) {
            let spring = SpringEffect::new(coefficient);
            // These must not panic
            let _ = spring.calculate(i16::MIN);
            let _ = spring.calculate(i16::MAX);
            let _ = spring.calculate(0);
        }

        #[test]
        fn prop_damper_extreme_inputs(coefficient in i16::MIN..=i16::MAX) {
            let damper = DamperEffect::new(coefficient);
            let _ = damper.calculate(i16::MIN);
            let _ = damper.calculate(i16::MAX);
            let _ = damper.calculate(0);
        }

        #[test]
        fn prop_friction_extreme_inputs(coefficient in i16::MIN..=i16::MAX) {
            let friction = FrictionEffect::new(coefficient);
            let _ = friction.calculate(i16::MIN);
            let _ = friction.calculate(i16::MAX);
            let _ = friction.calculate(0);
        }

        #[test]
        fn prop_constant_extreme_inputs(gain in -10.0f32..=10.0) {
            let effect_min = ConstantEffect::new(i16::MIN);
            let effect_max = ConstantEffect::new(i16::MAX);
            let effect_zero = ConstantEffect::new(0);
            // Must not panic
            let _ = effect_min.apply_gain(gain);
            let _ = effect_max.apply_gain(gain);
            let _ = effect_zero.apply_gain(gain);
        }

        #[test]
        fn prop_sine_extreme_inputs(freq in 0.001f32..=100_000.0) {
            let sine = SineEffect::new(freq, u32::MAX);
            let _ = sine.calculate(0);
            let _ = sine.calculate(u32::MAX);
        }
    }
}
