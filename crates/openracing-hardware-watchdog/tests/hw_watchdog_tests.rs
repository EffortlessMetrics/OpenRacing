//! Hardware watchdog hardening tests: mock operations, timeout configuration,
//! error injection, lifecycle stress, and state machine exhaustive transitions.
//!
//! All tests use `Result`-returning signatures and avoid `unwrap()`/`expect()`.

use openracing_hardware_watchdog::prelude::*;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// 1. Mock hardware watchdog operations
// ===========================================================================

mod mock_operations {
    use super::*;

    /// SoftwareWatchdog implements the full HardwareWatchdog trait lifecycle.
    #[test]
    fn full_trait_lifecycle() -> TestResult {
        let config = WatchdogConfig::new(100)?;
        let mut wd = SoftwareWatchdog::new(config);

        // Disarmed
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);
        assert!(!wd.is_armed());
        assert!(wd.is_healthy());
        assert!(wd.time_since_last_feed_us().is_none());

        // Arm
        wd.arm()?;
        assert!(wd.is_armed());
        assert_eq!(wd.status(), WatchdogStatus::Armed);

        // Feed
        wd.feed()?;
        wd.feed()?;
        wd.feed()?;
        let metrics = wd.metrics();
        assert_eq!(metrics.feed_count, 3);
        assert_eq!(metrics.arm_count, 1);

        // Disarm
        wd.disarm()?;
        assert!(!wd.is_armed());
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);

        // Reset
        wd.reset();
        let metrics = wd.metrics();
        assert_eq!(metrics.feed_count, 0);
        Ok(())
    }

    /// Feed after timeout returns error, not panic.
    #[test]
    fn feed_after_timeout_is_error() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.arm()?;
        wd.trigger_timeout()?;

        let result = wd.feed();
        assert!(
            matches!(result, Err(HardwareWatchdogError::TimedOut)),
            "feed after timeout should return TimedOut error"
        );
        Ok(())
    }

    /// Feed after safe state returns error.
    #[test]
    fn feed_after_safe_state_is_error() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.arm()?;
        wd.trigger_safe_state()?;

        let result = wd.feed();
        assert!(
            matches!(result, Err(HardwareWatchdogError::SafeStateAlreadyTriggered)),
            "feed after safe state should return SafeStateAlreadyTriggered error"
        );
        Ok(())
    }

    /// Arm after safe state (without reset) returns error.
    #[test]
    fn arm_after_safe_state_without_reset_is_error() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.trigger_safe_state()?;

        let result = wd.arm();
        assert!(result.is_err(), "arm after safe state should fail");
        Ok(())
    }

    /// Config is accessible through the trait.
    #[test]
    fn config_accessible_through_trait() -> TestResult {
        let config = WatchdogConfig::builder()
            .timeout_ms(250)
            .max_response_time_us(500)
            .max_feed_failures(3)
            .build()?;
        let wd = SoftwareWatchdog::new(config);

        assert_eq!(wd.config().timeout_ms, 250);
        assert_eq!(wd.config().max_response_time_us, 500);
        assert_eq!(wd.config().max_feed_failures, 3);
        assert_eq!(wd.timeout_ms(), 250);
        Ok(())
    }

    /// `time_since_last_feed_us` returns `Some` after a feed.
    #[test]
    fn time_since_last_feed_after_feeding() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.arm()?;
        wd.feed()?;

        let elapsed = wd.time_since_last_feed_us();
        assert!(elapsed.is_some(), "should have a timestamp after feeding");
        Ok(())
    }

    /// `time_since_last_feed_us` returns `None` before any feed.
    #[test]
    fn time_since_last_feed_before_feeding_is_none() {
        let wd = SoftwareWatchdog::with_default_timeout();
        assert!(wd.time_since_last_feed_us().is_none());
    }
}

// ===========================================================================
// 2. Timeout configuration
// ===========================================================================

mod timeout_config {
    use super::*;

    /// Minimum valid timeout (10ms) is accepted.
    #[test]
    fn minimum_timeout_accepted() -> TestResult {
        let config = WatchdogConfig::new(10)?;
        assert_eq!(config.timeout_ms, 10);
        assert_eq!(config.timeout_us(), 10_000);
        Ok(())
    }

    /// Maximum valid timeout (5000ms) is accepted.
    #[test]
    fn maximum_timeout_accepted() -> TestResult {
        let config = WatchdogConfig::new(5000)?;
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.timeout_us(), 5_000_000);
        Ok(())
    }

    /// Below-minimum timeout is rejected.
    #[test]
    fn below_minimum_timeout_rejected() {
        let result = WatchdogConfig::new(9);
        assert!(result.is_err());

        let result = WatchdogConfig::new(0);
        assert!(result.is_err());
    }

    /// Above-maximum timeout is rejected.
    #[test]
    fn above_maximum_timeout_rejected() {
        let result = WatchdogConfig::new(5001);
        assert!(result.is_err());

        let result = WatchdogConfig::new(u32::MAX);
        assert!(result.is_err());
    }

    /// Builder with invalid max_response_time_us is rejected.
    #[test]
    fn invalid_max_response_time_rejected() {
        let result = WatchdogConfig::builder()
            .timeout_ms(100)
            .max_response_time_us(20_000)
            .build();
        assert!(result.is_err());
    }

    /// Builder with health check enabled but low interval is rejected.
    #[test]
    fn health_check_low_interval_rejected() {
        let result = WatchdogConfig::builder()
            .timeout_ms(100)
            .health_check_enabled(true)
            .health_check_interval_ms(5)
            .build();
        assert!(result.is_err());
    }

    /// Builder with health check disabled allows any interval.
    #[test]
    fn health_check_disabled_allows_any_interval() -> TestResult {
        let config = WatchdogConfig::builder()
            .timeout_ms(100)
            .health_check_enabled(false)
            .health_check_interval_ms(1)
            .build()?;
        assert!(!config.health_check_enabled);
        Ok(())
    }

    /// `with_timeout` factory creates correctly.
    #[test]
    fn with_timeout_factory() -> TestResult {
        let wd = SoftwareWatchdog::with_timeout(200)?;
        assert_eq!(wd.timeout_ms(), 200);
        Ok(())
    }

    /// `with_timeout` rejects invalid values.
    #[test]
    fn with_timeout_rejects_invalid() {
        let result = SoftwareWatchdog::with_timeout(5);
        assert!(result.is_err());
    }

    /// Default config is always valid.
    #[test]
    fn default_config_valid() {
        let config = WatchdogConfig::default();
        assert!(config.validate().is_ok());
    }

    /// `max_response_time` duration conversion (requires std feature).
    #[cfg(feature = "std")]
    #[test]
    fn max_response_time_duration() -> TestResult {
        let config = WatchdogConfig::builder()
            .timeout_ms(100)
            .max_response_time_us(500)
            .build()?;
        assert_eq!(
            config.max_response_time(),
            core::time::Duration::from_micros(500)
        );
        Ok(())
    }
}

// ===========================================================================
// 3. Error injection
// ===========================================================================

mod error_injection {
    use super::*;

    /// Injecting a timeout (via `trigger_timeout`) makes `has_timed_out` true.
    #[test]
    fn injected_timeout_detected() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.arm()?;
        assert!(!wd.has_timed_out());

        wd.trigger_timeout()?;
        assert!(wd.has_timed_out());
        assert_eq!(wd.status(), WatchdogStatus::TimedOut);
        Ok(())
    }

    /// After injected timeout, safe state can still be triggered.
    #[test]
    fn safe_state_after_injected_timeout() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.arm()?;
        wd.trigger_timeout()?;

        wd.trigger_safe_state()?;
        assert!(wd.is_safe_state_triggered());
        assert_eq!(wd.status(), WatchdogStatus::SafeState);
        assert!(!wd.is_healthy());
        Ok(())
    }

    /// Cannot inject timeout when disarmed.
    #[test]
    fn cannot_inject_timeout_when_disarmed() {
        let wd = SoftwareWatchdog::with_default_timeout();
        let result = wd.trigger_timeout();
        assert!(result.is_err());
    }

    /// Cannot inject timeout twice.
    #[test]
    fn cannot_inject_timeout_twice() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();
        wd.arm()?;
        wd.trigger_timeout()?;

        let result = wd.trigger_timeout();
        assert!(result.is_err());
        Ok(())
    }

    /// Error types have correct Display messages.
    #[test]
    fn error_display_messages() {
        let errors: Vec<(HardwareWatchdogError, &str)> = vec![
            (HardwareWatchdogError::NotArmed, "not armed"),
            (HardwareWatchdogError::AlreadyArmed, "already armed"),
            (HardwareWatchdogError::TimedOut, "timed out"),
            (
                HardwareWatchdogError::SafeStateAlreadyTriggered,
                "already triggered",
            ),
            (HardwareWatchdogError::WcetExceeded, "WCET"),
        ];
        for (err, substring) in errors {
            let msg = format!("{err}");
            assert!(
                msg.contains(substring),
                "error '{msg}' should contain '{substring}'"
            );
        }
    }

    /// Hardware error variant carries a message.
    #[test]
    fn hardware_error_carries_message() {
        let err = HardwareWatchdogError::hardware_error("I2C bus fault");
        let msg = format!("{err}");
        assert!(msg.contains("I2C bus fault"));
    }

    /// InvalidTransition error carries from/to states.
    #[test]
    fn invalid_transition_error_message() {
        let err = HardwareWatchdogError::invalid_transition("Disarmed", "TimedOut");
        let msg = format!("{err}");
        assert!(msg.contains("Disarmed"));
        assert!(msg.contains("TimedOut"));
    }

    /// Real timeout detection via sleep (10ms timeout, sleep 15ms).
    #[test]
    fn real_timeout_via_sleep() -> TestResult {
        let config = WatchdogConfig::new(10)?;
        let mut wd = SoftwareWatchdog::new(config);
        wd.arm()?;

        thread::sleep(Duration::from_millis(15));
        assert!(wd.has_timed_out(), "should time out after sleeping past timeout");
        Ok(())
    }

    /// Feed prevents real timeout (feed every 4ms with 10ms timeout).
    #[test]
    fn feed_prevents_real_timeout() -> TestResult {
        let config = WatchdogConfig::new(10)?;
        let mut wd = SoftwareWatchdog::new(config);
        wd.arm()?;

        for _ in 0..5 {
            thread::sleep(Duration::from_millis(4));
            wd.feed()?;
        }
        assert!(!wd.has_timed_out());
        assert_eq!(wd.metrics().feed_count, 5);
        Ok(())
    }
}

// ===========================================================================
// 4. State machine exhaustive transitions
// ===========================================================================

mod state_machine {
    use super::*;

    /// Complete state machine: Disarmed → Armed → Feed → Disarm → Arm → Timeout
    /// → SafeState → Reset → Arm (verifying re-usability).
    #[test]
    fn exhaustive_state_lifecycle() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();

        // Disarmed → Armed
        wd.arm()?;
        assert_eq!(wd.status(), WatchdogStatus::Armed);

        // Feed while armed
        wd.feed()?;
        assert_eq!(wd.status(), WatchdogStatus::Armed);

        // Armed → Disarmed
        wd.disarm()?;
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);

        // Re-arm
        wd.arm()?;
        assert_eq!(wd.status(), WatchdogStatus::Armed);

        // Armed → TimedOut
        wd.trigger_timeout()?;
        assert_eq!(wd.status(), WatchdogStatus::TimedOut);
        assert!(wd.has_timed_out());

        // TimedOut → SafeState
        wd.trigger_safe_state()?;
        assert_eq!(wd.status(), WatchdogStatus::SafeState);
        assert!(!wd.is_healthy());

        // Reset from SafeState
        wd.reset();
        assert_eq!(wd.status(), WatchdogStatus::Disarmed);
        assert!(!wd.is_safe_state_triggered());

        // Can re-arm after reset
        wd.arm()?;
        assert!(wd.is_armed());
        Ok(())
    }

    /// WatchdogState atomic state machine: all valid transitions.
    #[test]
    fn atomic_state_all_valid_transitions() -> TestResult {
        let state = WatchdogState::new();

        // Disarmed → Armed
        state.arm()?;
        assert_eq!(state.status(), WatchdogStatus::Armed);

        // Armed → feed (stays Armed)
        state.feed()?;
        assert_eq!(state.status(), WatchdogStatus::Armed);

        // Armed → Disarmed
        state.disarm()?;
        assert_eq!(state.status(), WatchdogStatus::Disarmed);

        // Disarmed → Armed → TimedOut
        state.arm()?;
        state.timeout()?;
        assert_eq!(state.status(), WatchdogStatus::TimedOut);

        // TimedOut → SafeState
        state.trigger_safe_state()?;
        assert_eq!(state.status(), WatchdogStatus::SafeState);

        // Reset
        state.reset();
        assert_eq!(state.status(), WatchdogStatus::Disarmed);
        Ok(())
    }

    /// All invalid transitions return errors, never panic.
    #[test]
    fn all_invalid_transitions_are_errors() -> TestResult {
        let state = WatchdogState::new();

        // From Disarmed: cannot disarm, feed, or timeout
        assert!(state.disarm().is_err());
        assert!(state.feed().is_err());
        assert!(state.timeout().is_err());

        // From Armed: cannot arm again
        state.arm()?;
        assert!(state.arm().is_err());

        // From TimedOut: cannot arm, disarm, feed, or timeout again
        state.timeout()?;
        assert!(state.arm().is_err());
        assert!(state.disarm().is_err());
        assert!(state.feed().is_err());
        assert!(state.timeout().is_err());

        // From SafeState: nothing works except reset
        state.trigger_safe_state()?;
        assert!(state.arm().is_err());
        assert!(state.disarm().is_err());
        assert!(state.feed().is_err());
        assert!(state.timeout().is_err());
        assert!(state.trigger_safe_state().is_err());
        Ok(())
    }

    /// WatchdogStatus raw roundtrip for all valid discriminants.
    #[test]
    fn status_raw_roundtrip() {
        for raw in 0u32..4 {
            let status = WatchdogStatus::from_raw(raw);
            assert!(status.is_some());
            if let Some(s) = status {
                assert_eq!(s.to_raw(), raw);
            }
        }
    }

    /// Invalid raw values return None.
    #[test]
    fn status_from_raw_invalid() {
        assert!(WatchdogStatus::from_raw(4).is_none());
        assert!(WatchdogStatus::from_raw(100).is_none());
        assert!(WatchdogStatus::from_raw(u32::MAX).is_none());
    }

    /// Terminal state check.
    #[test]
    fn only_safe_state_is_terminal() {
        assert!(!WatchdogStatus::Disarmed.is_terminal());
        assert!(!WatchdogStatus::Armed.is_terminal());
        assert!(!WatchdogStatus::TimedOut.is_terminal());
        assert!(WatchdogStatus::SafeState.is_terminal());
    }

    /// Active state check.
    #[test]
    fn armed_and_timedout_are_active() {
        assert!(!WatchdogStatus::Disarmed.is_active());
        assert!(WatchdogStatus::Armed.is_active());
        assert!(WatchdogStatus::TimedOut.is_active());
        assert!(!WatchdogStatus::SafeState.is_active());
    }

    /// Repeated arm-feed-disarm cycles don't leak state.
    #[test]
    fn repeated_arm_disarm_cycles() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();

        for i in 0..20 {
            wd.arm()?;
            for _ in 0..5 {
                wd.feed()?;
            }
            wd.disarm()?;

            let metrics = wd.metrics();
            assert_eq!(metrics.arm_count, u64::try_from(i + 1)?);
            assert_eq!(metrics.feed_count, u64::try_from((i + 1) * 5)?);
        }
        Ok(())
    }

    /// Repeated timeout-reset cycles track counts correctly.
    #[test]
    fn repeated_timeout_reset_cycles() -> TestResult {
        let mut wd = SoftwareWatchdog::with_default_timeout();

        for _ in 0..10 {
            wd.arm()?;
            wd.trigger_timeout()?;
            assert!(wd.has_timed_out());
            wd.reset();
            assert_eq!(wd.status(), WatchdogStatus::Disarmed);
        }
        Ok(())
    }
}

// ===========================================================================
// 5. Concurrent access
// ===========================================================================

mod concurrent {
    use super::*;

    /// Multiple threads reading status while one thread transitions state.
    #[test]
    fn concurrent_status_reads_during_transitions() -> TestResult {
        let wd = Arc::new(Mutex::new(SoftwareWatchdog::with_default_timeout()));
        wd.lock().map_err(|e| e.to_string())?.arm()?;

        let mut handles = vec![];

        // Reader threads
        for _ in 0..4 {
            let w = Arc::clone(&wd);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    if let Ok(guard) = w.lock() {
                        let status = guard.status();
                        // Should be Armed, TimedOut, or Disarmed (if reset)
                        assert!(matches!(
                            status,
                            WatchdogStatus::Armed
                                | WatchdogStatus::TimedOut
                                | WatchdogStatus::Disarmed
                        ));
                    }
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }
        Ok(())
    }

    /// Concurrent feed operations under mutex don't panic.
    #[test]
    fn concurrent_feeds_under_mutex() -> TestResult {
        let wd = Arc::new(Mutex::new(SoftwareWatchdog::with_default_timeout()));
        wd.lock().map_err(|e| e.to_string())?.arm()?;

        let mut handles = vec![];
        for _ in 0..4 {
            let w = Arc::clone(&wd);
            handles.push(thread::spawn(move || {
                for _ in 0..50 {
                    if let Ok(mut guard) = w.lock() {
                        let _ = guard.feed();
                    }
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }

        let metrics = wd.lock().map_err(|e| e.to_string())?.metrics();
        assert!(metrics.feed_count > 0);
        Ok(())
    }

    /// Atomic WatchdogState handles concurrent arm/reset from multiple threads.
    #[test]
    fn atomic_state_concurrent_arm_reset() {
        let state = Arc::new(WatchdogState::new());
        let mut handles = vec![];

        for _ in 0..8 {
            let s = Arc::clone(&state);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = s.arm();
                    let _ = s.feed();
                    s.reset();
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }

        // After all threads complete, state should be consistent
        let status = state.status();
        assert!(
            matches!(status, WatchdogStatus::Disarmed | WatchdogStatus::Armed),
            "final status should be Disarmed or Armed, got {status}"
        );
    }
}

// ===========================================================================
// 6. Metrics
// ===========================================================================

mod metrics_tests {
    use super::*;

    /// Metrics record_feed updates feed count and tracks max interval.
    #[test]
    fn metrics_feed_tracking() {
        let mut metrics = WatchdogMetrics::new();

        metrics.record_feed(1000);
        assert_eq!(metrics.feed_count, 1);
        assert_eq!(metrics.max_feed_interval_us, 0); // first feed, no interval

        metrics.record_feed(2500);
        assert_eq!(metrics.feed_count, 2);
        assert_eq!(metrics.max_feed_interval_us, 1500);

        metrics.record_feed(3000);
        assert_eq!(metrics.feed_count, 3);
        assert_eq!(metrics.max_feed_interval_us, 1500); // 500 < 1500
    }

    /// Metrics success_rate handles edge cases.
    #[test]
    fn metrics_success_rate_edge_cases() {
        let metrics = WatchdogMetrics::new();
        assert!((metrics.success_rate() - 1.0).abs() < f32::EPSILON);

        let mut m = WatchdogMetrics::new();
        m.record_failure();
        assert!(m.success_rate() < 0.01);

        let mut m2 = WatchdogMetrics::new();
        m2.record_feed(1000);
        assert!((m2.success_rate() - 1.0).abs() < f32::EPSILON);
    }

    /// Metrics reset clears everything.
    #[test]
    fn metrics_reset_clears_all() {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_arm();
        metrics.record_feed(1000);
        metrics.record_feed(2000);
        metrics.record_timeout();
        metrics.record_safe_state();
        metrics.record_failure();

        metrics.reset();

        assert_eq!(metrics.feed_count, 0);
        assert_eq!(metrics.arm_count, 0);
        assert_eq!(metrics.timeout_count, 0);
        assert_eq!(metrics.safe_state_count, 0);
        assert_eq!(metrics.consecutive_failures, 0);
        assert_eq!(metrics.max_feed_interval_us, 0);
    }

    /// Feed resets consecutive failures.
    #[test]
    fn feed_resets_consecutive_failures() {
        let mut metrics = WatchdogMetrics::new();
        metrics.record_failure();
        metrics.record_failure();
        metrics.record_failure();
        assert_eq!(metrics.consecutive_failures, 3);

        metrics.record_feed(1000);
        assert_eq!(metrics.consecutive_failures, 0);
    }

    /// Saturating arithmetic on metrics prevents overflow.
    #[test]
    fn metrics_saturating_arithmetic() {
        let mut metrics = WatchdogMetrics::new();
        metrics.feed_count = u64::MAX;
        metrics.record_feed(1000);
        assert_eq!(metrics.feed_count, u64::MAX);

        metrics.consecutive_failures = u32::MAX;
        metrics.record_failure();
        assert_eq!(metrics.consecutive_failures, u32::MAX);
    }
}
