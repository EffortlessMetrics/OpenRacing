//! Snapshot tests for FFB effect serialization formats

use openracing_ffb::{
    ConstantEffect, DamperEffect, EffectParams, EffectType, FfbDirection, FfbGain, FrictionEffect,
    SineEffect, SpringEffect,
};

// --- Default snapshots for all effect types ---

#[test]
fn snapshot_effect_type_default() {
    insta::assert_json_snapshot!("effect_type_default", EffectType::default());
}

#[test]
fn snapshot_effect_params_default() {
    insta::assert_json_snapshot!("effect_params_default", EffectParams::default());
}

#[test]
fn snapshot_constant_effect_default() {
    insta::assert_json_snapshot!("constant_effect_default", ConstantEffect::default());
}

#[test]
fn snapshot_spring_effect_default() {
    insta::assert_json_snapshot!("spring_effect_default", SpringEffect::default());
}

#[test]
fn snapshot_damper_effect_default() {
    insta::assert_json_snapshot!("damper_effect_default", DamperEffect::default());
}

#[test]
fn snapshot_friction_effect_default() {
    insta::assert_json_snapshot!("friction_effect_default", FrictionEffect::default());
}

#[test]
fn snapshot_sine_effect_default() {
    insta::assert_json_snapshot!("sine_effect_default", SineEffect::default());
}

#[test]
fn snapshot_ffb_gain_default() {
    insta::assert_json_snapshot!("ffb_gain_default", FfbGain::default());
}

#[test]
fn snapshot_ffb_direction_default() {
    insta::assert_json_snapshot!("ffb_direction_default", FfbDirection::default());
}

// --- Typical torque command output scenarios ---

#[test]
fn snapshot_constant_effect_typical() {
    let effect = ConstantEffect::new(5000);
    insta::assert_json_snapshot!("constant_effect_typical", effect);
}

#[test]
fn snapshot_spring_effect_typical() {
    let effect = SpringEffect::new(800);
    insta::assert_json_snapshot!("spring_effect_typical", effect);
}

#[test]
fn snapshot_damper_effect_typical() {
    let effect = DamperEffect::new(500);
    insta::assert_json_snapshot!("damper_effect_typical", effect);
}

#[test]
fn snapshot_friction_effect_typical() {
    let effect = FrictionEffect::new(300);
    insta::assert_json_snapshot!("friction_effect_typical", effect);
}

#[test]
fn snapshot_sine_effect_typical() {
    let effect = SineEffect::new(20.0, 2000);
    insta::assert_json_snapshot!("sine_effect_typical", effect);
}

#[test]
fn snapshot_ffb_gain_typical() {
    let gain = FfbGain::new(0.8).with_torque(0.9).with_effects(0.7);
    insta::assert_json_snapshot!("ffb_gain_typical", gain);
}

#[test]
fn snapshot_effect_params_configured() {
    let params = EffectParams::new(EffectType::Sine, 1500)
        .with_gain(200)
        .with_direction(180);
    insta::assert_json_snapshot!("effect_params_configured", params);
}
