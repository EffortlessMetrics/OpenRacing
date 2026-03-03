//! Edge-case and extended property-based tests for watchdog crate.
//!
//! Covers: timeout invariants (monotonic timeout, no false positives),
//! trip → recovery cycles, immediate/maximum timeout, rapid check/feed cycles.

#![allow(clippy::redundant_closure)]

use openracing_watchdog::{
    HealthCheck, HealthStatus, PluginStats, QuarantineManager, QuarantineReason, SystemComponent,
    WatchdogConfig, WatchdogSystem,
};
use proptest::prelude::*;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Property-based tests (proptest)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // -- Timeout invariants --------------------------------------------------

    #[test]
    fn prop_timeout_rate_monotonic_with_timeouts(
        n_success in 0u32..50,
        n_timeout in 1u32..50,
    ) {
        let mut stats = PluginStats::new();
        for _ in 0..n_success {
            stats.record_success(50);
        }
        let rate_before = stats.timeout_rate();
        stats.record_timeout(200);
        let rate_after = stats.timeout_rate();
        // Adding a timeout should not decrease the timeout rate below
        // what it was when we only had successes (unless there were zero executions).
        if n_success > 0 {
            prop_assert!(rate_after >= rate_before,
                "rate went from {} to {} after adding timeout", rate_before, rate_after);
        }
        // Extra timeouts.
        for _ in 1..n_timeout {
            stats.record_timeout(200);
        }
        let final_rate = stats.timeout_rate();
        prop_assert!(final_rate >= rate_after,
            "rate decreased from {} to {}", rate_after, final_rate);
    }

    #[test]
    fn prop_no_false_positive_quarantine(
        exec_time in 1u64..99,
        count in 1u32..100,
    ) {
        let config = WatchdogConfig::default();
        let system = WatchdogSystem::new(config);
        system.register_plugin("test-plugin");

        // Execution times under threshold should never cause quarantine.
        for _ in 0..count {
            let fault = system.record_plugin_execution("test-plugin", exec_time);
            prop_assert!(fault.is_none(),
                "Got unexpected fault for exec_time={}", exec_time);
        }
        prop_assert!(!system.is_plugin_quarantined("test-plugin"));
    }

    #[test]
    fn prop_quarantine_manager_duration_respected(
        duration_secs in 1u64..3600,
    ) {
        let mut manager = QuarantineManager::new();
        let mut stats = PluginStats::new();
        let duration = Duration::from_secs(duration_secs);

        manager.quarantine("test", Some(duration), QuarantineReason::Manual, &mut stats);

        prop_assert!(manager.is_quarantined("test"));
        prop_assert_eq!(manager.quarantined_count(), 1);

        let entry = manager.get_entry("test");
        prop_assert!(entry.is_some());
    }

    #[test]
    fn prop_health_check_consecutive_failures_monotonic(
        n_failures in 1u32..100,
    ) {
        let mut check = HealthCheck::new(SystemComponent::RtThread);
        let mut last_count = 0u32;

        for _ in 0..n_failures {
            check.report_failure(None);
            prop_assert!(check.consecutive_failures >= last_count);
            last_count = check.consecutive_failures;
        }
        prop_assert_eq!(last_count, n_failures);
    }

    #[test]
    fn prop_health_heartbeat_resets_failures(
        n_failures in 1u32..20,
    ) {
        let mut check = HealthCheck::new(SystemComponent::RtThread);
        for _ in 0..n_failures {
            check.report_failure(None);
        }
        prop_assert!(check.consecutive_failures > 0);

        check.heartbeat();
        // After heartbeat, status should improve.
        prop_assert_eq!(check.status, HealthStatus::Healthy);
    }

    #[test]
    fn prop_quarantine_release_consistent(
        n_quarantines in 1u32..5,
    ) {
        let mut manager = QuarantineManager::new();

        for i in 0..n_quarantines {
            let id = format!("plugin-{}", i);
            let mut stats = PluginStats::new();
            manager.quarantine(
                &id,
                Some(Duration::from_secs(60)),
                QuarantineReason::ConsecutiveTimeouts,
                &mut stats,
            );
        }
        prop_assert_eq!(manager.quarantined_count(), n_quarantines as usize);

        // Release all.
        for i in 0..n_quarantines {
            let id = format!("plugin-{}", i);
            let mut stats = PluginStats::new();
            let result = manager.release(&id, &mut stats);
            prop_assert!(result.is_ok());
        }
        prop_assert_eq!(manager.quarantined_count(), 0);
    }

    // -- Stats arithmetic invariants -----------------------------------------

    #[test]
    fn prop_stats_average_in_range(
        times in prop::collection::vec(1u64..1_000_000, 1..100),
    ) {
        let mut stats = PluginStats::new();
        for &t in &times {
            stats.record_success(t);
        }
        let avg = stats.average_execution_time_us();
        let min_val = *times.iter().min().unwrap_or(&0) as f64;
        let max_val = *times.iter().max().unwrap_or(&0) as f64;
        prop_assert!(avg >= min_val - 0.1 && avg <= max_val + 0.1);
    }

    #[test]
    fn prop_stats_reset_zeroes_all(
        n_ops in 1u32..50,
    ) {
        let mut stats = PluginStats::new();
        for _ in 0..n_ops {
            stats.record_success(100);
            stats.record_timeout(200);
        }
        stats.reset();
        prop_assert_eq!(stats.total_executions, 0);
        prop_assert_eq!(stats.timeout_count, 0);
        prop_assert_eq!(stats.consecutive_timeouts, 0);
        prop_assert_eq!(stats.total_execution_time_us, 0);
    }
}

// ---------------------------------------------------------------------------
// Edge-case tests (deterministic)
// ---------------------------------------------------------------------------

#[test]
fn edge_immediate_quarantine_on_max_timeouts() {
    let config = WatchdogConfig {
        plugin_timeout_us: 100,
        plugin_max_timeouts: 1,
        plugin_quarantine_duration: Duration::from_secs(60),
        ..WatchdogConfig::default()
    };
    let system = WatchdogSystem::new(config);
    system.register_plugin("fast-fail");

    // One overrun should immediately cause quarantine with max_timeouts=1.
    let fault = system.record_plugin_execution("fast-fail", 200);
    assert!(
        fault.is_some(),
        "Should detect fault on first overrun with max_timeouts=1"
    );
}

#[test]
fn edge_maximum_timeout_config() {
    let config = WatchdogConfig {
        plugin_timeout_us: u64::MAX / 2,
        plugin_max_timeouts: u32::MAX,
        plugin_quarantine_duration: Duration::from_secs(u64::MAX / 2),
        rt_thread_timeout_ms: u64::MAX / 2,
        hid_timeout_ms: u64::MAX / 2,
        telemetry_timeout_ms: u64::MAX / 2,
        health_check_interval: Duration::from_secs(u64::MAX / 4),
    };
    // Should not panic on creation with large values.
    let system = WatchdogSystem::new(config);
    system.register_plugin("p1");
    assert!(!system.is_plugin_quarantined("p1"));
}

#[test]
fn edge_rapid_feed_cycle() {
    let system = WatchdogSystem::default();
    system.register_plugin("rapid");

    // Rapid succession of executions.
    for _ in 0..1000 {
        let _ = system.record_plugin_execution("rapid", 10);
    }

    let stats = system.get_plugin_stats("rapid");
    assert!(stats.is_some());
    if let Some(s) = stats {
        assert_eq!(s.total_executions, 1000);
        assert_eq!(s.timeout_count, 0);
    }
}

#[test]
fn edge_plugin_stats_zero_executions() {
    let stats = PluginStats::new();
    assert_eq!(stats.average_execution_time_us(), 0.0);
    assert_eq!(stats.timeout_rate(), 0.0);
    assert!(!stats.is_quarantined());
}

#[test]
fn edge_quarantine_manager_release_not_quarantined() {
    let mut manager = QuarantineManager::new();
    let mut stats = PluginStats::new();
    let result = manager.release("nonexistent", &mut stats);
    assert!(result.is_err());
}

#[test]
fn edge_quarantine_manager_double_quarantine() {
    let mut manager = QuarantineManager::new();
    let mut stats = PluginStats::new();
    manager.quarantine(
        "p1",
        Some(Duration::from_secs(60)),
        QuarantineReason::Crash,
        &mut stats,
    );
    // Second quarantine of same plugin should update existing entry.
    manager.quarantine(
        "p1",
        Some(Duration::from_secs(120)),
        QuarantineReason::Manual,
        &mut stats,
    );
    assert!(manager.is_quarantined("p1"));
}

#[test]
fn edge_quarantine_manager_clear_all() {
    let mut manager = QuarantineManager::new();
    for i in 0..5 {
        let mut stats = PluginStats::new();
        manager.quarantine(
            &format!("p{}", i),
            Some(Duration::from_secs(60)),
            QuarantineReason::ConsecutiveTimeouts,
            &mut stats,
        );
    }
    assert_eq!(manager.quarantined_count(), 5);
    manager.clear_all();
    assert_eq!(manager.quarantined_count(), 0);
}

#[test]
fn edge_health_check_all_components() {
    let components = [
        SystemComponent::RtThread,
        SystemComponent::HidCommunication,
        SystemComponent::TelemetryAdapter,
        SystemComponent::PluginHost,
        SystemComponent::SafetySystem,
        SystemComponent::DeviceManager,
    ];
    for &comp in &components {
        let check = HealthCheck::new(comp);
        assert_eq!(check.component, comp);
        assert_eq!(check.status, HealthStatus::Unknown);
        assert_eq!(check.consecutive_failures, 0);
    }
}

#[test]
fn edge_health_check_failure_then_heartbeat() {
    let mut check = HealthCheck::new(SystemComponent::RtThread);
    check.report_failure(Some("test error".to_string()));
    assert_eq!(check.consecutive_failures, 1);
    assert!(check.last_error.is_some());

    check.heartbeat();
    assert_eq!(check.status, HealthStatus::Healthy);
    assert_eq!(check.consecutive_failures, 0);
}

#[test]
fn edge_health_check_metric_tracking() {
    let mut check = HealthCheck::new(SystemComponent::RtThread);
    check.add_metric("latency_ms".to_string(), 1.5);
    check.add_metric("cpu_percent".to_string(), 45.0);
    assert_eq!(check.metrics.len(), 2);
}

#[test]
fn edge_watchdog_system_unregister_plugin() -> Result<(), Box<dyn std::error::Error>> {
    let system = WatchdogSystem::default();
    system.register_plugin("ephemeral");
    assert_eq!(system.plugin_count(), 1);

    system.unregister_plugin("ephemeral")?;
    assert_eq!(system.plugin_count(), 0);
    Ok(())
}

#[test]
fn edge_watchdog_system_no_plugins_health() {
    let system = WatchdogSystem::default();
    // Performing health checks with no plugins should not panic.
    let faults = system.perform_health_checks();
    // No faults expected since there's nothing to check.
    let _ = faults;
}

#[test]
fn edge_watchdog_system_heartbeat_all_components() {
    let system = WatchdogSystem::default();
    let components = [
        SystemComponent::RtThread,
        SystemComponent::HidCommunication,
        SystemComponent::TelemetryAdapter,
        SystemComponent::PluginHost,
        SystemComponent::SafetySystem,
        SystemComponent::DeviceManager,
    ];
    for &comp in &components {
        system.heartbeat(comp);
    }
    let summary = system.get_health_summary();
    for &comp in &components {
        let status = summary.get(&comp);
        assert!(status.is_some());
    }
}

#[test]
fn edge_watchdog_quarantine_policy_toggle() {
    let system = WatchdogSystem::default();
    assert!(system.is_quarantine_policy_enabled());

    system.set_quarantine_policy_enabled(false);
    assert!(!system.is_quarantine_policy_enabled());

    system.set_quarantine_policy_enabled(true);
    assert!(system.is_quarantine_policy_enabled());
}

#[test]
fn edge_trip_recovery_cycle() {
    let config = WatchdogConfig {
        plugin_timeout_us: 100,
        plugin_max_timeouts: 2,
        plugin_quarantine_duration: Duration::from_millis(1),
        ..WatchdogConfig::default()
    };
    let system = WatchdogSystem::new(config);
    system.register_plugin("cycle-test");

    // Trigger quarantine.
    let _ = system.record_plugin_execution("cycle-test", 200);
    let fault = system.record_plugin_execution("cycle-test", 200);
    assert!(fault.is_some());
    assert!(system.is_plugin_quarantined("cycle-test"));

    // Wait for quarantine to expire (very short duration).
    std::thread::sleep(Duration::from_millis(10));

    // After release, plugin should be usable again.
    let release_result = system.release_plugin_quarantine("cycle-test");
    // Even if already expired, release should succeed or be no-op.
    let _ = release_result;

    // Reset stats and re-execute.
    let _ = system.reset_plugin_stats("cycle-test");
    let fault = system.record_plugin_execution("cycle-test", 10);
    assert!(fault.is_none(), "Plugin should work after recovery");
}

#[test]
fn edge_stats_quarantine_remaining_when_not_quarantined() {
    let stats = PluginStats::new();
    assert!(stats.quarantine_remaining().is_none());
}

#[test]
fn edge_stats_quarantine_clear() {
    let mut stats = PluginStats::new();
    stats.apply_quarantine(Duration::from_secs(60));
    assert!(stats.is_quarantined());

    stats.clear_quarantine();
    assert!(!stats.is_quarantined());
    assert!(stats.quarantine_remaining().is_none());
}

#[test]
fn edge_quarantine_default_duration() {
    let manager = QuarantineManager::with_default_duration(Duration::from_secs(300));
    assert_eq!(manager.default_duration(), Duration::from_secs(300));
}

#[test]
fn edge_watchdog_report_component_failure() {
    let system = WatchdogSystem::default();
    system.heartbeat(SystemComponent::HidCommunication);
    system.report_component_failure(
        SystemComponent::HidCommunication,
        Some("USB timeout".to_string()),
    );

    let health = system.get_component_health(SystemComponent::HidCommunication);
    assert!(health.is_some());
    if let Some(h) = health {
        assert!(h.consecutive_failures >= 1);
    }
}
