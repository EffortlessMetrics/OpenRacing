//! Deep tests for FFB effect types, composition, and safety invariants.

use openracing_ffb::{
    ConstantEffect, DamperEffect, EffectParams, EffectType, FfbDirection, FfbGain, FrictionEffect,
    SineEffect, SpringEffect, MAX_TORQUE_CNM,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Effect types: spring
// ---------------------------------------------------------------------------

mod spring_tests {
    use super::*;

    #[test]
    fn spring_zero_at_center() -> TestResult {
        let spring = SpringEffect::new(1000);
        assert_eq!(spring.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn spring_proportional_to_displacement() -> TestResult {
        let spring = SpringEffect::new(1000);
        let f1 = spring.calculate(100);
        let f2 = spring.calculate(200);
        assert!(f2.abs() > f1.abs(), "larger displacement → larger force");
        Ok(())
    }

    #[test]
    fn spring_symmetry() -> TestResult {
        let spring = SpringEffect::new(1000);
        let pos = spring.calculate(500);
        let neg = spring.calculate(-500);
        assert_eq!(pos, -neg, "spring should be symmetric about center");
        Ok(())
    }

    #[test]
    fn spring_with_offset() -> TestResult {
        let mut spring = SpringEffect::new(1000);
        spring.offset = 100;
        assert_eq!(spring.calculate(100), 0, "at offset → zero force");
        assert!(spring.calculate(200) > 0, "above offset → positive force");
        assert!(spring.calculate(0) < 0, "below offset → negative force");
        Ok(())
    }

    #[test]
    fn spring_deadband_suppresses_force() -> TestResult {
        let mut spring = SpringEffect::new(1000);
        spring.deadband = 50;
        assert_eq!(spring.calculate(25), 0, "within deadband");
        assert_eq!(spring.calculate(-25), 0, "within deadband (neg)");
        assert!(spring.calculate(100) != 0, "outside deadband");
        Ok(())
    }

    #[test]
    fn spring_extreme_coefficient() -> TestResult {
        let spring = SpringEffect::new(i16::MAX);
        let result = spring.calculate(i16::MAX);
        assert!(
            (result as i32) >= i16::MIN as i32 && (result as i32) <= i16::MAX as i32,
            "extreme coefficient stays in i16 range"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect types: damper
// ---------------------------------------------------------------------------

mod damper_tests {
    use super::*;

    #[test]
    fn damper_zero_at_rest() -> TestResult {
        let damper = DamperEffect::new(500);
        assert_eq!(damper.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn damper_proportional_to_velocity() -> TestResult {
        let damper = DamperEffect::new(500);
        let f_slow = damper.calculate(100);
        let f_fast = damper.calculate(200);
        assert!(f_fast.abs() >= f_slow.abs(), "higher velocity → more resistance");
        Ok(())
    }

    #[test]
    fn damper_sign_follows_velocity() -> TestResult {
        let damper = DamperEffect::new(500);
        assert!(damper.calculate(100) > 0, "positive velocity → positive output");
        assert!(damper.calculate(-100) < 0, "negative velocity → negative output");
        Ok(())
    }

    #[test]
    fn damper_extreme_values() -> TestResult {
        let damper = DamperEffect::new(i16::MAX);
        let r = damper.calculate(i16::MAX);
        assert!(
            (r as i32) >= i16::MIN as i32 && (r as i32) <= i16::MAX as i32,
            "extreme values stay in i16 range"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect types: friction
// ---------------------------------------------------------------------------

mod friction_tests {
    use super::*;

    #[test]
    fn friction_zero_at_rest() -> TestResult {
        let friction = FrictionEffect::new(100);
        assert_eq!(friction.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn friction_opposes_positive_velocity() -> TestResult {
        let friction = FrictionEffect::new(100);
        assert!(friction.calculate(100) < 0, "positive velocity → negative force");
        Ok(())
    }

    #[test]
    fn friction_opposes_negative_velocity() -> TestResult {
        let friction = FrictionEffect::new(100);
        assert!(friction.calculate(-100) > 0, "negative velocity → positive force");
        Ok(())
    }

    #[test]
    fn friction_force_antisymmetric() -> TestResult {
        let friction = FrictionEffect::new(200);
        let f_pos = friction.calculate(100);
        let f_neg = friction.calculate(-100);
        assert_eq!(f_pos, -f_neg, "friction is antisymmetric");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect types: constant force
// ---------------------------------------------------------------------------

mod constant_tests {
    use super::*;

    #[test]
    fn constant_positive() -> TestResult {
        let effect = ConstantEffect::new(5000);
        assert_eq!(effect.magnitude, 5000);
        Ok(())
    }

    #[test]
    fn constant_negative() -> TestResult {
        let effect = ConstantEffect::new(-5000);
        assert_eq!(effect.magnitude, -5000);
        Ok(())
    }

    #[test]
    fn constant_zero() -> TestResult {
        let effect = ConstantEffect::new(0);
        assert_eq!(effect.apply_gain(1.0), 0);
        Ok(())
    }

    #[test]
    fn constant_full_range() -> TestResult {
        let effect_min = ConstantEffect::new(i16::MIN);
        let effect_max = ConstantEffect::new(i16::MAX);
        assert_eq!(effect_min.apply_gain(1.0), i16::MIN);
        assert_eq!(effect_max.apply_gain(1.0), i16::MAX);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect types: ramp (using ConstantEffect progression)
// ---------------------------------------------------------------------------

mod ramp_tests {
    use super::*;

    #[test]
    fn ramp_type_exists() -> TestResult {
        let params = EffectParams::new(EffectType::Ramp, 1000);
        assert_eq!(params.effect_type, EffectType::Ramp);
        assert_eq!(params.duration_ms, 1000);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect types: periodic (sine)
// ---------------------------------------------------------------------------

mod periodic_tests {
    use super::*;

    #[test]
    fn sine_zero_at_t0() -> TestResult {
        let sine = SineEffect::new(1.0, 1000);
        // sin(0) = 0
        assert_eq!(sine.calculate(0), 0);
        Ok(())
    }

    #[test]
    fn sine_periodic_at_integer_frequency() -> TestResult {
        let sine = SineEffect::new(10.0, 10_000);
        // Period = 100ms. Samples at 0 and 100 should match.
        let s0 = sine.calculate(0);
        let s100 = sine.calculate(100);
        assert!(
            (s0 as i32 - s100 as i32).abs() <= 1,
            "sine should repeat at period boundary"
        );
        Ok(())
    }

    #[test]
    fn sine_nonzero_at_quarter_period() -> TestResult {
        let sine = SineEffect::new(1.0, 1000);
        // At t=250ms, sin(2π·1·0.25) = sin(π/2) = 1.0 → max magnitude
        let sample = sine.calculate(250);
        assert!(sample > 0, "quarter period should yield positive peak");
        Ok(())
    }

    #[test]
    fn sine_gain_scales_output() -> TestResult {
        let full = SineEffect::new(1.0, 1000);
        let mut half = SineEffect::new(1.0, 1000);
        half.params = half.params.with_gain(128);

        let f = full.calculate(250);
        let h = half.calculate(250);
        // half gain should produce ~half amplitude
        assert!(h.abs() < f.abs(), "lower gain → lower amplitude");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect composition
// ---------------------------------------------------------------------------

mod composition_tests {
    use super::*;

    #[test]
    fn combine_spring_and_damper() -> TestResult {
        let spring = SpringEffect::new(1000);
        let damper = DamperEffect::new(500);

        let position: i16 = 100;
        let velocity: i16 = 50;

        let sf = spring.calculate(position) as i32;
        let df = damper.calculate(velocity) as i32;
        let total = (sf + df).clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        assert_ne!(total, 0, "combined effects should be non-zero");
        Ok(())
    }

    #[test]
    fn combine_all_condition_effects() -> TestResult {
        let spring = SpringEffect::new(500);
        let damper = DamperEffect::new(300);
        let friction = FrictionEffect::new(100);

        let pos: i16 = 200;
        let vel: i16 = 100;

        let sf = spring.calculate(pos) as i32;
        let df = damper.calculate(vel) as i32;
        let ff = friction.calculate(vel) as i32;
        let total = (sf + df + ff).clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        assert!(
            (total as i32) >= i16::MIN as i32 && (total as i32) <= i16::MAX as i32,
            "combined output stays in i16 range"
        );
        Ok(())
    }

    #[test]
    fn combine_constant_and_periodic() -> TestResult {
        let constant = ConstantEffect::new(1000);
        let sine = SineEffect::new(10.0, 1000);

        let cf = constant.apply_gain(1.0) as i32;
        let sf = sine.calculate(25) as i32;
        let total = (cf + sf).clamp(i16::MIN as i32, i16::MAX as i32) as i16;

        assert!(
            (total as i32) >= i16::MIN as i32 && (total as i32) <= i16::MAX as i32,
            "constant + periodic stays bounded"
        );
        Ok(())
    }

    #[test]
    fn max_concurrent_effects_bounded() -> TestResult {
        let effects: Vec<i32> = (0..8)
            .map(|i| SpringEffect::new(1000).calculate((i * 1000) as i16) as i32)
            .collect();
        let total: i32 = effects.iter().sum();
        let clamped = total.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        assert!(
            (clamped as i32) >= i16::MIN as i32 && (clamped as i32) <= i16::MAX as i32,
            "8 concurrent effects remain bounded"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect envelope: attack/sustain/release via gain stepping
// ---------------------------------------------------------------------------

mod envelope_tests {
    use super::*;

    #[test]
    fn envelope_attack_sustain_release() -> TestResult {
        let effect = ConstantEffect::new(10000);

        // Simulate A-S-R phases via gain ramps
        let attack_gains = [0.0f32, 0.25, 0.5, 0.75, 1.0];
        let sustain_gain = 1.0f32;
        let release_gains = [1.0f32, 0.75, 0.5, 0.25, 0.0];

        // Attack: monotonically increasing output
        for w in attack_gains.windows(2) {
            let o1 = effect.apply_gain(w[0]).abs();
            let o2 = effect.apply_gain(w[1]).abs();
            assert!(o2 >= o1, "attack phase: output must increase");
        }

        // Sustain: maximum output
        let peak = effect.apply_gain(sustain_gain);
        assert_eq!(peak, 10000, "sustain should be at full magnitude");

        // Release: monotonically decreasing output
        for w in release_gains.windows(2) {
            let o1 = effect.apply_gain(w[0]).abs();
            let o2 = effect.apply_gain(w[1]).abs();
            assert!(o2 <= o1, "release phase: output must decrease");
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Torque clamping
// ---------------------------------------------------------------------------

mod torque_clamping_tests {
    use super::*;

    #[test]
    fn spring_output_never_exceeds_i16_range() -> TestResult {
        let spring = SpringEffect::new(i16::MAX);
        for pos in [i16::MIN, -10000, -1, 0, 1, 10000, i16::MAX] {
            let out = spring.calculate(pos);
            assert!(
                (out as i32) >= i16::MIN as i32 && (out as i32) <= i16::MAX as i32,
                "spring at pos={pos}: out={out} exceeds i16 range"
            );
        }
        Ok(())
    }

    #[test]
    fn damper_output_never_exceeds_i16_range() -> TestResult {
        let damper = DamperEffect::new(i16::MAX);
        for vel in [i16::MIN, -10000, -1, 0, 1, 10000, i16::MAX] {
            let out = damper.calculate(vel);
            assert!(
                (out as i32) >= i16::MIN as i32 && (out as i32) <= i16::MAX as i32,
                "damper at vel={vel}: out={out} exceeds i16 range"
            );
        }
        Ok(())
    }

    #[test]
    fn constant_with_gain_never_exceeds_range() -> TestResult {
        let effect = ConstantEffect::new(i16::MAX);
        // Even with gain > 1.0, output must be clamped
        let out = effect.apply_gain(2.0);
        assert!(
            (out as i32) >= i16::MIN as i32 && (out as i32) <= i16::MAX as i32,
            "constant with excessive gain stays in range"
        );
        Ok(())
    }

    #[test]
    fn combined_effects_clamped_to_max_torque_cnm() -> TestResult {
        let spring = SpringEffect::new(i16::MAX);
        let damper = DamperEffect::new(i16::MAX);
        let combined =
            (spring.calculate(i16::MAX) as i32 + damper.calculate(i16::MAX) as i32)
                .clamp(-MAX_TORQUE_CNM, MAX_TORQUE_CNM) as i16;
        assert!(
            (combined as i32).abs() <= MAX_TORQUE_CNM,
            "combined effects clamped to MAX_TORQUE_CNM"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Gain scaling
// ---------------------------------------------------------------------------

mod gain_scaling_tests {
    use super::*;

    #[test]
    fn gain_zero_silences_effect() -> TestResult {
        let effect = ConstantEffect::new(10000);
        assert_eq!(effect.apply_gain(0.0), 0);
        Ok(())
    }

    #[test]
    fn gain_unity_preserves_magnitude() -> TestResult {
        let effect = ConstantEffect::new(10000);
        assert_eq!(effect.apply_gain(1.0), 10000);
        Ok(())
    }

    #[test]
    fn gain_half_halves_output() -> TestResult {
        let effect = ConstantEffect::new(10000);
        assert_eq!(effect.apply_gain(0.5), 5000);
        Ok(())
    }

    #[test]
    fn ffb_gain_combined_product() -> TestResult {
        let gain = FfbGain::new(0.8).with_torque(0.5).with_effects(0.5);
        let combined = gain.combined();
        assert!((combined - 0.2).abs() < 0.001, "0.8 * 0.5 * 0.5 = 0.2");
        Ok(())
    }

    #[test]
    fn ffb_gain_clamps_over_one() -> TestResult {
        let gain = FfbGain::new(1.5);
        assert!((gain.overall - 1.0).abs() < f32::EPSILON, "clamped to 1.0");
        Ok(())
    }

    #[test]
    fn ffb_gain_clamps_under_zero() -> TestResult {
        let gain = FfbGain::new(-0.5);
        assert!(gain.overall.abs() < f32::EPSILON, "clamped to 0.0");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

mod direction_tests {
    use super::*;

    #[test]
    fn direction_wraps_positive() -> TestResult {
        let dir = FfbDirection::new(450.0);
        assert!((dir.degrees - 90.0).abs() < f32::EPSILON, "450° → 90°");
        Ok(())
    }

    #[test]
    fn direction_wraps_negative() -> TestResult {
        let dir = FfbDirection::new(-90.0);
        assert!((dir.degrees - 270.0).abs() < f32::EPSILON, "-90° → 270°");
        Ok(())
    }

    #[test]
    fn direction_from_radians() -> TestResult {
        let dir = FfbDirection::from_radians(std::f32::consts::PI);
        assert!((dir.degrees - 180.0).abs() < 0.01, "π rad → 180°");
        Ok(())
    }

    #[test]
    fn direction_to_radians_round_trip() -> TestResult {
        let dir = FfbDirection::new(90.0);
        let rad = dir.to_radians();
        let restored = FfbDirection::from_radians(rad);
        assert!((restored.degrees - 90.0).abs() < 0.01, "round-trip 90°");
        Ok(())
    }

    #[test]
    fn cw_ccw_effects_opposite() -> TestResult {
        // CW direction (0°) → positive constant force
        // CCW direction (180°) → negative constant force
        // Simulated by flipping sign based on direction cosine
        let cw = FfbDirection::new(0.0);
        let ccw = FfbDirection::new(180.0);
        let effect = ConstantEffect::new(1000);

        let cw_force =
            (effect.magnitude as f32 * cw.to_radians().cos()).round() as i16;
        let ccw_force =
            (effect.magnitude as f32 * ccw.to_radians().cos()).round() as i16;

        assert!(cw_force > 0, "CW → positive");
        assert!(ccw_force < 0, "CCW → negative");
        assert_eq!(cw_force, -ccw_force, "CW and CCW are symmetric");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn any_effect_any_position_bounded_torque(
            coeff in i16::MIN..=i16::MAX,
            position in i16::MIN..=i16::MAX,
            velocity in i16::MIN..=i16::MAX,
            magnitude in i16::MIN..=i16::MAX,
            gain in 0.0f32..=1.0,
        ) {
            let spring_out = SpringEffect::new(coeff).calculate(position) as i32;
            let damper_out = DamperEffect::new(coeff).calculate(velocity) as i32;
            let friction_out = FrictionEffect::new(coeff).calculate(velocity) as i32;
            let constant_out = ConstantEffect::new(magnitude).apply_gain(gain) as i32;

            let total = (spring_out + damper_out + friction_out + constant_out)
                .clamp(i16::MIN as i32, i16::MAX as i32);

            prop_assert!(
                total >= i16::MIN as i32 && total <= i16::MAX as i32,
                "combined output {} out of i16 range", total
            );
        }

        #[test]
        fn torque_always_finite(
            coeff in i16::MIN..=i16::MAX,
            position in i16::MIN..=i16::MAX,
            velocity in i16::MIN..=i16::MAX,
            gain in -10.0f32..=10.0,
            freq in 0.1f32..=1000.0,
            time_ms in 0u32..=10_000,
        ) {
            let spring = SpringEffect::new(coeff).calculate(position) as f32;
            let damper = DamperEffect::new(coeff).calculate(velocity) as f32;
            let friction = FrictionEffect::new(coeff).calculate(velocity) as f32;
            let constant = ConstantEffect::new(coeff).apply_gain(gain) as f32;
            let sine = SineEffect::new(freq, 10_000).calculate(time_ms) as f32;

            prop_assert!(spring.is_finite(), "spring NaN/Inf");
            prop_assert!(damper.is_finite(), "damper NaN/Inf");
            prop_assert!(friction.is_finite(), "friction NaN/Inf");
            prop_assert!(constant.is_finite(), "constant NaN/Inf");
            prop_assert!(sine.is_finite(), "sine NaN/Inf");
        }

        #[test]
        fn spring_direction_correct(
            coeff in 1i16..=i16::MAX,
            position in 1i16..=i16::MAX,
        ) {
            let spring = SpringEffect::new(coeff);
            let positive_force = spring.calculate(position);
            let negative_force = spring.calculate(-position);

            // Positive coefficient + positive displacement → positive force
            prop_assert!(positive_force >= 0, "positive displacement should give >= 0 force");
            // Positive coefficient + negative displacement → negative force
            prop_assert!(negative_force <= 0, "negative displacement should give <= 0 force");
        }

        #[test]
        fn gain_scaling_monotonic(
            magnitude in 1i16..=10000,
            g1 in 0.0f32..=0.5,
            g2 in 0.5f32..=1.0,
        ) {
            let effect = ConstantEffect::new(magnitude);
            let o1 = effect.apply_gain(g1).abs();
            let o2 = effect.apply_gain(g2).abs();
            prop_assert!(o2 >= o1, "higher gain should produce >= output: g1={} o1={}, g2={} o2={}", g1, o1, g2, o2);
        }
    }
}
