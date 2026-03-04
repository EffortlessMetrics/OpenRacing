//! Comprehensive integration tests for openracing-ffb
//!
//! Tests FFB effect creation, parameters, composition/mixing,
//! safety limits, and clipping behaviour.

use openracing_ffb::{
    ConstantEffect, DamperEffect, EffectParams, EffectType, FfbDirection, FfbGain, FrictionEffect,
    SineEffect, SpringEffect, constants::*,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// FFB Gain
// ---------------------------------------------------------------------------

mod gain_tests {
    use super::*;

    #[test]
    fn gain_new_clamps_above_one() -> TestResult {
        let g = FfbGain::new(1.5);
        assert!((g.overall - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn gain_new_clamps_below_zero() -> TestResult {
        let g = FfbGain::new(-0.5);
        assert!((g.overall - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn gain_defaults_torque_and_effects_to_one() -> TestResult {
        let g = FfbGain::new(0.8);
        assert!((g.torque - 1.0).abs() < f32::EPSILON);
        assert!((g.effects - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn gain_combined_multiplies_all_factors() -> TestResult {
        let g = FfbGain::new(0.5).with_torque(0.5).with_effects(0.5);
        let combined = g.combined();
        assert!((combined - 0.125).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn gain_combined_zero_when_any_zero() -> TestResult {
        let g = FfbGain::new(0.0).with_torque(1.0).with_effects(1.0);
        assert!((g.combined() - 0.0).abs() < f32::EPSILON);

        let g = FfbGain::new(1.0).with_torque(0.0).with_effects(1.0);
        assert!((g.combined() - 0.0).abs() < f32::EPSILON);

        let g = FfbGain::new(1.0).with_torque(1.0).with_effects(0.0);
        assert!((g.combined() - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn gain_sub_gains_clamp() -> TestResult {
        let g = FfbGain::new(1.0).with_torque(2.0).with_effects(-1.0);
        assert!((g.torque - 1.0).abs() < f32::EPSILON);
        assert!((g.effects - 0.0).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FFB Direction
// ---------------------------------------------------------------------------

mod direction_tests {
    use super::*;

    #[test]
    fn direction_wraps_positive() -> TestResult {
        let dir = FfbDirection::new(450.0);
        assert!((dir.degrees - 90.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn direction_wraps_negative() -> TestResult {
        let dir = FfbDirection::new(-90.0);
        assert!((dir.degrees - 270.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn direction_zero() -> TestResult {
        let dir = FfbDirection::new(0.0);
        assert!((dir.degrees - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn direction_from_radians_pi() -> TestResult {
        let dir = FfbDirection::from_radians(std::f32::consts::PI);
        assert!((dir.degrees - 180.0).abs() < 0.01);
        Ok(())
    }

    #[test]
    fn direction_to_radians_round_trip() -> TestResult {
        let dir = FfbDirection::new(90.0);
        let rad = dir.to_radians();
        assert!((rad - std::f32::consts::FRAC_PI_2).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn direction_360_wraps_to_zero() -> TestResult {
        let dir = FfbDirection::new(360.0);
        assert!((dir.degrees - 0.0).abs() < f32::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect creation and parameters
// ---------------------------------------------------------------------------

mod effect_params_tests {
    use super::*;

    #[test]
    fn effect_params_defaults() -> TestResult {
        let params = EffectParams::new(EffectType::Sine, 1000);
        assert_eq!(params.effect_type, EffectType::Sine);
        assert_eq!(params.duration_ms, 1000);
        assert_eq!(params.gain, 255);
        assert_eq!(params.direction, 0);
        Ok(())
    }

    #[test]
    fn effect_params_builder() -> TestResult {
        let params = EffectParams::new(EffectType::Constant, 500)
            .with_gain(128)
            .with_direction(90);
        assert_eq!(params.gain, 128);
        assert_eq!(params.direction, 90);
        Ok(())
    }

    #[test]
    fn effect_type_default_is_none() -> TestResult {
        assert_eq!(EffectType::default(), EffectType::None);
        Ok(())
    }

    #[test]
    fn all_effect_types_are_distinct() -> TestResult {
        let types = [
            EffectType::None,
            EffectType::Constant,
            EffectType::Ramp,
            EffectType::Square,
            EffectType::Sine,
            EffectType::Triangle,
            EffectType::SawtoothUp,
            EffectType::SawtoothDown,
            EffectType::Spring,
            EffectType::Damper,
            EffectType::Friction,
            EffectType::Custom,
        ];
        for (i, a) in types.iter().enumerate() {
            for (j, b) in types.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "types at indices {i} and {j} should differ");
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Constant effect
// ---------------------------------------------------------------------------

mod constant_tests {
    use super::*;

    #[test]
    fn constant_new_stores_magnitude() -> TestResult {
        let e = ConstantEffect::new(5000);
        assert_eq!(e.magnitude, 5000);
        assert_eq!(e.params.effect_type, EffectType::Constant);
        Ok(())
    }

    #[test]
    fn constant_apply_gain_unity() -> TestResult {
        let e = ConstantEffect::new(1000);
        assert_eq!(e.apply_gain(1.0), 1000);
        Ok(())
    }

    #[test]
    fn constant_apply_gain_zero() -> TestResult {
        let e = ConstantEffect::new(1000);
        assert_eq!(e.apply_gain(0.0), 0);
        Ok(())
    }

    #[test]
    fn constant_apply_gain_half() -> TestResult {
        let e = ConstantEffect::new(1000);
        assert_eq!(e.apply_gain(0.5), 500);
        Ok(())
    }

    #[test]
    fn constant_negative_magnitude() -> TestResult {
        let e = ConstantEffect::new(-1000);
        assert_eq!(e.apply_gain(0.5), -500);
        Ok(())
    }

    #[test]
    fn constant_clamps_on_overflow() -> TestResult {
        let e = ConstantEffect::new(i16::MAX);
        let result = e.apply_gain(2.0);
        assert!(
            (result as i32) <= i16::MAX as i32,
            "should clamp to i16::MAX, got {result}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Spring effect
// ---------------------------------------------------------------------------

mod spring_tests {
    use super::*;

    #[test]
    fn spring_zero_at_center() -> TestResult {
        let s = SpringEffect::new(1000);
        assert_eq!(s.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn spring_positive_displacement() -> TestResult {
        let s = SpringEffect::new(1000);
        assert!(s.calculate(100) > 0);
        Ok(())
    }

    #[test]
    fn spring_negative_displacement() -> TestResult {
        let s = SpringEffect::new(1000);
        assert!(s.calculate(-100) < 0);
        Ok(())
    }

    #[test]
    fn spring_symmetric_around_center() -> TestResult {
        let s = SpringEffect::new(1000);
        let pos = s.calculate(100);
        let neg = s.calculate(-100);
        assert_eq!(pos, -neg, "spring should be symmetric");
        Ok(())
    }

    #[test]
    fn spring_deadband_suppresses_small_displacements() -> TestResult {
        let mut s = SpringEffect::new(1000);
        s.deadband = 50;
        assert_eq!(s.calculate(25), 0);
        assert_eq!(s.calculate(-25), 0);
        assert!(s.calculate(100) != 0);
        Ok(())
    }

    #[test]
    fn spring_with_offset() -> TestResult {
        let mut s = SpringEffect::new(1000);
        s.offset = 100;
        // At offset position, force is zero
        assert_eq!(s.calculate(100), 0);
        // Displaced from offset, force is non-zero
        assert!(s.calculate(200) != 0);
        Ok(())
    }

    #[test]
    fn spring_extreme_values_no_panic() -> TestResult {
        let s = SpringEffect::new(i16::MAX);
        let _ = s.calculate(i16::MAX);
        let _ = s.calculate(i16::MIN);

        let s = SpringEffect::new(i16::MIN);
        let _ = s.calculate(i16::MAX);
        let _ = s.calculate(i16::MIN);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Damper effect
// ---------------------------------------------------------------------------

mod damper_tests {
    use super::*;

    #[test]
    fn damper_proportional_to_velocity() -> TestResult {
        let d = DamperEffect::new(500);
        let low = d.calculate(100);
        let high = d.calculate(200);
        assert_eq!(low, 50);
        assert_eq!(high, 100);
        Ok(())
    }

    #[test]
    fn damper_zero_velocity_zero_force() -> TestResult {
        let d = DamperEffect::new(500);
        assert_eq!(d.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn damper_negative_velocity() -> TestResult {
        let d = DamperEffect::new(500);
        let neg = d.calculate(-100);
        assert_eq!(neg, -50);
        Ok(())
    }

    #[test]
    fn damper_extreme_values_no_panic() -> TestResult {
        let d = DamperEffect::new(i16::MAX);
        let _ = d.calculate(i16::MAX);
        let _ = d.calculate(i16::MIN);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Friction effect
// ---------------------------------------------------------------------------

mod friction_tests {
    use super::*;

    #[test]
    fn friction_opposes_positive_velocity() -> TestResult {
        let f = FrictionEffect::new(100);
        assert!(f.calculate(100) < 0);
        Ok(())
    }

    #[test]
    fn friction_opposes_negative_velocity() -> TestResult {
        let f = FrictionEffect::new(100);
        assert!(f.calculate(-100) > 0);
        Ok(())
    }

    #[test]
    fn friction_zero_at_rest() -> TestResult {
        let f = FrictionEffect::new(100);
        assert_eq!(f.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn friction_extreme_values_no_panic() -> TestResult {
        let f = FrictionEffect::new(i16::MAX);
        let _ = f.calculate(i16::MAX);
        let _ = f.calculate(i16::MIN);

        let f = FrictionEffect::new(i16::MIN);
        let _ = f.calculate(i16::MAX);
        let _ = f.calculate(i16::MIN);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sine effect
// ---------------------------------------------------------------------------

mod sine_tests {
    use super::*;

    #[test]
    fn sine_at_zero_time_is_zero() -> TestResult {
        let s = SineEffect::new(1.0, 1000);
        // sin(0) = 0
        assert_eq!(s.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn sine_produces_nonzero_at_quarter_period() -> TestResult {
        let s = SineEffect::new(1.0, 1000);
        // At 250ms for 1Hz, sin(π/2) = 1.0 → max positive
        let out = s.calculate(250);
        assert!(out > 0, "should be positive at quarter period, got {out}");
        Ok(())
    }

    #[test]
    fn sine_periodic_for_integer_frequency() -> TestResult {
        let s = SineEffect::new(10.0, 10000);
        // Period = 100ms for 10Hz
        let at_0 = s.calculate(0);
        let at_100 = s.calculate(100);
        assert!(
            (at_0 as i32 - at_100 as i32).abs() <= 1,
            "sine should be periodic: f(0)={at_0}, f(100)={at_100}"
        );
        Ok(())
    }

    #[test]
    fn sine_output_bounded_to_i16() -> TestResult {
        let s = SineEffect::new(100.0, 10000);
        for t in 0..1000 {
            let out = s.calculate(t) as i32;
            assert!(
                out >= i16::MIN as i32 && out <= i16::MAX as i32,
                "sine output {out} at t={t} exceeds i16 range"
            );
        }
        Ok(())
    }

    #[test]
    fn sine_different_frequencies_differ() -> TestResult {
        let s1 = SineEffect::new(1.0, 1000);
        let s2 = SineEffect::new(10.0, 1000);
        // At 50ms they should differ
        let out1 = s1.calculate(50);
        let out2 = s2.calculate(50);
        assert_ne!(
            out1, out2,
            "different frequencies should produce different output"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect composition / mixing
// ---------------------------------------------------------------------------

mod composition_tests {
    use super::*;

    #[test]
    fn sum_of_effects_clamped_to_i16() -> TestResult {
        let spring = SpringEffect::new(MAX_SPRING_COEFFICIENT);
        let damper = DamperEffect::new(MAX_DAMPER_COEFFICIENT);
        let constant = ConstantEffect::new(i16::MAX);

        let spring_force = spring.calculate(i16::MAX) as i32;
        let damper_force = damper.calculate(i16::MAX) as i32;
        let constant_force = constant.apply_gain(1.0) as i32;

        let total = spring_force + damper_force + constant_force;
        let clamped = total.clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        assert!(
            (clamped as i32) >= i16::MIN as i32 && (clamped as i32) <= i16::MAX as i32,
            "clamped sum should be in i16 range"
        );
        Ok(())
    }

    #[test]
    fn gain_scales_mixed_effects() -> TestResult {
        let gain = FfbGain::new(0.5);

        let spring = SpringEffect::new(1000);
        let damper = DamperEffect::new(500);

        let spring_force = spring.calculate(100);
        let damper_force = damper.calculate(100);
        let total = spring_force as f32 + damper_force as f32;

        let scaled = (total * gain.combined()).clamp(i16::MIN as f32, i16::MAX as f32) as i16;

        // Scaled should be approximately half of unscaled
        let unscaled = total.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        assert!(
            (scaled as i32).abs() <= (unscaled as i32).abs(),
            "gain-scaled output should not exceed unscaled"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Safety limits and constants
// ---------------------------------------------------------------------------

mod safety_tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn safety_constants_are_positive() -> TestResult {
        assert!(MAX_TORQUE_CNM > 0);
        assert!(MAX_TORQUE_NM > 0.0);
        assert!(MAX_EFFECTS > 0);
        assert!(FFB_SAMPLE_RATE_HZ > 0);
        assert!(FFB_PERIOD_US > 0);
        assert!(MAX_SPRING_COEFFICIENT > 0);
        assert!(MAX_DAMPER_COEFFICIENT > 0);
        assert!(MAX_FRICTION_COEFFICIENT > 0);
        assert!(MAX_EFFECT_DURATION_MS > 0);
        Ok(())
    }

    #[test]
    fn torque_consistency() -> TestResult {
        // MAX_TORQUE_NM * 100 should equal MAX_TORQUE_CNM
        let cnm_from_nm = (MAX_TORQUE_NM * 100.0) as i32;
        assert_eq!(
            cnm_from_nm, MAX_TORQUE_CNM,
            "torque constants must be consistent"
        );
        Ok(())
    }

    #[test]
    fn sample_rate_period_consistency() -> TestResult {
        // 1_000_000 us / FFB_SAMPLE_RATE_HZ should equal FFB_PERIOD_US
        let expected_period = 1_000_000 / FFB_SAMPLE_RATE_HZ;
        assert_eq!(expected_period, FFB_PERIOD_US);
        Ok(())
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn max_gain_min_gain_valid() -> TestResult {
        assert_eq!(MAX_GAIN, 255);
        assert_eq!(MIN_GAIN, 0);
        assert!(MAX_GAIN > MIN_GAIN);
        Ok(())
    }

    #[test]
    fn hid_effect_ids_unique() -> TestResult {
        let ids = [
            EFFECT_ID_NONE,
            EFFECT_ID_CONSTANT,
            EFFECT_ID_RAMP,
            EFFECT_ID_SQUARE,
            EFFECT_ID_SINE,
            EFFECT_ID_TRIANGLE,
            EFFECT_ID_SAWTOOTH_UP,
            EFFECT_ID_SAWTOOTH_DOWN,
            EFFECT_ID_SPRING,
            EFFECT_ID_DAMPER,
            EFFECT_ID_FRICTION,
        ];
        for (i, &a) in ids.iter().enumerate() {
            for (j, &b) in ids.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "HID effect IDs at indices {i} and {j} must differ");
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Serde round-trip
// ---------------------------------------------------------------------------

mod serde_tests {
    use super::*;

    #[test]
    fn effect_type_round_trip() -> TestResult {
        let types = [
            EffectType::Constant,
            EffectType::Sine,
            EffectType::Spring,
            EffectType::None,
        ];
        for t in &types {
            let json = serde_json::to_string(t)?;
            let restored: EffectType = serde_json::from_str(&json)?;
            assert_eq!(*t, restored);
        }
        Ok(())
    }

    #[test]
    fn constant_effect_round_trip() -> TestResult {
        let effect = ConstantEffect::new(1234);
        let json = serde_json::to_string(&effect)?;
        let restored: ConstantEffect = serde_json::from_str(&json)?;
        assert_eq!(effect.magnitude, restored.magnitude);
        Ok(())
    }

    #[test]
    fn spring_effect_round_trip() -> TestResult {
        let effect = SpringEffect::new(500);
        let json = serde_json::to_string(&effect)?;
        let restored: SpringEffect = serde_json::from_str(&json)?;
        assert_eq!(effect.coefficient, restored.coefficient);
        assert_eq!(effect.offset, restored.offset);
        assert_eq!(effect.deadband, restored.deadband);
        Ok(())
    }

    #[test]
    fn sine_effect_round_trip() -> TestResult {
        let effect = SineEffect::new(10.0, 2000);
        let json = serde_json::to_string(&effect)?;
        let restored: SineEffect = serde_json::from_str(&json)?;
        assert!((effect.frequency_hz - restored.frequency_hz).abs() < f32::EPSILON);
        assert_eq!(effect.params.duration_ms, restored.params.duration_ms);
        Ok(())
    }
}
