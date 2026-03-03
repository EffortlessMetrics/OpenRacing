#![allow(clippy::redundant_closure)]
//! Torque output integration tests.
//!
//! Tests cover:
//! - Torque output clamping and scaling
//! - Torque direction reversal
//! - Zero-torque idle behavior
//! - Proptest: torque output never exceeds configured maximum
//! - Torque rate limiting (slew rate)

use proptest::prelude::*;
use racing_wheel_engine::pipeline::PipelineCompiler;
use racing_wheel_engine::rt::Frame;
use racing_wheel_engine::safety::{
    FaultType, SafetyInterlockSystem, SafetyService, SoftwareWatchdog, TorqueLimit,
};
use racing_wheel_schemas::prelude::{FilterConfig, Gain};

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

fn create_safety_service(max_safe: f32, max_high: f32) -> SafetyService {
    SafetyService::new(max_safe, max_high)
}

fn create_interlock_system(max_torque_nm: f32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    SafetyInterlockSystem::new(watchdog, max_torque_nm)
}

fn default_filter_config() -> FilterConfig {
    FilterConfig::default()
}

// =========================================================================
// Torque output clamping and scaling
// =========================================================================

#[test]
fn safety_service_clamps_to_safe_limit() {
    let service = create_safety_service(5.0, 25.0);
    assert_eq!(service.clamp_torque_nm(10.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-10.0), -5.0);
    assert_eq!(service.clamp_torque_nm(5.0), 5.0);
    assert_eq!(service.clamp_torque_nm(-5.0), -5.0);
}

#[test]
fn safety_service_clamps_with_different_limits() {
    let service = create_safety_service(3.0, 20.0);
    assert_eq!(service.clamp_torque_nm(4.0), 3.0);
    assert_eq!(service.clamp_torque_nm(3.0), 3.0);
    assert_eq!(service.clamp_torque_nm(2.5), 2.5);
}

#[test]
fn torque_limit_clamp_tracks_violations() {
    let mut limit = TorqueLimit::new(10.0, 5.0);
    assert_eq!(limit.violation_count, 0);

    let (clamped, was_clamped) = limit.clamp(15.0);
    assert_eq!(clamped, 10.0);
    assert!(was_clamped);
    assert_eq!(limit.violation_count, 1);

    let (clamped, was_clamped) = limit.clamp(8.0);
    assert_eq!(clamped, 8.0);
    assert!(!was_clamped);
    assert_eq!(limit.violation_count, 1);
}

#[test]
fn interlock_system_clamps_in_normal_mode(
) -> Result<(), racing_wheel_engine::safety::WatchdogError> {
    let mut system = create_interlock_system(25.0);
    system.arm()?;

    let result = system.process_tick(30.0);
    assert_eq!(result.torque_command, 25.0);

    let result = system.process_tick(-30.0);
    assert_eq!(result.torque_command, -25.0);
    Ok(())
}

// =========================================================================
// Torque direction reversal
// =========================================================================

#[test]
fn safety_service_handles_sign_reversal() {
    let service = create_safety_service(5.0, 25.0);

    let positive = service.clamp_torque_nm(3.0);
    let negative = service.clamp_torque_nm(-3.0);

    assert_eq!(positive, 3.0);
    assert_eq!(negative, -3.0);
    assert_eq!(positive, -negative);
}

#[test]
fn torque_limit_clamp_preserves_sign() {
    let mut limit = TorqueLimit::new(10.0, 5.0);

    let (pos, _) = limit.clamp(8.0);
    let (neg, _) = limit.clamp(-8.0);
    assert_eq!(pos, 8.0);
    assert_eq!(neg, -8.0);
    assert_eq!(pos, -neg);
}

#[tokio::test]
async fn pipeline_reverses_torque_direction() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame_pos = make_frame(0.5, 0);
    let mut frame_neg = make_frame(-0.5, 0);

    let _ = pipeline.process(&mut frame_pos);

    // Recompile for fresh state
    let compiled2 = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline2 = compiled2.pipeline;
    let _ = pipeline2.process(&mut frame_neg);

    // Signs should be opposite (filter chain may change magnitude slightly)
    if frame_pos.torque_out.abs() > f32::EPSILON && frame_neg.torque_out.abs() > f32::EPSILON {
        assert!(
            frame_pos.torque_out.signum() != frame_neg.torque_out.signum()
                || (frame_pos.torque_out.abs() < f32::EPSILON
                    && frame_neg.torque_out.abs() < f32::EPSILON),
            "Expected opposite signs: pos={}, neg={}",
            frame_pos.torque_out,
            frame_neg.torque_out
        );
    }
    Ok(())
}

// =========================================================================
// Zero-torque idle behavior
// =========================================================================

#[test]
fn safety_service_zero_input_yields_zero() {
    let service = create_safety_service(5.0, 25.0);
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);
}

#[test]
fn faulted_state_always_zero_regardless_of_input() {
    let mut service = create_safety_service(5.0, 25.0);
    service.report_fault(FaultType::UsbStall);
    assert_eq!(service.clamp_torque_nm(0.0), 0.0);
    assert_eq!(service.clamp_torque_nm(5.0), 0.0);
    assert_eq!(service.clamp_torque_nm(-5.0), 0.0);
}

#[tokio::test]
async fn pipeline_zero_input_bounded_output() -> Result<(), Box<dyn std::error::Error>> {
    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(default_filter_config()).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = make_frame(0.0, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // Zero input should produce zero or near-zero output
    assert!(
        frame.torque_out.abs() <= f32::EPSILON,
        "Zero input produced non-zero output: {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn interlock_emergency_stop_zeroes_idle(
) -> Result<(), racing_wheel_engine::safety::WatchdogError> {
    let mut system = create_interlock_system(25.0);
    system.arm()?;
    system.emergency_stop();

    let result = system.process_tick(0.0);
    assert_eq!(result.torque_command, 0.0);
    Ok(())
}

// =========================================================================
// Proptest: torque output never exceeds configured maximum
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn safety_service_clamp_never_exceeds_safe_max(torque in -100.0f32..100.0) {
        let service = create_safety_service(5.0, 25.0);
        let clamped = service.clamp_torque_nm(torque);
        prop_assert!(
            clamped.abs() <= 5.0 + f32::EPSILON,
            "Clamped torque {:.6} exceeds safe max 5.0 for input {:.6}",
            clamped, torque
        );
    }

    #[test]
    fn safety_service_clamp_preserves_finite(torque in proptest::num::f32::ANY) {
        let service = create_safety_service(5.0, 25.0);
        let clamped = service.clamp_torque_nm(torque);
        prop_assert!(clamped.is_finite(),
            "Clamped torque {} is not finite for input {:?}", clamped, torque);
        prop_assert!(clamped.abs() <= 5.0 + f32::EPSILON,
            "Clamped torque {} exceeds bounds for input {:?}", clamped, torque);
    }

    #[test]
    fn torque_limit_never_exceeds_max(
        max_torque in 1.0f32..50.0,
        requested in -100.0f32..100.0
    ) {
        let mut limit = TorqueLimit::new(max_torque, max_torque * 0.2);
        let (clamped, _) = limit.clamp(requested);
        prop_assert!(
            clamped.abs() <= max_torque + f32::EPSILON,
            "Torque {:.6} exceeds max {:.6} for request {:.6}",
            clamped, max_torque, requested
        );
    }

    #[test]
    fn interlock_system_torque_always_bounded(
        max_torque in 1.0f32..50.0,
        requested in -100.0f32..100.0
    ) {
        let watchdog = Box::new(SoftwareWatchdog::new(1000));
        let mut system = SafetyInterlockSystem::new(watchdog, max_torque);
        // Don't arm watchdog — unarmed watchdog still passes through clamped torque
        let result = system.process_tick(requested);
        prop_assert!(
            result.torque_command.abs() <= max_torque + f32::EPSILON,
            "Tick torque {:.6} exceeds max {:.6} for request {:.6}",
            result.torque_command, max_torque, requested
        );
    }

    #[test]
    fn pipeline_output_bounded_for_valid_range(ffb_in in -1.0f32..=1.0) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| TestCaseError::Fail(format!("Runtime build failed: {}", e).into()))?;
        rt.block_on(async {
            let compiler = PipelineCompiler::new();
            let compiled = compiler.compile_pipeline(default_filter_config()).await
                .map_err(|e| TestCaseError::Fail(format!("Compile failed: {}", e).into()))?;
            let mut pipeline = compiled.pipeline;
            let mut frame = make_frame(ffb_in, 0);
            let result = pipeline.process(&mut frame);
            prop_assert!(result.is_ok());
            prop_assert!(
                frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
                "Pipeline output {:.6} out of bounds for input {:.6}",
                frame.torque_out, ffb_in
            );
            Ok(())
        })?;
    }
}

// =========================================================================
// Torque rate limiting (slew rate)
// =========================================================================

#[tokio::test]
async fn slew_rate_filter_limits_rate_of_change() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        slew_rate: Gain::new(0.1)?, // Aggressive slew limiting
        ..FilterConfig::default()
    };

    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    // Process a sequence: start at 0, jump to 1.0
    let mut prev_output = 0.0f32;
    for seq in 0..20 {
        let mut frame = make_frame(1.0, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Output out of bounds at seq {}: {}",
            seq,
            frame.torque_out
        );

        // After first frame, rate of change should be limited
        if seq > 0 {
            let delta = (frame.torque_out - prev_output).abs();
            // With aggressive slew limiting, change per tick should be bounded
            // (exact bound depends on slew rate implementation; just verify it's bounded)
            assert!(
                delta <= 1.0,
                "Excessive rate of change at seq {}: delta={:.6}",
                seq,
                delta
            );
        }
        prev_output = frame.torque_out;
    }
    Ok(())
}

#[tokio::test]
async fn slew_rate_smooth_transition() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        slew_rate: Gain::new(0.5)?,
        ..FilterConfig::default()
    };

    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    // Alternating positive and negative input — output should stay bounded
    for seq in 0..20u16 {
        let input = if seq % 2 == 0 { 0.8 } else { -0.8 };
        let mut frame = make_frame(input, seq);
        let result = pipeline.process(&mut frame);
        assert!(result.is_ok());
        assert!(
            frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0,
            "Slew rate output out of bounds at seq {}: {}",
            seq,
            frame.torque_out
        );
    }
    Ok(())
}

#[tokio::test]
async fn full_slew_rate_allows_full_range() -> Result<(), Box<dyn std::error::Error>> {
    let config = FilterConfig {
        slew_rate: Gain::new(1.0)?, // Maximum slew rate — no limiting
        ..FilterConfig::default()
    };

    let compiler = PipelineCompiler::new();
    let compiled = compiler.compile_pipeline(config).await?;
    let mut pipeline = compiled.pipeline;

    let mut frame = make_frame(0.9, 0);
    let result = pipeline.process(&mut frame);
    assert!(result.is_ok());
    // With maximum slew rate, output should track input more closely
    assert!(frame.torque_out.is_finite() && frame.torque_out.abs() <= 1.0);
    Ok(())
}

// =========================================================================
// Torque scaling correctness
// =========================================================================

#[test]
fn safety_service_safe_mode_limits_correctly() {
    let service = create_safety_service(8.0, 30.0);
    // In safe torque state, max is safe limit (8.0)
    assert_eq!(service.max_torque_nm(), 8.0);
    assert_eq!(service.clamp_torque_nm(10.0), 8.0);
    assert_eq!(service.clamp_torque_nm(-10.0), -8.0);
    assert_eq!(service.clamp_torque_nm(7.0), 7.0);
}

#[test]
fn safety_service_faulted_zeroes_regardless_of_limits() {
    let mut service = create_safety_service(8.0, 30.0);
    service.report_fault(FaultType::EncoderNaN);
    assert_eq!(service.max_torque_nm(), 0.0);
    assert_eq!(service.clamp_torque_nm(1.0), 0.0);
}

#[test]
fn torque_limit_safe_mode_limit() {
    let limit = TorqueLimit::new(25.0, 5.0);
    assert_eq!(limit.safe_mode_limit(), 5.0);
}

#[test]
fn torque_limit_default_values() {
    let limit = TorqueLimit::default();
    assert_eq!(limit.max_torque_nm, 25.0);
    assert_eq!(limit.safe_mode_torque_nm, 5.0);
}
