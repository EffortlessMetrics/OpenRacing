//! Edge-case tests for Pipeline processing that complement existing
//! compilation and atomicity tests.
//!
//! Focus areas:
//! - Empty pipeline processing (passthrough behavior)
//! - Response curve application during processing
//! - Pipeline metadata queries (`is_empty`, `node_count`, `config_hash`)
//! - Compiled pipeline with response curve end-to-end
//! - Frame field preservation through processing

use racing_wheel_engine::curves::CurveLut;
use racing_wheel_engine::pipeline::{Pipeline, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_schemas::prelude::FilterConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_frame(ffb_in: f32, torque_out: f32) -> Frame {
    Frame {
        ffb_in,
        torque_out,
        wheel_speed: 0.0,
        hands_off: false,
        ts_mono_ns: 0,
        seq: 0,
    }
}

// =========================================================================
// Empty pipeline: passthrough behavior
// =========================================================================

#[test]
fn empty_pipeline_process_succeeds() {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.5, 0.5);

    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
}

#[test]
fn empty_pipeline_preserves_torque_out() {
    let mut pipeline = Pipeline::new();
    let mut frame = make_frame(0.8, 0.42);

    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!((frame.torque_out - 0.42).abs() < f32::EPSILON);
}

#[test]
fn empty_pipeline_preserves_all_frame_fields() {
    let mut pipeline = Pipeline::new();
    let mut frame = Frame {
        ffb_in: 0.7,
        torque_out: 0.35,
        wheel_speed: 12.5,
        hands_off: true,
        ts_mono_ns: 123_456_789,
        seq: 42,
    };

    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!((frame.ffb_in - 0.7).abs() < f32::EPSILON);
    assert!((frame.torque_out - 0.35).abs() < f32::EPSILON);
    assert!((frame.wheel_speed - 12.5).abs() < f32::EPSILON);
    assert!(frame.hands_off);
    assert_eq!(frame.ts_mono_ns, 123_456_789);
    assert_eq!(frame.seq, 42);
}

// =========================================================================
// Pipeline metadata queries
// =========================================================================

#[test]
fn new_pipeline_is_empty() {
    let pipeline = Pipeline::new();
    assert!(pipeline.is_empty());
    assert_eq!(pipeline.node_count(), 0);
    assert_eq!(pipeline.config_hash(), 0);
}

#[test]
fn pipeline_with_hash_records_hash() {
    let pipeline = Pipeline::with_hash(0xDEAD_BEEF);
    assert_eq!(pipeline.config_hash(), 0xDEAD_BEEF);
    assert!(pipeline.is_empty());
}

// =========================================================================
// Response curve application on empty pipeline
// =========================================================================

#[test]
fn empty_pipeline_with_linear_response_curve_is_identity() {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());
    assert!(pipeline.response_curve().is_some());

    let mut frame = make_frame(0.5, 0.6);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());

    // Linear curve: output ≈ input (within LUT interpolation tolerance)
    assert!(
        (frame.torque_out - 0.6).abs() < 0.01,
        "Expected ~0.6, got {}",
        frame.torque_out
    );
}

#[test]
fn response_curve_preserves_sign_for_negative_torque() {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::linear());

    let mut frame = make_frame(0.0, -0.5);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());

    // Negative torque: magnitude mapped through curve, sign preserved
    assert!(
        frame.torque_out < 0.0,
        "Expected negative output, got {}",
        frame.torque_out
    );
    assert!(
        (frame.torque_out + 0.5).abs() < 0.01,
        "Expected ~-0.5, got {}",
        frame.torque_out
    );
}

#[test]
fn response_curve_zero_torque_stays_zero() {
    let mut pipeline = Pipeline::new();
    // Custom curve: squares input (aggressive)
    pipeline.set_response_curve(CurveLut::from_fn(|x| x * x));

    let mut frame = make_frame(0.0, 0.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    assert!(
        frame.torque_out.abs() < f32::EPSILON,
        "Zero torque should stay zero"
    );
}

#[test]
fn response_curve_full_torque_maps_to_curve_max() {
    let mut pipeline = Pipeline::new();
    pipeline.set_response_curve(CurveLut::from_fn(|x| x * x));

    let mut frame = make_frame(0.0, 1.0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());

    // x² at 1.0 = 1.0
    assert!(
        (frame.torque_out - 1.0).abs() < 0.01,
        "Expected ~1.0 for full torque through x² curve, got {}",
        frame.torque_out
    );
}

#[test]
fn response_curve_half_torque_maps_through_curve() {
    let mut pipeline = Pipeline::new();
    // x² curve: 0.5 → 0.25
    pipeline.set_response_curve(CurveLut::from_fn(|x| x * x));

    let mut frame = make_frame(0.0, 0.5);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());

    assert!(
        (frame.torque_out - 0.25).abs() < 0.02,
        "Expected ~0.25 for 0.5 through x² curve, got {}",
        frame.torque_out
    );
}

// =========================================================================
// Pipeline swap semantics
// =========================================================================

#[test]
fn swap_at_tick_boundary_replaces_pipeline() {
    let mut pipeline1 = Pipeline::new();
    let pipeline2 = Pipeline::with_hash(0xCAFE_BABE);

    assert_eq!(pipeline1.config_hash(), 0);
    pipeline1.swap_at_tick_boundary(pipeline2);
    assert_eq!(pipeline1.config_hash(), 0xCAFE_BABE);
}

#[test]
fn swap_at_tick_boundary_replaces_response_curve() {
    let mut pipeline1 = Pipeline::new();
    pipeline1.set_response_curve(CurveLut::linear());
    assert!(pipeline1.response_curve().is_some());

    // Swap with a pipeline that has no response curve
    let pipeline2 = Pipeline::new();
    pipeline1.swap_at_tick_boundary(pipeline2);
    assert!(pipeline1.response_curve().is_none());
}

// =========================================================================
// Compiled pipeline with filters produces bounded output
// =========================================================================

#[tokio::test]
async fn compiled_default_pipeline_produces_bounded_output()
-> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let config = FilterConfig::default();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    for i in 0..100 {
        let input = (i as f32 / 100.0) * 2.0 - 1.0; // Range [-1, 1]
        let mut frame = Frame {
            ffb_in: input,
            torque_out: input,
            wheel_speed: 5.0,
            hands_off: false,
            ts_mono_ns: i as u64 * 1_000_000,
            seq: i as u16,
        };

        let result = pipeline.process(&mut frame);
        assert!(
            result.is_ok(),
            "Pipeline failed at input {input}: {:?}",
            result
        );
        assert!(
            frame.torque_out.is_finite(),
            "Non-finite output at input {input}: {}",
            frame.torque_out
        );
        assert!(
            frame.torque_out.abs() <= 1.0,
            "Output out of [-1,1] bounds at input {input}: {}",
            frame.torque_out
        );
    }

    Ok(())
}

#[tokio::test]
async fn compiled_pipeline_has_nonzero_hash() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let config = FilterConfig::default();
    let compiled = compiler.compile_pipeline(config).await?;

    assert!(compiled.config_hash != 0);
    assert_eq!(compiled.pipeline.config_hash(), compiled.config_hash);
    Ok(())
}
