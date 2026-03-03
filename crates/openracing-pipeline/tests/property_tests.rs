#![allow(clippy::redundant_closure)]
//! Property-based integration tests for openracing-pipeline
//!
//! Tests pipeline stage ordering, construction, execution,
//! and property-based invariants.

use openracing_curves::CurveType;
use openracing_filters::Frame;
use openracing_pipeline::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Pipeline construction
// ---------------------------------------------------------------------------

mod construction_tests {
    use super::*;

    #[test]
    fn new_pipeline_is_empty() -> TestResult {
        let p = Pipeline::new();
        assert!(p.is_empty());
        assert_eq!(p.node_count(), 0);
        assert_eq!(p.config_hash(), 0);
        Ok(())
    }

    #[test]
    fn with_hash_stores_hash() -> TestResult {
        let p = Pipeline::with_hash(0xCAFEBABE);
        assert_eq!(p.config_hash(), 0xCAFEBABE);
        assert!(p.is_empty());
        Ok(())
    }

    #[test]
    fn default_equals_new() -> TestResult {
        let def = Pipeline::default();
        let new = Pipeline::new();
        assert_eq!(def.is_empty(), new.is_empty());
        assert_eq!(def.config_hash(), new.config_hash());
        assert_eq!(def.node_count(), new.node_count());
        Ok(())
    }

    #[test]
    fn clone_preserves_hash_and_count() -> TestResult {
        let p = Pipeline::with_hash(0x12345678);
        let cloned = p.clone();
        assert_eq!(p.config_hash(), cloned.config_hash());
        assert_eq!(p.node_count(), cloned.node_count());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline execution
// ---------------------------------------------------------------------------

mod execution_tests {
    use super::*;

    #[test]
    fn empty_pipeline_passthrough() -> TestResult {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame::from_torque(0.42);
        pipeline.process(&mut frame)?;
        assert!(
            (frame.torque_out - 0.42).abs() < 0.001,
            "empty pipeline should pass through, got {}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn process_preserves_frame_metadata() -> TestResult {
        let mut pipeline = Pipeline::new();
        let mut frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.5,
            wheel_speed: 1.0,
            hands_off: false,
            ts_mono_ns: 12345,
            seq: 42,
        };
        pipeline.process(&mut frame)?;
        assert_eq!(frame.ts_mono_ns, 12345);
        assert_eq!(frame.seq, 42);
        assert!((frame.wheel_speed - 1.0).abs() < f32::EPSILON);
        Ok(())
    }

    #[test]
    fn linear_response_curve_is_identity() -> TestResult {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        let mut frame = Frame::from_torque(0.7);
        pipeline.process(&mut frame)?;
        assert!(
            (frame.torque_out - 0.7).abs() < 0.02,
            "linear curve should be ~identity, got {}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn exponential_curve_reduces_midrange() -> TestResult {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve(curve.to_lut());

        let mut frame = Frame::from_torque(0.5);
        pipeline.process(&mut frame)?;
        // x^2 at 0.5 = 0.25
        assert!(
            (frame.torque_out - 0.25).abs() < 0.02,
            "exponential curve at 0.5 should be ~0.25, got {}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn response_curve_preserves_sign() -> TestResult {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve(curve.to_lut());

        let mut pos = Frame::from_torque(0.5);
        pipeline.process(&mut pos)?;

        let mut neg = Frame::from_torque(-0.5);
        pipeline.process(&mut neg)?;

        assert!(pos.torque_out > 0.0, "positive input should stay positive");
        assert!(neg.torque_out < 0.0, "negative input should stay negative");
        assert!(
            (pos.torque_out.abs() - neg.torque_out.abs()).abs() < 0.01,
            "magnitudes should match"
        );
        Ok(())
    }

    #[test]
    fn response_curve_endpoints() -> TestResult {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        let mut frame_zero = Frame::from_torque(0.0);
        pipeline.process(&mut frame_zero)?;
        assert!(
            frame_zero.torque_out.abs() < 0.02,
            "zero input should produce ~zero output"
        );

        let mut frame_one = Frame::from_torque(1.0);
        pipeline.process(&mut frame_one)?;
        assert!(
            (frame_one.torque_out - 1.0).abs() < 0.02,
            "unity input should produce ~unity output"
        );
        Ok(())
    }

    #[test]
    fn process_with_curve_override() -> TestResult {
        let mut pipeline = Pipeline::new();
        let linear = CurveType::Linear.to_lut();

        let mut frame = Frame::from_torque(0.5);
        pipeline.process_with_curve(&mut frame, Some(&linear))?;
        assert!(
            (frame.torque_out - 0.5).abs() < 0.02,
            "linear curve override should be ~identity"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline swap
// ---------------------------------------------------------------------------

mod swap_tests {
    use super::*;

    #[test]
    fn swap_replaces_pipeline() -> TestResult {
        let mut p1 = Pipeline::new();
        let p2 = Pipeline::with_hash(0xABCD);

        p1.swap_at_tick_boundary(p2);
        assert_eq!(p1.config_hash(), 0xABCD);
        Ok(())
    }

    #[test]
    fn swap_replaces_response_curve() -> TestResult {
        let mut p1 = Pipeline::new();
        p1.set_response_curve(CurveType::Linear.to_lut());
        assert!(p1.response_curve().is_some());

        let p2 = Pipeline::new();
        p1.swap_at_tick_boundary(p2);
        assert!(p1.response_curve().is_none());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// State management
// ---------------------------------------------------------------------------

mod state_tests {
    use super::*;

    #[test]
    fn empty_pipeline_state() -> TestResult {
        let p = Pipeline::new();
        assert_eq!(p.state_size(), 0);
        assert!(p.is_state_aligned());
        assert!(p.state_offset(0).is_none());
        assert!(p.node_state_size(0).is_none());
        Ok(())
    }

    #[test]
    fn snapshot_reflects_empty_pipeline() -> TestResult {
        let p = Pipeline::new();
        let snap = p.state_snapshot();
        assert_eq!(snap.node_count, 0);
        assert_eq!(snap.state_size, 0);
        assert_eq!(snap.config_hash, 0);
        assert!(!snap.has_response_curve);
        assert!(snap.is_empty());
        Ok(())
    }

    #[test]
    fn snapshot_with_response_curve() -> TestResult {
        let mut p = Pipeline::new();
        p.set_response_curve(CurveType::Linear.to_lut());
        let snap = p.state_snapshot();
        assert!(snap.has_response_curve);
        Ok(())
    }

    #[test]
    fn reset_state_zeroes_buffer() -> TestResult {
        let mut p = Pipeline::new();
        p.reset_state();
        assert_eq!(p.state_size(), 0);
        Ok(())
    }

    #[test]
    fn state_efficiency_empty() -> TestResult {
        let snap = openracing_pipeline::PipelineStateSnapshot {
            node_count: 0,
            state_size: 0,
            config_hash: 0,
            has_response_curve: false,
        };
        assert!((snap.state_efficiency() - 1.0).abs() < f64::EPSILON);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

mod validation_tests {
    use super::*;
    use racing_wheel_schemas::entities::FilterConfig;
    use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};

    fn create_valid_config() -> Result<FilterConfig, Box<dyn std::error::Error>> {
        Ok(FilterConfig::new_complete(
            4,
            Gain::new(0.1)?,
            Gain::new(0.15)?,
            Gain::new(0.05)?,
            vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
            Gain::new(0.8)?,
            vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(0.5, 0.6)?,
                CurvePoint::new(1.0, 1.0)?,
            ],
            Gain::new(0.9)?,
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )?)
    }

    #[test]
    fn valid_config_passes_validation() -> TestResult {
        let validator = PipelineValidator::new();
        let config = create_valid_config()?;
        validator.validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn invalid_reconstruction_level() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = create_valid_config()?;
        config.reconstruction = 9;
        assert!(validator.validate_config(&config).is_err());
        Ok(())
    }

    #[test]
    fn default_config_is_valid() -> TestResult {
        let validator = PipelineValidator::new();
        let config = FilterConfig::default();
        validator.validate_config(&config)?;
        Ok(())
    }

    #[test]
    fn empty_config_detected() -> TestResult {
        let validator = PipelineValidator::new();
        let mut config = FilterConfig::default();
        config.bumpstop.enabled = false;
        config.hands_off.enabled = false;
        assert!(validator.is_empty_config(&config));
        Ok(())
    }

    #[test]
    fn non_empty_config_detected() -> TestResult {
        let validator = PipelineValidator::new();
        let config = create_valid_config()?;
        assert!(!validator.is_empty_config(&config));
        Ok(())
    }

    #[test]
    fn response_curve_validation() -> TestResult {
        let validator = PipelineValidator::new();
        validator.validate_response_curve(&CurveType::Linear)?;

        let exp = CurveType::exponential(2.0)?;
        validator.validate_response_curve(&exp)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Hash determinism
// ---------------------------------------------------------------------------

mod hash_tests {
    use super::*;
    use racing_wheel_schemas::entities::FilterConfig;
    use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};

    fn create_config() -> Result<FilterConfig, Box<dyn std::error::Error>> {
        Ok(FilterConfig::new_complete(
            4,
            Gain::new(0.1)?,
            Gain::new(0.15)?,
            Gain::new(0.05)?,
            vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
            Gain::new(0.8)?,
            vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(0.5, 0.6)?,
                CurvePoint::new(1.0, 1.0)?,
            ],
            Gain::new(0.9)?,
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        )?)
    }

    #[test]
    fn hash_is_deterministic() -> TestResult {
        let config = create_config()?;
        let h1 = calculate_config_hash(&config);
        let h2 = calculate_config_hash(&config);
        assert_eq!(h1, h2);
        Ok(())
    }

    #[test]
    fn different_configs_different_hashes() -> TestResult {
        let c1 = create_config()?;
        let c2 = FilterConfig::default();
        assert_ne!(calculate_config_hash(&c1), calculate_config_hash(&c2));
        Ok(())
    }

    #[test]
    fn hash_with_curve_differs_from_without() -> TestResult {
        let config = create_config()?;
        let without = calculate_config_hash_with_curve(&config, None);
        let with_linear = calculate_config_hash_with_curve(&config, Some(&CurveType::Linear));
        assert_ne!(without, with_linear);
        Ok(())
    }

    #[test]
    fn different_curves_different_hashes() -> TestResult {
        let config = create_config()?;
        let linear = calculate_config_hash_with_curve(&config, Some(&CurveType::Linear));
        let exp_curve = CurveType::exponential(2.0)?;
        let exponential = calculate_config_hash_with_curve(&config, Some(&exp_curve));
        assert_ne!(linear, exponential);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

mod proptest_invariants {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        // --- Empty pipeline is identity transform ---

        #[test]
        fn prop_empty_pipeline_identity(torque in -1.0f32..=1.0) {
            if !torque.is_finite() {
                return Ok(());
            }
            let mut pipeline = Pipeline::new();
            let mut frame = Frame::from_torque(torque);
            let result = pipeline.process(&mut frame);
            prop_assert!(result.is_ok());
            prop_assert!(
                (frame.torque_out - torque).abs() < f32::EPSILON,
                "empty pipeline should be identity: in={}, out={}",
                torque, frame.torque_out
            );
        }

        // --- Processing same input twice produces same output ---

        #[test]
        fn prop_process_deterministic(torque in -1.0f32..=1.0) {
            if !torque.is_finite() {
                return Ok(());
            }
            let mut p1 = Pipeline::new();
            let mut p2 = Pipeline::new();

            let mut f1 = Frame::from_torque(torque);
            let mut f2 = Frame::from_torque(torque);

            let r1 = p1.process(&mut f1);
            let r2 = p2.process(&mut f2);

            prop_assert!(r1.is_ok());
            prop_assert!(r2.is_ok());
            prop_assert!(
                (f1.torque_out - f2.torque_out).abs() < f32::EPSILON,
                "process should be deterministic: {} vs {}",
                f1.torque_out, f2.torque_out
            );
        }

        // --- Pipeline hash is stable across calls ---

        #[test]
        fn prop_hash_stable(hash_val in 0u64..=u64::MAX) {
            let p1 = Pipeline::with_hash(hash_val);
            let p2 = Pipeline::with_hash(hash_val);
            prop_assert_eq!(p1.config_hash(), p2.config_hash());
        }

        // --- State snapshot reflects pipeline state ---

        #[test]
        fn prop_snapshot_matches_pipeline(hash_val in 0u64..=u64::MAX) {
            let p = Pipeline::with_hash(hash_val);
            let snap = p.state_snapshot();
            prop_assert_eq!(snap.node_count, p.node_count());
            prop_assert_eq!(snap.state_size, p.state_size());
            prop_assert_eq!(snap.config_hash, p.config_hash());
        }

        // --- Swap always replaces hash ---

        #[test]
        fn prop_swap_replaces_hash(h1 in 0u64..=u64::MAX, h2 in 0u64..=u64::MAX) {
            let mut p1 = Pipeline::with_hash(h1);
            let p2 = Pipeline::with_hash(h2);
            p1.swap_at_tick_boundary(p2);
            prop_assert_eq!(p1.config_hash(), h2);
        }

        // --- Linear response curve is ~identity for valid torque ---

        #[test]
        fn prop_linear_curve_identity(torque in 0.0f32..=1.0) {
            let mut pipeline = Pipeline::new();
            pipeline.set_response_curve(CurveType::Linear.to_lut());
            let mut frame = Frame::from_torque(torque);
            let result = pipeline.process(&mut frame);
            prop_assert!(result.is_ok());
            prop_assert!(
                (frame.torque_out - torque).abs() < 0.02,
                "linear curve should be ~identity: in={}, out={}",
                torque, frame.torque_out
            );
        }
    }
}

// ---------------------------------------------------------------------------
// RT safety simulation
// ---------------------------------------------------------------------------

mod rt_safety_tests {
    use super::*;

    #[test]
    fn sustained_processing_stable() -> TestResult {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        for i in 0..10_000 {
            let torque = (i as f32 / 10_000.0 * std::f32::consts::TAU).sin();
            let clamped = torque.clamp(-1.0, 1.0);
            let mut frame = Frame::from_torque(clamped);
            pipeline.process(&mut frame)?;
            assert!(
                frame.torque_out.is_finite(),
                "output must be finite at iteration {i}"
            );
            assert!(
                frame.torque_out.abs() <= 1.0,
                "output must be in [-1,1] at iteration {i}, got {}",
                frame.torque_out
            );
        }
        Ok(())
    }

    #[test]
    fn swap_during_processing() -> TestResult {
        let mut pipeline = Pipeline::new();

        for i in 0..100 {
            let mut frame = Frame::from_torque(0.5);
            pipeline.process(&mut frame)?;

            // Swap every 10 frames
            if i % 10 == 0 {
                let new = Pipeline::with_hash(i as u64);
                pipeline.swap_at_tick_boundary(new);
            }
        }

        // Last swap was at i=90 with hash=90
        assert_eq!(pipeline.config_hash(), 90);
        Ok(())
    }
}
