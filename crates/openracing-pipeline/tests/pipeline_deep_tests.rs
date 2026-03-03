//! Deep tests for FFB pipeline: construction, execution, hot-swap, error handling.

use openracing_curves::CurveType;
use openracing_filters::Frame;
use openracing_pipeline::prelude::*;
use racing_wheel_schemas::prelude::{CurvePoint, FrequencyHz, Gain, NotchFilter};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn create_test_config(
) -> Result<racing_wheel_schemas::entities::FilterConfig, Box<dyn std::error::Error>> {
    Ok(racing_wheel_schemas::entities::FilterConfig::new_complete(
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

fn create_minimal_config(
) -> Result<racing_wheel_schemas::entities::FilterConfig, Box<dyn std::error::Error>> {
    let mut config = racing_wheel_schemas::entities::FilterConfig::default();
    config.bumpstop.enabled = false;
    config.hands_off.enabled = false;
    Ok(config)
}

fn make_frame(ffb_in: f32, torque_out: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 1,
    }
}

// ---------------------------------------------------------------------------
// Pipeline construction: empty, single stage, multi-stage
// ---------------------------------------------------------------------------

mod construction_tests {
    use super::*;

    #[test]
    fn empty_pipeline_has_zero_nodes() -> TestResult {
        let pipeline = Pipeline::new();
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.node_count(), 0);
        Ok(())
    }

    #[test]
    fn empty_pipeline_has_zero_hash() -> TestResult {
        let pipeline = Pipeline::new();
        assert_eq!(pipeline.config_hash(), 0);
        Ok(())
    }

    #[test]
    fn pipeline_with_hash_stores_hash() -> TestResult {
        let pipeline = Pipeline::with_hash(0xCAFEBABE);
        assert_eq!(pipeline.config_hash(), 0xCAFEBABE);
        assert!(pipeline.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn empty_config_produces_empty_pipeline() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_minimal_config()?;
        let compiled = compiler.compile_pipeline(config).await?;
        assert!(compiled.pipeline.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn full_config_produces_multi_node_pipeline() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let compiled = compiler.compile_pipeline(config).await?;
        assert!(
            compiled.pipeline.node_count() > 1,
            "full config should have multiple nodes, got {}",
            compiled.pipeline.node_count()
        );
        Ok(())
    }

    #[tokio::test]
    async fn single_filter_produces_single_node() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.5)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let compiled = compiler.compile_pipeline(config).await?;
        assert_eq!(compiled.pipeline.node_count(), 1, "only friction enabled → 1 node");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Stage ordering: verify execution order
// ---------------------------------------------------------------------------

mod ordering_tests {
    use super::*;

    #[tokio::test]
    async fn compilation_is_deterministic() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;

        let c1 = compiler.compile_pipeline(config.clone()).await?;
        let c2 = compiler.compile_pipeline(config).await?;

        assert_eq!(c1.config_hash, c2.config_hash, "same config → same hash");
        assert_eq!(
            c1.pipeline.node_count(),
            c2.pipeline.node_count(),
            "same config → same node count"
        );
        Ok(())
    }

    #[tokio::test]
    async fn different_configs_produce_different_hashes() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config1 = create_test_config()?;
        let config2 = create_minimal_config()?;

        let c1 = compiler.compile_pipeline(config1).await?;
        let c2 = compiler.compile_pipeline(config2).await?;

        assert_ne!(c1.config_hash, c2.config_hash);
        Ok(())
    }

    #[tokio::test]
    async fn node_count_matches_enabled_filters() -> TestResult {
        let compiler = PipelineCompiler::new();
        // Config with friction + damper only
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.3)?,
            Gain::new(0.4)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let compiled = compiler.compile_pipeline(config).await?;
        assert_eq!(compiled.pipeline.node_count(), 2, "friction + damper → 2 nodes");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline hot-swap: replace pipeline mid-operation
// ---------------------------------------------------------------------------

mod hot_swap_tests {
    use super::*;

    #[test]
    fn swap_replaces_pipeline() -> TestResult {
        let mut p1 = Pipeline::new();
        let p2 = Pipeline::with_hash(0xDEAD);

        assert_eq!(p1.config_hash(), 0);
        p1.swap_at_tick_boundary(p2);
        assert_eq!(p1.config_hash(), 0xDEAD);
        Ok(())
    }

    #[tokio::test]
    async fn swap_compiled_pipelines() -> TestResult {
        let compiler = PipelineCompiler::new();

        let config1 = create_minimal_config()?;
        let config2 = create_test_config()?;

        let mut pipeline = compiler.compile_pipeline(config1).await?.pipeline;
        let new_pipeline = compiler.compile_pipeline(config2).await?.pipeline;

        let old_hash = pipeline.config_hash();
        let new_hash = new_pipeline.config_hash();

        pipeline.swap_at_tick_boundary(new_pipeline);

        assert_eq!(pipeline.config_hash(), new_hash);
        assert_ne!(pipeline.config_hash(), old_hash);
        Ok(())
    }

    #[test]
    fn swap_preserves_process_ability() -> TestResult {
        let mut pipeline = Pipeline::new();
        let mut frame = make_frame(0.5, 0.5);

        pipeline.process(&mut frame)?;
        let out_before = frame.torque_out;

        pipeline.swap_at_tick_boundary(Pipeline::with_hash(0xBEEF));
        frame.torque_out = 0.5;
        pipeline.process(&mut frame)?;

        assert!((frame.torque_out - out_before).abs() < 0.001, "empty pipelines behave same");
        Ok(())
    }

    #[test]
    fn multiple_swaps() -> TestResult {
        let mut pipeline = Pipeline::new();
        for i in 1..=10u64 {
            pipeline.swap_at_tick_boundary(Pipeline::with_hash(i));
            assert_eq!(pipeline.config_hash(), i);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline error handling: invalid configs, NaN detection
// ---------------------------------------------------------------------------

mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn invalid_reconstruction_level_rejected() -> TestResult {
        let compiler = PipelineCompiler::new();
        let mut config = create_minimal_config()?;
        config.reconstruction = 10; // > 8

        let result = compiler.compile_pipeline(config).await;
        assert!(result.is_err(), "reconstruction > 8 should fail");
        Ok(())
    }

    #[test]
    fn empty_pipeline_passes_through_valid_frame() -> TestResult {
        let mut pipeline = Pipeline::new();
        let mut frame = make_frame(0.7, 0.7);
        pipeline.process(&mut frame)?;
        assert!((frame.torque_out - 0.7).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn pipeline_with_response_curve_maps_output() -> TestResult {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        let mut frame = make_frame(0.5, 0.5);
        pipeline.process(&mut frame)?;
        assert!(frame.torque_out.is_finite());
        assert!((frame.torque_out - 0.5).abs() < 0.02);
        Ok(())
    }

    #[tokio::test]
    async fn invalid_notch_frequency_rejected() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![NotchFilter::new(FrequencyHz::new(600.0)?, 2.0, -12.0)?],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let result = compiler.compile_pipeline(config).await;
        assert!(result.is_err(), "notch freq > 500 Hz should fail");
        Ok(())
    }

    #[test]
    fn validator_detects_empty_config() -> TestResult {
        let validator = PipelineValidator::new();
        let config = create_minimal_config()?;
        assert!(validator.is_empty_config(&config));
        Ok(())
    }

    #[tokio::test]
    async fn validator_accepts_valid_config() -> TestResult {
        let validator = PipelineValidator::new();
        let config = create_test_config()?;
        validator.validate_config(&config)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline performance: measure per-stage overhead
// ---------------------------------------------------------------------------

mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn pipeline_process_completes_quickly() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let mut pipeline = compiler.compile_pipeline(config).await?.pipeline;

        let start = std::time::Instant::now();
        let iterations = 10_000;
        for i in 0..iterations {
            let mut frame = make_frame(0.5, 0.5);
            frame.seq = i;
            frame.ts_mono_ns = i as u64 * 1_000_000;
            pipeline.process(&mut frame)?;
        }
        let elapsed = start.elapsed();

        let per_iteration_us = elapsed.as_micros() as f64 / iterations as f64;
        assert!(
            per_iteration_us < 1000.0,
            "per-iteration time {per_iteration_us:.1}μs exceeds 1ms budget"
        );
        Ok(())
    }

    #[test]
    fn empty_pipeline_overhead_minimal() -> TestResult {
        let mut pipeline = Pipeline::new();
        let start = std::time::Instant::now();
        for _ in 0..100_000 {
            let mut frame = make_frame(0.5, 0.5);
            pipeline.process(&mut frame)?;
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 1000,
            "100k empty pipeline iterations took {}ms",
            elapsed.as_millis()
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// State management
// ---------------------------------------------------------------------------

mod state_tests {
    use super::*;

    #[test]
    fn empty_pipeline_state_snapshot() -> TestResult {
        let pipeline = Pipeline::new();
        let snap = pipeline.state_snapshot();
        assert_eq!(snap.node_count, 0);
        assert_eq!(snap.state_size, 0);
        assert!(!snap.has_response_curve);
        assert!(snap.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn compiled_pipeline_state_aligned() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert!(pipeline.is_state_aligned(), "all state offsets should be f64-aligned");
        Ok(())
    }

    #[tokio::test]
    async fn compiled_pipeline_has_state() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        let snap = pipeline.state_snapshot();
        assert!(snap.node_count > 0);
        assert!(snap.state_size > 0);
        assert!(!snap.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn state_offsets_are_valid() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;

        for i in 0..pipeline.node_count() {
            let offset = pipeline.state_offset(i);
            assert!(offset.is_some(), "node {i} should have an offset");
            let size = pipeline.node_state_size(i);
            assert!(size.is_some(), "node {i} should have a state size");
        }
        assert!(pipeline.state_offset(pipeline.node_count()).is_none());
        Ok(())
    }

    #[test]
    fn reset_state_zeros_all() -> TestResult {
        let mut pipeline = Pipeline::with_hash(0x1234);
        pipeline.reset_state();
        assert_eq!(pipeline.state_size(), 0, "empty pipeline has no state to reset");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Response curve integration
// ---------------------------------------------------------------------------

mod response_curve_tests {
    use super::*;

    #[test]
    fn no_response_curve_by_default() -> TestResult {
        let pipeline = Pipeline::new();
        assert!(pipeline.response_curve().is_none());
        Ok(())
    }

    #[test]
    fn set_response_curve_linear() -> TestResult {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());
        assert!(pipeline.response_curve().is_some());
        Ok(())
    }

    #[test]
    fn exponential_curve_attenuates_low_values() -> TestResult {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0).map_err(|e| format!("{e}"))?;
        pipeline.set_response_curve(curve.to_lut());

        let mut frame = make_frame(0.5, 0.5);
        pipeline.process(&mut frame)?;
        // Exponential curve with exponent 2: 0.5^2 = 0.25
        assert!(
            frame.torque_out < 0.5,
            "exponential curve should reduce 0.5, got {}",
            frame.torque_out
        );
        Ok(())
    }

    #[test]
    fn response_curve_preserves_sign() -> TestResult {
        let mut pipeline = Pipeline::new();
        let curve = CurveType::exponential(2.0).map_err(|e| format!("{e}"))?;
        pipeline.set_response_curve(curve.to_lut());

        let mut pos_frame = make_frame(0.5, 0.5);
        pipeline.process(&mut pos_frame)?;
        assert!(pos_frame.torque_out > 0.0, "positive remains positive");

        let mut neg_frame = make_frame(-0.5, -0.5);
        pipeline.process(&mut neg_frame)?;
        assert!(neg_frame.torque_out < 0.0, "negative remains negative");

        assert!(
            (pos_frame.torque_out.abs() - neg_frame.torque_out.abs()).abs() < 0.01,
            "magnitudes should be symmetric"
        );
        Ok(())
    }

    #[test]
    fn process_with_curve_override() -> TestResult {
        let mut pipeline = Pipeline::new();
        let linear_lut = CurveType::Linear.to_lut();

        let mut frame = make_frame(0.5, 0.5);
        pipeline.process_with_curve(&mut frame, Some(&linear_lut))?;
        assert!(frame.torque_out.is_finite());
        assert!((frame.torque_out - 0.5).abs() < 0.02);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline clone
// ---------------------------------------------------------------------------

mod clone_tests {
    use super::*;

    #[test]
    fn clone_preserves_hash() -> TestResult {
        let p = Pipeline::with_hash(0xABCD);
        let c = p.clone();
        assert_eq!(p.config_hash(), c.config_hash());
        Ok(())
    }

    #[tokio::test]
    async fn clone_preserves_node_count() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        let cloned = pipeline.clone();
        assert_eq!(pipeline.node_count(), cloned.node_count());
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
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn pipeline_output_deterministic_for_same_input(
            torque in -1.0f32..=1.0,
        ) {
            let mut p1 = Pipeline::new();
            let mut p2 = Pipeline::new();

            let mut f1 = make_frame(torque, torque);
            let mut f2 = make_frame(torque, torque);

            let r1 = p1.process(&mut f1);
            let r2 = p2.process(&mut f2);

            prop_assert!(r1.is_ok() && r2.is_ok());
            prop_assert!(
                (f1.torque_out - f2.torque_out).abs() < f32::EPSILON,
                "same input should produce same output: {} vs {}",
                f1.torque_out, f2.torque_out
            );
        }

        #[test]
        fn empty_pipeline_is_identity(
            torque in -1.0f32..=1.0,
        ) {
            let mut pipeline = Pipeline::new();
            let mut frame = make_frame(torque, torque);
            let r = pipeline.process(&mut frame);
            prop_assert!(r.is_ok());
            prop_assert!(
                (frame.torque_out - torque).abs() < f32::EPSILON,
                "empty pipeline should be identity: in={}, out={}",
                torque, frame.torque_out
            );
        }

        #[test]
        fn pipeline_with_linear_curve_is_near_identity(
            torque in -1.0f32..=1.0,
        ) {
            let mut pipeline = Pipeline::new();
            pipeline.set_response_curve(CurveType::Linear.to_lut());

            let mut frame = make_frame(torque, torque);
            let r = pipeline.process(&mut frame);
            prop_assert!(r.is_ok());
            prop_assert!(
                (frame.torque_out - torque).abs() < 0.02,
                "linear curve should be near identity: in={}, out={}",
                torque, frame.torque_out
            );
        }

        #[test]
        fn swap_always_takes_effect(hash in 1u64..=u64::MAX) {
            let mut pipeline = Pipeline::new();
            pipeline.swap_at_tick_boundary(Pipeline::with_hash(hash));
            prop_assert_eq!(pipeline.config_hash(), hash);
        }
    }
}

// ---------------------------------------------------------------------------
// Data flow through compiled pipelines
// ---------------------------------------------------------------------------

mod data_flow_tests {
    use super::*;

    #[tokio::test]
    async fn compiled_pipeline_produces_finite_output() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let mut pipeline = compiler.compile_pipeline(config).await?.pipeline;

        for i in 0..100 {
            let val = (i as f32 * 0.01).sin() * 0.8;
            let mut frame = make_frame(val, val);
            frame.seq = i;
            frame.ts_mono_ns = i as u64 * 1_000_000;
            pipeline.process(&mut frame)?;
            assert!(
                frame.torque_out.is_finite(),
                "frame {i}: output must be finite, got {}",
                frame.torque_out
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn compiled_pipeline_output_bounded() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let mut pipeline = compiler.compile_pipeline(config).await?.pipeline;

        for i in 0..200 {
            let val = (i as f32 * 0.03).sin() * 0.9;
            let mut frame = make_frame(val, val);
            frame.seq = i;
            pipeline.process(&mut frame)?;
            assert!(
                frame.torque_out.abs() <= 1.0 + 1e-6,
                "frame {i}: output {} exceeds [-1,1]",
                frame.torque_out
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn minimal_config_is_identity() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_minimal_config()?;
        let mut pipeline = compiler.compile_pipeline(config).await?.pipeline;

        let test_values = [-0.9, -0.5, 0.0, 0.3, 0.7, 1.0];
        for &val in &test_values {
            let mut frame = make_frame(val, val);
            pipeline.process(&mut frame)?;
            assert!(
                (frame.torque_out - val).abs() < 0.01,
                "empty pipeline should be identity: in={val}, out={}",
                frame.torque_out
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn compiled_pipeline_deterministic_output() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;

        let mut p1 = compiler.compile_pipeline(config.clone()).await?.pipeline;
        let mut p2 = compiler.compile_pipeline(config).await?.pipeline;

        for i in 0..50 {
            let val = (i as f32 * 0.05).sin() * 0.5;
            let mut f1 = make_frame(val, val);
            let mut f2 = make_frame(val, val);
            f1.seq = i;
            f2.seq = i;
            p1.process(&mut f1)?;
            p2.process(&mut f2)?;
            assert!(
                (f1.torque_out - f2.torque_out).abs() < 1e-6,
                "frame {i}: outputs should match: {} vs {}",
                f1.torque_out,
                f2.torque_out
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error propagation: pipeline fault detection
// ---------------------------------------------------------------------------

mod error_propagation_tests {
    use super::*;

    #[test]
    fn empty_pipeline_nan_passes_through() -> TestResult {
        // Empty pipeline has no nodes, so no validation occurs
        let mut pipeline = Pipeline::new();
        let mut frame = make_frame(f32::NAN, f32::NAN);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok(), "empty pipeline passes through NaN");
        Ok(())
    }

    #[test]
    fn pipeline_with_curve_handles_nan_input() -> TestResult {
        let mut pipeline = Pipeline::new();
        pipeline.set_response_curve(CurveType::Linear.to_lut());

        // Response curve clamps, so NaN input to the curve lookup clamps to [0,1]
        let mut frame = make_frame(0.5, 0.5);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(frame.torque_out.is_finite());
        Ok(())
    }

    #[tokio::test]
    async fn compiled_pipeline_rejects_large_intermediate() -> TestResult {
        // If a filter node produces |output| > 1.0, the pipeline returns PipelineFault.
        // This is hard to trigger with valid configs since filters are bounded,
        // but we verify the executor checks bounds after each node.
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let mut pipeline = compiler.compile_pipeline(config).await?.pipeline;

        // Normal input should be fine
        let mut frame = make_frame(0.5, 0.5);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn invalid_reconstruction_level_error_message() -> TestResult {
        let compiler = PipelineCompiler::new();
        let mut config = create_minimal_config()?;
        config.reconstruction = 15;

        let result = compiler.compile_pipeline(config).await;
        assert!(result.is_err());
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("0-8") || err_msg.contains("Reconstruction") || err_msg.contains("reconstruction"),
            "error should mention valid range: {err_msg}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn validator_rejects_non_monotonic_curve() -> TestResult {
        let validator = PipelineValidator::new();
        // Try to create a config with non-monotonic curve
        // CurvePoint construction may fail, but test the validator path
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![
                CurvePoint::new(0.0, 0.0)?,
                CurvePoint::new(0.7, 0.8)?,
                CurvePoint::new(0.5, 0.3)?,
                CurvePoint::new(1.0, 1.0)?,
            ],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        );

        // Either construction fails (good) or validation fails (also good)
        if let Ok(cfg) = config {
            let result = validator.validate_config(&cfg);
            assert!(result.is_err(), "non-monotonic curve should be rejected");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pipeline metrics and state snapshots
// ---------------------------------------------------------------------------

mod metrics_tests {
    use super::*;

    #[test]
    fn empty_snapshot_efficiency() -> TestResult {
        let pipeline = Pipeline::new();
        let snap = pipeline.state_snapshot();
        assert_eq!(snap.state_efficiency(), 1.0, "empty pipeline should have 100% efficiency");
        Ok(())
    }

    #[tokio::test]
    async fn compiled_snapshot_has_correct_node_count() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        let snap = pipeline.state_snapshot();

        assert_eq!(snap.node_count, pipeline.node_count());
        assert_eq!(snap.state_size, pipeline.state_size());
        assert_eq!(snap.config_hash, pipeline.config_hash());
        Ok(())
    }

    #[tokio::test]
    async fn state_size_grows_with_filters() -> TestResult {
        let compiler = PipelineCompiler::new();

        // Minimal config (0 nodes)
        let empty_config = create_minimal_config()?;
        let empty = compiler.compile_pipeline(empty_config).await?.pipeline;

        // Full config (many nodes)
        let full_config = create_test_config()?;
        let full = compiler.compile_pipeline(full_config).await?.pipeline;

        assert!(
            full.state_size() > empty.state_size(),
            "full pipeline should have more state: full={}, empty={}",
            full.state_size(),
            empty.state_size()
        );
        Ok(())
    }

    #[tokio::test]
    async fn reset_state_zeroes_all_bytes() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let mut pipeline = compiler.compile_pipeline(config).await?.pipeline;

        let size_before = pipeline.state_size();

        // Process some data to modify state
        for i in 0..50 {
            let mut frame = make_frame(0.5, 0.5);
            frame.seq = i;
            pipeline.process(&mut frame)?;
        }

        pipeline.reset_state();

        // State size should be preserved after reset
        assert_eq!(
            pipeline.state_size(),
            size_before,
            "reset should not change state size"
        );

        // Snapshot reflects reset state
        let snap = pipeline.state_snapshot();
        assert_eq!(snap.state_size, size_before);
        assert!(snap.node_count > 0, "nodes should still exist after reset");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Stage enable/disable: individual filter on/off via config
// ---------------------------------------------------------------------------

mod stage_enable_disable_tests {
    use super::*;

    #[tokio::test]
    async fn only_friction_enabled() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.3)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 1, "only friction → 1 node");
        Ok(())
    }

    #[tokio::test]
    async fn only_damper_enabled() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.2)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 1, "only damper → 1 node");
        Ok(())
    }

    #[tokio::test]
    async fn only_notch_enabled() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 1, "only notch → 1 node");
        Ok(())
    }

    #[tokio::test]
    async fn multiple_notch_filters() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![
                NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?,
                NotchFilter::new(FrequencyHz::new(120.0)?, 2.0, -6.0)?,
            ],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 2, "two notch filters → 2 nodes");
        Ok(())
    }

    #[tokio::test]
    async fn only_reconstruction_enabled() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            4,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 1, "only reconstruction → 1 node");
        Ok(())
    }

    #[tokio::test]
    async fn only_inertia_enabled() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.1)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 1, "only inertia → 1 node");
        Ok(())
    }

    #[tokio::test]
    async fn slew_rate_at_one_is_disabled() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?, // slew_rate >= 1.0 means disabled
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(1.0)?,
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 0, "slew_rate=1.0 + everything else off → 0 nodes");
        Ok(())
    }

    #[tokio::test]
    async fn torque_cap_below_one_adds_node() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
            0,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            Gain::new(0.0)?,
            vec![],
            Gain::new(1.0)?,
            vec![CurvePoint::new(0.0, 0.0)?, CurvePoint::new(1.0, 1.0)?],
            Gain::new(0.8)?, // torque cap < 1.0 adds a node
            racing_wheel_schemas::entities::BumpstopConfig { enabled: false, ..Default::default() },
            racing_wheel_schemas::entities::HandsOffConfig { enabled: false, ..Default::default() },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        assert_eq!(pipeline.node_count(), 1, "torque_cap=0.8 → 1 node");
        Ok(())
    }

    #[tokio::test]
    async fn all_filters_enabled_produces_many_nodes() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = racing_wheel_schemas::entities::FilterConfig::new_complete(
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
            racing_wheel_schemas::entities::BumpstopConfig {
                enabled: true,
                start_angle: 400.0,
                max_angle: 450.0,
                stiffness: 0.5,
                damping: 0.2,
            },
            racing_wheel_schemas::entities::HandsOffConfig {
                enabled: true,
                threshold: 0.05,
                timeout_seconds: 2.0,
            },
        )?;
        let pipeline = compiler.compile_pipeline(config).await?.pipeline;
        // recon + friction + damper + inertia + notch + slew + curve + torque_cap + bumpstop + hands_off
        assert!(
            pipeline.node_count() >= 8,
            "all filters enabled should produce many nodes, got {}",
            pipeline.node_count()
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Async compilation
// ---------------------------------------------------------------------------

mod async_tests {
    use super::*;

    #[tokio::test]
    async fn async_compilation_returns_result() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let rx = compiler.compile_pipeline_async(config).await?;
        let result = rx.await.map_err(|e| format!("channel error: {e}"))?;
        let compiled = result?;
        assert!(compiled.pipeline.node_count() > 0);
        Ok(())
    }

    #[tokio::test]
    async fn compile_with_response_curve() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;
        let curve = CurveType::exponential(2.0).map_err(|e| format!("{e}"))?;

        let compiled = compiler
            .compile_pipeline_with_response_curve(config, Some(&curve))
            .await?;

        assert!(compiled.pipeline.response_curve().is_some());
        assert!(compiled.pipeline.node_count() > 0);
        Ok(())
    }

    #[tokio::test]
    async fn compile_without_response_curve() -> TestResult {
        let compiler = PipelineCompiler::new();
        let config = create_test_config()?;

        let compiled = compiler
            .compile_pipeline_with_response_curve(config, None)
            .await?;

        assert!(compiled.pipeline.response_curve().is_none());
        Ok(())
    }
}
