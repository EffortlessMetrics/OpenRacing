//! Mutation-targeted safety tests for the engine crate.
//!
//! Each test is designed to catch a specific class of mutation that
//! cargo-mutants might introduce in safety-critical code paths:
//!
//! - Sign/direction errors in torque clamping
//! - Off-by-one / boundary errors in range checks
//! - Removed safety checks (missing clamp, missing fault, missing e-stop)
//! - Swapped comparisons (< vs >, <= vs >=)
//! - Replaced constants (0.0 vs 1.0, true vs false)

use racing_wheel_engine::safety::{
    FaultType, HardwareWatchdog, SafetyInterlockState, SafetyInterlockSystem, SafetyService,
    SoftwareWatchdog, TorqueLimit, WatchdogError, WatchdogTimeoutHandler,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn armed_system(max_torque: f32) -> Result<SafetyInterlockSystem, WatchdogError> {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    let mut sys = SafetyInterlockSystem::new(watchdog, max_torque);
    sys.arm()?;
    sys.report_communication();
    Ok(sys)
}

// ===========================================================================
// 1. Torque limit — positive boundary
// ===========================================================================

#[test]
fn torque_at_exact_limit_is_not_clamped() -> Result<(), WatchdogError> {
    let max = 25.0_f32;
    let mut sys = armed_system(max)?;
    let result = sys.process_tick(max);
    assert!(
        (result.torque_command - max).abs() < f32::EPSILON,
        "torque at exact limit should pass through: got {}",
        result.torque_command
    );
    Ok(())
}

#[test]
fn torque_just_above_limit_is_clamped() -> Result<(), WatchdogError> {
    let max = 25.0_f32;
    let mut sys = armed_system(max)?;
    let result = sys.process_tick(max + 0.001);
    assert!(
        result.torque_command <= max,
        "torque just above limit must be clamped: got {}",
        result.torque_command
    );
    Ok(())
}

// ===========================================================================
// 2. Torque limit — negative boundary (sign mutation)
// ===========================================================================

#[test]
fn negative_torque_at_exact_limit_is_not_clamped() -> Result<(), WatchdogError> {
    let max = 25.0_f32;
    let mut sys = armed_system(max)?;
    let result = sys.process_tick(-max);
    assert!(
        (result.torque_command - (-max)).abs() < f32::EPSILON,
        "negative torque at exact limit should pass: got {}",
        result.torque_command
    );
    Ok(())
}

#[test]
fn negative_torque_beyond_limit_is_clamped() -> Result<(), WatchdogError> {
    let max = 25.0_f32;
    let mut sys = armed_system(max)?;
    let result = sys.process_tick(-max - 0.001);
    assert!(
        result.torque_command >= -max,
        "negative torque below -max must be clamped: got {}",
        result.torque_command
    );
    Ok(())
}

// ===========================================================================
// 3. Zero torque — replacement mutation (0.0 → 1.0)
// ===========================================================================

#[test]
fn zero_torque_request_produces_zero_output() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    let result = sys.process_tick(0.0);
    assert!(
        result.torque_command.abs() < f32::EPSILON,
        "zero request must yield zero output: got {}",
        result.torque_command
    );
    Ok(())
}

// ===========================================================================
// 4. TorqueLimit::clamp symmetry — catches sign swap
// ===========================================================================

#[test]
fn torque_limit_clamp_positive_overflow() {
    let mut tl = TorqueLimit::new(10.0, 5.0);
    let (clamped, was_limited) = tl.clamp(15.0);
    assert!(
        (clamped - 10.0).abs() < f32::EPSILON,
        "positive overflow should clamp to max: got {clamped}"
    );
    assert!(was_limited, "should report limiting");
}

#[test]
fn torque_limit_clamp_negative_overflow() {
    let mut tl = TorqueLimit::new(10.0, 5.0);
    let (clamped, was_limited) = tl.clamp(-15.0);
    assert!(
        (clamped - (-10.0)).abs() < f32::EPSILON,
        "negative overflow should clamp to -max: got {clamped}"
    );
    assert!(was_limited, "should report limiting");
}

#[test]
fn torque_limit_clamp_within_range_not_limited() {
    let mut tl = TorqueLimit::new(10.0, 5.0);
    let (clamped, was_limited) = tl.clamp(5.0);
    assert!(
        (clamped - 5.0).abs() < f32::EPSILON,
        "within-range value should pass through: got {clamped}"
    );
    assert!(!was_limited, "should not report limiting");
}

#[test]
fn torque_limit_clamp_violation_count_increments() {
    let mut tl = TorqueLimit::new(10.0, 5.0);
    assert_eq!(tl.violation_count, 0);
    let _ = tl.clamp(15.0);
    assert_eq!(tl.violation_count, 1, "first violation should be counted");
    let _ = tl.clamp(-15.0);
    assert_eq!(tl.violation_count, 2, "second violation should be counted");
    let _ = tl.clamp(5.0);
    assert_eq!(tl.violation_count, 2, "within-range should not increment");
}

// ===========================================================================
// 5. Safe mode torque limit — must be lower than max
// ===========================================================================

#[test]
fn safe_mode_limit_is_distinct_from_max() {
    let tl = TorqueLimit::new(25.0, 5.0);
    assert!(
        tl.safe_mode_limit() < tl.max_torque_nm,
        "safe mode limit {} must be less than max {}",
        tl.safe_mode_limit(),
        tl.max_torque_nm
    );
}

// ===========================================================================
// 6. Emergency stop — torque must go to zero
// ===========================================================================

#[test]
fn emergency_stop_zeroes_torque() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    // Apply some torque first
    let _ = sys.process_tick(15.0);
    let result = sys.emergency_stop();
    assert!(
        result.torque_command.abs() < f32::EPSILON,
        "emergency stop must produce zero torque: got {}",
        result.torque_command
    );
    Ok(())
}

#[test]
fn emergency_stop_enters_estop_state() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    let _ = sys.emergency_stop();
    assert!(
        matches!(sys.state(), SafetyInterlockState::EmergencyStop { .. }),
        "must be in EmergencyStop state, got {:?}",
        sys.state()
    );
    Ok(())
}

#[test]
fn torque_stays_zero_after_emergency_stop() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    let _ = sys.emergency_stop();
    // Subsequent ticks must also produce zero
    let result = sys.process_tick(20.0);
    assert!(
        result.torque_command.abs() < f32::EPSILON,
        "torque after e-stop must remain zero: got {}",
        result.torque_command
    );
    Ok(())
}

// ===========================================================================
// 7. Fault reporting — transitions to non-Normal state
// ===========================================================================

#[test]
fn fault_report_leaves_normal_state() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    sys.report_fault(FaultType::Overcurrent);
    assert!(
        !matches!(sys.state(), SafetyInterlockState::Normal),
        "after fault, state must not be Normal: got {:?}",
        sys.state()
    );
    Ok(())
}

#[test]
fn fault_reduces_torque_output() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    let before = sys.process_tick(20.0).torque_command;
    sys.report_fault(FaultType::ThermalLimit);
    let after = sys.process_tick(20.0).torque_command;
    assert!(
        after <= before,
        "torque after fault ({after}) must not exceed pre-fault ({before})"
    );
    Ok(())
}

#[test]
fn fault_is_logged() -> Result<(), WatchdogError> {
    let mut sys = armed_system(25.0)?;
    assert!(sys.fault_log().is_empty(), "fault log should start empty");
    sys.report_fault(FaultType::EncoderNaN);
    assert!(
        !sys.fault_log().is_empty(),
        "fault log must contain entry after fault report"
    );
    Ok(())
}

// ===========================================================================
// 8. Watchdog timeout — must trigger safe state
// ===========================================================================

#[test]
fn watchdog_timeout_handler_zeroes_torque() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);
    assert!(
        response.torque_command.abs() < f32::EPSILON,
        "timeout must command zero torque: got {}",
        response.torque_command
    );
}

#[test]
fn watchdog_timeout_records_previous_torque() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(15.0);
    assert!(
        (response.previous_torque - 15.0).abs() < f32::EPSILON,
        "previous torque should be captured: got {}",
        response.previous_torque
    );
}

#[test]
fn watchdog_timeout_sets_triggered_flag() {
    let mut handler = WatchdogTimeoutHandler::new();
    assert!(
        !handler.is_timeout_triggered(),
        "should not be triggered initially"
    );
    let _ = handler.handle_timeout(10.0);
    assert!(
        handler.is_timeout_triggered(),
        "must be triggered after timeout"
    );
}

#[test]
fn watchdog_timeout_response_within_budget() {
    let mut handler = WatchdogTimeoutHandler::new();
    let response = handler.handle_timeout(10.0);
    assert!(
        response.within_budget,
        "timeout response must complete within timing budget"
    );
}

// ===========================================================================
// 9. SafetyService — torque clamping in safe mode
// ===========================================================================

#[test]
fn safety_service_clamp_limits_to_safe_torque() {
    let svc = SafetyService::default(); // 5Nm safe, 25Nm high
    let clamped = svc.clamp_torque_nm(100.0);
    assert!(
        clamped <= 5.0,
        "in safe mode, torque must be ≤ 5Nm: got {clamped}"
    );
}

#[test]
fn safety_service_clamp_preserves_sign() {
    let svc = SafetyService::default();
    let positive = svc.clamp_torque_nm(100.0);
    let negative = svc.clamp_torque_nm(-100.0);
    assert!(
        positive > 0.0,
        "positive input should yield positive output"
    );
    assert!(
        negative < 0.0,
        "negative input should yield negative output"
    );
}

#[test]
fn safety_service_clamp_symmetric() {
    let svc = SafetyService::default();
    let pos = svc.clamp_torque_nm(100.0);
    let neg = svc.clamp_torque_nm(-100.0);
    assert!(
        (pos + neg).abs() < f32::EPSILON,
        "clamping must be symmetric: pos={pos}, neg={neg}"
    );
}

// ===========================================================================
// 10. Software watchdog arm/disarm state
// ===========================================================================

#[test]
fn software_watchdog_starts_disarmed() {
    let wd = SoftwareWatchdog::new(100);
    assert!(!wd.is_armed(), "watchdog must start disarmed");
}

#[test]
fn software_watchdog_arm_then_feed() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    assert!(wd.is_armed(), "watchdog must be armed after arm()");
    wd.feed()?;
    assert!(
        !wd.has_timed_out(),
        "freshly fed watchdog must not be timed out"
    );
    Ok(())
}

#[test]
fn software_watchdog_double_arm_fails() -> Result<(), WatchdogError> {
    let mut wd = SoftwareWatchdog::new(100);
    wd.arm()?;
    let result = wd.arm();
    assert!(result.is_err(), "double arm must fail: got {:?}", result);
    Ok(())
}

#[test]
fn software_watchdog_feed_when_disarmed_fails() {
    let mut wd = SoftwareWatchdog::new(100);
    let result = wd.feed();
    assert!(result.is_err(), "feed on disarmed watchdog must fail");
}
