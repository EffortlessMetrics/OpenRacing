#![allow(clippy::redundant_closure)]
//! Property-based tests for FFB types: torque values, effect parameters,
//! mode transitions, and serde roundtrips.

use openracing_ffb::{
    ConstantEffect, DamperEffect, EffectParams, EffectType, FfbDirection, FfbGain, FrictionEffect,
    SineEffect, SpringEffect,
};
use proptest::prelude::*;

// ── Strategies ──────────────────────────────────────────────────────────────

fn effect_type_strategy() -> impl Strategy<Value = EffectType> {
    prop_oneof![
        Just(EffectType::None),
        Just(EffectType::Constant),
        Just(EffectType::Ramp),
        Just(EffectType::Square),
        Just(EffectType::Sine),
        Just(EffectType::Triangle),
        Just(EffectType::SawtoothUp),
        Just(EffectType::SawtoothDown),
        Just(EffectType::Spring),
        Just(EffectType::Damper),
        Just(EffectType::Friction),
        Just(EffectType::Custom),
    ]
}

// ── Tests ───────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // === EffectType serde roundtrip ===

    #[test]
    fn prop_effect_type_serde_roundtrip(et in effect_type_strategy()) {
        let json = serde_json::to_string(&et);
        prop_assert!(json.is_ok(), "serialize failed: {:?}", json.err());
        let json = json.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: Result<EffectType, _> = serde_json::from_str(&json);
        prop_assert!(decoded.is_ok(), "deserialize failed: {:?}", decoded.err());
        prop_assert_eq!(et, decoded.map_err(|e| TestCaseError::Fail(format!("{e}").into()))?);
    }

    // === EffectParams serde roundtrip ===

    #[test]
    fn prop_effect_params_serde_roundtrip(
        et in effect_type_strategy(),
        duration_ms in any::<u32>(),
        gain in any::<u8>(),
        direction in any::<u16>(),
    ) {
        let params = EffectParams::new(et, duration_ms)
            .with_gain(gain)
            .with_direction(direction);
        let json = serde_json::to_string(&params)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: EffectParams = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(params.effect_type, decoded.effect_type);
        prop_assert_eq!(params.duration_ms, decoded.duration_ms);
        prop_assert_eq!(params.gain, decoded.gain);
        prop_assert_eq!(params.direction, decoded.direction);
    }

    // === ConstantEffect serde roundtrip ===

    #[test]
    fn prop_constant_effect_serde_roundtrip(magnitude in i16::MIN..=i16::MAX) {
        let effect = ConstantEffect::new(magnitude);
        let json = serde_json::to_string(&effect)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: ConstantEffect = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(effect.magnitude, decoded.magnitude);
    }

    // === SpringEffect serde roundtrip ===

    #[test]
    fn prop_spring_effect_serde_roundtrip(
        coefficient in i16::MIN..=i16::MAX,
        offset in i16::MIN..=i16::MAX,
        deadband in 0i16..=1000i16,
    ) {
        let mut spring = SpringEffect::new(coefficient);
        spring.offset = offset;
        spring.deadband = deadband;
        let json = serde_json::to_string(&spring)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: SpringEffect = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(spring.coefficient, decoded.coefficient);
        prop_assert_eq!(spring.offset, decoded.offset);
        prop_assert_eq!(spring.deadband, decoded.deadband);
    }

    // === DamperEffect serde roundtrip ===

    #[test]
    fn prop_damper_effect_serde_roundtrip(coefficient in i16::MIN..=i16::MAX) {
        let damper = DamperEffect::new(coefficient);
        let json = serde_json::to_string(&damper)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: DamperEffect = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(damper.coefficient, decoded.coefficient);
    }

    // === FrictionEffect serde roundtrip ===

    #[test]
    fn prop_friction_effect_serde_roundtrip(
        coefficient in i16::MIN..=i16::MAX,
        offset in i16::MIN..=i16::MAX,
    ) {
        let mut friction = FrictionEffect::new(coefficient);
        friction.offset = offset;
        let json = serde_json::to_string(&friction)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: FrictionEffect = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert_eq!(friction.coefficient, decoded.coefficient);
        prop_assert_eq!(friction.offset, decoded.offset);
    }

    // === SineEffect serde roundtrip ===

    #[test]
    fn prop_sine_effect_serde_roundtrip(
        freq in 0.1f32..=1000.0,
        duration_ms in 0u32..=10_000,
    ) {
        let sine = SineEffect::new(freq, duration_ms);
        let json = serde_json::to_string(&sine)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: SineEffect = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((sine.frequency_hz - decoded.frequency_hz).abs() < f32::EPSILON);
        prop_assert_eq!(sine.params.duration_ms, decoded.params.duration_ms);
    }

    // === FfbGain serde roundtrip ===

    #[test]
    fn prop_ffb_gain_serde_roundtrip(
        overall in 0.0f32..=1.0,
        torque in 0.0f32..=1.0,
        effects in 0.0f32..=1.0,
    ) {
        let gain = FfbGain::new(overall)
            .with_torque(torque)
            .with_effects(effects);
        let json = serde_json::to_string(&gain)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: FfbGain = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((gain.overall - decoded.overall).abs() < f32::EPSILON);
        prop_assert!((gain.torque - decoded.torque).abs() < f32::EPSILON);
        prop_assert!((gain.effects - decoded.effects).abs() < f32::EPSILON);
    }

    // === FfbDirection serde roundtrip ===

    #[test]
    fn prop_ffb_direction_serde_roundtrip(degrees in 0.0f32..360.0) {
        let dir = FfbDirection::new(degrees);
        let json = serde_json::to_string(&dir)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        let decoded: FfbDirection = serde_json::from_str(&json)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
        prop_assert!((dir.degrees - decoded.degrees).abs() < f32::EPSILON);
    }

    // === FfbDirection radians roundtrip ===

    #[test]
    fn prop_direction_radians_roundtrip(degrees in 0.0f32..360.0) {
        let dir = FfbDirection::new(degrees);
        let radians = dir.to_radians();
        let back = FfbDirection::from_radians(radians);
        // Tolerance for floating point
        prop_assert!(
            (dir.degrees - back.degrees).abs() < 0.01,
            "radians roundtrip: {} -> {} -> {}", dir.degrees, radians, back.degrees
        );
    }

    // === Torque: constant effect sign preservation ===

    #[test]
    fn prop_constant_effect_sign_preserved(
        magnitude in i16::MIN..=i16::MAX,
        gain in 0.01f32..=1.0,
    ) {
        let effect = ConstantEffect::new(magnitude);
        let output = effect.apply_gain(gain);
        if magnitude > 0 {
            prop_assert!(output >= 0, "positive magnitude {} with gain {} gave {}", magnitude, gain, output);
        } else if magnitude < 0 {
            prop_assert!(output <= 0, "negative magnitude {} with gain {} gave {}", magnitude, gain, output);
        } else {
            prop_assert_eq!(output, 0);
        }
    }

    // === Torque: gain monotonicity ===

    #[test]
    fn prop_constant_effect_gain_monotonic(
        magnitude in 1i16..=i16::MAX,
        g1 in 0.0f32..=0.5,
    ) {
        let g2 = g1 + 0.5; // g2 > g1 always
        let effect = ConstantEffect::new(magnitude);
        let out1 = effect.apply_gain(g1);
        let out2 = effect.apply_gain(g2);
        prop_assert!(
            out2 >= out1,
            "higher gain should produce >= output: g1={} out={}, g2={} out={}",
            g1, out1, g2, out2
        );
    }

    // === Spring effect: sign agrees with position ===

    #[test]
    fn prop_spring_sign_agrees_with_position(coefficient in 1i16..=10000i16) {
        let spring = SpringEffect::new(coefficient);
        let out_pos = spring.calculate(500);
        let out_neg = spring.calculate(-500);
        prop_assert!(out_pos >= 0, "positive position should give non-negative force, got {}", out_pos);
        prop_assert!(out_neg <= 0, "negative position should give non-positive force, got {}", out_neg);
    }

    // === Damper: sign follows velocity sign ===

    #[test]
    fn prop_damper_sign_follows_velocity(coefficient in 1i16..=10000i16) {
        let damper = DamperEffect::new(coefficient);
        let out_pos = damper.calculate(500);
        let out_neg = damper.calculate(-500);
        prop_assert!(out_pos >= 0, "positive velocity should give non-negative damper, got {}", out_pos);
        prop_assert!(out_neg <= 0, "negative velocity should give non-positive damper, got {}", out_neg);
    }

    // === Sine: output bounded by i16 for any time ===

    #[test]
    fn prop_sine_output_always_i16(
        freq in 0.1f32..=500.0,
        time_ms in 0u32..=100_000,
        gain in 0u8..=255u8,
    ) {
        let mut sine = SineEffect::new(freq, 100_000);
        sine.params.gain = gain;
        let output = sine.calculate(time_ms);
        let output_i32 = output as i32;
        prop_assert!(
            output_i32 >= i16::MIN as i32 && output_i32 <= i16::MAX as i32,
            "sine output {} out of i16 range", output_i32
        );
    }

    // === EffectParams: default gain is max ===

    #[test]
    fn prop_effect_params_default_gain(
        et in effect_type_strategy(),
        duration in any::<u32>(),
    ) {
        let params = EffectParams::new(et, duration);
        prop_assert_eq!(params.gain, 255, "default gain should be 255");
        prop_assert_eq!(params.direction, 0, "default direction should be 0");
    }

    // === FfbGain: combined never exceeds any individual factor ===

    #[test]
    fn prop_gain_combined_le_each_factor(
        overall in 0.0f32..=1.0,
        torque in 0.0f32..=1.0,
        effects in 0.0f32..=1.0,
    ) {
        let gain = FfbGain::new(overall)
            .with_torque(torque)
            .with_effects(effects);
        let combined = gain.combined();
        prop_assert!(combined <= gain.overall + f32::EPSILON);
        prop_assert!(combined <= gain.torque + f32::EPSILON);
        prop_assert!(combined <= gain.effects + f32::EPSILON);
    }
}
