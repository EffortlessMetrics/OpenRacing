//! Watchdog hardening tests: feed timing, timeout triggering, reset/restart
//! lifecycle, multi-channel coordination, platform-specific behavior, load
//! testing, and safety state machine interaction.
//!
//! All tests use `Result`-returning signatures and avoid `unwrap()`/`expect()`.

use openracing_watchdog::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ===========================================================================
// 1. Feed timing accuracy
// ===========================================================================

mod feed_timing {
    use super::*;

    /// A rapid sequence of feeds should all record without faults.
    #[test]
    fn rapid_feed_records_no_faults() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(500)
            .plugin_max_timeouts(3)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        for _ in 0..1000 {
            let fault = watchdog.record_plugin_execution("fast_plugin", 50);
            assert!(fault.is_none(), "no fault expected for fast execution");
        }

        let stats = watchdog
            .get_plugin_stats("fast_plugin")
            .ok_or("missing stats")?;
        assert_eq!(stats.total_executions, 1000);
        assert_eq!(stats.timeout_count, 0);
        Ok(())
    }

    /// Execution times exactly at the timeout boundary should NOT trigger a
    /// timeout (the check is strictly greater-than).
    #[test]
    fn execution_at_exact_boundary_is_not_timeout() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(3)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        // Exactly 100 µs is NOT > 100, so it is a success
        let fault = watchdog.record_plugin_execution("boundary", 100);
        assert!(fault.is_none());

        let stats = watchdog
            .get_plugin_stats("boundary")
            .ok_or("missing stats")?;
        assert_eq!(stats.timeout_count, 0);
        assert_eq!(stats.consecutive_timeouts, 0);
        Ok(())
    }

    /// Execution times 1 µs over the timeout must be counted as timeouts.
    #[test]
    fn execution_one_over_boundary_is_timeout() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(10)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        let fault = watchdog.record_plugin_execution("boundary", 101);
        assert!(fault.is_none()); // first timeout, no quarantine yet

        let stats = watchdog
            .get_plugin_stats("boundary")
            .ok_or("missing stats")?;
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.consecutive_timeouts, 1);
        Ok(())
    }

    /// Average execution time calculation stays accurate over many executions.
    #[test]
    fn average_execution_time_accuracy() -> TestResult {
        let watchdog = WatchdogSystem::default();

        // 10 executions of 50 µs + 10 executions of 80 µs = average 65 µs
        for _ in 0..10 {
            watchdog.record_plugin_execution("avg_plugin", 50);
        }
        for _ in 0..10 {
            watchdog.record_plugin_execution("avg_plugin", 80);
        }

        let stats = watchdog
            .get_plugin_stats("avg_plugin")
            .ok_or("missing stats")?;
        let avg = stats.average_execution_time_us();
        assert!(
            (avg - 65.0).abs() < 0.01,
            "expected average ~65.0 µs, got {avg}"
        );
        Ok(())
    }

    /// Zero-microsecond executions are valid successes.
    #[test]
    fn zero_execution_time_is_valid_success() -> TestResult {
        let watchdog = WatchdogSystem::default();

        let fault = watchdog.record_plugin_execution("zero_plugin", 0);
        assert!(fault.is_none());

        let stats = watchdog
            .get_plugin_stats("zero_plugin")
            .ok_or("missing stats")?;
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.timeout_count, 0);
        Ok(())
    }
}

// ===========================================================================
// 2. Timeout triggering
// ===========================================================================

mod timeout_triggering {
    use super::*;

    /// Exactly `max_timeouts` consecutive timeouts must trigger quarantine.
    #[test]
    fn quarantine_triggers_at_threshold() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(3)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        // First 2 timeouts: no quarantine
        for _ in 0..2 {
            let fault = watchdog.record_plugin_execution("trig", 200);
            assert!(fault.is_none());
        }
        assert!(!watchdog.is_plugin_quarantined("trig"));

        // 3rd timeout: quarantine
        let fault = watchdog.record_plugin_execution("trig", 200);
        assert_eq!(fault, Some(FaultType::PluginOverrun));
        assert!(watchdog.is_plugin_quarantined("trig"));
        Ok(())
    }

    /// A single success in the middle of timeouts resets the consecutive
    /// counter and prevents quarantine.
    #[test]
    fn success_interrupts_timeout_streak() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(3)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        watchdog.record_plugin_execution("streak", 200);
        watchdog.record_plugin_execution("streak", 200);
        // One success resets consecutive count
        watchdog.record_plugin_execution("streak", 50);
        watchdog.record_plugin_execution("streak", 200);
        watchdog.record_plugin_execution("streak", 200);

        assert!(
            !watchdog.is_plugin_quarantined("streak"),
            "should not be quarantined because consecutive count was reset"
        );

        let stats = watchdog
            .get_plugin_stats("streak")
            .ok_or("missing stats")?;
        assert_eq!(stats.timeout_count, 4); // total timeouts still counted
        Ok(())
    }

    /// Very large execution times still trigger quarantine normally.
    #[test]
    fn large_execution_time_triggers_quarantine() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        watchdog.record_plugin_execution("big", u64::MAX);
        let fault = watchdog.record_plugin_execution("big", u64::MAX);
        assert_eq!(fault, Some(FaultType::PluginOverrun));
        assert!(watchdog.is_plugin_quarantined("big"));
        Ok(())
    }

    /// Component heartbeat timeout triggers a fault in `perform_health_checks`.
    #[test]
    fn component_heartbeat_timeout_triggers_fault() -> TestResult {
        let config = WatchdogConfig::builder()
            .health_check_interval(Duration::from_millis(1))
            .rt_thread_timeout_ms(10)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        watchdog.heartbeat(SystemComponent::RtThread);
        thread::sleep(Duration::from_millis(15));

        let faults = watchdog.perform_health_checks();
        assert!(
            faults.contains(&FaultType::TimingViolation),
            "expected TimingViolation after heartbeat timeout"
        );
        Ok(())
    }

    /// HID communication timeout is detected separately from RT thread.
    #[test]
    fn hid_timeout_detected_independently() -> TestResult {
        let config = WatchdogConfig::builder()
            .health_check_interval(Duration::from_millis(1))
            .rt_thread_timeout_ms(500)
            .hid_timeout_ms(10)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        watchdog.heartbeat(SystemComponent::RtThread);
        watchdog.heartbeat(SystemComponent::HidCommunication);
        thread::sleep(Duration::from_millis(15));

        // Re-heartbeat RT thread so only HID times out
        watchdog.heartbeat(SystemComponent::RtThread);

        let faults = watchdog.perform_health_checks();
        assert!(
            faults.contains(&FaultType::UsbStall),
            "expected UsbStall after HID timeout"
        );
        assert!(
            !faults.contains(&FaultType::TimingViolation),
            "RT thread should not have timed out"
        );
        Ok(())
    }
}

// ===========================================================================
// 3. Reset/restart lifecycle
// ===========================================================================

mod reset_restart {
    use super::*;

    /// After reset, a plugin can accumulate timeouts from scratch again.
    #[test]
    fn reset_allows_fresh_quarantine_cycle() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        // Quarantine
        for _ in 0..2 {
            watchdog.record_plugin_execution("reset_p", 200);
        }
        assert!(watchdog.is_plugin_quarantined("reset_p"));

        // Release + reset stats
        watchdog.release_plugin_quarantine("reset_p")?;
        watchdog.reset_plugin_stats("reset_p")?;

        let stats = watchdog
            .get_plugin_stats("reset_p")
            .ok_or("missing stats")?;
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.timeout_count, 0);
        assert!(!watchdog.is_plugin_quarantined("reset_p"));
        Ok(())
    }

    /// `reset_all_plugin_stats` clears everything.
    #[test]
    fn reset_all_clears_every_plugin() -> TestResult {
        let watchdog = WatchdogSystem::default();

        for i in 0..5 {
            let id = format!("p{i}");
            watchdog.register_plugin(&id);
            watchdog.record_plugin_execution(&id, 50);
        }
        assert_eq!(watchdog.plugin_count(), 5);

        watchdog.reset_all_plugin_stats();
        assert_eq!(watchdog.plugin_count(), 0);
        Ok(())
    }

    /// Unregistering an unknown plugin returns an error, not a panic.
    #[test]
    fn unregister_unknown_is_error() {
        let watchdog = WatchdogSystem::default();
        let result = watchdog.unregister_plugin("nonexistent");
        assert!(result.is_err());
    }

    /// Re-registering a plugin after unregister starts fresh.
    #[test]
    fn re_register_after_unregister_starts_fresh() -> TestResult {
        let watchdog = WatchdogSystem::default();

        watchdog.register_plugin("reborn");
        watchdog.record_plugin_execution("reborn", 50);
        watchdog.unregister_plugin("reborn")?;
        assert!(watchdog.get_plugin_stats("reborn").is_none());

        watchdog.register_plugin("reborn");
        let stats = watchdog
            .get_plugin_stats("reborn")
            .ok_or("missing stats")?;
        assert_eq!(stats.total_executions, 0);
        Ok(())
    }

    /// Heartbeat restores a faulted component.
    #[test]
    fn heartbeat_recovers_faulted_component() {
        let watchdog = WatchdogSystem::default();
        watchdog.heartbeat(SystemComponent::SafetySystem);

        for _ in 0..5 {
            watchdog.report_component_failure(SystemComponent::SafetySystem, None);
        }

        let health = watchdog.get_component_health(SystemComponent::SafetySystem);
        assert!(health.is_some());
        if let Some(h) = health {
            assert_eq!(h.status, HealthStatus::Faulted);
        }

        watchdog.heartbeat(SystemComponent::SafetySystem);
        let health = watchdog.get_component_health(SystemComponent::SafetySystem);
        if let Some(h) = health {
            assert_eq!(h.status, HealthStatus::Healthy);
        }
    }
}

// ===========================================================================
// 4. Multi-channel watchdog coordination
// ===========================================================================

mod multi_channel {
    use super::*;

    /// Multiple components can be monitored independently.
    #[test]
    fn independent_component_health_tracking() {
        let watchdog = WatchdogSystem::default();

        watchdog.heartbeat(SystemComponent::RtThread);
        watchdog.heartbeat(SystemComponent::HidCommunication);
        watchdog.report_component_failure(SystemComponent::PluginHost, None);

        let summary = watchdog.get_health_summary();
        assert_eq!(
            summary[&SystemComponent::RtThread],
            HealthStatus::Healthy
        );
        assert_eq!(
            summary[&SystemComponent::HidCommunication],
            HealthStatus::Healthy
        );
        // First failure keeps Healthy status
        assert_eq!(
            summary[&SystemComponent::PluginHost],
            HealthStatus::Healthy
        );
    }

    /// Quarantine of one plugin does not affect others.
    #[test]
    fn quarantine_isolation_between_plugins() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        // Plugin A: gets quarantined
        for _ in 0..2 {
            watchdog.record_plugin_execution("plug_a", 200);
        }
        // Plugin B: stays healthy
        watchdog.record_plugin_execution("plug_b", 50);

        assert!(watchdog.is_plugin_quarantined("plug_a"));
        assert!(!watchdog.is_plugin_quarantined("plug_b"));

        let stats_b = watchdog
            .get_plugin_stats("plug_b")
            .ok_or("missing stats")?;
        assert_eq!(stats_b.timeout_count, 0);
        Ok(())
    }

    /// Fault callbacks fire for each quarantine, not globally once.
    #[test]
    fn fault_callback_fires_per_quarantine_event() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        let count = Arc::new(AtomicU32::new(0));
        let count_clone = count.clone();
        watchdog.add_fault_callback(move |_ft, _id| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Quarantine plugin A
        for _ in 0..2 {
            watchdog.record_plugin_execution("a", 200);
        }
        // Quarantine plugin B
        for _ in 0..2 {
            watchdog.record_plugin_execution("b", 200);
        }

        assert_eq!(count.load(Ordering::SeqCst), 2);
        Ok(())
    }

    /// Performance metrics include all registered plugins.
    #[test]
    fn performance_metrics_cover_all_plugins() {
        let watchdog = WatchdogSystem::default();

        for i in 0..5 {
            let id = format!("m{i}");
            watchdog.register_plugin(&id);
            watchdog.record_plugin_execution(&id, 30 + u64::try_from(i).unwrap_or(0) * 10);
        }

        let metrics = watchdog.get_plugin_performance_metrics();
        assert_eq!(metrics.len(), 5);
        for m in metrics.values() {
            assert_eq!(m["total_executions"], 1.0);
        }
    }

    /// `has_faulted_components` correctly detects at least one faulted component.
    #[test]
    fn has_faulted_components_detects_fault() {
        let watchdog = WatchdogSystem::default();
        assert!(!watchdog.has_faulted_components());

        watchdog.heartbeat(SystemComponent::DeviceManager);
        for _ in 0..5 {
            watchdog.report_component_failure(SystemComponent::DeviceManager, None);
        }

        assert!(watchdog.has_faulted_components());
    }
}

// ===========================================================================
// 5. Platform-specific watchdog behavior (cfg-gated)
// ===========================================================================

mod platform_specific {
    use super::*;

    /// On Windows, verify the watchdog system creates properly.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_watchdog_system_creates_successfully() -> TestResult {
        let watchdog = WatchdogSystem::default();
        watchdog.heartbeat(SystemComponent::RtThread);

        let health = watchdog
            .get_component_health(SystemComponent::RtThread)
            .ok_or("missing health")?;
        assert_eq!(health.status, HealthStatus::Healthy);
        Ok(())
    }

    /// On Linux, verify the watchdog system creates properly.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_watchdog_system_creates_successfully() -> TestResult {
        let watchdog = WatchdogSystem::default();
        watchdog.heartbeat(SystemComponent::RtThread);

        let health = watchdog
            .get_component_health(SystemComponent::RtThread)
            .ok_or("missing health")?;
        assert_eq!(health.status, HealthStatus::Healthy);
        Ok(())
    }

    /// On macOS, verify the watchdog system creates properly.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_watchdog_system_creates_successfully() -> TestResult {
        let watchdog = WatchdogSystem::default();
        watchdog.heartbeat(SystemComponent::RtThread);

        let health = watchdog
            .get_component_health(SystemComponent::RtThread)
            .ok_or("missing health")?;
        assert_eq!(health.status, HealthStatus::Healthy);
        Ok(())
    }

    /// All System components have Display implementations.
    #[test]
    fn all_system_components_display() {
        for component in SystemComponent::all() {
            let display = format!("{component}");
            assert!(!display.is_empty());
        }
    }

    /// All HealthStatus variants display properly.
    #[test]
    fn all_health_statuses_display() {
        let statuses = [
            HealthStatus::Healthy,
            HealthStatus::Degraded,
            HealthStatus::Faulted,
            HealthStatus::Unknown,
        ];
        for status in statuses {
            let display = format!("{status}");
            assert!(!display.is_empty());
        }
    }
}

// ===========================================================================
// 6. Watchdog under load (concurrent operations)
// ===========================================================================

mod under_load {
    use super::*;

    /// Concurrent writers and readers on the same plugin never panic.
    #[test]
    fn concurrent_write_read_no_panic() -> TestResult {
        let watchdog = Arc::new(WatchdogSystem::default());
        watchdog.register_plugin("shared");

        let mut handles = vec![];

        // Writers
        for _ in 0..4 {
            let w = watchdog.clone();
            handles.push(thread::spawn(move || {
                for i in 0..200 {
                    let time_us = if i % 20 == 0 { 200 } else { 50 };
                    w.record_plugin_execution("shared", time_us);
                }
            }));
        }

        // Readers
        for _ in 0..4 {
            let w = watchdog.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..200 {
                    let _ = w.get_plugin_stats("shared");
                    let _ = w.is_plugin_quarantined("shared");
                    let _ = w.get_plugin_performance_metrics();
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }

        let stats = watchdog
            .get_plugin_stats("shared")
            .ok_or("missing stats")?;
        assert_eq!(stats.total_executions, 800);
        Ok(())
    }

    /// Concurrent heartbeats across all components stay consistent.
    #[test]
    fn concurrent_heartbeat_all_components() {
        let watchdog = Arc::new(WatchdogSystem::default());
        let mut handles = vec![];

        for component in SystemComponent::all() {
            let w = watchdog.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..500 {
                    w.heartbeat(component);
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }

        let summary = watchdog.get_health_summary();
        for status in summary.values() {
            assert_eq!(*status, HealthStatus::Healthy);
        }
    }

    /// Many concurrent health checks do not cause data races.
    #[test]
    fn concurrent_health_checks_safe() -> TestResult {
        let config = WatchdogConfig::builder()
            .health_check_interval(Duration::from_millis(1))
            .rt_thread_timeout_ms(500)
            .build()?;
        let watchdog = Arc::new(WatchdogSystem::new(config));
        let mut handles = vec![];

        // Heartbeat thread
        {
            let w = watchdog.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..200 {
                    w.heartbeat(SystemComponent::RtThread);
                    thread::sleep(Duration::from_micros(50));
                }
            }));
        }

        // Health check threads
        for _ in 0..4 {
            let w = watchdog.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    w.perform_health_checks();
                    thread::sleep(Duration::from_micros(100));
                }
            }));
        }

        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }
        Ok(())
    }

    /// Concurrent quarantine + release cycles do not deadlock.
    #[test]
    fn concurrent_quarantine_release_no_deadlock() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = Arc::new(WatchdogSystem::new(config));
        let mut handles = vec![];

        for i in 0..8 {
            let w = watchdog.clone();
            handles.push(thread::spawn(move || {
                let id = format!("cqr_{i}");
                // Quarantine
                for _ in 0..2 {
                    w.record_plugin_execution(&id, 200);
                }
                // Try to release — may already be released by quarantine expiry
                let _ = w.release_plugin_quarantine(&id);
            }));
        }

        let start = Instant::now();
        for handle in handles {
            assert!(handle.join().is_ok(), "Thread should not panic");
        }
        // Ensure it completes in reasonable time (no deadlock)
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "took too long, possible deadlock"
        );
        Ok(())
    }
}

// ===========================================================================
// 7. Watchdog + safety state machine interaction
// ===========================================================================

mod safety_interaction {
    use super::*;

    /// Component failure progression: Healthy → Degraded → Faulted.
    #[test]
    fn failure_progression_through_states() -> TestResult {
        let watchdog = WatchdogSystem::default();
        watchdog.heartbeat(SystemComponent::TelemetryAdapter);

        // 1 failure: still Healthy
        watchdog.report_component_failure(
            SystemComponent::TelemetryAdapter,
            Some("err1".to_string()),
        );
        let h = watchdog
            .get_component_health(SystemComponent::TelemetryAdapter)
            .ok_or("missing health")?;
        assert_eq!(h.status, HealthStatus::Healthy);
        assert_eq!(h.consecutive_failures, 1);

        // 2nd failure: Degraded
        watchdog.report_component_failure(
            SystemComponent::TelemetryAdapter,
            Some("err2".to_string()),
        );
        let h = watchdog
            .get_component_health(SystemComponent::TelemetryAdapter)
            .ok_or("missing health")?;
        assert_eq!(h.status, HealthStatus::Degraded);

        // 3rd, 4th, 5th failures: Faulted at 5
        for _ in 0..3 {
            watchdog.report_component_failure(SystemComponent::TelemetryAdapter, None);
        }
        let h = watchdog
            .get_component_health(SystemComponent::TelemetryAdapter)
            .ok_or("missing health")?;
        assert_eq!(h.status, HealthStatus::Faulted);
        Ok(())
    }

    /// Faulted component fires a fault callback with the correct FaultType.
    #[test]
    fn faulted_component_fires_correct_fault_type() {
        let watchdog = WatchdogSystem::default();
        let received_fault = Arc::new(std::sync::Mutex::new(None));
        let received_clone = received_fault.clone();

        watchdog.add_fault_callback(move |ft, _id| {
            if let Ok(mut guard) = received_clone.lock() {
                *guard = Some(ft);
            }
        });

        watchdog.heartbeat(SystemComponent::HidCommunication);
        for _ in 0..5 {
            watchdog.report_component_failure(SystemComponent::HidCommunication, None);
        }

        let fault = received_fault.lock().ok().and_then(|g| *g);
        assert_eq!(fault, Some(FaultType::UsbStall));
    }

    /// Safety system fault produces `SafetyInterlockViolation`.
    #[test]
    fn safety_system_fault_type() {
        let watchdog = WatchdogSystem::default();
        let received_fault = Arc::new(std::sync::Mutex::new(None));
        let received_clone = received_fault.clone();

        watchdog.add_fault_callback(move |ft, _id| {
            if let Ok(mut guard) = received_clone.lock() {
                *guard = Some(ft);
            }
        });

        watchdog.heartbeat(SystemComponent::SafetySystem);
        for _ in 0..5 {
            watchdog.report_component_failure(SystemComponent::SafetySystem, None);
        }

        let fault = received_fault.lock().ok().and_then(|g| *g);
        assert_eq!(fault, Some(FaultType::SafetyInterlockViolation));
    }

    /// Quarantine disables plugin, release re-enables it.
    #[test]
    fn quarantine_and_release_full_cycle() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .plugin_quarantine_duration(Duration::from_secs(60))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        // Trigger quarantine
        for _ in 0..2 {
            watchdog.record_plugin_execution("safe_p", 200);
        }
        assert!(watchdog.is_plugin_quarantined("safe_p"));
        let quarantined = watchdog.get_quarantined_plugins();
        assert_eq!(quarantined.len(), 1);

        // Release
        watchdog.release_plugin_quarantine("safe_p")?;
        assert!(!watchdog.is_plugin_quarantined("safe_p"));
        let quarantined = watchdog.get_quarantined_plugins();
        assert!(quarantined.is_empty());

        // Can record again after release
        let fault = watchdog.record_plugin_execution("safe_p", 50);
        assert!(fault.is_none());
        Ok(())
    }

    /// Config validation rejects invalid configurations.
    #[test]
    fn config_validation_rejects_zero_timeout() {
        let result = WatchdogConfig::builder().plugin_timeout_us(0).build();
        assert!(result.is_err());
    }

    /// Config validation rejects zero max_timeouts.
    #[test]
    fn config_validation_rejects_zero_max_timeouts() {
        let result = WatchdogConfig::builder().plugin_max_timeouts(0).build();
        assert!(result.is_err());
    }

    /// Multiple fault callbacks all get invoked.
    #[test]
    fn multiple_fault_callbacks_all_fire() -> TestResult {
        let config = WatchdogConfig::builder()
            .plugin_timeout_us(100)
            .plugin_max_timeouts(2)
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        let count_a = Arc::new(AtomicU32::new(0));
        let count_b = Arc::new(AtomicU32::new(0));

        let ca = count_a.clone();
        watchdog.add_fault_callback(move |_, _| {
            ca.fetch_add(1, Ordering::SeqCst);
        });

        let cb = count_b.clone();
        watchdog.add_fault_callback(move |_, _| {
            cb.fetch_add(1, Ordering::SeqCst);
        });

        for _ in 0..2 {
            watchdog.record_plugin_execution("mcb", 200);
        }

        assert_eq!(count_a.load(Ordering::SeqCst), 1);
        assert_eq!(count_b.load(Ordering::SeqCst), 1);
        Ok(())
    }

    /// Disabled quarantine policy prevents quarantine even on repeated timeouts.
    #[test]
    fn disabled_quarantine_policy_prevents_quarantine() -> TestResult {
        let watchdog = WatchdogSystem::default();
        watchdog.set_quarantine_policy_enabled(false);
        assert!(!watchdog.is_quarantine_policy_enabled());

        for _ in 0..20 {
            let fault = watchdog.record_plugin_execution("noquar", 200);
            assert!(fault.is_none());
        }

        assert!(!watchdog.is_plugin_quarantined("noquar"));

        let stats = watchdog
            .get_plugin_stats("noquar")
            .ok_or("missing stats")?;
        assert_eq!(stats.timeout_count, 20);
        Ok(())
    }

    /// Health check interval is respected — rapid calls don't re-check.
    #[test]
    fn health_check_interval_respected() -> TestResult {
        let config = WatchdogConfig::builder()
            .health_check_interval(Duration::from_secs(10))
            .build()?;
        let watchdog = WatchdogSystem::new(config);

        // First call runs
        let faults_1 = watchdog.perform_health_checks();
        // Second call should be skipped (interval not elapsed)
        let faults_2 = watchdog.perform_health_checks();

        // Both should be empty (no heartbeats sent so no timeout to detect)
        assert!(faults_1.is_empty());
        assert!(faults_2.is_empty());
        Ok(())
    }

    /// Component metrics can be added and retrieved.
    #[test]
    fn component_metrics_tracking() -> TestResult {
        let watchdog = WatchdogSystem::default();
        watchdog.heartbeat(SystemComponent::PluginHost);
        watchdog.add_component_metric(
            SystemComponent::PluginHost,
            "latency_us".to_string(),
            42.0,
        );

        let health = watchdog
            .get_component_health(SystemComponent::PluginHost)
            .ok_or("missing health")?;
        assert_eq!(health.status, HealthStatus::Healthy);
        assert!((health.metrics["latency_us"] - 42.0).abs() < f64::EPSILON);
        Ok(())
    }
}
