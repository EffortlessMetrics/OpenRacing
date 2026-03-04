//! Mutation-targeted filter tests for the openracing-filters crate.
//!
//! Each test is designed to catch a specific class of mutation that
//! cargo-mutants might introduce in filter processing code:
//!
//! - Sign/direction errors in torque output
//! - Removed clamp/bounds checks
//! - Off-by-one in LUT indexing
//! - Swapped coefficients (0.0 vs 1.0)
//! - Broken saturation logic

use openracing_filters::Frame;
use openracing_filters::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn frame_with_torque(torque: f32) -> Frame {
    let mut f = Frame::from_torque(torque);
    f.torque_out = torque;
    f
}

fn frame_with_speed(torque: f32, wheel_speed: f32) -> Frame {
    let mut f = Frame::from_ffb(torque, wheel_speed);
    f.torque_out = torque;
    f
}

// ===========================================================================
// 1. Torque cap — positive clamping
// ===========================================================================

#[test]
fn torque_cap_clamps_positive_overflow() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = frame_with_torque(1.5);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out <= 1.0,
        "positive overflow must be clamped: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn torque_cap_clamps_negative_overflow() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = frame_with_torque(-1.5);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out >= -1.0,
        "negative overflow must be clamped: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn torque_cap_preserves_within_range() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = frame_with_torque(0.5);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "in-range value must pass through: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn torque_cap_symmetric_clamping() -> Result<(), Box<dyn std::error::Error>> {
    let mut pos = frame_with_torque(2.0);
    let mut neg = frame_with_torque(-2.0);
    torque_cap_filter(&mut pos, 1.0);
    torque_cap_filter(&mut neg, 1.0);
    assert!(
        (pos.torque_out + neg.torque_out).abs() < f32::EPSILON,
        "clamping must be symmetric: pos={}, neg={}",
        pos.torque_out,
        neg.torque_out
    );
    Ok(())
}

// ===========================================================================
// 2. Torque cap — non-finite handling
// ===========================================================================

#[test]
fn torque_cap_handles_nan() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = frame_with_torque(f32::NAN);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out.is_finite(),
        "NaN input must produce finite output: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn torque_cap_handles_positive_infinity() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = frame_with_torque(f32::INFINITY);
    torque_cap_filter(&mut frame, 1.0);
    assert!(
        frame.torque_out <= 1.0 && frame.torque_out.is_finite(),
        "+inf must be clamped to finite: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 3. Slew rate — limits rate of change
// ===========================================================================

#[test]
fn slew_rate_limits_large_positive_step() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::slow(); // very limited rate
    let mut frame = frame_with_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        frame.torque_out < 1.0,
        "large step from 0 must be rate-limited: got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out > 0.0,
        "limited output must be positive: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn slew_rate_limits_large_negative_step() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::slow();
    let mut frame = frame_with_torque(-1.0);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        frame.torque_out > -1.0,
        "large negative step must be limited: got {}",
        frame.torque_out
    );
    assert!(
        frame.torque_out < 0.0,
        "limited output must be negative: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn slew_rate_symmetric() -> Result<(), Box<dyn std::error::Error>> {
    let mut pos_state = SlewRateState::medium();
    let mut neg_state = SlewRateState::medium();
    let mut pos_frame = frame_with_torque(1.0);
    let mut neg_frame = frame_with_torque(-1.0);
    slew_rate_filter(&mut pos_frame, &mut pos_state);
    slew_rate_filter(&mut neg_frame, &mut neg_state);
    assert!(
        (pos_frame.torque_out + neg_frame.torque_out).abs() < f32::EPSILON,
        "slew rate must be symmetric: pos={}, neg={}",
        pos_frame.torque_out,
        neg_frame.torque_out
    );
    Ok(())
}

#[test]
fn slew_rate_unlimited_passes_through() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::unlimited();
    let mut frame = frame_with_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 1.0).abs() < 0.01,
        "unlimited slew rate should pass through: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn slew_rate_converges_to_target_over_multiple_ticks() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::medium();
    let target = 0.8;
    for _ in 0..10_000 {
        let mut frame = frame_with_torque(target);
        slew_rate_filter(&mut frame, &mut state);
    }
    // After many ticks the output should be very close to target
    let mut final_frame = frame_with_torque(target);
    slew_rate_filter(&mut final_frame, &mut state);
    assert!(
        (final_frame.torque_out - target).abs() < 0.01,
        "slew rate must converge to target: got {}",
        final_frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 4. Damper — opposes motion
// ===========================================================================

#[test]
fn damper_positive_speed_reduces_torque() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::fixed(0.5);
    let mut frame = frame_with_speed(1.0, 10.0);
    damper_filter(&mut frame, &state);
    assert!(
        frame.torque_out < 1.0,
        "damper with positive speed must reduce torque: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn damper_zero_speed_no_effect() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::fixed(0.5);
    let mut frame = frame_with_speed(1.0, 0.0);
    damper_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 1.0).abs() < 0.01,
        "damper with zero speed should have minimal effect: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn damper_zero_coefficient_no_effect() -> Result<(), Box<dyn std::error::Error>> {
    let state = DamperState::fixed(0.0);
    let mut frame = frame_with_speed(1.0, 10.0);
    damper_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 1.0).abs() < f32::EPSILON,
        "zero damper coefficient should not affect torque: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 5. Friction — opposes direction of motion
// ===========================================================================

#[test]
fn friction_positive_speed_opposes() -> Result<(), Box<dyn std::error::Error>> {
    let state = FrictionState::fixed(0.5);
    let mut frame = frame_with_speed(0.0, 10.0);
    friction_filter(&mut frame, &state);
    // Friction should add a force opposing the direction of motion
    // With positive speed, friction force should be negative (or reduce positive torque)
    assert!(
        frame.torque_out <= 0.01,
        "friction should oppose positive motion: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn friction_zero_coefficient_no_effect() -> Result<(), Box<dyn std::error::Error>> {
    let state = FrictionState::fixed(0.0);
    let mut frame = frame_with_speed(0.5, 10.0);
    friction_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "zero friction coefficient should not affect torque: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 6. Bumpstop — inactive below start angle
// ===========================================================================

#[test]
fn bumpstop_disabled_has_no_effect() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = BumpstopState::disabled();
    let mut frame = frame_with_torque(0.5);
    bumpstop_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "disabled bumpstop must not affect torque: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn bumpstop_zero_angle_has_no_spring_force() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = BumpstopState::standard();
    // At zero angle (below start), bumpstop should not engage
    state.current_angle = 0.0;
    let mut frame = frame_with_speed(0.5, 0.0); // zero wheel speed
    bumpstop_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.01,
        "bumpstop below start angle should not affect torque: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 7. Notch filter — bypass mode
// ===========================================================================

#[test]
fn notch_bypass_passes_through() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = NotchState::bypass();
    let mut frame = frame_with_torque(0.7);
    notch_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.7).abs() < 0.01,
        "bypass notch should pass through: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 8. Reconstruction — bypass vs active
// ===========================================================================

#[test]
fn reconstruction_bypass_passes_through() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = ReconstructionState::bypass();
    let mut frame = Frame::from_ffb(0.6, 0.0);
    frame.torque_out = 0.6;
    reconstruction_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.6).abs() < 0.01,
        "bypass reconstruction should pass through: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn reconstruction_heavy_smooths_step() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = ReconstructionState::heavy();
    // Apply a step input
    let mut frame = Frame::from_ffb(1.0, 0.0);
    frame.torque_out = 0.0;
    reconstruction_filter(&mut frame, &mut state);
    // Heavy smoothing should not immediately reach the target
    assert!(
        frame.torque_out < 1.0,
        "heavy reconstruction should smooth step input: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 9. Curve filter — linear passthrough
// ===========================================================================

#[test]
fn curve_linear_preserves_midpoint() -> Result<(), Box<dyn std::error::Error>> {
    let state = CurveState::linear();
    let mut frame = frame_with_torque(0.5);
    curve_filter(&mut frame, &state);
    assert!(
        (frame.torque_out - 0.5).abs() < 0.02,
        "linear curve at midpoint should preserve value: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn curve_linear_preserves_extremes() -> Result<(), Box<dyn std::error::Error>> {
    let state = CurveState::linear();
    let mut frame_zero = frame_with_torque(0.0);
    curve_filter(&mut frame_zero, &state);
    assert!(
        frame_zero.torque_out.abs() < 0.02,
        "linear curve at zero: got {}",
        frame_zero.torque_out
    );

    let mut frame_one = frame_with_torque(1.0);
    curve_filter(&mut frame_one, &state);
    assert!(
        (frame_one.torque_out - 1.0).abs() < 0.02,
        "linear curve at 1.0: got {}",
        frame_one.torque_out
    );
    Ok(())
}

// ===========================================================================
// 10. Response curve — linear identity
// ===========================================================================

#[test]
fn response_curve_linear_preserves_sign() -> Result<(), Box<dyn std::error::Error>> {
    let state = ResponseCurveState::linear();
    let mut pos = frame_with_torque(0.5);
    response_curve_filter(&mut pos, &state);
    assert!(
        pos.torque_out > 0.0,
        "positive input through linear response curve should remain positive: got {}",
        pos.torque_out
    );

    let mut neg = frame_with_torque(-0.5);
    response_curve_filter(&mut neg, &state);
    assert!(
        neg.torque_out < 0.0,
        "negative input should remain negative: got {}",
        neg.torque_out
    );
    Ok(())
}

// ===========================================================================
// 11. Inertia — zero coefficient has no effect
// ===========================================================================

#[test]
fn inertia_zero_coefficient_no_effect() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = InertiaState::new(0.0);
    let mut frame = frame_with_speed(0.5, 10.0);
    inertia_filter(&mut frame, &mut state);
    assert!(
        (frame.torque_out - 0.5).abs() < f32::EPSILON,
        "zero inertia coefficient should not affect torque: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 12. FilterState::reset — clears internal state
// ===========================================================================

#[test]
fn slew_rate_reset_clears_prev_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::slow();
    // Build up some state
    let mut frame = frame_with_torque(0.5);
    slew_rate_filter(&mut frame, &mut state);
    assert!(state.prev_output != 0.0, "should have nonzero state");
    state.reset();
    assert!(
        state.prev_output.abs() < f32::EPSILON,
        "reset must clear prev_output: got {}",
        state.prev_output
    );
    Ok(())
}

#[test]
fn bumpstop_reset_clears_angle() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = BumpstopState::standard();
    state.current_angle = 45.0;
    state.reset();
    assert!(
        state.current_angle.abs() < f32::EPSILON,
        "reset must clear current_angle: got {}",
        state.current_angle
    );
    Ok(())
}

// ===========================================================================
// 13. Torque cap — boundary at exactly max_torque
// ===========================================================================

#[test]
fn torque_cap_at_exact_boundary_passes() -> Result<(), Box<dyn std::error::Error>> {
    let max = 0.8;
    let mut frame = frame_with_torque(max);
    torque_cap_filter(&mut frame, max);
    assert!(
        (frame.torque_out - max).abs() < f32::EPSILON,
        "value at exact boundary should pass through: got {}",
        frame.torque_out
    );
    Ok(())
}

#[test]
fn torque_cap_at_negative_exact_boundary_passes() -> Result<(), Box<dyn std::error::Error>> {
    let max = 0.8;
    let mut frame = frame_with_torque(-max);
    torque_cap_filter(&mut frame, max);
    assert!(
        (frame.torque_out - (-max)).abs() < f32::EPSILON,
        "negative value at exact boundary should pass through: got {}",
        frame.torque_out
    );
    Ok(())
}

// ===========================================================================
// 14. Slew rate — prev_output tracking
// ===========================================================================

#[test]
fn slew_rate_tracks_output_for_next_tick() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = SlewRateState::slow();
    let mut frame = frame_with_torque(1.0);
    slew_rate_filter(&mut frame, &mut state);
    let first_output = frame.torque_out;
    assert!(
        (state.prev_output - first_output).abs() < f32::EPSILON,
        "prev_output must track last output: state={}, output={}",
        state.prev_output,
        first_output
    );
    Ok(())
}

// ===========================================================================
// 15. Torque cap — zero max_torque forces zero output
// ===========================================================================

#[test]
fn torque_cap_zero_max_forces_zero() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = frame_with_torque(0.5);
    torque_cap_filter(&mut frame, 0.0);
    assert!(
        frame.torque_out.abs() < f32::EPSILON,
        "zero max_torque must force zero output: got {}",
        frame.torque_out
    );
    Ok(())
}
