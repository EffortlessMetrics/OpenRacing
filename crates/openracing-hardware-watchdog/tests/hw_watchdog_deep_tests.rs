//! Deep tests for the hardware watchdog subsystem.
//!
//! Covers:
//! - Hardware watchdog abstraction
//! - Platform-specific behaviors
//! - Watchdog configuration
//! - Recovery actions
//! - Fault escalation
//! - State machine transitions
//! - Error conditions

use openracing_hardware_watchdog::prelude::*;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_watchdog(timeout_ms: u32) -> Result<SoftwareWatchdog, HardwareWatchdogError> {
    let config = WatchdogConfig::new(timeout_ms)?;
    Ok(SoftwareWatchdog::new(config))
}

// ===========================================================================
// 1. Hardware watchdog abstraction (basic lifecycle)
// ===========================================================================

#[test]
fn hw_lifecycle_basic_arm_feed_disarm() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    assert!(!wd.is_armed());

    wd.arm()?;
    assert_eq!(wd.status(), WatchdogStatus::Armed);
    assert!(wd.is_armed());

    wd.feed()?;
    assert!(wd.is_armed());
    assert!(!wd.has_timed_out());

    wd.disarm()?;
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);

    Ok(())
}

#[test]
fn hw_lifecycle_default_constructor() {
    let wd = SoftwareWatchdog::with_default_timeout();
    assert_eq!(wd.timeout_ms(), 100);
    assert!(!wd.is_armed());
    assert!(!wd.has_timed_out());
    assert!(!wd.is_safe_state_triggered());
    assert!(wd.is_healthy());
}

#[test]
fn hw_lifecycle_with_timeout_constructor() -> Result<(), HardwareWatchdogError> {
    let wd = SoftwareWatchdog::with_timeout(200)?;
    assert_eq!(wd.timeout_ms(), 200);
    Ok(())
}

#[test]
fn hw_lifecycle_with_timeout_invalid() {
    let result = SoftwareWatchdog::with_timeout(5);
    assert!(result.is_err());

    let result = SoftwareWatchdog::with_timeout(6000);
    assert!(result.is_err());
}

#[test]
fn hw_lifecycle_default_trait() {
    let wd = SoftwareWatchdog::default();
    assert_eq!(wd.timeout_ms(), 100);
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
}

#[test]
fn hw_lifecycle_time_since_last_feed_none_initially() {
    let wd = SoftwareWatchdog::with_default_timeout();
    assert!(wd.time_since_last_feed_us().is_none());
}

#[test]
fn hw_lifecycle_time_since_last_feed_some_after_feed() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;
    wd.feed()?;
    let elapsed = wd.time_since_last_feed_us();
    assert!(elapsed.is_some());
    Ok(())
}

#[test]
fn hw_lifecycle_config_accessor() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(200)?;
    let cfg = wd.config();
    assert_eq!(cfg.timeout_ms, 200);

    wd.arm()?;
    let cfg = wd.config();
    assert_eq!(cfg.timeout_ms, 200);
    Ok(())
}

// ===========================================================================
// 2. Platform-specific behaviors (timeout precision)
// ===========================================================================

#[test]
fn hw_timeout_precision_immediate_no_timeout() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(50)?;
    wd.arm()?;
    wd.feed()?;

    assert!(!wd.has_timed_out());
    assert!(wd.is_armed());
    Ok(())
}

#[test]
fn hw_timeout_precision_times_out_after_deadline() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(50)?;
    wd.arm()?;
    wd.feed()?;

    std::thread::sleep(std::time::Duration::from_millis(80));

    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);
    Ok(())
}

#[test]
fn hw_timeout_feed_prevents_timeout() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;

    for _ in 0..5 {
        wd.feed()?;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    assert!(!wd.has_timed_out());
    assert!(wd.is_armed());
    Ok(())
}

#[test]
fn hw_timeout_manual_trigger() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;
    wd.trigger_timeout()?;

    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);
    Ok(())
}

#[test]
fn hw_timeout_trigger_from_disarmed_fails() {
    let wd = make_watchdog(100);
    assert!(wd.is_ok());
    if let Ok(wd) = wd {
        let result = wd.trigger_timeout();
        assert!(result.is_err());
    }
}

// ===========================================================================
// 3. Watchdog configuration
// ===========================================================================

#[test]
fn hw_config_new_valid_range_boundaries() -> Result<(), HardwareWatchdogError> {
    let config = WatchdogConfig::new(10)?;
    assert_eq!(config.timeout_ms, 10);
    assert_eq!(config.timeout_us(), 10_000);

    let config = WatchdogConfig::new(5000)?;
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.timeout_us(), 5_000_000);
    Ok(())
}

#[test]
fn hw_config_new_out_of_range() {
    assert!(WatchdogConfig::new(5).is_err());
    assert!(WatchdogConfig::new(9).is_err());
    assert!(WatchdogConfig::new(5001).is_err());
    assert!(WatchdogConfig::new(6000).is_err());
    assert!(WatchdogConfig::new(0).is_err());
    assert!(WatchdogConfig::new(u32::MAX).is_err());
}

#[test]
fn hw_config_default_values() {
    let config = WatchdogConfig::default();
    assert_eq!(config.timeout_ms, 100);
    assert_eq!(config.max_response_time_us, 1000);
    assert_eq!(config.max_feed_failures, 0);
    assert!(config.health_check_enabled);
    assert_eq!(config.health_check_interval_ms, 100);
}

#[test]
fn hw_config_builder_full() -> Result<(), HardwareWatchdogError> {
    let config = WatchdogConfig::builder()
        .timeout_ms(200)
        .max_response_time_us(500)
        .max_feed_failures(3)
        .health_check_enabled(false)
        .health_check_interval_ms(50)
        .build()?;

    assert_eq!(config.timeout_ms, 200);
    assert_eq!(config.max_response_time_us, 500);
    assert_eq!(config.max_feed_failures, 3);
    assert!(!config.health_check_enabled);
    assert_eq!(config.health_check_interval_ms, 50);
    Ok(())
}

#[test]
fn hw_config_builder_validation_rejects_bad_timeout() {
    let result = WatchdogConfig::builder().timeout_ms(5).build();
    assert!(result.is_err());
}

#[test]
fn hw_config_builder_validation_rejects_huge_response_time() {
    let result = WatchdogConfig::builder()
        .max_response_time_us(20000)
        .build();
    assert!(result.is_err());
}

#[test]
fn hw_config_builder_validation_rejects_low_health_check_interval() {
    let result = WatchdogConfig::builder()
        .health_check_enabled(true)
        .health_check_interval_ms(5)
        .build();
    assert!(result.is_err());
}

#[test]
fn hw_config_builder_disabled_health_check_allows_low_interval() -> Result<(), HardwareWatchdogError>
{
    let config = WatchdogConfig::builder()
        .health_check_enabled(false)
        .health_check_interval_ms(1)
        .build()?;
    assert!(!config.health_check_enabled);
    Ok(())
}

#[test]
fn hw_config_validate_standalone() -> Result<(), HardwareWatchdogError> {
    let mut config = WatchdogConfig::default();
    assert!(config.validate().is_ok());

    config.timeout_ms = 5;
    assert!(config.validate().is_err());

    config.timeout_ms = 100;
    config.max_response_time_us = 20000;
    assert!(config.validate().is_err());
    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn hw_config_max_response_time_duration() -> Result<(), HardwareWatchdogError> {
    let config = WatchdogConfig::new(100)?;
    let duration = config.max_response_time();
    assert_eq!(duration, std::time::Duration::from_micros(1000));
    Ok(())
}

#[test]
fn hw_config_copy_clone() -> Result<(), HardwareWatchdogError> {
    let config = WatchdogConfig::new(100)?;
    let copied = config;
    let cloned = config;
    assert_eq!(config, copied);
    assert_eq!(config, cloned);
    Ok(())
}

// ===========================================================================
// 4. Recovery actions
// ===========================================================================

#[test]
fn hw_recovery_after_timeout_via_reset() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(50)?;

    wd.arm()?;
    wd.trigger_timeout()?;
    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);

    // Feed should fail in timed-out state
    assert!(wd.feed().is_err());

    // Reset and re-arm
    wd.reset();
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    assert!(!wd.has_timed_out());
    assert!(!wd.is_safe_state_triggered());

    wd.arm()?;
    assert!(wd.is_armed());
    wd.feed()?;
    assert!(!wd.has_timed_out());

    Ok(())
}

#[test]
fn hw_recovery_after_safe_state() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    wd.arm()?;
    wd.trigger_safe_state()?;
    assert!(wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
    assert!(!wd.is_healthy());

    // Cannot trigger safe state twice
    assert!(wd.trigger_safe_state().is_err());
    // Feed should fail
    assert!(wd.feed().is_err());
    // Arm should fail
    assert!(wd.arm().is_err());

    // Reset restores
    wd.reset();
    assert!(!wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    assert!(wd.is_healthy());

    wd.arm()?;
    assert!(wd.is_armed());
    Ok(())
}

#[test]
fn hw_recovery_multiple_reset_cycles() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    for _ in 0..10 {
        wd.arm()?;
        wd.feed()?;
        wd.disarm()?;
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    }
    Ok(())
}

#[test]
fn hw_recovery_reset_clears_feed_timestamp() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;
    wd.feed()?;
    assert!(wd.time_since_last_feed_us().is_some());

    wd.reset();
    assert!(wd.time_since_last_feed_us().is_none());
    Ok(())
}

#[test]
fn hw_recovery_reset_clears_metrics() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;
    wd.feed()?;
    wd.feed()?;

    let m = wd.metrics();
    assert_eq!(m.feed_count, 2);

    wd.reset();
    let m = wd.metrics();
    assert_eq!(m.feed_count, 0);
    assert_eq!(m.arm_count, 0);
    Ok(())
}

// ===========================================================================
// 5. Fault escalation
// ===========================================================================

#[test]
fn hw_escalation_timeout_then_safe_state() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    wd.arm()?;
    wd.trigger_timeout()?;
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);

    // Escalate to safe state
    wd.trigger_safe_state()?;
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
    assert!(wd.is_safe_state_triggered());
    assert!(!wd.is_healthy());
    Ok(())
}

#[test]
fn hw_escalation_safe_state_from_disarmed() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    // Should be possible from any non-SafeState
    wd.trigger_safe_state()?;
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
    Ok(())
}

#[test]
fn hw_escalation_safe_state_from_armed() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;

    wd.trigger_safe_state()?;
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
    assert!(wd.is_safe_state_triggered());
    Ok(())
}

#[test]
fn hw_escalation_double_safe_state_fails() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.trigger_safe_state()?;
    let result = wd.trigger_safe_state();
    assert_eq!(
        result,
        Err(HardwareWatchdogError::SafeStateAlreadyTriggered)
    );
    Ok(())
}

#[test]
fn hw_escalation_is_healthy_check() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    // Disarmed → healthy
    assert!(wd.is_healthy());

    // Armed → healthy
    wd.arm()?;
    assert!(wd.is_healthy());

    // TimedOut → not healthy
    wd.trigger_timeout()?;
    assert!(!wd.is_healthy());

    // Reset → healthy again
    wd.reset();
    assert!(wd.is_healthy());

    // SafeState → not healthy
    wd.trigger_safe_state()?;
    assert!(!wd.is_healthy());
    Ok(())
}

// ===========================================================================
// 6. State machine transitions
// ===========================================================================

#[test]
fn hw_state_machine_full_transitions() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    // Disarmed → Armed
    state.arm()?;
    assert_eq!(state.status(), WatchdogStatus::Armed);
    assert_eq!(state.arm_count(), 1);

    // Armed → feed (stays armed)
    state.feed()?;
    assert_eq!(state.status(), WatchdogStatus::Armed);
    assert_eq!(state.feed_count(), 1);

    // Armed → TimedOut
    state.timeout()?;
    assert_eq!(state.status(), WatchdogStatus::TimedOut);
    assert_eq!(state.timeout_count(), 1);

    // TimedOut → SafeState
    state.trigger_safe_state()?;
    assert_eq!(state.status(), WatchdogStatus::SafeState);
    assert_eq!(state.safe_state_count(), 1);

    // SafeState → cannot trigger again
    let result = state.trigger_safe_state();
    assert!(result.is_err());

    // Reset → Disarmed
    state.reset();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    Ok(())
}

#[test]
fn hw_state_machine_invalid_arm_from_armed() {
    let state = WatchdogState::new();
    assert!(state.arm().is_ok());
    let result = state.arm();
    assert!(result.is_err());
}

#[test]
fn hw_state_machine_invalid_disarm_from_disarmed() {
    let state = WatchdogState::new();
    let result = state.disarm();
    assert!(result.is_err());
}

#[test]
fn hw_state_machine_feed_requires_armed() {
    let state = WatchdogState::new();

    // Disarmed → feed fails
    let result = state.feed();
    assert!(result.is_err());
}

#[test]
fn hw_state_machine_feed_fails_in_timed_out() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();
    state.arm()?;
    state.timeout()?;

    let result = state.feed();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn hw_state_machine_feed_fails_in_safe_state() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();
    state.trigger_safe_state()?;

    let result = state.feed();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn hw_state_machine_timeout_only_from_armed() {
    let state = WatchdogState::new();
    // Disarmed → timeout fails
    let result = state.timeout();
    assert!(result.is_err());
}

#[test]
fn hw_state_machine_timeout_fails_from_timed_out() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();
    state.arm()?;
    state.timeout()?;
    let result = state.timeout();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn hw_state_machine_disarm_only_from_armed() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();
    state.arm()?;
    state.timeout()?;

    // TimedOut → disarm fails
    let result = state.disarm();
    assert!(result.is_err());
    Ok(())
}

#[test]
fn hw_state_machine_counters_accumulate() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();

    for i in 1..=5 {
        state.arm()?;
        assert_eq!(state.arm_count(), i);
        state.feed()?;
        assert_eq!(state.feed_count(), i);
        state.disarm()?;
    }
    Ok(())
}

// ===========================================================================
// 7. Error conditions
// ===========================================================================

#[test]
fn hw_error_not_armed() {
    let err = HardwareWatchdogError::NotArmed;
    assert_eq!(err.to_string(), "Watchdog is not armed");
}

#[test]
fn hw_error_already_armed() {
    let err = HardwareWatchdogError::AlreadyArmed;
    assert_eq!(err.to_string(), "Watchdog is already armed");
}

#[test]
fn hw_error_timed_out() {
    let err = HardwareWatchdogError::TimedOut;
    assert_eq!(err.to_string(), "Watchdog has timed out");
}

#[test]
fn hw_error_hardware_error() {
    let err = HardwareWatchdogError::hardware_error("I2C bus failure");
    assert_eq!(err.to_string(), "Hardware error: I2C bus failure");
}

#[test]
fn hw_error_invalid_configuration() {
    let err = HardwareWatchdogError::invalid_configuration("timeout too low");
    assert_eq!(err.to_string(), "Invalid configuration: timeout too low");
}

#[test]
fn hw_error_invalid_transition() {
    let err = HardwareWatchdogError::invalid_transition("Disarmed", "TimedOut");
    assert_eq!(
        err.to_string(),
        "Invalid state transition: Disarmed -> TimedOut"
    );
}

#[test]
fn hw_error_safe_state_already_triggered() {
    let err = HardwareWatchdogError::SafeStateAlreadyTriggered;
    assert_eq!(err.to_string(), "Safe state already triggered");
}

#[test]
fn hw_error_wcet_exceeded() {
    let err = HardwareWatchdogError::WcetExceeded;
    assert_eq!(err.to_string(), "Operation would exceed WCET budget");
}

#[test]
fn hw_error_feed_disarmed_returns_not_armed() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    let result = wd.feed();
    assert_eq!(result, Err(HardwareWatchdogError::NotArmed));
    Ok(())
}

#[test]
fn hw_error_feed_timed_out_returns_timed_out() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;
    wd.trigger_timeout()?;
    let result = wd.feed();
    assert_eq!(result, Err(HardwareWatchdogError::TimedOut));
    Ok(())
}

#[test]
fn hw_error_feed_safe_state_returns_safe_state() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.trigger_safe_state()?;
    let result = wd.feed();
    assert_eq!(
        result,
        Err(HardwareWatchdogError::SafeStateAlreadyTriggered)
    );
    Ok(())
}

#[test]
fn hw_error_arm_when_armed_returns_invalid_transition() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;
    let result = wd.arm();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, HardwareWatchdogError::InvalidTransition { .. }));
    }
    Ok(())
}

#[test]
fn hw_error_disarm_when_disarmed_returns_invalid_transition() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    let result = wd.disarm();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, HardwareWatchdogError::InvalidTransition { .. }));
    }
    Ok(())
}

// ===========================================================================
// 8. Metrics tracking
// ===========================================================================

#[test]
fn hw_metrics_initial_values() {
    let m = WatchdogMetrics::new();
    assert_eq!(m.feed_count, 0);
    assert_eq!(m.arm_count, 0);
    assert_eq!(m.timeout_count, 0);
    assert_eq!(m.safe_state_count, 0);
    assert_eq!(m.consecutive_failures, 0);
    assert_eq!(m.max_feed_interval_us, 0);
    assert_eq!(m.last_feed_timestamp_us, 0);
}

#[test]
fn hw_metrics_record_operations() {
    let mut m = WatchdogMetrics::new();

    m.record_arm();
    assert_eq!(m.arm_count, 1);

    m.record_feed(1000);
    m.record_feed(2000);
    m.record_feed(4000);
    assert_eq!(m.feed_count, 3);
    assert_eq!(m.max_feed_interval_us, 2000);
    assert_eq!(m.last_feed_timestamp_us, 4000);

    m.record_failure();
    m.record_failure();
    assert_eq!(m.consecutive_failures, 2);

    // Feed resets consecutive failures
    m.record_feed(5000);
    assert_eq!(m.consecutive_failures, 0);

    m.record_timeout();
    assert_eq!(m.timeout_count, 1);

    m.record_safe_state();
    assert_eq!(m.safe_state_count, 1);
}

#[test]
fn hw_metrics_success_rate_calculations() {
    let mut m = WatchdogMetrics::new();

    // No operations → 100%
    assert!((m.success_rate() - 1.0).abs() < f32::EPSILON);

    // 3 feeds → 100%
    m.record_feed(1000);
    m.record_feed(2000);
    m.record_feed(3000);
    assert!((m.success_rate() - 1.0).abs() < f32::EPSILON);

    // 1 failure → 3/(3+1) = 75%
    m.record_failure();
    assert!((m.success_rate() - 0.75).abs() < 0.01);
}

#[test]
fn hw_metrics_reset() {
    let mut m = WatchdogMetrics::new();
    m.record_feed(1000);
    m.record_arm();
    m.record_timeout();
    m.record_failure();
    m.record_safe_state();

    m.reset();

    assert_eq!(m.feed_count, 0);
    assert_eq!(m.arm_count, 0);
    assert_eq!(m.timeout_count, 0);
    assert_eq!(m.safe_state_count, 0);
    assert_eq!(m.consecutive_failures, 0);
    assert_eq!(m.max_feed_interval_us, 0);
}

#[test]
fn hw_metrics_watchdog_integration() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    wd.arm()?;
    let m = wd.metrics();
    assert_eq!(m.arm_count, 1);

    wd.feed()?;
    wd.feed()?;
    wd.feed()?;
    let m = wd.metrics();
    assert_eq!(m.feed_count, 3);

    wd.trigger_timeout()?;
    let m = wd.metrics();
    assert_eq!(m.timeout_count, 1);

    wd.reset();
    wd.trigger_safe_state()?;
    let m = wd.metrics();
    assert_eq!(m.safe_state_count, 1);

    Ok(())
}

// ===========================================================================
// 9. WatchdogStatus helpers
// ===========================================================================

#[test]
fn hw_status_from_raw_valid() {
    assert_eq!(WatchdogStatus::from_raw(0), Some(WatchdogStatus::Disarmed));
    assert_eq!(WatchdogStatus::from_raw(1), Some(WatchdogStatus::Armed));
    assert_eq!(WatchdogStatus::from_raw(2), Some(WatchdogStatus::TimedOut));
    assert_eq!(WatchdogStatus::from_raw(3), Some(WatchdogStatus::SafeState));
}

#[test]
fn hw_status_from_raw_invalid() {
    assert_eq!(WatchdogStatus::from_raw(4), None);
    assert_eq!(WatchdogStatus::from_raw(u32::MAX), None);
}

#[test]
fn hw_status_to_raw_roundtrip() {
    for raw in 0..=3 {
        let status = WatchdogStatus::from_raw(raw);
        assert!(status.is_some());
        if let Some(s) = status {
            assert_eq!(s.to_raw(), raw);
        }
    }
}

#[test]
fn hw_status_is_terminal() {
    assert!(!WatchdogStatus::Disarmed.is_terminal());
    assert!(!WatchdogStatus::Armed.is_terminal());
    assert!(!WatchdogStatus::TimedOut.is_terminal());
    assert!(WatchdogStatus::SafeState.is_terminal());
}

#[test]
fn hw_status_is_active() {
    assert!(!WatchdogStatus::Disarmed.is_active());
    assert!(WatchdogStatus::Armed.is_active());
    assert!(WatchdogStatus::TimedOut.is_active());
    assert!(!WatchdogStatus::SafeState.is_active());
}

#[test]
fn hw_status_as_str() {
    assert_eq!(WatchdogStatus::Disarmed.as_str(), "Disarmed");
    assert_eq!(WatchdogStatus::Armed.as_str(), "Armed");
    assert_eq!(WatchdogStatus::TimedOut.as_str(), "TimedOut");
    assert_eq!(WatchdogStatus::SafeState.as_str(), "SafeState");
}

#[test]
fn hw_status_display() {
    assert_eq!(WatchdogStatus::Disarmed.to_string(), "Disarmed");
    assert_eq!(WatchdogStatus::Armed.to_string(), "Armed");
    assert_eq!(WatchdogStatus::TimedOut.to_string(), "TimedOut");
    assert_eq!(WatchdogStatus::SafeState.to_string(), "SafeState");
}

#[test]
fn hw_status_default() {
    let status = WatchdogStatus::default();
    assert_eq!(status, WatchdogStatus::Disarmed);
}

// ===========================================================================
// 10. Debug trait
// ===========================================================================

#[test]
fn hw_debug_software_watchdog() {
    let wd = SoftwareWatchdog::with_default_timeout();
    let debug = format!("{wd:?}");
    assert!(debug.contains("SoftwareWatchdog"));
}

#[test]
fn hw_debug_watchdog_state() {
    let state = WatchdogState::new();
    let debug = format!("{state:?}");
    assert!(debug.contains("WatchdogState"));
}

#[test]
fn hw_debug_watchdog_metrics() {
    let m = WatchdogMetrics::new();
    let debug = format!("{m:?}");
    assert!(debug.contains("WatchdogMetrics"));
}

#[test]
fn hw_debug_config() -> Result<(), HardwareWatchdogError> {
    let config = WatchdogConfig::new(100)?;
    let debug = format!("{config:?}");
    assert!(debug.contains("WatchdogConfig"));
    Ok(())
}

// ===========================================================================
// 11. Property-based tests
// ===========================================================================

proptest! {
    #[test]
    fn prop_arm_disarm_cycles_stable(cycles in 1_u32..50) {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        for _ in 0..cycles {
            prop_assert!(wd.arm().is_ok());
            prop_assert!(wd.is_armed());
            prop_assert!(wd.feed().is_ok());
            prop_assert!(wd.disarm().is_ok());
            prop_assert!(!wd.is_armed());
        }
        prop_assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    }

    #[test]
    fn prop_reset_always_returns_disarmed(action in 0_u8..4) {
        let mut wd = SoftwareWatchdog::with_default_timeout();

        match action {
            0 => { /* Disarmed already */ }
            1 => { let _ = wd.arm(); }
            2 => {
                let _ = wd.arm();
                let _ = wd.trigger_timeout();
            }
            _ => { let _ = wd.trigger_safe_state(); }
        }

        wd.reset();
        prop_assert_eq!(wd.status(), WatchdogStatus::Disarmed);
        prop_assert!(!wd.is_safe_state_triggered());
        prop_assert!(!wd.has_timed_out());
    }

    #[test]
    fn prop_valid_timeout_config(timeout_ms in 10_u32..=5000) {
        let result = WatchdogConfig::new(timeout_ms);
        prop_assert!(result.is_ok());
        if let Ok(config) = result {
            prop_assert_eq!(config.timeout_ms, timeout_ms);
            prop_assert_eq!(config.timeout_us(), u64::from(timeout_ms) * 1000);
        }
    }

    #[test]
    fn prop_invalid_timeout_config_low(timeout_ms in 0_u32..10) {
        let result = WatchdogConfig::new(timeout_ms);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_invalid_timeout_config_high(timeout_ms in 5001_u32..=10000) {
        let result = WatchdogConfig::new(timeout_ms);
        prop_assert!(result.is_err());
    }

    #[test]
    fn prop_feed_count_matches_calls(feeds in 1_u32..100) {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        prop_assert!(wd.arm().is_ok());
        for _ in 0..feeds {
            prop_assert!(wd.feed().is_ok());
        }
        let m = wd.metrics();
        prop_assert_eq!(m.feed_count, u64::from(feeds));
    }

    #[test]
    fn prop_status_raw_roundtrip(raw in 0_u32..=3) {
        let status = WatchdogStatus::from_raw(raw);
        prop_assert!(status.is_some());
        if let Some(s) = status {
            prop_assert_eq!(s.to_raw(), raw);
        }
    }
}
