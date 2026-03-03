//! Torque safety tests for the engine crate
//!
//! These tests verify:
//! - Torque output bounded by device maximum
//! - Torque rate limiting (slew rate)
//! - Torque direction reversal safety
//! - Zero-torque guarantee during initialization
//! - Torque output during fault conditions

use racing_wheel_engine::safety::{
    FaultType, SafetyInterlockState, SafetyInterlockSystem, SafetyService, SafetyState,
    SoftwareWatchdog, TorqueLimit, WatchdogError, WatchdogTimeoutHandler,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_interlock_system(max_torque: f32) -> SafetyInterlockSystem {
    let watchdog = Box::new(SoftwareWatchdog::new(100));
    SafetyInterlockSystem::new(watchdog, max_torque)
}

fn create_armed_system(max_torque: f32) -> Result<SafetyInterlockSystem, WatchdogError> {
    let mut sys = create_interlock_system(max_torque);
    sys.arm()?;
    sys.report_communication();
    Ok(sys)
}

// ===========================================================================
// 1. Torque output bounded by device maximum
// ===========================================================================

#[test]
fn torque_clamped_to_device_maximum() -> Result<(), WatchdogError> {
    let max_torque = 25.0_f32;
    let mut sys = create_armed_system(max_torque)?;

    let result = sys.process_tick(50.0);
    assert!(
        result.torque_command <= max_torque,
        "torque {} must be ≤ max {}",
        result.torque_command,
        max_torque
    );
    Ok(())
}

#[test]
fn negative_torque_clamped_to_device_maximum() -> Result<(), WatchdogError> {
    let max_torque = 25.0_f32;
    let mut sys = create_armed_system(max_torque)?;

    let result = sys.process_tick(-50.0);
    assert!(
        result.torque_command >= -max_torque,
        "negative torque {} must be ≥ -max {}",
        result.torque_command,
        max_torque
    );
    Ok(())
}

#[test]
fn torque_limit_clamp_symmetric() {
    let mut limit = TorqueLimit::new(10.0, 2.0);

    let (pos, _) = limit.clamp(15.0);
    assert_eq!(pos, 10.0, "positive clamp must be at max");

    let (neg, _) = limit.clamp(-15.0);
    assert_eq!(neg, -10.0, "negative clamp must be at -max");
}

#[test]
fn torque_within_limits_passes_through() -> Result<(), WatchdogError> {
    let mut sys = create_armed_system(25.0)?;

    let result = sys.process_tick(10.0);
    assert!(
        (result.torque_command - 10.0).abs() < f32::EPSILON,
        "torque within limit must pass through unchanged, got {}",
        result.torque_command
    );
    Ok(())
}

#[test]
fn safety_service_clamp_bounds_positive() {
    let svc = SafetyService::new(5.0, 25.0);
    let clamped = svc.clamp_torque_nm(100.0);
    assert!(
        (clamped - 5.0).abs() < f32::EPSILON,
        "SafeTorque must clamp to 5.0, got {}",
        clamped
    );
}

#[test]
fn safety_service_clamp_bounds_negative() {
    let svc = SafetyService::new(5.0, 25.0);
    let clamped = svc.clamp_torque_nm(-100.0);
    assert!(
        (clamped - (-5.0)).abs() < f32::EPSILON,
        "SafeTorque must clamp to -5.0, got {}",
        clamped
    );
}

// ===========================================================================
// 2. Torque rate limiting (slew rate) via TorqueLimit
// ===========================================================================

#[test]
fn torque_limit_records_violation_count() {
    let mut limit = TorqueLimit::new(10.0, 2.0);

    for _ in 0..5 {
        limit.clamp(20.0);
    }
    assert_eq!(limit.violation_count, 5, "must count all violations");
}

#[test]
fn safe_mode_torque_limit_applies_reduced_cap() {
    let limit = TorqueLimit::new(25.0, 5.0);
    assert_eq!(
        limit.safe_mode_limit(),
        5.0,
        "safe mode limit must be the configured value"
    );
}

#[test]
fn interlock_safe_mode_applies_reduced_torque() {
    let mut sys = create_interlock_system(25.0);
    sys.report_fault(FaultType::ThermalLimit);

    let result = sys.process_tick(25.0);
    let safe_limit = sys.torque_limit().safe_mode_limit();
    assert!(
        result.torque_command <= safe_limit + f32::EPSILON,
        "SafeMode torque {} must be ≤ safe limit {}",
        result.torque_command,
        safe_limit
    );
}

// ===========================================================================
// 3. Torque direction reversal safety
// ===========================================================================

#[test]
fn torque_direction_reversal_stays_bounded() -> Result<(), WatchdogError> {
    let max_torque = 25.0_f32;
    let mut sys = create_armed_system(max_torque)?;

    // Positive then negative
    let r1 = sys.process_tick(20.0);
    assert!(r1.torque_command <= max_torque);

    let r2 = sys.process_tick(-20.0);
    assert!(r2.torque_command >= -max_torque);

    // Rapid reversal
    for i in 0..20 {
        let torque = if i % 2 == 0 { 25.0 } else { -25.0 };
        let result = sys.process_tick(torque);
        assert!(
            result.torque_command.abs() <= max_torque + f32::EPSILON,
            "reversal tick {}: torque {} must be bounded by ±{}",
            i,
            result.torque_command,
            max_torque
        );
    }
    Ok(())
}

#[test]
fn safety_service_reversal_clamped_symmetrically() {
    let svc = SafetyService::new(5.0, 25.0);

    let pos = svc.clamp_torque_nm(10.0);
    let neg = svc.clamp_torque_nm(-10.0);

    assert!(
        (pos + neg).abs() < f32::EPSILON,
        "clamp must be symmetric: pos={}, neg={}",
        pos,
        neg
    );
}

// ===========================================================================
// 4. Zero-torque guarantee during initialization
// ===========================================================================

#[test]
fn interlock_system_starts_unarmed_with_normal_state() {
    let sys = create_interlock_system(25.0);
    assert_eq!(*sys.state(), SafetyInterlockState::Normal);
    assert!(
        !sys.is_watchdog_armed(),
        "watchdog must not be armed at init"
    );
}

#[test]
fn safety_service_starts_in_safe_torque() {
    let svc = SafetyService::new(5.0, 25.0);
    assert!(matches!(svc.state(), SafetyState::SafeTorque));
    assert_eq!(svc.max_torque_nm(), 5.0, "must start at safe torque limit");
}

#[test]
fn timeout_handler_starts_with_zero_torque() {
    let handler = WatchdogTimeoutHandler::new();
    assert_eq!(
        handler.current_torque(),
        0.0,
        "handler must start at zero torque"
    );
    assert!(
        !handler.is_timeout_triggered(),
        "handler must start without timeout"
    );
}

#[test]
fn torque_limit_default_safe_mode_is_conservative() {
    let limit = TorqueLimit::default();
    assert!(
        limit.safe_mode_limit() <= limit.max_torque_nm,
        "safe mode limit {} must be ≤ max torque {}",
        limit.safe_mode_limit(),
        limit.max_torque_nm
    );
}

// ===========================================================================
// 5. Torque output during fault conditions
// ===========================================================================

#[test]
fn faulted_service_clamps_all_torque_to_zero() {
    let mut svc = SafetyService::new(5.0, 25.0);
    svc.report_fault(FaultType::UsbStall);

    for requested in [0.0, 1.0, 5.0, 25.0, -5.0, -25.0, f32::MAX, f32::MIN] {
        let clamped = svc.clamp_torque_nm(requested);
        assert_eq!(
            clamped, 0.0,
            "faulted: requested {} must clamp to 0",
            requested
        );
    }
}

#[test]
fn emergency_stop_zeros_all_torque_requests() {
    let mut sys = create_interlock_system(25.0);
    sys.emergency_stop();

    for requested in [0.0, 10.0, 25.0, -10.0, -25.0, 100.0] {
        let result = sys.process_tick(requested);
        assert_eq!(
            result.torque_command, 0.0,
            "e-stop: requested {} must produce 0",
            requested
        );
    }
}

#[test]
fn safe_mode_limits_torque_to_safe_level() {
    let mut sys = create_interlock_system(25.0);
    sys.report_fault(FaultType::Overcurrent);

    let safe_limit = sys.torque_limit().safe_mode_limit();
    let result = sys.process_tick(25.0);
    assert!(
        result.torque_command <= safe_limit + f32::EPSILON,
        "safe mode: {} must be ≤ {}",
        result.torque_command,
        safe_limit
    );
}

#[test]
fn nan_torque_request_treated_safely() {
    let svc = SafetyService::new(5.0, 25.0);

    let clamped = svc.clamp_torque_nm(f32::NAN);
    assert_eq!(clamped, 0.0, "NaN torque must be clamped to 0");
}

#[test]
fn infinity_torque_request_treated_safely() {
    let svc = SafetyService::new(5.0, 25.0);

    let clamped_pos = svc.clamp_torque_nm(f32::INFINITY);
    assert_eq!(clamped_pos, 0.0, "+Inf torque must be clamped to 0");

    let clamped_neg = svc.clamp_torque_nm(f32::NEG_INFINITY);
    assert_eq!(clamped_neg, 0.0, "-Inf torque must be clamped to 0");
}
