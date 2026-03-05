//! Hardening tests for the FFB processing pipeline.
//!
//! Tests pipeline stage composition, data flow through stages, stage error
//! handling, and pipeline metrics.

use openracing_pipeline::prelude::*;
use openracing_pipeline::{Frame, PipelineStateSnapshot};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_frame() -> Frame {
    Frame {
        ffb_in: 0.5,
        torque_out: 0.5,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 1,
    }
}

fn frame_with(ffb_in: f32, torque_out: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 1,
    }
}

// ===========================================================================
// 1. Pipeline stage composition
// ===========================================================================

#[test]
fn test_empty_pipeline_passthrough() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    let mut frame = default_frame();
    let original_torque = frame.torque_out;

    pipeline.process(&mut frame)?;
    assert!(
        (frame.torque_out - original_torque).abs() < f32::EPSILON,
        "Empty pipeline should pass through unchanged"
    );
    Ok(())
}

#[test]
fn test_pipeline_with_hash_creation() {
    let pipeline = Pipeline::with_hash(0xDEADBEEF);
    assert_eq!(pipeline.config_hash(), 0xDEADBEEF);
    assert!(pipeline.is_empty());
    assert_eq!(pipeline.node_count(), 0);
}

#[test]
fn test_pipeline_default_is_empty() {
    let pipeline = Pipeline::default();
    assert!(pipeline.is_empty());
    assert_eq!(pipeline.node_count(), 0);
    assert_eq!(pipeline.config_hash(), 0);
}

#[test]
fn test_pipeline_response_curve_linear() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(openracing_curves::CurveLut::linear());

    let mut frame = frame_with(0.5, 0.5);
    pipeline.process(&mut frame)?;

    // Linear curve: output ≈ input
    assert!(
        (frame.torque_out - 0.5).abs() < 0.02,
        "Linear curve should preserve value roughly, got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn test_pipeline_response_curve_can_be_set_and_read() {
    let mut pipeline = Pipeline::new();
    assert!(pipeline.response_curve().is_none());

    pipeline.set_response_curve(openracing_curves::CurveLut::linear());
    assert!(pipeline.response_curve().is_some());
}

#[test]
fn test_pipeline_clone_preserves_hash() {
    let original = Pipeline::with_hash(0x12345678);
    let cloned = original.clone();
    assert_eq!(original.config_hash(), cloned.config_hash());
}

#[test]
fn test_pipeline_clone_preserves_empty_state() {
    let original = Pipeline::new();
    let cloned = original.clone();
    assert!(cloned.is_empty());
    assert_eq!(cloned.node_count(), original.node_count());
}

// ===========================================================================
// 2. Data flow through stages
// ===========================================================================

#[test]
fn test_process_multiple_frames_sequentially() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();

    for i in 0..100 {
        let mut frame = Frame {
            ffb_in: (i as f32) * 0.01,
            torque_out: (i as f32) * 0.01,
            wheel_speed: 10.0,
            hands_off: false,
            ts_mono_ns: i * 1_000_000,
            seq: i as u16,
        };
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite(),
            "torque_out should always be finite"
        );
    }
    Ok(())
}

#[test]
fn test_process_with_response_curve_preserves_sign() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Pipeline::new();
    let curve = openracing_curves::CurveType::exponential(2.0)?;
    pipeline.set_response_curve(curve.to_lut());

    let mut frame_pos = frame_with(0.5, 0.5);
    pipeline.process(&mut frame_pos)?;
    assert!(
        frame_pos.torque_out > 0.0,
        "Positive input → positive output"
    );

    let mut frame_neg = frame_with(-0.5, -0.5);
    pipeline.process(&mut frame_neg)?;
    assert!(
        frame_neg.torque_out < 0.0,
        "Negative input → negative output"
    );

    let diff = (frame_pos.torque_out.abs() - frame_neg.torque_out.abs()).abs();
    assert!(diff < 0.01, "Magnitudes should be equal, diff = {diff}");
    Ok(())
}

#[test]
fn test_process_with_curve_override() -> Result<(), Box<dyn std::error::Error>> {
    let mut pipeline = Pipeline::new();
    let override_curve = openracing_curves::CurveLut::linear();

    let mut frame = frame_with(0.7, 0.7);
    pipeline.process_with_curve(&mut frame, Some(&override_curve))?;

    assert!(
        (frame.torque_out - 0.7).abs() < 0.02,
        "Override linear curve should preserve value"
    );
    Ok(())
}

#[test]
fn test_process_with_curve_none_override() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(openracing_curves::CurveLut::linear());

    let mut frame = frame_with(0.5, 0.5);
    // Passing None as the override should bypass any curve
    pipeline.process_with_curve(&mut frame, None)?;

    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "No override curve should leave torque unchanged (empty pipeline)"
    );
    Ok(())
}

#[test]
fn test_process_zero_torque() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(openracing_curves::CurveLut::linear());

    let mut frame = frame_with(0.0, 0.0);
    pipeline.process(&mut frame)?;
    assert!(
        frame.torque_out.abs() < 0.01,
        "Zero torque should stay near zero"
    );
    Ok(())
}

#[test]
fn test_process_boundary_torque_values() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(openracing_curves::CurveLut::linear());

    for &torque in &[-1.0f32, -0.5, 0.0, 0.5, 1.0] {
        let mut frame = frame_with(torque, torque);
        pipeline.process(&mut frame)?;
        assert!(
            frame.torque_out.is_finite(),
            "torque_out should be finite for input {torque}"
        );
    }
    Ok(())
}

// ===========================================================================
// 3. Stage error handling
// ===========================================================================

#[test]
fn test_pipeline_empty_process_does_not_fail() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    let mut frame = default_frame();
    pipeline.process(&mut frame)?;
    Ok(())
}

#[test]
fn test_pipeline_nan_torque_passthrough_empty() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    let mut frame = frame_with(0.5, f32::NAN);
    // Empty pipeline has no nodes, so no per-node validation occurs
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok(), "Empty pipeline passes NaN through");
    Ok(())
}

#[test]
fn test_pipeline_infinity_torque_passthrough_empty() -> Result<(), openracing_errors::RTError> {
    let mut pipeline = Pipeline::new();
    let mut frame = frame_with(0.5, f32::INFINITY);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok(), "Empty pipeline passes infinity through");
    Ok(())
}

#[test]
fn test_pipeline_error_types() {
    let err = PipelineError::InvalidConfig("test".to_string());
    assert!(format!("{err}").contains("test"));

    let err = PipelineError::CompilationFailed("compile err".to_string());
    assert!(format!("{err}").contains("compile err"));

    let err = PipelineError::SwapFailed("swap err".to_string());
    assert!(format!("{err}").contains("swap err"));

    let err = PipelineError::NonMonotonicCurve;
    assert!(format!("{err}").contains("Non-monotonic"));

    let err = PipelineError::InvalidParameters("bad param".to_string());
    assert!(format!("{err}").contains("bad param"));
}

// ===========================================================================
// 4. Pipeline metrics
// ===========================================================================

#[test]
fn test_pipeline_state_snapshot_empty() {
    let pipeline = Pipeline::new();
    let snapshot = pipeline.state_snapshot();

    assert_eq!(snapshot.node_count, 0);
    assert_eq!(snapshot.state_size, 0);
    assert_eq!(snapshot.config_hash, 0);
    assert!(!snapshot.has_response_curve);
    assert!(snapshot.is_empty());
}

#[test]
fn test_pipeline_state_snapshot_with_curve() {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(openracing_curves::CurveLut::linear());

    let snapshot = pipeline.state_snapshot();
    assert!(snapshot.has_response_curve);
}

#[test]
fn test_pipeline_state_snapshot_with_hash() {
    let pipeline = Pipeline::with_hash(0xCAFE);
    let snapshot = pipeline.state_snapshot();
    assert_eq!(snapshot.config_hash, 0xCAFE);
}

#[test]
fn test_pipeline_state_size_empty() {
    let pipeline = Pipeline::new();
    assert_eq!(pipeline.state_size(), 0);
}

#[test]
fn test_pipeline_state_aligned_empty() {
    let pipeline = Pipeline::new();
    assert!(pipeline.is_state_aligned());
}

#[test]
fn test_pipeline_state_offset_out_of_bounds() {
    let pipeline = Pipeline::new();
    assert!(pipeline.state_offset(0).is_none());
    assert!(pipeline.state_offset(99).is_none());
}

#[test]
fn test_pipeline_node_state_size_out_of_bounds() {
    let pipeline = Pipeline::new();
    assert!(pipeline.node_state_size(0).is_none());
}

#[test]
fn test_pipeline_reset_state_no_panic() {
    let mut pipeline = Pipeline::new();
    pipeline.reset_state();
    assert_eq!(pipeline.state_size(), 0);
}

#[test]
fn test_snapshot_state_efficiency() {
    let snapshot = PipelineStateSnapshot {
        node_count: 0,
        state_size: 0,
        config_hash: 0,
        has_response_curve: false,
    };
    assert_eq!(snapshot.state_efficiency(), 1.0);
}

#[test]
fn test_snapshot_is_empty() {
    let empty = PipelineStateSnapshot {
        node_count: 0,
        state_size: 0,
        config_hash: 0,
        has_response_curve: false,
    };
    assert!(empty.is_empty());

    let non_empty = PipelineStateSnapshot {
        node_count: 3,
        state_size: 64,
        config_hash: 0xBEEF,
        has_response_curve: true,
    };
    assert!(!non_empty.is_empty());
}

// ===========================================================================
// Pipeline swap
// ===========================================================================

#[test]
fn test_pipeline_swap_at_tick_boundary() {
    let mut p1 = Pipeline::new();
    let p2 = Pipeline::with_hash(0xABCD);

    assert_eq!(p1.config_hash(), 0);
    p1.swap_at_tick_boundary(p2);
    assert_eq!(p1.config_hash(), 0xABCD);
}

#[test]
fn test_pipeline_swap_preserves_curve() {
    let mut p1 = Pipeline::new();

    let mut p2 = Pipeline::with_hash(0x1234);
    p2.set_response_curve(openracing_curves::CurveLut::linear());

    p1.swap_at_tick_boundary(p2);
    assert!(p1.response_curve().is_some());
    assert_eq!(p1.config_hash(), 0x1234);
}

// ===========================================================================
// Config hash
// ===========================================================================

#[test]
fn test_config_hash_deterministic() {
    let config = racing_wheel_schemas::entities::FilterConfig::default();
    let h1 = calculate_config_hash(&config);
    let h2 = calculate_config_hash(&config);
    assert_eq!(h1, h2);
}

#[test]
fn test_config_hash_nonzero_for_default() {
    let config = racing_wheel_schemas::entities::FilterConfig::default();
    let hash = calculate_config_hash(&config);
    assert_ne!(hash, 0, "Default config should produce nonzero hash");
}

#[test]
fn test_config_hash_with_curve_differs() -> Result<(), openracing_curves::CurveError> {
    let config = racing_wheel_schemas::entities::FilterConfig::default();

    let h_none = calculate_config_hash_with_curve(&config, None);
    let h_linear =
        calculate_config_hash_with_curve(&config, Some(&openracing_curves::CurveType::Linear));
    let exp = openracing_curves::CurveType::exponential(2.0)?;
    let h_exp = calculate_config_hash_with_curve(&config, Some(&exp));

    assert_ne!(h_none, h_linear);
    assert_ne!(h_linear, h_exp);
    Ok(())
}

// ===========================================================================
// Validator
// ===========================================================================

#[test]
fn test_validator_default_config_valid() {
    let validator = PipelineValidator::new();
    let config = racing_wheel_schemas::entities::FilterConfig::default();
    assert!(validator.validate_config(&config).is_ok());
}

#[test]
fn test_validator_invalid_reconstruction_level() {
    let validator = PipelineValidator::new();
    let config = racing_wheel_schemas::entities::FilterConfig {
        reconstruction: 10,
        ..Default::default()
    };
    let result = validator.validate_config(&config);
    assert!(result.is_err());
}

#[test]
fn test_validator_linear_response_curve() {
    let validator = PipelineValidator::new();
    assert!(
        validator
            .validate_response_curve(&openracing_curves::CurveType::Linear)
            .is_ok()
    );
}

#[test]
fn test_validator_is_empty_config() {
    let validator = PipelineValidator::new();
    let config = racing_wheel_schemas::entities::FilterConfig {
        bumpstop: racing_wheel_schemas::entities::BumpstopConfig {
            enabled: false,
            ..Default::default()
        },
        hands_off: racing_wheel_schemas::entities::HandsOffConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(validator.is_empty_config(&config));
}
