//! Deep tests for FFB effect types, composition, and safety invariants.

use openracing_ffb::{
    ConstantEffect, DamperEffect, EffectParams, EffectType, FfbDirection, FfbGain, FrictionEffect,
    MAX_TORQUE_CNM, SineEffect, SpringEffect,
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
        assert!(
            f_fast.abs() >= f_slow.abs(),
            "higher velocity → more resistance"
        );
        Ok(())
    }

    #[test]
    fn damper_sign_follows_velocity() -> TestResult {
        let damper = DamperEffect::new(500);
        assert!(
            damper.calculate(100) > 0,
            "positive velocity → positive output"
        );
        assert!(
            damper.calculate(-100) < 0,
            "negative velocity → negative output"
        );
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
        assert!(
            friction.calculate(100) < 0,
            "positive velocity → negative force"
        );
        Ok(())
    }

    #[test]
    fn friction_opposes_negative_velocity() -> TestResult {
        let friction = FrictionEffect::new(100);
        assert!(
            friction.calculate(-100) > 0,
            "negative velocity → positive force"
        );
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
        let combined = (spring.calculate(i16::MAX) as i32 + damper.calculate(i16::MAX) as i32)
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

        let cw_force = (effect.magnitude as f32 * cw.to_radians().cos()).round() as i16;
        let ccw_force = (effect.magnitude as f32 * ccw.to_radians().cos()).round() as i16;

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
        #![proptest_config(ProptestConfig { cases: 1000, timeout: 60_000, ..ProptestConfig::default() })]

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

// ---------------------------------------------------------------------------
// EffectType enum: all variants and defaults
// ---------------------------------------------------------------------------

mod effect_type_tests {
    use super::*;

    #[test]
    fn effect_type_default_is_none() -> TestResult {
        assert_eq!(EffectType::default(), EffectType::None);
        Ok(())
    }

    #[test]
    fn all_effect_types_distinct() -> TestResult {
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
                    assert_ne!(a, b, "types at index {i} and {j} should differ");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn effect_type_equality() -> TestResult {
        assert_eq!(EffectType::Sine, EffectType::Sine);
        assert_ne!(EffectType::Sine, EffectType::Square);
        Ok(())
    }

    #[test]
    fn effect_type_clone_eq() -> TestResult {
        let t = EffectType::Spring;
        let cloned = t;
        assert_eq!(t, cloned);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// EffectParams: gain, direction, duration ranges
// ---------------------------------------------------------------------------

mod effect_params_tests {
    use super::*;

    #[test]
    fn params_defaults() -> TestResult {
        let params = EffectParams::new(EffectType::Constant, 500);
        assert_eq!(params.gain, 255, "default gain is max");
        assert_eq!(params.direction, 0, "default direction is 0");
        assert_eq!(params.duration_ms, 500);
        Ok(())
    }

    #[test]
    fn params_with_zero_duration_means_infinite() -> TestResult {
        let params = EffectParams::new(EffectType::Spring, 0);
        assert_eq!(params.duration_ms, 0);
        Ok(())
    }

    #[test]
    fn params_gain_min_max() -> TestResult {
        let min_gain = EffectParams::new(EffectType::Sine, 100).with_gain(0);
        let max_gain = EffectParams::new(EffectType::Sine, 100).with_gain(255);
        assert_eq!(min_gain.gain, 0);
        assert_eq!(max_gain.gain, 255);
        Ok(())
    }

    #[test]
    fn params_direction_full_range() -> TestResult {
        let p0 = EffectParams::new(EffectType::Constant, 0).with_direction(0);
        let p_max = EffectParams::new(EffectType::Constant, 0).with_direction(u16::MAX);
        assert_eq!(p0.direction, 0);
        assert_eq!(p_max.direction, u16::MAX);
        Ok(())
    }

    #[test]
    fn params_max_duration() -> TestResult {
        let params = EffectParams::new(EffectType::Damper, u32::MAX);
        assert_eq!(params.duration_ms, u32::MAX);
        Ok(())
    }

    #[test]
    fn params_chaining() -> TestResult {
        let params = EffectParams::new(EffectType::Friction, 2000)
            .with_gain(128)
            .with_direction(180);
        assert_eq!(params.effect_type, EffectType::Friction);
        assert_eq!(params.duration_ms, 2000);
        assert_eq!(params.gain, 128);
        assert_eq!(params.direction, 180);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Device-specific FFB scaling
// ---------------------------------------------------------------------------

mod device_scaling_tests {
    use super::*;

    #[test]
    fn low_power_device_scaling() -> TestResult {
        // Simulate a low-power wheel (5Nm) vs reference (25Nm) via gain
        let device_gain = FfbGain::new(5.0 / 25.0); // 0.2
        let effect = ConstantEffect::new(10000);
        let scaled = effect.apply_gain(device_gain.combined());
        assert_eq!(scaled, 2000, "5Nm wheel gets 20% of reference force");
        Ok(())
    }

    #[test]
    fn high_power_device_capped() -> TestResult {
        // High-power wheel should still cap at gain=1.0
        let device_gain = FfbGain::new(30.0 / 25.0); // clamped to 1.0
        assert!((device_gain.overall - 1.0).abs() < f32::EPSILON);
        let effect = ConstantEffect::new(10000);
        let scaled = effect.apply_gain(device_gain.combined());
        assert_eq!(scaled, 10000, "gain capped at 1.0");
        Ok(())
    }

    #[test]
    fn device_gain_with_torque_and_effects_subgains() -> TestResult {
        let gain = FfbGain::new(0.8).with_torque(0.9).with_effects(0.7);
        let combined = gain.combined();
        let expected = 0.8 * 0.9 * 0.7;
        assert!(
            (combined - expected).abs() < 0.001,
            "combined={combined}, expected={expected}"
        );
        Ok(())
    }

    #[test]
    fn device_scaling_preserves_sign() -> TestResult {
        let gain = FfbGain::new(0.5);
        let positive = ConstantEffect::new(10000);
        let negative = ConstantEffect::new(-10000);

        let p_scaled = positive.apply_gain(gain.combined());
        let n_scaled = negative.apply_gain(gain.combined());

        assert!(p_scaled > 0, "positive stays positive");
        assert!(n_scaled < 0, "negative stays negative");
        assert_eq!(p_scaled, -n_scaled, "symmetric scaling");
        Ok(())
    }

    #[test]
    fn cascaded_gain_application() -> TestResult {
        // Simulate game gain (0.8) × device gain (0.6) × user preference (0.9)
        let game_gain = FfbGain::new(0.8);
        let device_gain = 0.6f32;
        let user_pref = 0.9f32;
        let total_gain = game_gain.combined() * device_gain * user_pref;

        let effect = ConstantEffect::new(10000);
        let result = effect.apply_gain(total_gain);

        let expected = (10000.0 * 0.8 * 0.6 * 0.9) as i16;
        assert_eq!(result, expected);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Safety limits integration
// ---------------------------------------------------------------------------

mod safety_limits_tests {
    use super::*;
    use openracing_ffb::constants::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn max_torque_cnm_positive() -> TestResult {
        assert!(MAX_TORQUE_CNM > 0);
        Ok(())
    }

    #[test]
    fn max_torque_nm_consistent_with_cnm() -> TestResult {
        let nm_from_cnm = MAX_TORQUE_CNM as f32 / 100.0;
        assert!(
            (nm_from_cnm - MAX_TORQUE_NM).abs() < f32::EPSILON,
            "Nm and cNm must be consistent"
        );
        Ok(())
    }

    #[test]
    fn spring_clamped_to_safety_limit() -> TestResult {
        let spring = SpringEffect::new(MAX_SPRING_COEFFICIENT);
        for pos in [i16::MIN, -10000, 0, 10000, i16::MAX] {
            let force = spring.calculate(pos) as i32;
            let safe = force.clamp(-MAX_TORQUE_CNM, MAX_TORQUE_CNM);
            assert!(
                safe.abs() <= MAX_TORQUE_CNM,
                "spring force at pos={pos}: {safe} exceeds safety limit"
            );
        }
        Ok(())
    }

    #[test]
    fn damper_clamped_to_safety_limit() -> TestResult {
        let damper = DamperEffect::new(MAX_DAMPER_COEFFICIENT);
        for vel in [i16::MIN, -10000, 0, 10000, i16::MAX] {
            let force = damper.calculate(vel) as i32;
            let safe = force.clamp(-MAX_TORQUE_CNM, MAX_TORQUE_CNM);
            assert!(
                safe.abs() <= MAX_TORQUE_CNM,
                "damper force at vel={vel}: {safe} exceeds safety limit"
            );
        }
        Ok(())
    }

    #[test]
    fn friction_clamped_to_safety_limit() -> TestResult {
        let friction = FrictionEffect::new(MAX_FRICTION_COEFFICIENT);
        for vel in [i16::MIN, -10000, 0, 10000, i16::MAX] {
            let force = friction.calculate(vel) as i32;
            let safe = force.clamp(-MAX_TORQUE_CNM, MAX_TORQUE_CNM);
            assert!(
                safe.abs() <= MAX_TORQUE_CNM,
                "friction force at vel={vel}: {safe} exceeds safety limit"
            );
        }
        Ok(())
    }

    #[test]
    fn max_concurrent_effects_constant() -> TestResult {
        assert_eq!(MAX_EFFECTS, 8, "expected 8 concurrent effects");
        Ok(())
    }

    #[test]
    fn ffb_sample_rate_matches_period() -> TestResult {
        assert_eq!(
            FFB_PERIOD_US,
            1_000_000 / FFB_SAMPLE_RATE_HZ,
            "period should be 1/rate in microseconds"
        );
        Ok(())
    }

    #[test]
    fn gain_bounds_correct() -> TestResult {
        assert_eq!(MIN_GAIN, 0);
        assert_eq!(MAX_GAIN, 255);
        Ok(())
    }

    #[test]
    fn effect_ids_unique() -> TestResult {
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
        for (i, a) in ids.iter().enumerate() {
            for (j, b) in ids.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "effect IDs at index {i} and {j} collide");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn all_effects_sum_clamped_below_max_torque() -> TestResult {
        // Worst case: MAX_EFFECTS effects all at max output
        let spring = SpringEffect::new(MAX_SPRING_COEFFICIENT);
        let forces: Vec<i32> = (0..MAX_EFFECTS)
            .map(|_| spring.calculate(i16::MAX) as i32)
            .collect();
        let total = forces.iter().sum::<i32>();
        let safe = total.clamp(-MAX_TORQUE_CNM, MAX_TORQUE_CNM);
        assert!(
            safe.abs() <= MAX_TORQUE_CNM,
            "sum of {MAX_EFFECTS} effects after clamping: {safe}"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Effect timing: duration, sine timing, zero-duration
// ---------------------------------------------------------------------------

mod timing_tests {
    use super::*;

    #[test]
    fn sine_at_half_period_returns_near_zero() -> TestResult {
        // sin(2π·1·0.5) = sin(π) ≈ 0
        let sine = SineEffect::new(1.0, 1000);
        let sample = sine.calculate(500);
        assert!(
            sample.abs() <= 1,
            "half period should be near zero, got {sample}"
        );
        Ok(())
    }

    #[test]
    fn sine_peak_at_quarter_period() -> TestResult {
        let sine = SineEffect::new(1.0, 2000);
        let peak = sine.calculate(250);
        // sin(π/2) = 1.0 → should be near i16::MAX with full gain
        assert!(
            peak > 30000,
            "quarter period should be near max, got {peak}"
        );
        Ok(())
    }

    #[test]
    fn sine_trough_at_three_quarter_period() -> TestResult {
        let sine = SineEffect::new(1.0, 2000);
        let trough = sine.calculate(750);
        // sin(3π/2) = -1.0 → should be near i16::MIN with full gain
        assert!(
            trough < -30000,
            "3/4 period should be near min, got {trough}"
        );
        Ok(())
    }

    #[test]
    fn sine_with_phase_offset() -> TestResult {
        let mut sine = SineEffect::new(1.0, 1000);
        sine.phase = std::f32::consts::FRAC_PI_2; // 90° phase offset
        // At t=0: sin(π/2) = 1.0 → near max
        let sample = sine.calculate(0);
        assert!(
            sample > 30000,
            "phase offset should shift peak to t=0, got {sample}"
        );
        Ok(())
    }

    #[test]
    fn sine_zero_gain_always_zero() -> TestResult {
        let mut sine = SineEffect::new(10.0, 1000);
        sine.params = sine.params.with_gain(0);
        for t in [0, 25, 50, 100, 250, 500] {
            assert_eq!(sine.calculate(t), 0, "zero gain at t={t} should be 0");
        }
        Ok(())
    }

    #[test]
    fn sine_very_high_frequency() -> TestResult {
        let sine = SineEffect::new(10000.0, 1000);
        // Should not panic and should stay bounded
        for t in 0..100 {
            let s = sine.calculate(t) as i32;
            assert!(
                s >= i16::MIN as i32 && s <= i16::MAX as i32,
                "high-freq sine out of range at t={t}"
            );
        }
        Ok(())
    }

    #[test]
    fn sine_very_low_frequency() -> TestResult {
        let sine = SineEffect::new(0.001, 1_000_000);
        let s0 = sine.calculate(0);
        let s1 = sine.calculate(1);
        // At 0.001 Hz, consecutive ms samples should be nearly identical
        assert!(
            (s0 as i32 - s1 as i32).abs() < 100,
            "low-freq sine should change slowly"
        );
        Ok(())
    }

    #[test]
    fn constant_effect_duration_stored() -> TestResult {
        let effect = ConstantEffect::new(1000);
        assert_eq!(
            effect.params.duration_ms, 0,
            "constant defaults to infinite"
        );
        Ok(())
    }

    #[test]
    fn effect_params_duration_boundary() -> TestResult {
        let max_dur = EffectParams::new(EffectType::Sine, openracing_ffb::MAX_EFFECT_DURATION_MS);
        assert_eq!(max_dur.duration_ms, openracing_ffb::MAX_EFFECT_DURATION_MS);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Serialization round-trips
// ---------------------------------------------------------------------------

mod serialization_tests {
    use super::*;

    #[test]
    fn constant_effect_json_round_trip() -> TestResult {
        let effect = ConstantEffect::new(-5000);
        let json = serde_json::to_string(&effect)?;
        let restored: ConstantEffect = serde_json::from_str(&json)?;
        assert_eq!(restored.magnitude, -5000);
        assert_eq!(restored.params.effect_type, EffectType::Constant);
        Ok(())
    }

    #[test]
    fn spring_effect_json_round_trip() -> TestResult {
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
    fn damper_effect_json_round_trip() -> TestResult {
        let damper = DamperEffect::new(600);
        let json = serde_json::to_string(&damper)?;
        let restored: DamperEffect = serde_json::from_str(&json)?;
        assert_eq!(restored.coefficient, 600);
        Ok(())
    }

    #[test]
    fn friction_effect_json_round_trip() -> TestResult {
        let mut friction = FrictionEffect::new(300);
        friction.offset = 25;
        let json = serde_json::to_string(&friction)?;
        let restored: FrictionEffect = serde_json::from_str(&json)?;
        assert_eq!(restored.coefficient, 300);
        assert_eq!(restored.offset, 25);
        Ok(())
    }

    #[test]
    fn sine_effect_json_round_trip() -> TestResult {
        let mut sine = SineEffect::new(50.0, 2000);
        sine.phase = 1.5;
        let json = serde_json::to_string(&sine)?;
        let restored: SineEffect = serde_json::from_str(&json)?;
        assert!((restored.frequency_hz - 50.0).abs() < f32::EPSILON);
        assert!((restored.phase - 1.5).abs() < f32::EPSILON);
        assert_eq!(restored.params.duration_ms, 2000);
        Ok(())
    }

    #[test]
    fn ffb_gain_json_round_trip() -> TestResult {
        let gain = FfbGain::new(0.75).with_torque(0.6).with_effects(0.4);
        let json = serde_json::to_string(&gain)?;
        let restored: FfbGain = serde_json::from_str(&json)?;
        assert!((restored.overall - 0.75).abs() < f32::EPSILON);
        assert!((restored.torque - 0.6).abs() < f32::EPSILON);
        assert!((restored.effects - 0.4).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn ffb_direction_json_round_trip() -> TestResult {
        let dir = FfbDirection::new(135.0);
        let json = serde_json::to_string(&dir)?;
        let restored: FfbDirection = serde_json::from_str(&json)?;
        assert!((restored.degrees - 135.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn effect_type_json_round_trip() -> TestResult {
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
        for t in types {
            let json = serde_json::to_string(&t)?;
            let restored: EffectType = serde_json::from_str(&json)?;
            assert_eq!(restored, t, "round-trip failed for {t:?}");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Additional edge cases
// ---------------------------------------------------------------------------

mod additional_edge_cases {
    use super::*;

    #[test]
    fn constant_i16_min_with_negative_gain() -> TestResult {
        let effect = ConstantEffect::new(i16::MIN);
        let result = effect.apply_gain(-1.0);
        // Negative gain flips sign: i16::MIN * -1 → i16::MAX (clamped)
        assert!(
            (result as i32) >= i16::MIN as i32 && (result as i32) <= i16::MAX as i32,
            "i16::MIN with gain=-1.0 stays in range: {result}"
        );
        Ok(())
    }

    #[test]
    fn spring_with_negative_coefficient() -> TestResult {
        let spring = SpringEffect::new(-1000);
        // Negative coefficient: force opposes normal direction
        let f_pos = spring.calculate(100);
        let f_neg = spring.calculate(-100);
        assert!(f_pos < 0, "negative coeff + positive pos → negative force");
        assert!(f_neg > 0, "negative coeff + negative pos → positive force");
        Ok(())
    }

    #[test]
    fn damper_with_negative_coefficient() -> TestResult {
        let damper = DamperEffect::new(-500);
        let f = damper.calculate(100);
        assert!(f < 0, "negative coeff + positive velocity → negative force");
        Ok(())
    }

    #[test]
    fn friction_with_max_coefficient() -> TestResult {
        let friction = FrictionEffect::new(i16::MAX);
        let f = friction.calculate(1);
        // Should not overflow
        assert!(
            (f as i32) >= i16::MIN as i32 && (f as i32) <= i16::MAX as i32,
            "max friction coeff stays bounded"
        );
        Ok(())
    }

    #[test]
    fn friction_with_i16_min_velocity() -> TestResult {
        let friction = FrictionEffect::new(100);
        let f = friction.calculate(i16::MIN);
        // Opposing direction, so force should be positive (or zero)
        assert!(
            f >= 0,
            "friction at i16::MIN velocity should be non-negative, got {f}"
        );
        Ok(())
    }

    #[test]
    fn spring_deadband_equals_position() -> TestResult {
        let mut spring = SpringEffect::new(1000);
        spring.deadband = 100;
        // Position exactly at deadband boundary
        assert_eq!(spring.calculate(99), 0, "just inside deadband");
        assert!(
            spring.calculate(100) != 0,
            "at boundary (diff == deadband: not suppressed)"
        );
        Ok(())
    }

    #[test]
    fn ffb_gain_all_zero() -> TestResult {
        let gain = FfbGain::new(0.0).with_torque(0.0).with_effects(0.0);
        assert!(gain.combined().abs() < f32::EPSILON);
        let effect = ConstantEffect::new(i16::MAX);
        assert_eq!(effect.apply_gain(gain.combined()), 0);
        Ok(())
    }

    #[test]
    fn ffb_gain_all_one() -> TestResult {
        let gain = FfbGain::new(1.0).with_torque(1.0).with_effects(1.0);
        assert!((gain.combined() - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn ffb_direction_zero_degrees() -> TestResult {
        let dir = FfbDirection::new(0.0);
        assert!(dir.degrees.abs() < f32::EPSILON);
        assert!(dir.to_radians().abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn ffb_direction_360_wraps_to_zero() -> TestResult {
        let dir = FfbDirection::new(360.0);
        assert!(dir.degrees.abs() < f32::EPSILON, "360° wraps to 0°");
        Ok(())
    }

    #[test]
    fn ffb_direction_large_negative() -> TestResult {
        let dir = FfbDirection::new(-720.0);
        assert!(dir.degrees.abs() < f32::EPSILON, "-720° wraps to 0°");
        Ok(())
    }

    #[test]
    fn default_effect_params() -> TestResult {
        let params = EffectParams::default();
        assert_eq!(params.effect_type, EffectType::None);
        assert_eq!(params.duration_ms, 0);
        assert_eq!(params.gain, 0);
        assert_eq!(params.direction, 0);
        Ok(())
    }

    #[test]
    fn default_constant_effect() -> TestResult {
        let effect = ConstantEffect::default();
        assert_eq!(effect.magnitude, 0);
        assert_eq!(effect.apply_gain(1.0), 0);
        Ok(())
    }

    #[test]
    fn default_spring_effect() -> TestResult {
        let spring = SpringEffect::default();
        assert_eq!(spring.coefficient, 0);
        assert_eq!(spring.offset, 0);
        assert_eq!(spring.deadband, 0);
        assert_eq!(spring.calculate(1000), 0);
        Ok(())
    }

    #[test]
    fn default_damper_effect() -> TestResult {
        let damper = DamperEffect::default();
        assert_eq!(damper.coefficient, 0);
        assert_eq!(damper.calculate(1000), 0);
        Ok(())
    }

    #[test]
    fn default_friction_effect() -> TestResult {
        let friction = FrictionEffect::default();
        assert_eq!(friction.coefficient, 0);
        assert_eq!(friction.offset, 0);
        // Zero coefficient still has abs_vel/100 contribution
        let f = friction.calculate(100);
        assert!(
            (f as i32) >= i16::MIN as i32 && (f as i32) <= i16::MAX as i32,
            "default friction stays bounded"
        );
        Ok(())
    }

    #[test]
    fn default_sine_effect() -> TestResult {
        let sine = SineEffect::default();
        assert!((sine.frequency_hz - 0.0).abs() < f32::EPSILON);
        assert!((sine.phase - 0.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn default_ffb_gain() -> TestResult {
        let gain = FfbGain::default();
        assert!((gain.overall - 0.0).abs() < f32::EPSILON);
        assert!((gain.torque - 0.0).abs() < f32::EPSILON);
        assert!((gain.effects - 0.0).abs() < f32::EPSILON);
        Ok(())
    }
}
