//! Tests for full watchdog lifecycle scenarios.

use openracing_watchdog::prelude::*;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn test_full_plugin_lifecycle() -> TestResult {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(100)
        .plugin_max_timeouts(3)
        .plugin_quarantine_duration(Duration::from_secs(60))
        .build()?;

    let watchdog = WatchdogSystem::new(config);

    // 1. Register plugin
    watchdog.register_plugin("test_plugin");
    assert_eq!(watchdog.plugin_count(), 1);

    // 2. Record successful executions
    for _ in 0..10 {
        let fault = watchdog.record_plugin_execution("test_plugin", 50);
        assert!(fault.is_none());
    }

    let stats = watchdog.get_plugin_stats("test_plugin");
    assert!(stats.is_some());
    let stats = stats.ok_or("Expected plugin stats")?;
    assert_eq!(stats.total_executions, 10);
    assert_eq!(stats.timeout_count, 0);
    assert!(!watchdog.is_plugin_quarantined("test_plugin"));

    // 3. Record timeouts leading to quarantine
    for i in 0..3 {
        let fault = watchdog.record_plugin_execution("test_plugin", 150);
        if i == 2 {
            assert_eq!(fault, Some(FaultType::PluginOverrun));
        }
    }

    assert!(watchdog.is_plugin_quarantined("test_plugin"));

    // 4. Release from quarantine
    watchdog.release_plugin_quarantine("test_plugin")?;
    assert!(!watchdog.is_plugin_quarantined("test_plugin"));

    // 5. Unregister plugin
    watchdog.unregister_plugin("test_plugin")?;
    assert_eq!(watchdog.plugin_count(), 0);
    Ok(())
}

#[test]
fn test_component_health_lifecycle() -> TestResult {
    let watchdog = WatchdogSystem::default();

    // Initial state - all components unknown
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .ok_or("Expected component health")?;
    assert_eq!(health.status, HealthStatus::Unknown);

    // Send heartbeat - becomes healthy
    watchdog.heartbeat(SystemComponent::RtThread);
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .ok_or("Expected component health")?;
    assert_eq!(health.status, HealthStatus::Healthy);

    // Report failures - progression through statuses
    watchdog.report_component_failure(SystemComponent::RtThread, Some("Error 1".to_string()));
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .ok_or("Expected component health")?;
    assert_eq!(health.status, HealthStatus::Healthy); // First failure still healthy

    watchdog.report_component_failure(SystemComponent::RtThread, Some("Error 2".to_string()));
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .ok_or("Expected component health")?;
    assert_eq!(health.status, HealthStatus::Degraded);

    // Heartbeat restores health
    watchdog.heartbeat(SystemComponent::RtThread);
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .ok_or("Expected component health")?;
    assert_eq!(health.status, HealthStatus::Healthy);
    Ok(())
}

#[test]
fn test_multiple_plugins_interaction() {
    let watchdog = WatchdogSystem::default();

    // Register multiple plugins
    watchdog.register_plugin("plugin_a");
    watchdog.register_plugin("plugin_b");
    watchdog.register_plugin("plugin_c");

    // Plugin A: consistently fast
    for _ in 0..10 {
        watchdog.record_plugin_execution("plugin_a", 30);
    }

    // Plugin B: some timeouts
    watchdog.record_plugin_execution("plugin_b", 50);
    watchdog.record_plugin_execution("plugin_b", 150);
    watchdog.record_plugin_execution("plugin_b", 75);

    // Plugin C: will be quarantined
    for _ in 0..5 {
        watchdog.record_plugin_execution("plugin_c", 200);
    }

    // Verify states
    assert!(!watchdog.is_plugin_quarantined("plugin_a"));
    assert!(!watchdog.is_plugin_quarantined("plugin_b"));
    assert!(watchdog.is_plugin_quarantined("plugin_c"));

    // Verify metrics
    let metrics = watchdog.get_plugin_performance_metrics();
    assert_eq!(metrics["plugin_a"]["timeout_rate_percent"], 0.0);
    assert!((metrics["plugin_b"]["timeout_rate_percent"] - 33.33).abs() < 0.1);
    assert_eq!(metrics["plugin_c"]["timeout_rate_percent"], 100.0);
}

#[test]
fn test_health_check_interval() -> TestResult {
    let config = WatchdogConfig::builder()
        .health_check_interval(Duration::from_millis(10))
        .rt_thread_timeout_ms(100) // Set higher timeout to avoid race
        .build()?;

    let watchdog = WatchdogSystem::new(config);

    // First check runs immediately
    watchdog.heartbeat(SystemComponent::RtThread);
    let faults = watchdog.perform_health_checks();
    assert!(faults.is_empty());

    // Quick second check - should be skipped due to interval
    let faults = watchdog.perform_health_checks();
    assert!(faults.is_empty());

    // Wait for interval to pass
    std::thread::sleep(Duration::from_millis(15));

    // Send a fresh heartbeat before checking
    watchdog.heartbeat(SystemComponent::RtThread);

    // Now check should run again
    let faults = watchdog.perform_health_checks();
    // No faults since we have a recent heartbeat
    assert!(faults.is_empty());
    Ok(())
}

#[test]
fn test_fault_callback_invocation() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    let fault_count = Arc::new(AtomicU32::new(0));
    let fault_count_clone = fault_count.clone();

    let watchdog = WatchdogSystem::default();
    watchdog.add_fault_callback(move |_fault_type, _component| {
        fault_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    // Trigger quarantine
    for _ in 0..5 {
        watchdog.record_plugin_execution("test_plugin", 200);
    }

    assert_eq!(fault_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_quarantine_policy_affects_behavior() -> TestResult {
    let watchdog = WatchdogSystem::default();

    // Disable quarantine policy
    watchdog.set_quarantine_policy_enabled(false);

    // Record many timeouts
    for _ in 0..20 {
        let fault = watchdog.record_plugin_execution("test_plugin", 200);
        assert!(fault.is_none()); // Should never return a fault
    }

    assert!(!watchdog.is_plugin_quarantined("test_plugin"));

    // Stats should still track timeouts
    let stats = watchdog
        .get_plugin_stats("test_plugin")
        .ok_or("Expected plugin stats")?;
    assert_eq!(stats.timeout_count, 20);

    // Re-enable and verify quarantine works
    watchdog.set_quarantine_policy_enabled(true);

    // Reset consecutive timeouts by one success
    watchdog.record_plugin_execution("test_plugin", 50);

    // Now trigger quarantine
    for i in 0..5 {
        let fault = watchdog.record_plugin_execution("test_plugin", 200);
        if i == 4 {
            assert_eq!(fault, Some(FaultType::PluginOverrun));
        }
    }
    Ok(())
}

#[test]
fn test_reset_statistics() -> TestResult {
    let watchdog = WatchdogSystem::default();

    // Record various executions
    for _ in 0..10 {
        watchdog.record_plugin_execution("test_plugin", 100);
    }
    for _ in 0..3 {
        watchdog.record_plugin_execution("test_plugin", 200);
    }

    let stats = watchdog
        .get_plugin_stats("test_plugin")
        .ok_or("Expected plugin stats")?;
    assert_eq!(stats.total_executions, 13);
    assert_eq!(stats.timeout_count, 3);

    // Reset stats
    watchdog.reset_plugin_stats("test_plugin")?;

    let stats = watchdog
        .get_plugin_stats("test_plugin")
        .ok_or("Expected plugin stats")?;
    assert_eq!(stats.total_executions, 0);
    assert_eq!(stats.timeout_count, 0);
    Ok(())
}

// ---------------------------------------------------------------------------
// Watchdog crate hardening: successive timeouts, rapid cycling, health checks
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_successive_quarantines() -> TestResult {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(100)
        .plugin_max_timeouts(3)
        .plugin_quarantine_duration(Duration::from_secs(60))
        .build()?;
    let watchdog = WatchdogSystem::new(config);

    // First quarantine cycle
    for _ in 0..3 {
        watchdog.record_plugin_execution("plugin", 200);
    }
    assert!(watchdog.is_plugin_quarantined("plugin"));

    // Release and trigger again
    watchdog.release_plugin_quarantine("plugin")?;
    assert!(!watchdog.is_plugin_quarantined("plugin"));

    // Second quarantine cycle
    for _ in 0..3 {
        watchdog.record_plugin_execution("plugin", 200);
    }
    assert!(watchdog.is_plugin_quarantined("plugin"));

    // Stats should reflect both cycles
    let stats = watchdog
        .get_plugin_stats("plugin")
        .ok_or("Expected stats")?;
    assert!(stats.quarantine_count >= 2);
    Ok(())
}

#[test]
fn test_timeout_count_resets_on_success() -> TestResult {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(100)
        .plugin_max_timeouts(5)
        .plugin_quarantine_duration(Duration::from_secs(60))
        .build()?;
    let watchdog = WatchdogSystem::new(config);

    // 4 timeouts (below threshold of 5)
    for _ in 0..4 {
        let fault = watchdog.record_plugin_execution("plugin", 200);
        assert!(fault.is_none());
    }

    // One success resets consecutive count
    watchdog.record_plugin_execution("plugin", 50);

    // 4 more timeouts — still below threshold because consecutive was reset
    for _ in 0..4 {
        let fault = watchdog.record_plugin_execution("plugin", 200);
        assert!(fault.is_none());
    }

    assert!(!watchdog.is_plugin_quarantined("plugin"));
    Ok(())
}

#[test]
fn test_health_check_timeout_detection() -> TestResult {
    let config = WatchdogConfig::builder()
        .health_check_interval(Duration::from_millis(1))
        .rt_thread_timeout_ms(10)
        .build()?;
    let watchdog = WatchdogSystem::new(config);

    // Send heartbeat then let it expire
    watchdog.heartbeat(SystemComponent::RtThread);
    std::thread::sleep(Duration::from_millis(15));

    let faults = watchdog.perform_health_checks();
    assert!(
        faults.contains(&FaultType::TimingViolation),
        "Expected TimingViolation fault after RT thread heartbeat timeout"
    );
    Ok(())
}

#[test]
fn test_component_failure_progression_to_faulted() {
    let watchdog = WatchdogSystem::default();
    watchdog.heartbeat(SystemComponent::HidCommunication);

    // 5+ failures needed: Healthy → Degraded → Faulted
    for _ in 0..5 {
        watchdog.report_component_failure(SystemComponent::HidCommunication, None);
    }

    let health = watchdog.get_component_health(SystemComponent::HidCommunication);
    assert!(health.is_some());
    let health = health.expect("checked");
    assert_eq!(health.status, HealthStatus::Faulted);
}

#[test]
fn test_release_non_quarantined_plugin_errors() {
    let watchdog = WatchdogSystem::default();
    watchdog.register_plugin("plugin");

    // Not quarantined — release should fail
    let result = watchdog.release_plugin_quarantine("plugin");
    assert!(result.is_err());
}

#[test]
fn test_unregister_unknown_plugin_errors() {
    let watchdog = WatchdogSystem::default();
    let result = watchdog.unregister_plugin("ghost");
    assert!(result.is_err());
}

#[test]
fn test_faulted_component_recovers_on_heartbeat() {
    let watchdog = WatchdogSystem::default();
    watchdog.heartbeat(SystemComponent::RtThread);

    // Drive to faulted (5+ failures)
    for _ in 0..5 {
        watchdog.report_component_failure(SystemComponent::RtThread, None);
    }
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .expect("component exists");
    assert_eq!(health.status, HealthStatus::Faulted);

    // Heartbeat restores
    watchdog.heartbeat(SystemComponent::RtThread);
    let health = watchdog
        .get_component_health(SystemComponent::RtThread)
        .expect("component exists");
    assert_eq!(health.status, HealthStatus::Healthy);
}
