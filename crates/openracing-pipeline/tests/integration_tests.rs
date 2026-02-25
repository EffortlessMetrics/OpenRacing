//! Integration tests for openracing-pipeline
//!
//! Tests the full pipeline lifecycle from compilation to execution.

use openracing_curves::CurveType;
use openracing_filters::Frame;
use openracing_pipeline::prelude::*;
use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};

fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("must() failed: {:?}", e),
    }
}

fn create_test_config() -> racing_wheel_schemas::entities::FilterConfig {
    racing_wheel_schemas::entities::FilterConfig::new_complete(
        4,
        must(Gain::new(0.1)),
        must(Gain::new(0.15)),
        must(Gain::new(0.05)),
        vec![must(NotchFilter::new(
            must(FrequencyHz::new(60.0)),
            2.0,
            -12.0,
        ))],
        must(Gain::new(0.8)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.5, 0.6)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(0.9)),
        racing_wheel_schemas::entities::BumpstopConfig::default(),
        racing_wheel_schemas::entities::HandsOffConfig::default(),
    )
    .unwrap()
}

fn create_linear_config() -> racing_wheel_schemas::entities::FilterConfig {
    racing_wheel_schemas::entities::FilterConfig::new_complete(
        0,
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        must(Gain::new(0.0)),
        vec![],
        must(Gain::new(1.0)),
        vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(1.0, 1.0)),
        ],
        must(Gain::new(1.0)),
        racing_wheel_schemas::entities::BumpstopConfig::default(),
        racing_wheel_schemas::entities::HandsOffConfig::default(),
    )
    .unwrap()
}

fn create_test_frame(torque: f32) -> Frame {
    Frame {
        ffb_in: torque,
        torque_out: torque,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 1,
    }
}

mod compilation_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_compilation_lifecycle() {
        let compiler = PipelineCompiler::new();
        let config = create_test_config();

        let compiled = compiler.compile_pipeline(config).await.unwrap();
        assert!(compiled.pipeline.node_count() > 0);
        assert!(compiled.config_hash != 0);
    }

    #[tokio::test]
    async fn test_compilation_with_response_curve() {
        let compiler = PipelineCompiler::new();
        let config = create_linear_config();
        let curve = CurveType::exponential(2.0).unwrap();

        let compiled = compiler
            .compile_pipeline_with_response_curve(config, Some(&curve))
            .await
            .unwrap();

        assert!(compiled.pipeline.response_curve().is_some());
    }

    #[tokio::test]
    async fn test_compilation_determinism() {
        let compiler = PipelineCompiler::new();
        let config = create_test_config();

        let compiled1 = compiler.compile_pipeline(config.clone()).await.unwrap();
        let compiled2 = compiler.compile_pipeline(config).await.unwrap();

        assert_eq!(compiled1.config_hash, compiled2.config_hash);
        assert_eq!(
            compiled1.pipeline.node_count(),
            compiled2.pipeline.node_count()
        );
    }

    #[tokio::test]
    async fn test_async_compilation() {
        let compiler = PipelineCompiler::new();
        let config = create_test_config();

        let rx = compiler.compile_pipeline_async(config).await.unwrap();
        let result = rx.await.unwrap();

        assert!(result.is_ok());
        let compiled = result.unwrap();
        assert!(compiled.pipeline.node_count() > 0);
    }
}

mod execution_tests {
    use super::*;

    #[test]
    fn test_empty_pipeline_execution() {
        let mut pipeline = Pipeline::new();
        let mut frame = create_test_frame(0.5);

        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!((frame.torque_out - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pipeline_with_response_curve_linear() {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        let mut frame = create_test_frame(0.5);
        let result = pipeline.process(&mut frame);

        assert!(result.is_ok());
        assert!((frame.torque_out - 0.5).abs() < 0.02);
    }

    #[test]
    fn test_pipeline_with_response_curve_exponential() -> Result<(), openracing_curves::CurveError>
    {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve(curve.to_lut());

        let mut frame = create_test_frame(0.5);
        let result = pipeline.process(&mut frame);

        assert!(result.is_ok());
        assert!(
            (frame.torque_out - 0.25).abs() < 0.02,
            "Expected ~0.25, got {}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn test_response_curve_preserves_sign() -> Result<(), openracing_curves::CurveError> {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0)?;
        pipeline.set_response_curve(curve.to_lut());

        let mut frame_pos = create_test_frame(0.5);
        pipeline.process(&mut frame_pos).unwrap();
        assert!(frame_pos.torque_out > 0.0);

        let mut frame_neg = create_test_frame(-0.5);
        pipeline.process(&mut frame_neg).unwrap();
        assert!(frame_neg.torque_out < 0.0);

        assert!(
            (frame_pos.torque_out.abs() - frame_neg.torque_out.abs()).abs() < 0.01,
            "Magnitudes should be equal"
        );
        Ok(())
    }

    #[test]
    fn test_pipeline_output_validation() {
        let mut pipeline = Pipeline::new();

        // Empty pipeline doesn't validate - it just passes through
        // Validation happens at filter node boundaries
        let mut frame_nan = create_test_frame(f32::NAN);
        assert!(pipeline.process(&mut frame_nan).is_ok());

        let mut frame_inf = create_test_frame(f32::INFINITY);
        assert!(pipeline.process(&mut frame_inf).is_ok());

        let mut frame_out_of_bounds = create_test_frame(2.0);
        assert!(pipeline.process(&mut frame_out_of_bounds).is_ok());
    }

    #[test]
    fn test_pipeline_swap_atomicity() {
        let mut pipeline1 = Pipeline::new();
        let pipeline2 = Pipeline::with_hash(0x12345678);

        assert_eq!(pipeline1.config_hash(), 0);

        pipeline1.swap_at_tick_boundary(pipeline2);

        assert_eq!(pipeline1.config_hash(), 0x12345678);
    }
}

mod validation_tests {
    use super::*;

    #[test]
    fn test_valid_config_passes() {
        let validator = PipelineValidator::new();
        let config = create_test_config();

        assert!(validator.validate_config(&config).is_ok());
    }

    #[test]
    fn test_invalid_reconstruction_fails() {
        let validator = PipelineValidator::new();
        let mut config = create_test_config();
        config.reconstruction = 10;

        assert!(validator.validate_config(&config).is_err());
    }
}

mod hash_tests {
    use super::*;

    #[test]
    fn test_hash_determinism() {
        let config = create_test_config();

        let hash1 = calculate_config_hash(&config);
        let hash2 = calculate_config_hash(&config);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_configs_different_hashes() {
        let config1 = create_test_config();
        let config2 = create_linear_config();

        let hash1 = calculate_config_hash(&config1);
        let hash2 = calculate_config_hash(&config2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_with_curve_different() {
        let config = create_linear_config();
        let curve = CurveType::exponential(2.0).unwrap();

        let hash_no_curve = calculate_config_hash_with_curve(&config, None);
        let hash_with_curve = calculate_config_hash_with_curve(&config, Some(&curve));

        assert_ne!(hash_no_curve, hash_with_curve);
    }
}

mod state_tests {
    use super::*;

    #[test]
    fn test_state_snapshot() {
        let pipeline = Pipeline::new();
        let snapshot = pipeline.state_snapshot();

        assert_eq!(snapshot.node_count, 0);
        assert_eq!(snapshot.state_size, 0);
        assert!(snapshot.is_empty());
    }

    #[test]
    fn test_state_alignment() {
        let pipeline = Pipeline::new();
        assert!(pipeline.is_state_aligned());
    }
}

mod property_tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_hash_stability(_config_seed: u64) -> bool {
        let config = create_test_config();
        let hash1 = calculate_config_hash(&config);
        let hash2 = calculate_config_hash(&config);
        hash1 == hash2
    }

    #[quickcheck]
    fn prop_process_determinism(torque: f32) -> bool {
        if !torque.is_finite() || torque.abs() > 1.0 {
            return true;
        }

        let mut pipeline = Pipeline::new();

        let mut frame1 = create_test_frame(torque);
        let mut frame2 = create_test_frame(torque);

        let result1 = pipeline.process(&mut frame1);
        let result2 = pipeline.process(&mut frame2);

        match (result1, result2) {
            (Ok(_), Ok(_)) => (frame1.torque_out - frame2.torque_out).abs() < f32::EPSILON,
            (Err(_), Err(_)) => true,
            _ => false,
        }
    }
}

mod rt_safety_tests {
    use super::*;

    #[test]
    fn test_many_frames_rt_safe() {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        for i in 0..10000 {
            let torque = (i as f32 / 10000.0).sin();
            let mut frame = create_test_frame(torque);

            let result = pipeline.process(&mut frame);
            assert!(result.is_ok());
            assert!(frame.torque_out.is_finite());
            assert!(frame.torque_out.abs() <= 1.0);
        }
    }
}
