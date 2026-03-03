//! RT no-allocation enforcement tests
//!
//! The real-time hot path must NEVER allocate after initialization.
//! These tests use the global `TrackingAllocator` (set when `#[cfg(test)]`)
//! to detect any heap activity during pipeline processing, filter execution,
//! safety interlock checks, and bounded-buffer operations.

use racing_wheel_engine::allocation_tracker::{AllocationGuard, track};
use racing_wheel_engine::pipeline::{FilterNodeFn, PipelineCompiler};
use racing_wheel_engine::rt::Frame;
use racing_wheel_engine::safety::{FaultType, SafetyService};
use racing_wheel_schemas::prelude::{CurvePoint, FilterConfig, FrequencyHz, Gain, NotchFilter};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_frame(ffb_in: f32, seq: u16) -> Frame {
    Frame {
        ffb_in,
        torque_out: 0.0,
        wheel_speed: 5.0,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn default_config() -> FilterConfig {
    FilterConfig::default()
}

fn full_config() -> Result<FilterConfig, Box<dyn std::error::Error>> {
    Ok(FilterConfig {
        reconstruction: 4,
        friction: Gain::new(0.15)?,
        damper: Gain::new(0.20)?,
        inertia: Gain::new(0.10)?,
        notch_filters: vec![NotchFilter::new(FrequencyHz::new(60.0)?, 2.0, -12.0)?],
        slew_rate: Gain::new(0.80)?,
        curve_points: vec![
            CurvePoint::new(0.0, 0.0)?,
            CurvePoint::new(0.25, 0.20)?,
            CurvePoint::new(0.50, 0.45)?,
            CurvePoint::new(0.75, 0.75)?,
            CurvePoint::new(1.0, 1.0)?,
        ],
        ..FilterConfig::default()
    })
}

/// Assert that the guard recorded zero allocations, returning an error otherwise.
fn assert_no_allocs(guard: &AllocationGuard, ctx: &str) -> Result<(), String> {
    let count = guard.allocations_since_start();
    let bytes = guard.bytes_allocated_since_start();
    if count > 0 {
        Err(format!(
            "{ctx}: {count} allocations ({bytes} bytes) in RT path"
        ))
    } else {
        Ok(())
    }
}

// ===========================================================================
// 1. Pipeline processing on fixed-size Frame (not Vec)
// ===========================================================================

#[tokio::test]
async fn alloc_01_pipeline_process_default_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline = compiled.pipeline;

    // warm up
    let mut warmup = make_frame(0.3, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    let mut frame = make_frame(0.5, 1);
    let result = pipeline.process(&mut frame);
    assert_no_allocs(&guard, "default pipeline single frame")?;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn alloc_02_pipeline_process_full_config_no_alloc() -> Result<(), Box<dyn std::error::Error>>
{
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.3, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    let mut frame = make_frame(0.7, 1);
    let _ = pipeline.process(&mut frame);
    assert_no_allocs(&guard, "full pipeline single frame")?;
    Ok(())
}

#[tokio::test]
async fn alloc_03_pipeline_1000_frames_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.1, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for i in 1..=1000u16 {
        let mut frame = make_frame((i as f32 / 1000.0).sin() * 0.8, i);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "1000-frame sustained loop")?;
    Ok(())
}

// ===========================================================================
// 2. Filter chain processes without String/Vec/HashMap creation
// ===========================================================================

#[tokio::test]
async fn alloc_04_filter_chain_no_string_vec_creation() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.2, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=500u16 {
        let mut frame = make_frame(0.6, seq);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "filter chain sustained (no String/Vec/HashMap)")?;
    Ok(())
}

#[tokio::test]
async fn alloc_05_notch_filter_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        notch_filters: vec![
            NotchFilter::new(FrequencyHz::new(50.0)?, 3.0, -18.0)?,
            NotchFilter::new(FrequencyHz::new(120.0)?, 1.5, -6.0)?,
        ],
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=200u16 {
        let mut frame = make_frame(0.4, seq);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "notch filter chain")?;
    Ok(())
}

#[tokio::test]
async fn alloc_06_reconstruction_filter_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        reconstruction: 8,
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.1, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=200u16 {
        let mut frame = make_frame(0.9, seq);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "reconstruction filter")?;
    Ok(())
}

#[tokio::test]
async fn alloc_07_damper_friction_inertia_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        friction: Gain::new(0.25)?,
        damper: Gain::new(0.30)?,
        inertia: Gain::new(0.15)?,
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=300u16 {
        let mut frame = Frame {
            ffb_in: (seq as f32 * 0.01).sin() * 0.7,
            torque_out: 0.0,
            wheel_speed: 10.0 + (seq as f32 * 0.02).cos() * 5.0,
            hands_off: false,
            ts_mono_ns: seq as u64 * 1_000_000,
            seq,
        };
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "damper+friction+inertia chain")?;
    Ok(())
}

#[tokio::test]
async fn alloc_08_slew_rate_filter_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        slew_rate: Gain::new(0.50)?,
        ..FilterConfig::default()
    };
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.0, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    // Large step changes to exercise slew limiting
    for seq in 1..=100u16 {
        let val = if seq % 2 == 0 { 0.9 } else { -0.9 };
        let mut frame = make_frame(val, seq);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "slew rate filter")?;
    Ok(())
}

// ===========================================================================
// 3. Safety interlock checks use no heap allocation
// ===========================================================================

#[test]
fn alloc_09_safety_clamp_torque_no_alloc() -> Result<(), String> {
    let service = SafetyService::new(5.0, 25.0);
    let guard = track();

    // Clamp various torque values
    let _ = service.clamp_torque_nm(100.0);
    let _ = service.clamp_torque_nm(-100.0);
    let _ = service.clamp_torque_nm(0.0);
    let _ = service.clamp_torque_nm(f32::NAN);
    let _ = service.clamp_torque_nm(f32::INFINITY);
    let _ = service.clamp_torque_nm(f32::NEG_INFINITY);
    let _ = service.clamp_torque_nm(5.0);
    let _ = service.clamp_torque_nm(-5.0);

    assert_no_allocs(&guard, "safety clamp_torque")
}

#[test]
fn alloc_10_safety_max_torque_no_alloc() -> Result<(), String> {
    let service = SafetyService::new(5.0, 25.0);
    let guard = track();

    for _ in 0..1000 {
        let _ = service.max_torque_nm();
    }

    assert_no_allocs(&guard, "safety max_torque_nm")
}

#[test]
fn alloc_11_safety_state_query_no_alloc() -> Result<(), String> {
    let service = SafetyService::new(5.0, 25.0);
    let guard = track();

    for _ in 0..1000 {
        let _ = service.state();
        let _ = service.max_torque_nm();
    }

    assert_no_allocs(&guard, "safety state query")
}

#[test]
fn alloc_12_safety_clamp_faulted_state_no_alloc() -> Result<(), String> {
    let mut service = SafetyService::new(5.0, 25.0);
    service.report_fault(FaultType::UsbStall);

    let guard = track();
    // In faulted state, clamp must still be allocation-free
    for _ in 0..500 {
        let clamped = service.clamp_torque_nm(10.0);
        assert!((clamped - 0.0).abs() < f32::EPSILON);
    }

    assert_no_allocs(&guard, "safety clamp in faulted state")
}

#[test]
fn alloc_13_safety_challenge_expiry_check_no_alloc() -> Result<(), String> {
    let service = SafetyService::new(5.0, 25.0);
    let guard = track();

    // Challenge time remaining queries must not allocate
    for _ in 0..500 {
        let _ = service.get_challenge_time_remaining();
    }

    assert_no_allocs(&guard, "challenge expiry check")
}

// ===========================================================================
// 4. Bounded buffer sizes / pre-allocated buffers
// ===========================================================================

#[tokio::test]
async fn alloc_14_pipeline_node_vec_preallocated() -> Result<(), Box<dyn std::error::Error>> {
    // After compilation, the pipeline's internal Vecs are fully allocated.
    // Processing must not grow them.
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    // Run many frames — if the pipeline tried to push into its internal Vecs
    // this would trigger allocations.
    for seq in 1..=2000u16 {
        let mut frame = make_frame((seq as f32 * 0.005).sin(), seq);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "pipeline buffers pre-allocated (2000 frames)")?;
    Ok(())
}

#[test]
fn alloc_15_frame_is_stack_allocated() {
    // Frame is Copy + repr(C) — it lives on the stack, no heap.
    let guard = track();
    let frame = Frame {
        ffb_in: 0.5,
        torque_out: 0.0,
        wheel_speed: 10.0,
        hands_off: false,
        ts_mono_ns: 12345,
        seq: 42,
    };
    let _copy = frame; // Copy, not Clone
    assert_eq!(
        guard.allocations_since_start(),
        0,
        "Frame construction must not allocate"
    );
}

#[test]
fn alloc_16_frame_fixed_size_not_dynamic() {
    // Frame must have a known compile-time size (no Vec/String fields)
    let size = std::mem::size_of::<Frame>();
    assert!(size > 0, "Frame has non-zero size");
    // Frame contains f32 × 3 + bool + u64 + u16; should be well under 64 bytes
    assert!(
        size <= 64,
        "Frame unexpectedly large ({size} bytes), may contain heap types"
    );
}

// ===========================================================================
// 5. Allocation tracker integration — detect allocations during RT processing
// ===========================================================================

#[tokio::test]
async fn alloc_17_extreme_inputs_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.1, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    let extremes: &[f32] = &[
        0.0,
        1.0,
        -1.0,
        0.999999,
        -0.999999,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
    ];
    for (i, &val) in extremes.iter().enumerate() {
        let mut frame = make_frame(val, i as u16 + 1);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "extreme input values")?;
    Ok(())
}

#[tokio::test]
async fn alloc_18_rapid_sign_changes_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(full_config()?).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.0, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=500u16 {
        let val = if seq % 2 == 0 { 0.8 } else { -0.8 };
        let mut frame = make_frame(val, seq);
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "rapid sign changes")?;
    Ok(())
}

#[tokio::test]
async fn alloc_19_varying_wheel_speed_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let config = FilterConfig {
        friction: Gain::new(0.20)?,
        damper: Gain::new(0.25)?,
        ..FilterConfig::default()
    };
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.5, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=300u16 {
        let mut frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.0,
            wheel_speed: (seq as f32 * 0.1).sin() * 20.0,
            hands_off: false,
            ts_mono_ns: seq as u64 * 1_000_000,
            seq,
        };
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "varying wheel speed")?;
    Ok(())
}

#[tokio::test]
async fn alloc_20_hands_off_flag_no_alloc() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut warmup = make_frame(0.3, 0);
    let _ = pipeline.process(&mut warmup);

    let guard = track();
    for seq in 1..=200u16 {
        let mut frame = Frame {
            ffb_in: 0.5,
            torque_out: 0.0,
            wheel_speed: 5.0,
            hands_off: seq % 3 == 0, // toggle hands-off periodically
            ts_mono_ns: seq as u64 * 1_000_000,
            seq,
        };
        let _ = pipeline.process(&mut frame);
    }
    assert_no_allocs(&guard, "hands-off flag toggling")?;
    Ok(())
}

#[test]
fn alloc_21_filter_node_fn_is_thin_pointer() {
    // FilterNodeFn must be a plain function pointer — not a boxed closure
    // that would require heap allocation per invocation.
    assert_eq!(
        std::mem::size_of::<FilterNodeFn>(),
        std::mem::size_of::<usize>(),
        "FilterNodeFn must be a thin function pointer (no heap indirection)"
    );
}
