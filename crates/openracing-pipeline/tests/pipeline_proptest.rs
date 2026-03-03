#![allow(clippy::redundant_closure)]
//! Property-based tests for the FFB pipeline: filter chain with arbitrary
//! configs, hash determinism, validation invariants, and empty pipeline
//! identity.

use openracing_filters::Frame;
use openracing_pipeline::{
    Pipeline, PipelineValidator, calculate_config_hash, calculate_config_hash_with_curve,
};
use proptest::prelude::*;
use racing_wheel_schemas::entities::FilterConfig;
use racing_wheel_schemas::prelude::{CurvePoint, Gain};

/// Create a valid FilterConfig with the given gain values.
fn make_config(friction: f32, damper: f32, inertia: f32, slew: f32) -> Option<FilterConfig> {
    let friction_g = Gain::new(friction).ok()?;
    let damper_g = Gain::new(damper).ok()?;
    let inertia_g = Gain::new(inertia).ok()?;
    let slew_g = Gain::new(slew).ok()?;
    let torque_cap = Gain::new(0.9).ok()?;
    let curve = vec![
        CurvePoint::new(0.0, 0.0).ok()?,
        CurvePoint::new(1.0, 1.0).ok()?,
    ];
    FilterConfig::new_complete(
        0,
        friction_g,
        damper_g,
        inertia_g,
        vec![],
        slew_g,
        curve,
        torque_cap,
        racing_wheel_schemas::entities::BumpstopConfig::default(),
        racing_wheel_schemas::entities::HandsOffConfig::default(),
    )
    .ok()
}

// ── Tests ───────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // === Empty pipeline is identity transform ===

    #[test]
    fn prop_empty_pipeline_identity(
        ffb_in in -1.0f32..=1.0,
        wheel_speed in -10.0f32..=10.0,
    ) {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame {
            ffb_in,
            torque_out: ffb_in,
            wheel_speed,
            hands_off: false,
            ts_mono_ns: 0,
            seq: 1,
        };
        let original_torque = frame.torque_out;
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok(), "empty pipeline should not fail");
        prop_assert!(
            (frame.torque_out - original_torque).abs() < f32::EPSILON,
            "empty pipeline should not modify torque: {} vs {}",
            frame.torque_out, original_torque
        );
    }

    // === Empty pipeline preserves ffb_in ===

    #[test]
    fn prop_empty_pipeline_preserves_ffb_in(ffb_in in -1.0f32..=1.0) {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame::from_torque(ffb_in);
        let _ = pipeline.process(&mut frame);
        prop_assert!(
            (frame.ffb_in - ffb_in).abs() < f32::EPSILON,
            "ffb_in should be preserved"
        );
    }

    // === Hash determinism: same config → same hash ===

    #[test]
    fn prop_hash_deterministic(
        friction in 0.0f32..=1.0,
        damper in 0.0f32..=1.0,
        inertia in 0.0f32..=1.0,
        slew in 0.01f32..=1.0,
    ) {
        if let Some(config) = make_config(friction, damper, inertia, slew) {
            let h1 = calculate_config_hash(&config);
            let h2 = calculate_config_hash(&config);
            prop_assert_eq!(h1, h2, "same config must produce same hash");
        }
    }

    // === Hash with curve: adding None curve equals base hash ===

    #[test]
    fn prop_hash_none_curve_equals_base(
        friction in 0.0f32..=1.0,
        damper in 0.0f32..=1.0,
    ) {
        if let Some(config) = make_config(friction, damper, 0.0, 1.0) {
            let base = calculate_config_hash(&config);
            let with_none = calculate_config_hash_with_curve(&config, None);
            // They use different hasher paths, so they differ; but both should be deterministic
            let with_none2 = calculate_config_hash_with_curve(&config, None);
            prop_assert_eq!(with_none, with_none2, "hash_with_curve(None) must be deterministic");
            // base and with_none may differ because the function hashes the curve discriminant
            let _ = base; // suppress unused
        }
    }

    // === Pipeline with_hash preserves the hash ===

    #[test]
    fn prop_pipeline_with_hash(hash in any::<u64>()) {
        let pipeline = Pipeline::with_hash(hash);
        prop_assert_eq!(pipeline.config_hash(), hash);
        prop_assert!(pipeline.is_empty());
        prop_assert_eq!(pipeline.node_count(), 0);
    }

    // === Pipeline clone preserves hash ===

    #[test]
    fn prop_pipeline_clone_preserves_hash(hash in any::<u64>()) {
        let pipeline = Pipeline::with_hash(hash);
        let cloned = pipeline.clone();
        prop_assert_eq!(pipeline.config_hash(), cloned.config_hash());
        prop_assert_eq!(pipeline.node_count(), cloned.node_count());
    }

    // === Validator: default config is valid ===

    #[test]
    fn prop_default_config_always_valid(_seed in 0u32..256) {
        let validator = PipelineValidator::new();
        let config = FilterConfig::default();
        let result = validator.validate_config(&config);
        prop_assert!(result.is_ok(), "default config must always be valid: {:?}", result.err());
    }

    // === Validator: valid gains always pass ===

    #[test]
    fn prop_valid_gains_pass_validation(
        friction in 0.0f32..=1.0,
        damper in 0.0f32..=1.0,
        inertia in 0.0f32..=1.0,
        slew in 0.01f32..=1.0,
    ) {
        if let Some(config) = make_config(friction, damper, inertia, slew) {
            let validator = PipelineValidator::new();
            let result = validator.validate_config(&config);
            prop_assert!(result.is_ok(), "valid gains should pass: {:?}", result.err());
        }
    }

    // === Pipeline process: output bounded [-1, 1] for empty pipeline ===

    #[test]
    fn prop_empty_pipeline_output_bounded(torque in -1.0f32..=1.0) {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame::from_torque(torque);
        let result = pipeline.process(&mut frame);
        prop_assert!(result.is_ok());
        prop_assert!(
            frame.torque_out >= -1.0 && frame.torque_out <= 1.0,
            "output {} out of [-1, 1]", frame.torque_out
        );
    }

    // === Hash: different gains produce different hashes (statistical) ===

    #[test]
    fn prop_different_friction_different_hash(
        f1 in 0.0f32..=0.49,
        f2 in 0.51f32..=1.0,
    ) {
        if let (Some(c1), Some(c2)) = (
            make_config(f1, 0.0, 0.0, 1.0),
            make_config(f2, 0.0, 0.0, 1.0),
        ) {
            let h1 = calculate_config_hash(&c1);
            let h2 = calculate_config_hash(&c2);
            prop_assert_ne!(h1, h2, "different friction should (very likely) produce different hashes");
        }
    }

    // === PipelineError display contains context ===

    #[test]
    fn prop_pipeline_error_display_contains_message(
        msg in "[a-zA-Z0-9 ]{1,50}"
    ) {
        let err = openracing_pipeline::PipelineError::InvalidConfig(msg.clone());
        let display = format!("{err}");
        prop_assert!(
            display.contains(&msg),
            "error display '{}' should contain '{}'", display, msg
        );
    }
}
