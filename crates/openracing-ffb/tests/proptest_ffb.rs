//! Property-based tests for FFB normalization and gain chaining.

#[cfg(test)]
mod proptest_ffb {
    use openracing_ffb::{ConstantEffect, FfbDirection, FfbGain};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        // --- Normalization: FfbGain.combined() always in [0.0, 1.0] ---

        #[test]
        fn gain_combined_always_bounded(
            overall in -10.0f32..10.0f32,
            torque in -10.0f32..10.0f32,
            effects in -10.0f32..10.0f32,
        ) {
            let gain = FfbGain::new(overall)
                .with_torque(torque)
                .with_effects(effects);
            let combined = gain.combined();
            prop_assert!(combined >= 0.0, "combined gain {} must be >= 0", combined);
            prop_assert!(combined <= 1.0, "combined gain {} must be <= 1", combined);
        }

        // --- Normalization: FfbDirection always in [0, 360) ---

        #[test]
        fn direction_always_normalized(degrees in -1e6f32..1e6f32) {
            let dir = FfbDirection::new(degrees);
            prop_assert!(dir.degrees >= 0.0, "direction {} must be >= 0", dir.degrees);
            prop_assert!(dir.degrees < 360.0, "direction {} must be < 360", dir.degrees);
        }

        // --- Normalization: ConstantEffect.apply_gain maps to valid i16 ---

        #[test]
        fn constant_effect_apply_gain_within_i16(
            magnitude in i16::MIN..=i16::MAX,
            gain in -10.0f32..10.0f32,
        ) {
            let effect = ConstantEffect::new(magnitude);
            let output = effect.apply_gain(gain);
            let output_i32 = output as i32;
            prop_assert!(
                output_i32 >= i16::MIN as i32 && output_i32 <= i16::MAX as i32,
                "apply_gain output {} out of i16 range", output_i32
            );
        }

        // --- Gain chaining: applying multiple gains never exceeds bounds ---

        #[test]
        fn gain_chaining_never_exceeds_bounds(
            g1_overall in 0.0f32..=1.0,
            g1_torque in 0.0f32..=1.0,
            g1_effects in 0.0f32..=1.0,
            g2_overall in 0.0f32..=1.0,
            g2_torque in 0.0f32..=1.0,
            g2_effects in 0.0f32..=1.0,
            magnitude in i16::MIN..=i16::MAX,
        ) {
            let gain1 = FfbGain::new(g1_overall)
                .with_torque(g1_torque)
                .with_effects(g1_effects);
            let gain2 = FfbGain::new(g2_overall)
                .with_torque(g2_torque)
                .with_effects(g2_effects);

            let effect = ConstantEffect::new(magnitude);

            // Apply first gain
            let after_first = effect.apply_gain(gain1.combined());
            // Apply second gain to the result
            let chained = ConstantEffect::new(after_first).apply_gain(gain2.combined());

            let chained_i32 = chained as i32;
            prop_assert!(
                chained_i32 >= i16::MIN as i32 && chained_i32 <= i16::MAX as i32,
                "chained gain output {} out of i16 range", chained_i32
            );
            // Chained gain magnitude should not exceed original magnitude
            prop_assert!(
                (chained as i32).abs() <= (magnitude as i32).abs(),
                "chained gain {} exceeded original magnitude {}",
                chained, magnitude
            );
        }

        // --- Gain chaining: three gains still bounded ---

        #[test]
        fn triple_gain_chain_bounded(
            g1 in 0.0f32..=1.0,
            g2 in 0.0f32..=1.0,
            g3 in 0.0f32..=1.0,
            magnitude in i16::MIN..=i16::MAX,
        ) {
            let effect = ConstantEffect::new(magnitude);
            let step1 = effect.apply_gain(g1);
            let step2 = ConstantEffect::new(step1).apply_gain(g2);
            let step3 = ConstantEffect::new(step2).apply_gain(g3);

            let result_i32 = step3 as i32;
            prop_assert!(
                result_i32 >= i16::MIN as i32 && result_i32 <= i16::MAX as i32,
                "triple-chained gain output {} out of i16 range", result_i32
            );
            prop_assert!(
                result_i32.abs() <= (magnitude as i32).abs(),
                "triple-chained gain {} exceeded original magnitude {}",
                step3, magnitude
            );
        }

        // --- Normalization: FfbGain fields are clamped to [0.0, 1.0] ---

        #[test]
        fn gain_fields_clamped(
            overall in -100.0f32..100.0f32,
            torque in -100.0f32..100.0f32,
            effects in -100.0f32..100.0f32,
        ) {
            let gain = FfbGain::new(overall)
                .with_torque(torque)
                .with_effects(effects);
            prop_assert!(gain.overall >= 0.0 && gain.overall <= 1.0,
                "overall {} not in [0,1]", gain.overall);
            prop_assert!(gain.torque >= 0.0 && gain.torque <= 1.0,
                "torque {} not in [0,1]", gain.torque);
            prop_assert!(gain.effects >= 0.0 && gain.effects <= 1.0,
                "effects {} not in [0,1]", gain.effects);
        }
    }
}
