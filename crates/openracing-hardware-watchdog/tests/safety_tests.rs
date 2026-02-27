//! Safety-hardening tests for hardware watchdog.
//!
//! These tests verify the complete lifecycle of the software watchdog
//! including timeout behavior and safe state management.
//! All tests use `Result<>` return types and avoid `unwrap`/`expect`.

#![cfg(test)]

use openracing_hardware_watchdog::prelude::*;

/// Tests arm and feed lifecycle with Result return type.
#[test]
fn test_arm_and_feed_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    assert!(!watchdog.is_armed());
    watchdog.arm()?;
    assert!(watchdog.is_armed());
    assert!(!watchdog.has_timed_out());

    watchdog.feed()?;
    assert!(watchdog.is_armed());
    assert!(!watchdog.has_timed_out());

    let metrics = watchdog.metrics();
    assert_eq!(metrics.arm_count, 1);
    assert_eq!(metrics.feed_count, 1);

    Ok(())
}

/// Tests that watchdog triggers timeout after the configured period elapses.
/// Uses 10ms timeout (minimum allowed) and sleeps 15ms to ensure trigger.
#[test]
fn test_watchdog_triggers_after_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    watchdog.arm()?;
    // Sleep past the 10ms timeout
    std::thread::sleep(std::time::Duration::from_millis(15));
    assert!(watchdog.has_timed_out());

    Ok(())
}

/// Tests that feeding the watchdog repeatedly prevents timeout.
/// Feeds every 5ms with a 10ms timeout — each feed resets the clock.
#[test]
fn test_multiple_feeds_prevent_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    watchdog.arm()?;

    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(4));
        watchdog.feed()?;
    }

    assert!(!watchdog.has_timed_out());
    assert_eq!(watchdog.metrics().feed_count, 5);

    Ok(())
}

/// Tests that safe state can only be triggered once after a timeout.
/// The second trigger must return an error.
#[test]
fn test_safe_state_triggered_exactly_once_after_timeout() -> Result<(), Box<dyn std::error::Error>>
{
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    watchdog.arm()?;
    watchdog.trigger_timeout()?;

    // First trigger succeeds
    watchdog.trigger_safe_state()?;
    assert!(watchdog.is_safe_state_triggered());
    assert_eq!(watchdog.metrics().safe_state_count, 1);

    // Second trigger must fail
    let result = watchdog.trigger_safe_state();
    assert!(
        result.is_err(),
        "second trigger_safe_state should be an error"
    );
    // Count stays at 1
    assert_eq!(watchdog.metrics().safe_state_count, 1);

    Ok(())
}

/// Tests that reset returns watchdog to initial disarmed state, clearing all flags.
#[test]
fn test_reset_restores_initial_state() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    watchdog.arm()?;
    watchdog.feed()?;
    watchdog.trigger_timeout()?;
    watchdog.trigger_safe_state()?;

    watchdog.reset();

    assert!(!watchdog.is_armed());
    assert!(!watchdog.is_safe_state_triggered());
    assert!(!watchdog.has_timed_out());
    assert_eq!(watchdog.status(), WatchdogStatus::Disarmed);
    assert_eq!(watchdog.metrics().feed_count, 0);

    Ok(())
}

/// Tests that feeding when disarmed returns an error (not a panic).
#[test]
fn test_feed_when_disarmed_is_error() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    let result = watchdog.feed();
    assert!(
        matches!(result, Err(HardwareWatchdogError::NotArmed)),
        "feed on disarmed watchdog should be NotArmed error"
    );

    Ok(())
}

/// Tests that arming an already-armed watchdog returns an error.
#[test]
fn test_double_arm_is_error() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    watchdog.arm()?;
    let result = watchdog.arm();
    assert!(
        result.is_err(),
        "arming an already-armed watchdog should fail"
    );

    Ok(())
}

/// Tests that the watchdog can be re-armed after a full reset cycle.
#[test]
fn test_rearm_after_reset() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::new(10)?;
    let mut watchdog = SoftwareWatchdog::new(config);

    watchdog.arm()?;
    watchdog.trigger_safe_state()?;
    watchdog.reset();

    // Must be re-armable after reset
    watchdog.arm()?;
    assert!(watchdog.is_armed());
    watchdog.feed()?;

    Ok(())
}
