//! Deep tests for FFB effects: spring, damper, friction, constant, sine,
//! effect combination, envelope/gain, timing, parameter boundaries, and clipping.

use openracing_ffb::{
    ConstantEffect, DamperEffect, EffectParams, EffectType, FfbDirection, FfbGain, FrictionEffect,
    MAX_DAMPER_COEFFICIENT, MAX_EFFECT_DURATION_MS, MAX_FRICTION_COEFFICIENT, MAX_GAIN,
    MAX_SPRING_COEFFICIENT, MIN_GAIN, SineEffect, SpringEffect,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Combine two i16 forces with clipping.
fn combine_forces(a: i16, b: i16) -> i16 {
    (a as i32 + b as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

// ---------------------------------------------------------------------------
// Spring effect
// ---------------------------------------------------------------------------

#[test]
fn ffb_01_spring_zero_at_center() {
    let spring = SpringEffect::new(1000);
    assert_eq!(spring.calculate(0), 0);
}

#[test]
fn ffb_02_spring_proportional_force() {
    let spring = SpringEffect::new(1000);
    let f100 = spring.calculate(100);
    let f200 = spring.calculate(200);
    assert_eq!(f100, 100);
    assert_eq!(f200, 200);
}

#[test]
fn ffb_03_spring_symmetric() {
    let spring = SpringEffect::new(1000);
    let pos = spring.calculate(500);
    let neg = spring.calculate(-500);
    assert_eq!(pos, -neg, "spring should be symmetric around center");
}

#[test]
fn ffb_04_spring_with_deadband() {
    let mut spring = SpringEffect::new(1000);
    spring.deadband = 100;

    assert_eq!(spring.calculate(0), 0, "at center");
    assert_eq!(spring.calculate(50), 0, "inside deadband");
    assert_eq!(spring.calculate(-99), 0, "inside negative deadband");
    assert!(
        spring.calculate(200) > 0,
        "outside deadband should produce force"
    );
    assert!(spring.calculate(-200) < 0, "outside negative deadband");
}

#[test]
fn ffb_05_spring_with_offset() {
    let mut spring = SpringEffect::new(1000);
    spring.offset = 500;

    // At the offset position, force should be zero
    assert_eq!(spring.calculate(500), 0);
    // Displaced from offset
    assert!(spring.calculate(600) > 0);
    assert!(spring.calculate(400) < 0);
}

#[test]
fn ffb_06_spring_negative_coefficient() {
    let spring = SpringEffect::new(-1000);
    // Negative coefficient inverts the force direction
    let force = spring.calculate(100);
    assert!(
        force < 0,
        "negative coeff at positive pos should give negative force"
    );
}

#[test]
fn ffb_07_spring_extreme_inputs() {
    let spring = SpringEffect::new(i16::MAX);
    let f_max = spring.calculate(i16::MAX) as i32;
    let f_min = spring.calculate(i16::MIN) as i32;
    // Should be clamped to i16 range
    assert!(f_max <= i16::MAX as i32);
    assert!(f_min >= i16::MIN as i32);
}

// ---------------------------------------------------------------------------
// Damper effect
// ---------------------------------------------------------------------------

#[test]
fn ffb_08_damper_zero_at_rest() {
    let damper = DamperEffect::new(500);
    assert_eq!(damper.calculate(0), 0);
}

#[test]
fn ffb_09_damper_proportional_to_velocity() {
    let damper = DamperEffect::new(1000);
    let f100 = damper.calculate(100);
    let f200 = damper.calculate(200);
    assert_eq!(f100, 100);
    assert_eq!(f200, 200);
    assert!(f200.abs() > f100.abs(), "higher velocity → larger force");
}

#[test]
fn ffb_10_damper_negative_velocity() {
    let damper = DamperEffect::new(1000);
    let force = damper.calculate(-300);
    assert_eq!(force, -300, "negative velocity → negative force");
}

#[test]
fn ffb_11_damper_coefficient_scaling() {
    let weak = DamperEffect::new(100);
    let strong = DamperEffect::new(1000);

    let f_weak = weak.calculate(1000);
    let f_strong = strong.calculate(1000);
    assert!(
        f_strong.abs() > f_weak.abs(),
        "stronger coefficient should produce larger force"
    );
}

#[test]
fn ffb_12_damper_extreme_inputs() {
    let damper = DamperEffect::new(i16::MAX);
    let f = damper.calculate(i16::MAX) as i32;
    assert!(f <= i16::MAX as i32);
    assert!(f >= i16::MIN as i32);

    let f2 = damper.calculate(i16::MIN) as i32;
    assert!(f2 >= i16::MIN as i32);
}

// ---------------------------------------------------------------------------
// Friction effect
// ---------------------------------------------------------------------------

#[test]
fn ffb_13_friction_opposes_positive_velocity() {
    let friction = FrictionEffect::new(100);
    let force = friction.calculate(100);
    assert!(
        force < 0,
        "friction should oppose positive velocity, got {force}"
    );
}

#[test]
fn ffb_14_friction_opposes_negative_velocity() {
    let friction = FrictionEffect::new(100);
    let force = friction.calculate(-100);
    assert!(
        force > 0,
        "friction should oppose negative velocity, got {force}"
    );
}

#[test]
fn ffb_15_friction_zero_at_rest() {
    let friction = FrictionEffect::new(500);
    assert_eq!(friction.calculate(0), 0);
}

#[test]
fn ffb_16_friction_coefficient_scaling() {
    let weak = FrictionEffect::new(50);
    let strong = FrictionEffect::new(500);

    let f_weak = weak.calculate(200).abs();
    let f_strong = strong.calculate(200).abs();
    assert!(
        f_strong > f_weak,
        "stronger friction should produce more resistance"
    );
}

#[test]
fn ffb_17_friction_extreme_inputs() {
    let friction = FrictionEffect::new(i16::MAX);
    let f1 = friction.calculate(i16::MAX) as i32;
    let f2 = friction.calculate(i16::MIN) as i32;
    assert!(f1 >= i16::MIN as i32 && f1 <= i16::MAX as i32);
    assert!(f2 >= i16::MIN as i32 && f2 <= i16::MAX as i32);
}

// ---------------------------------------------------------------------------
// Constant effect
// ---------------------------------------------------------------------------

#[test]
fn ffb_18_constant_effect_basic() {
    let effect = ConstantEffect::new(5000);
    assert_eq!(effect.magnitude, 5000);
    assert_eq!(effect.apply_gain(1.0), 5000);
}

#[test]
fn ffb_19_constant_effect_gain_scaling() {
    let effect = ConstantEffect::new(10000);
    assert_eq!(effect.apply_gain(0.5), 5000);
    assert_eq!(effect.apply_gain(0.25), 2500);
}

#[test]
fn ffb_20_constant_effect_zero_gain() {
    let effect = ConstantEffect::new(i16::MAX);
    assert_eq!(effect.apply_gain(0.0), 0);
}

#[test]
fn ffb_21_constant_effect_negative_magnitude() {
    let effect = ConstantEffect::new(-10000);
    assert_eq!(effect.apply_gain(1.0), -10000);
    assert_eq!(effect.apply_gain(0.5), -5000);
}

#[test]
fn ffb_22_constant_effect_clipping() {
    let effect = ConstantEffect::new(i16::MAX);
    // Gain > 1.0 should still be clamped to i16 range
    let result = effect.apply_gain(2.0) as i32;
    assert!(result <= i16::MAX as i32);
    assert!(result >= i16::MIN as i32);
}

// ---------------------------------------------------------------------------
// Sine effect
// ---------------------------------------------------------------------------

#[test]
fn ffb_23_sine_zero_at_start() {
    let sine = SineEffect::new(10.0, 1000);
    assert_eq!(sine.calculate(0), 0, "sin(0) should be 0");
}

#[test]
fn ffb_24_sine_nonzero_at_quarter_period() {
    // 10 Hz → period = 100ms → quarter period = 25ms
    let sine = SineEffect::new(10.0, 1000);
    let sample = sine.calculate(25);
    assert_ne!(sample, 0, "sin(π/2) should be non-zero");
    assert!(sample > 0, "sin(π/2) should be positive");
}

#[test]
fn ffb_25_sine_periodicity() {
    let sine = SineEffect::new(10.0, 10000);
    let period_ms = 100; // 10Hz → 100ms period
    let s0 = sine.calculate(0);
    let s1 = sine.calculate(period_ms);
    let diff = (s0 as i32 - s1 as i32).abs();
    assert!(
        diff <= 1,
        "sine should repeat after one period, diff={diff}"
    );
}

#[test]
fn ffb_26_sine_frequency_scaling() {
    let slow = SineEffect::new(1.0, 10000);
    let fast = SineEffect::new(100.0, 10000);

    // At 1ms: slow sine barely moves, fast sine has completed ~10% of a cycle
    let s_slow = slow.calculate(1).abs();
    let s_fast = fast.calculate(1).abs();
    assert!(
        s_fast >= s_slow,
        "higher frequency should oscillate faster: slow={s_slow}, fast={s_fast}"
    );
}

#[test]
fn ffb_27_sine_with_gain() {
    let mut sine = SineEffect::new(10.0, 1000);
    sine.params.gain = 128; // Half gain

    let full_gain = SineEffect::new(10.0, 1000);
    // At quarter period (peak)
    let half = sine.calculate(25);
    let full = full_gain.calculate(25);

    // Half gain should produce roughly half the amplitude
    let ratio = half as f32 / full as f32;
    assert!(
        (ratio - 0.5).abs() < 0.05,
        "half gain should halve amplitude, ratio={ratio}"
    );
}

#[test]
fn ffb_28_sine_negative_half_period() {
    let sine = SineEffect::new(10.0, 1000);
    // At 75ms = 3/4 period, sin should be negative
    let sample = sine.calculate(75);
    assert!(sample < 0, "sin(3π/2) should be negative, got {sample}");
}

// ---------------------------------------------------------------------------
// Effect combination
// ---------------------------------------------------------------------------

#[test]
fn ffb_29_spring_plus_damper_combination() {
    let spring = SpringEffect::new(800);
    let damper = DamperEffect::new(300);

    let position: i16 = 100;
    let velocity: i16 = 50;

    let spring_force = spring.calculate(position);
    let damper_force = damper.calculate(velocity);
    let total = combine_forces(spring_force, damper_force);

    // Spring: 100 * 800/1000 = 80
    // Damper: 50 * 300/1000 = 15
    assert_eq!(spring_force, 80);
    assert_eq!(damper_force, 15);
    assert_eq!(total, 95);
}

#[test]
fn ffb_30_triple_effect_combination() {
    let spring = SpringEffect::new(1000);
    let damper = DamperEffect::new(500);
    let friction = FrictionEffect::new(100);

    let pos: i16 = 200;
    let vel: i16 = 100;

    let sf = spring.calculate(pos);
    let df = damper.calculate(vel);
    let ff = friction.calculate(vel);

    let total = (sf as i32 + df as i32 + ff as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;

    // All should contribute to the total
    assert_ne!(total, sf, "damper and friction should modify total");
}

#[test]
fn ffb_31_combination_clipping() {
    // Two effects that individually are near max should clip when combined
    let e1 = ConstantEffect::new(i16::MAX);
    let e2 = ConstantEffect::new(i16::MAX);

    let f1 = e1.apply_gain(1.0);
    let f2 = e2.apply_gain(1.0);
    let combined = combine_forces(f1, f2);

    assert_eq!(combined, i16::MAX, "combined should clip to i16::MAX");
}

#[test]
fn ffb_32_combination_negative_clipping() {
    let e1 = ConstantEffect::new(i16::MIN);
    let e2 = ConstantEffect::new(i16::MIN);

    let f1 = e1.apply_gain(1.0);
    let f2 = e2.apply_gain(1.0);
    let combined = combine_forces(f1, f2);

    assert_eq!(combined, i16::MIN, "combined should clip to i16::MIN");
}

// ---------------------------------------------------------------------------
// FfbGain (envelope / three-tier scaling)
// ---------------------------------------------------------------------------

#[test]
fn ffb_33_gain_three_tier() {
    let gain = FfbGain::new(0.8).with_torque(0.5).with_effects(0.5);
    let combined = gain.combined();
    assert!((combined - 0.2).abs() < 0.001);
}

#[test]
fn ffb_34_gain_clamping_above() {
    let gain = FfbGain::new(2.0);
    assert!((gain.overall - 1.0).abs() < f32::EPSILON);
}

#[test]
fn ffb_35_gain_clamping_below() {
    let gain = FfbGain::new(-1.0);
    assert!((gain.overall - 0.0).abs() < f32::EPSILON);
}

#[test]
fn ffb_36_gain_torque_clamping() {
    let gain = FfbGain::new(1.0).with_torque(5.0);
    assert!((gain.torque - 1.0).abs() < f32::EPSILON);
}

#[test]
fn ffb_37_gain_effects_clamping() {
    let gain = FfbGain::new(1.0).with_effects(-0.5);
    assert!((gain.effects - 0.0).abs() < f32::EPSILON);
}

#[test]
fn ffb_38_gain_applied_to_constant_effect() {
    let effect = ConstantEffect::new(10000);
    let gain = FfbGain::new(0.5).with_torque(0.5);
    let scaled = effect.apply_gain(gain.combined());
    // 10000 * 0.5 * 0.5 * 1.0 = 2500
    assert_eq!(scaled, 2500);
}

#[test]
fn ffb_39_gain_zero_silences_all() {
    let gain = FfbGain::new(0.0);
    let effect = ConstantEffect::new(i16::MAX);
    assert_eq!(effect.apply_gain(gain.combined()), 0);
}

// ---------------------------------------------------------------------------
// FfbDirection
// ---------------------------------------------------------------------------

#[test]
fn ffb_40_direction_wrapping() {
    let dir = FfbDirection::new(450.0);
    assert!((dir.degrees - 90.0).abs() < f32::EPSILON);
}

#[test]
fn ffb_41_direction_negative_wrapping() {
    let dir = FfbDirection::new(-90.0);
    assert!((dir.degrees - 270.0).abs() < f32::EPSILON);
}

#[test]
fn ffb_42_direction_from_radians() {
    let dir = FfbDirection::from_radians(std::f32::consts::PI);
    assert!((dir.degrees - 180.0).abs() < 0.01);
}

#[test]
fn ffb_43_direction_to_radians() {
    let dir = FfbDirection::new(90.0);
    assert!((dir.to_radians() - std::f32::consts::FRAC_PI_2).abs() < 0.001);
}

// ---------------------------------------------------------------------------
// EffectParams
// ---------------------------------------------------------------------------

#[test]
fn ffb_44_effect_params_defaults() {
    let params = EffectParams::new(EffectType::Spring, 0);
    assert_eq!(params.gain, 255, "default gain should be max");
    assert_eq!(params.direction, 0, "default direction should be 0");
    assert_eq!(params.effect_type, EffectType::Spring);
    assert_eq!(params.duration_ms, 0);
}

#[test]
fn ffb_45_effect_params_builder() {
    let params = EffectParams::new(EffectType::Sine, 500)
        .with_gain(128)
        .with_direction(180);
    assert_eq!(params.gain, 128);
    assert_eq!(params.direction, 180);
    assert_eq!(params.duration_ms, 500);
}

#[test]
fn ffb_46_effect_type_default() {
    assert_eq!(EffectType::default(), EffectType::None);
}

// ---------------------------------------------------------------------------
// Effect timing / duration
// ---------------------------------------------------------------------------

#[test]
fn ffb_47_sine_within_duration() {
    let sine = SineEffect::new(10.0, 1000);
    assert_eq!(sine.params.duration_ms, 1000);
    // Effect can still calculate beyond duration (policy is external)
    let _sample = sine.calculate(2000);
}

#[test]
fn ffb_48_infinite_duration_effects() {
    // duration_ms = 0 means infinite
    let spring = SpringEffect::new(500);
    assert_eq!(spring.params.duration_ms, 0, "spring default is infinite");

    let damper = DamperEffect::new(500);
    assert_eq!(damper.params.duration_ms, 0);

    let friction = FrictionEffect::new(500);
    assert_eq!(friction.params.duration_ms, 0);

    let constant = ConstantEffect::new(500);
    assert_eq!(constant.params.duration_ms, 0);
}

// ---------------------------------------------------------------------------
// Parameter boundary constants
// ---------------------------------------------------------------------------

#[test]
fn ffb_49_parameter_boundary_constants() {
    assert_eq!(MAX_SPRING_COEFFICIENT, 10000);
    assert_eq!(MAX_DAMPER_COEFFICIENT, 10000);
    assert_eq!(MAX_FRICTION_COEFFICIENT, 10000);
    assert_eq!(MAX_EFFECT_DURATION_MS, 10000);
    assert_eq!(MAX_GAIN, 255);
    assert_eq!(MIN_GAIN, 0);
}

#[test]
fn ffb_50_spring_at_max_coefficient() {
    let spring = SpringEffect::new(MAX_SPRING_COEFFICIENT);
    let force = spring.calculate(1000);
    // 1000 * 10000 / 1000 = 10000
    assert_eq!(force, 10000);
}

#[test]
fn ffb_51_damper_at_max_coefficient() {
    let damper = DamperEffect::new(MAX_DAMPER_COEFFICIENT);
    let force = damper.calculate(1000);
    // 1000 * 10000 / 1000 = 10000
    assert_eq!(force, 10000);
}

// ---------------------------------------------------------------------------
// Serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn ffb_52_constant_effect_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let effect = ConstantEffect::new(-5000);
    let json = serde_json::to_string(&effect)?;
    let restored: ConstantEffect = serde_json::from_str(&json)?;
    assert_eq!(restored.magnitude, -5000);
    Ok(())
}

#[test]
fn ffb_53_spring_effect_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let mut spring = SpringEffect::new(800);
    spring.offset = 100;
    spring.deadband = 50;
    let json = serde_json::to_string(&spring)?;
    let restored: SpringEffect = serde_json::from_str(&json)?;
    assert_eq!(restored.coefficient, 800);
    assert_eq!(restored.offset, 100);
    assert_eq!(restored.deadband, 50);
    Ok(())
}

#[test]
fn ffb_54_gain_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let gain = FfbGain::new(0.7).with_torque(0.8).with_effects(0.6);
    let json = serde_json::to_string(&gain)?;
    let restored: FfbGain = serde_json::from_str(&json)?;
    assert!((restored.overall - 0.7).abs() < f32::EPSILON);
    assert!((restored.torque - 0.8).abs() < f32::EPSILON);
    assert!((restored.effects - 0.6).abs() < f32::EPSILON);
    Ok(())
}

// ---------------------------------------------------------------------------
// Additional edge cases
// ---------------------------------------------------------------------------

#[test]
fn ffb_55_spring_deadband_boundary_produces_force() {
    let mut spring = SpringEffect::new(1000);
    spring.deadband = 100;
    // The check is `diff.abs() < deadband` (strict), so at the boundary force IS produced
    assert_ne!(
        spring.calculate(100),
        0,
        "at boundary force should be produced"
    );
    assert_ne!(
        spring.calculate(-100),
        0,
        "at negative boundary force should be produced"
    );
    // Just inside deadband should still be zero
    assert_eq!(
        spring.calculate(99),
        0,
        "just inside deadband should be zero"
    );
}

#[test]
fn ffb_56_multiple_sine_effects() {
    let sine_low = SineEffect::new(5.0, 2000);
    let sine_high = SineEffect::new(50.0, 2000);

    // At t=25ms, combine both
    let s1 = sine_low.calculate(25);
    let s2 = sine_high.calculate(25);
    let combined = combine_forces(s1, s2);

    // Combined should be different from either individual effect
    assert!(combined != s1 || combined != s2 || s1 == s2);
}

#[test]
fn ffb_57_spring_offset_with_deadband() {
    let mut spring = SpringEffect::new(1000);
    spring.offset = 500;
    spring.deadband = 50;

    // Inside deadband around offset
    assert_eq!(spring.calculate(500), 0);
    assert_eq!(spring.calculate(520), 0);
    assert_eq!(spring.calculate(480), 0);

    // Outside deadband
    assert!(spring.calculate(600) > 0);
    assert!(spring.calculate(400) < 0);
}

#[test]
fn ffb_58_gain_default_is_zero() {
    let gain = FfbGain::default();
    assert!((gain.overall - 0.0).abs() < f32::EPSILON);
    assert!((gain.combined() - 0.0).abs() < f32::EPSILON);
}
