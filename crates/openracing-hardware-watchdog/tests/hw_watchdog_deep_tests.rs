//! Deep tests for the hardware watchdog subsystem.
//!
//! Covers software watchdog mock behavior, timeout precision, recovery
//! after safe-state, and repeated arm/disarm cycles.

use openracing_hardware_watchdog::prelude::*;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_watchdog(timeout_ms: u32) -> Result<SoftwareWatchdog, HardwareWatchdogError> {
    let config = WatchdogConfig::new(timeout_ms)?;
    Ok(SoftwareWatchdog::new(config))
}

// ===== 1. Hardware watchdog mock: simulate arm → feed → check =====

#[test]
fn hw_watchdog_mock_basic_lifecycle() -> Result<(), HardwareWatchdogError> {
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

// ===== 2. Timeout precision: verify timeout within tolerance =====

#[test]
fn hw_watchdog_timeout_precision() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(50)?;

    wd.arm()?;
    wd.feed()?;

    // Immediately after feed, should not be timed out
    assert!(!wd.has_timed_out());
    assert!(wd.is_armed());

    // Sleep beyond the timeout
    std::thread::sleep(std::time::Duration::from_millis(80));

    // Now it should be timed out
    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);

    Ok(())
}

// ===== 3. Feed prevents timeout =====

#[test]
fn hw_watchdog_feed_prevents_timeout() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;
    wd.arm()?;

    // Feed several times within timeout
    for _ in 0..5 {
        wd.feed()?;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    // Should still be armed, not timed out
    assert!(!wd.has_timed_out());
    assert!(wd.is_armed());

    Ok(())
}

// ===== 4. Recovery: reset after timeout restores normal operation =====

#[test]
fn hw_watchdog_recovery_after_timeout() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(50)?;

    wd.arm()?;
    // Force timeout via trigger_timeout
    wd.trigger_timeout()?;
    assert!(wd.has_timed_out());
    assert_eq!(wd.status(), WatchdogStatus::TimedOut);

    // Feed should fail in timed-out state
    let feed_result = wd.feed();
    assert!(feed_result.is_err());

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

// ===== 5. Recovery after safe-state trigger =====

#[test]
fn hw_watchdog_recovery_after_safe_state() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    wd.arm()?;
    wd.trigger_safe_state()?;

    assert!(wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::SafeState);
    assert!(!wd.is_healthy());

    // Cannot trigger safe state twice
    let result = wd.trigger_safe_state();
    assert!(result.is_err());

    // Feed should fail in safe state
    let feed_result = wd.feed();
    assert!(feed_result.is_err());

    // Reset restores to disarmed
    wd.reset();
    assert!(!wd.is_safe_state_triggered());
    assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    assert!(wd.is_healthy());

    // Can arm again
    wd.arm()?;
    assert!(wd.is_armed());

    Ok(())
}

// ===== 6. Multiple resets: repeated arm/disarm cycles =====

#[test]
fn hw_watchdog_multiple_arm_disarm_cycles() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    for _ in 0..10 {
        wd.arm()?;
        assert!(wd.is_armed());

        wd.feed()?;
        assert!(!wd.has_timed_out());

        wd.disarm()?;
        assert!(!wd.is_armed());
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);
    }

    Ok(())
}

// ===== 7. Metrics tracking across operations =====

#[test]
fn hw_watchdog_metrics_tracking() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    // Arm
    wd.arm()?;
    let m = wd.metrics();
    assert_eq!(m.arm_count, 1);

    // Multiple feeds
    wd.feed()?;
    wd.feed()?;
    wd.feed()?;
    let m = wd.metrics();
    assert_eq!(m.feed_count, 3);

    // Timeout
    wd.trigger_timeout()?;
    let m = wd.metrics();
    assert_eq!(m.timeout_count, 1);

    // Reset and safe-state
    wd.reset();
    wd.trigger_safe_state()?;
    let m = wd.metrics();
    assert_eq!(m.safe_state_count, 1);

    Ok(())
}

// ===== 8. Feed in wrong states returns proper errors =====

#[test]
fn hw_watchdog_feed_wrong_state_errors() -> Result<(), HardwareWatchdogError> {
    let mut wd = make_watchdog(100)?;

    // Feed when disarmed
    let result = wd.feed();
    assert_eq!(result, Err(HardwareWatchdogError::NotArmed));

    // Arm then disarm then feed
    wd.arm()?;
    wd.disarm()?;
    let result = wd.feed();
    assert_eq!(result, Err(HardwareWatchdogError::NotArmed));

    // Trigger safe state and feed
    wd.trigger_safe_state()?;
    let result = wd.feed();
    assert_eq!(
        result,
        Err(HardwareWatchdogError::SafeStateAlreadyTriggered)
    );

    Ok(())
}

// ===== 9. Config validation =====

#[test]
fn hw_watchdog_config_validation() -> Result<(), HardwareWatchdogError> {
    // Too low
    let result = WatchdogConfig::new(5);
    assert!(result.is_err());

    // Too high
    let result = WatchdogConfig::new(6000);
    assert!(result.is_err());

    // Valid range boundaries
    let config = WatchdogConfig::new(10)?;
    assert_eq!(config.timeout_ms, 10);
    assert_eq!(config.timeout_us(), 10_000);

    let config = WatchdogConfig::new(5000)?;
    assert_eq!(config.timeout_ms, 5000);

    // Builder
    let config = WatchdogConfig::builder()
        .timeout_ms(200)
        .max_response_time_us(500)
        .max_feed_failures(3)
        .health_check_enabled(false)
        .build()?;
    assert_eq!(config.timeout_ms, 200);
    assert_eq!(config.max_response_time_us, 500);
    assert_eq!(config.max_feed_failures, 3);
    assert!(!config.health_check_enabled);

    Ok(())
}

// ===== 10. State machine atomic transitions =====

#[test]
fn hw_watchdog_state_machine_transitions() -> Result<(), HardwareWatchdogError> {
    let state = WatchdogState::new();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    // Disarmed → Armed
    state.arm()?;
    assert_eq!(state.status(), WatchdogStatus::Armed);

    // Cannot arm when already armed
    let result = state.arm();
    assert!(result.is_err());

    // Armed → feed (stays armed)
    state.feed()?;
    assert_eq!(state.status(), WatchdogStatus::Armed);
    assert_eq!(state.feed_count(), 1);

    // Armed → TimedOut
    state.timeout()?;
    assert_eq!(state.status(), WatchdogStatus::TimedOut);
    assert_eq!(state.timeout_count(), 1);

    // Cannot feed when timed out
    let result = state.feed();
    assert!(result.is_err());

    // TimedOut → SafeState
    state.trigger_safe_state()?;
    assert_eq!(state.status(), WatchdogStatus::SafeState);

    // Cannot trigger safe state again
    let result = state.trigger_safe_state();
    assert!(result.is_err());

    // Reset back to Disarmed
    state.reset();
    assert_eq!(state.status(), WatchdogStatus::Disarmed);

    Ok(())
}

// ===== 11. Metrics success rate calculation =====

#[test]
fn hw_watchdog_metrics_success_rate() {
    let mut m = WatchdogMetrics::new();

    // No operations → 100% success
    assert!((m.success_rate() - 1.0).abs() < f32::EPSILON);

    // 3 feeds, 0 failures → 100%
    m.record_feed(1000);
    m.record_feed(2000);
    m.record_feed(3000);
    assert!((m.success_rate() - 1.0).abs() < f32::EPSILON);

    // 1 failure → 3/(3+1) = 75%
    m.record_failure();
    assert!((m.success_rate() - 0.75).abs() < 0.01);

    // Max feed interval
    assert_eq!(m.max_feed_interval_us, 1000);
}

// ===== 12. Property test: arm/disarm cycle never corrupts state =====

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
}

// ===== 13. Property test: reset from any state returns to Disarmed =====

proptest! {
    #[test]
    fn prop_reset_always_returns_disarmed(action in 0_u8..4) {
        let mut wd = SoftwareWatchdog::with_default_timeout();

        // Drive into various states
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
}
