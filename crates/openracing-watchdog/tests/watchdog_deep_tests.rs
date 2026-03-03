//! Deep tests for the watchdog subsystem.
//!
//! Covers:
//! - Watchdog timer management
//! - Timeout detection
//! - Watchdog kick/reset
//! - Multiple watchdog instances
//! - Cascading timeouts
//! - Watchdog disable/enable
//! - Safety interlock integration
//! - Thread safety of watchdog operations
//! - Property-based tests for feed sequences

use openracing_watchdog::prelude::*;
use proptest::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn system_with_timeout(timeout_us: u64, max_timeouts: u32) -> WatchdogSystem {
    let config = WatchdogConfig {
        plugin_timeout_us: timeout_us,
        plugin_max_timeouts: max_timeouts,
        plugin_quarantine_duration: Duration::from_millis(500),
        ..Default::default()
    };
    WatchdogSystem::new(config)
}

// ===========================================================================
// 1. Watchdog timer management
// ===========================================================================

#[test]
fn timer_default_config_has_valid_values() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::default();
    assert!(config.validate().is_ok());
    assert_eq!(config.plugin_timeout_us, 100);
    assert_eq!(config.plugin_max_timeouts, 5);
    assert_eq!(config.rt_thread_timeout_ms, 10);
    assert_eq!(config.hid_timeout_ms, 50);
    assert_eq!(config.telemetry_timeout_ms, 1000);
    Ok(())
}

#[test]
fn timer_builder_produces_valid_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig::builder()
        .plugin_timeout_us(200)
        .plugin_max_timeouts(3)
        .plugin_quarantine_duration(Duration::from_secs(60))
        .rt_thread_timeout_ms(20)
        .hid_timeout_ms(100)
        .telemetry_timeout_ms(2000)
        .health_check_interval(Duration::from_millis(50))
        .build()?;

    assert_eq!(config.plugin_timeout_us, 200);
    assert_eq!(config.plugin_max_timeouts, 3);
    assert_eq!(config.plugin_quarantine_duration, Duration::from_secs(60));
    assert_eq!(config.rt_thread_timeout_ms, 20);
    assert_eq!(config.hid_timeout_ms, 100);
    assert_eq!(config.telemetry_timeout_ms, 2000);
    assert_eq!(config.health_check_interval, Duration::from_millis(50));
    Ok(())
}

#[test]
fn timer_builder_rejects_zero_timeout() {
    let result = WatchdogConfig::builder().plugin_timeout_us(0).build();
    assert!(result.is_err());
}

#[test]
fn timer_builder_rejects_zero_max_timeouts() {
    let result = WatchdogConfig::builder().plugin_max_timeouts(0).build();
    assert!(result.is_err());
}

#[test]
fn timer_builder_rejects_zero_rt_thread_timeout() {
    let result = WatchdogConfig::builder().rt_thread_timeout_ms(0).build();
    assert!(result.is_err());
}

#[test]
fn timer_builder_rejects_zero_quarantine_duration() {
    let result = WatchdogConfig::builder()
        .plugin_quarantine_duration(Duration::ZERO)
        .build();
    assert!(result.is_err());
}

#[test]
fn timer_system_exposes_config() {
    let wd = system_with_timeout(250, 7);
    let cfg = wd.get_config();
    assert_eq!(cfg.plugin_timeout_us, 250);
    assert_eq!(cfg.plugin_max_timeouts, 7);
}

#[test]
fn timer_health_check_interval_gates_check_frequency() {
    let config = WatchdogConfig {
        health_check_interval: Duration::from_secs(60),
        ..Default::default()
    };
    let wd = WatchdogSystem::new(config);

    // First call runs
    let faults1 = wd.perform_health_checks();
    // Second call within 60s should be no-op
    let faults2 = wd.perform_health_checks();

    // Both should return empty since no heartbeats have been sent (all Unknown status)
    assert!(faults1.is_empty());
    assert!(faults2.is_empty());
}

// ===========================================================================
// 2. Timeout detection
// ===========================================================================

#[test]
fn timeout_single_overrun_does_not_quarantine() {
    let wd = system_with_timeout(100, 5);
    let fault = wd.record_plugin_execution("plugin", 200);
    assert!(fault.is_none());
    assert!(!wd.is_plugin_quarantined("plugin"));
}

#[test]
fn timeout_exact_boundary_is_not_timeout() {
    let wd = system_with_timeout(100, 3);
    // Execution exactly at the threshold is NOT a timeout (> not >=)
    let fault = wd.record_plugin_execution("plugin", 100);
    assert!(fault.is_none());
    let stats = wd.get_plugin_stats("plugin");
    assert!(stats.is_some());
    if let Some(s) = stats {
        assert_eq!(s.timeout_count, 0);
    }
}

#[test]
fn timeout_one_over_boundary_is_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);
    let fault = wd.record_plugin_execution("plugin", 101);
    assert!(fault.is_none()); // first timeout, not yet quarantined
    let stats = wd.get_plugin_stats("plugin").ok_or("missing stats")?;
    assert_eq!(stats.timeout_count, 1);
    assert_eq!(stats.consecutive_timeouts, 1);
    Ok(())
}

#[test]
fn timeout_consecutive_threshold_triggers_quarantine() {
    let wd = system_with_timeout(100, 3);
    let mut last_fault = None;
    for _ in 0..3 {
        last_fault = wd.record_plugin_execution("plugin", 200);
    }
    assert_eq!(last_fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("plugin"));
}

#[test]
fn timeout_triggers_quarantine_and_callback() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);
    let triggered = Arc::new(AtomicU32::new(0));
    let t = Arc::clone(&triggered);
    wd.add_fault_callback(move |ft, _plugin| {
        if ft == FaultType::PluginOverrun {
            t.fetch_add(1, Ordering::SeqCst);
        }
    });

    for _ in 0..3 {
        wd.record_plugin_execution("slow_plugin", 200);
    }

    assert!(wd.is_plugin_quarantined("slow_plugin"));
    assert_eq!(triggered.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn timeout_component_heartbeat_rt_thread() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig {
        rt_thread_timeout_ms: 10,
        health_check_interval: Duration::from_millis(0),
        ..Default::default()
    };
    let wd = WatchdogSystem::new(config);

    wd.heartbeat(SystemComponent::RtThread);
    std::thread::sleep(Duration::from_millis(20));

    let faults = wd.perform_health_checks();
    assert!(
        faults.contains(&FaultType::TimingViolation),
        "Expected TimingViolation after heartbeat timeout"
    );
    Ok(())
}

#[test]
fn timeout_component_heartbeat_hid() -> Result<(), Box<dyn std::error::Error>> {
    let config = WatchdogConfig {
        hid_timeout_ms: 10,
        health_check_interval: Duration::from_millis(0),
        ..Default::default()
    };
    let wd = WatchdogSystem::new(config);

    wd.heartbeat(SystemComponent::HidCommunication);
    std::thread::sleep(Duration::from_millis(20));

    let faults = wd.perform_health_checks();
    assert!(
        faults.contains(&FaultType::UsbStall),
        "Expected UsbStall after HID heartbeat timeout"
    );
    Ok(())
}

// ===========================================================================
// 3. Watchdog kick/reset (re-arm behavior)
// ===========================================================================

#[test]
fn kick_success_resets_consecutive_timeouts() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 5);

    // Build up consecutive timeouts
    for _ in 0..4 {
        wd.record_plugin_execution("plugin", 200);
    }
    let stats = wd.get_plugin_stats("plugin").ok_or("missing")?;
    assert_eq!(stats.consecutive_timeouts, 4);

    // A successful execution resets consecutive counter
    wd.record_plugin_execution("plugin", 50);
    let stats = wd.get_plugin_stats("plugin").ok_or("missing")?;
    assert_eq!(stats.consecutive_timeouts, 0);
    assert_eq!(stats.timeout_count, 4); // total timeouts preserved
    Ok(())
}

#[test]
fn kick_alternating_fast_slow_never_quarantines() {
    let wd = system_with_timeout(100, 5);

    for _ in 0..50 {
        let fault = wd.record_plugin_execution("plugin", 200);
        assert!(fault.is_none());
        let fault = wd.record_plugin_execution("plugin", 50);
        assert!(fault.is_none());
    }

    assert!(!wd.is_plugin_quarantined("plugin"));
}

#[test]
fn kick_reset_plugin_stats_clears_counters() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 10);

    wd.record_plugin_execution("plugin", 50);
    wd.record_plugin_execution("plugin", 200);
    wd.reset_plugin_stats("plugin")?;

    let stats = wd.get_plugin_stats("plugin").ok_or("missing")?;
    assert_eq!(stats.total_executions, 0);
    assert_eq!(stats.timeout_count, 0);
    assert_eq!(stats.consecutive_timeouts, 0);
    Ok(())
}

#[test]
fn kick_reset_all_stats_clears_everything() {
    let wd = system_with_timeout(100, 10);

    wd.record_plugin_execution("a", 50);
    wd.record_plugin_execution("b", 50);
    assert_eq!(wd.plugin_count(), 2);

    wd.reset_all_plugin_stats();
    assert_eq!(wd.plugin_count(), 0);
}

#[test]
fn kick_reset_unknown_plugin_returns_error() {
    let wd = system_with_timeout(100, 10);
    let result = wd.reset_plugin_stats("nonexistent");
    assert!(result.is_err());
}

// ===========================================================================
// 4. Multiple watchdog instances
// ===========================================================================

#[test]
fn multiple_instances_are_independent() {
    let wd1 = system_with_timeout(100, 3);
    let wd2 = system_with_timeout(200, 5);

    // Quarantine plugin in wd1
    for _ in 0..3 {
        wd1.record_plugin_execution("shared_name", 150);
    }
    assert!(wd1.is_plugin_quarantined("shared_name"));

    // wd2 uses same plugin name but different threshold — not quarantined
    for _ in 0..3 {
        wd2.record_plugin_execution("shared_name", 150);
    }
    assert!(!wd2.is_plugin_quarantined("shared_name"));
}

#[test]
fn multiple_instances_different_configs() {
    let wd_fast = system_with_timeout(50, 2);
    let wd_slow = system_with_timeout(500, 10);

    assert_eq!(wd_fast.get_config().plugin_timeout_us, 50);
    assert_eq!(wd_slow.get_config().plugin_timeout_us, 500);

    // Fast watchdog quarantines at 2 overruns
    wd_fast.record_plugin_execution("p", 100);
    let fault = wd_fast.record_plugin_execution("p", 100);
    assert_eq!(fault, Some(FaultType::PluginOverrun));

    // Slow watchdog: 100us is within its budget
    for _ in 0..10 {
        let fault = wd_slow.record_plugin_execution("p", 100);
        assert!(fault.is_none());
    }
}

// ===========================================================================
// 5. Cascading timeouts
// ===========================================================================

#[test]
fn cascading_only_faulty_plugin_quarantined() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);

    wd.register_plugin("good_plugin");
    wd.register_plugin("bad_plugin");
    wd.register_plugin("neutral_plugin");

    for _ in 0..10 {
        let f = wd.record_plugin_execution("good_plugin", 50);
        assert!(f.is_none());
    }

    for i in 0..3 {
        let f = wd.record_plugin_execution("bad_plugin", 200);
        if i < 2 {
            assert!(f.is_none());
        } else {
            assert_eq!(f, Some(FaultType::PluginOverrun));
        }
    }

    assert!(wd.is_plugin_quarantined("bad_plugin"));
    assert!(!wd.is_plugin_quarantined("good_plugin"));
    assert!(!wd.is_plugin_quarantined("neutral_plugin"));
    Ok(())
}

#[test]
fn cascading_multiple_plugins_quarantined_independently() {
    let wd = system_with_timeout(100, 2);

    // Quarantine plugin_a
    wd.record_plugin_execution("plugin_a", 200);
    wd.record_plugin_execution("plugin_a", 200);
    assert!(wd.is_plugin_quarantined("plugin_a"));

    // plugin_b still running
    wd.record_plugin_execution("plugin_b", 200);
    assert!(!wd.is_plugin_quarantined("plugin_b"));

    // Now quarantine plugin_b
    wd.record_plugin_execution("plugin_b", 200);
    assert!(wd.is_plugin_quarantined("plugin_b"));

    let quarantined = wd.get_quarantined_plugins();
    assert_eq!(quarantined.len(), 2);
}

#[test]
fn cascading_quarantine_release_and_requarantine() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 2);

    wd.record_plugin_execution("cycle", 200);
    let fault = wd.record_plugin_execution("cycle", 200);
    assert_eq!(fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("cycle"));

    wd.release_plugin_quarantine("cycle")?;
    assert!(!wd.is_plugin_quarantined("cycle"));

    wd.record_plugin_execution("cycle", 200);
    let fault = wd.record_plugin_execution("cycle", 200);
    assert_eq!(fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("cycle"));
    Ok(())
}

#[test]
fn cascading_metrics_track_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 3);

    wd.record_plugin_execution("m_plugin", 50);
    wd.record_plugin_execution("m_plugin", 80);
    wd.record_plugin_execution("m_plugin", 200);
    wd.record_plugin_execution("m_plugin", 300);
    wd.record_plugin_execution("m_plugin", 60);

    let stats = wd.get_plugin_stats("m_plugin").ok_or("missing stats")?;
    assert_eq!(stats.total_executions, 5);
    assert_eq!(stats.timeout_count, 2);
    assert_eq!(stats.consecutive_timeouts, 0);
    assert_eq!(stats.last_execution_time_us, 60);

    let perf = wd.get_plugin_performance_metrics();
    let pm = perf.get("m_plugin").ok_or("missing perf metrics")?;
    assert!((pm["total_executions"] - 5.0).abs() < f64::EPSILON);
    assert!((pm["timeout_rate_percent"] - 40.0).abs() < f64::EPSILON);
    Ok(())
}

// ===========================================================================
// 6. Watchdog disable/enable (quarantine policy)
// ===========================================================================

#[test]
fn disable_quarantine_policy_prevents_quarantine() {
    let wd = system_with_timeout(100, 2);
    wd.set_quarantine_policy_enabled(false);
    assert!(!wd.is_quarantine_policy_enabled());

    for _ in 0..20 {
        let fault = wd.record_plugin_execution("plugin", 200);
        assert!(fault.is_none());
    }

    assert!(!wd.is_plugin_quarantined("plugin"));
}

#[test]
fn enable_quarantine_policy_after_disable_quarantines_new_offenders() {
    let wd = system_with_timeout(100, 2);

    wd.set_quarantine_policy_enabled(false);
    for _ in 0..10 {
        wd.record_plugin_execution("plugin_disabled", 200);
    }
    assert!(!wd.is_plugin_quarantined("plugin_disabled"));

    wd.set_quarantine_policy_enabled(true);
    assert!(wd.is_quarantine_policy_enabled());

    // New plugin should be quarantined normally
    wd.record_plugin_execution("plugin_enabled", 200);
    let fault = wd.record_plugin_execution("plugin_enabled", 200);
    assert_eq!(fault, Some(FaultType::PluginOverrun));
    assert!(wd.is_plugin_quarantined("plugin_enabled"));
}

#[test]
fn disable_does_not_release_existing_quarantines() -> Result<(), Box<dyn std::error::Error>> {
    let wd = system_with_timeout(100, 2);

    // Quarantine first
    wd.record_plugin_execution("plugin", 200);
    wd.record_plugin_execution("plugin", 200);
    assert!(wd.is_plugin_quarantined("plugin"));

    // Disable policy — existing quarantine remains
    wd.set_quarantine_policy_enabled(false);
    assert!(wd.is_plugin_quarantined("plugin"));
    Ok(())
}

#[test]
fn release_quarantine_of_nonexistent_plugin_returns_error() {
    let wd = system_with_timeout(100, 2);
    let result = wd.release_plugin_quarantine("nonexistent");
    assert!(result.is_err());
}

#[test]
fn release_quarantine_of_non_quarantined_plugin_returns_error() {
    let wd = system_with_timeout(100, 10);
    wd.record_plugin_execution("plugin", 50);
    let result = wd.release_plugin_quarantine("plugin");
    assert!(result.is_err());
}

// ===========================================================================
// 7. Safety interlock integration
// ===========================================================================

#[test]
fn safety_component_failure_escalation_to_faulted() {
    let wd = WatchdogSystem::default();

    // 5 consecutive failures → Faulted
    for _ in 0..5 {
        wd.report_component_failure(SystemComponent::RtThread, Some("test".to_string()));
    }

    let health = wd.get_component_health(SystemComponent::RtThread);
    assert!(health.is_some());
    if let Some(h) = health {
        assert_eq!(h.status, HealthStatus::Faulted);
        assert_eq!(h.consecutive_failures, 5);
    }
    assert!(wd.has_faulted_components());
}

#[test]
fn safety_component_failure_triggers_fault_callback() {
    let wd = WatchdogSystem::default();
    let triggered = Arc::new(AtomicU32::new(0));
    let t = Arc::clone(&triggered);
    wd.add_fault_callback(move |_ft, _comp| {
        t.fetch_add(1, Ordering::SeqCst);
    });

    // Reach Faulted state (5 failures)
    for _ in 0..5 {
        wd.report_component_failure(SystemComponent::SafetySystem, None);
    }

    assert!(triggered.load(Ordering::SeqCst) > 0);
}

#[test]
fn safety_heartbeat_recovers_from_degraded() {
    let wd = WatchdogSystem::default();

    // Cause 3 failures → Degraded
    for _ in 0..3 {
        wd.report_component_failure(SystemComponent::PluginHost, None);
    }
    let h = wd.get_component_health(SystemComponent::PluginHost);
    if let Some(h) = h {
        assert_eq!(h.status, HealthStatus::Degraded);
    }

    // Heartbeat recovers to Healthy
    wd.heartbeat(SystemComponent::PluginHost);
    let h = wd.get_component_health(SystemComponent::PluginHost);
    if let Some(h) = h {
        assert_eq!(h.status, HealthStatus::Healthy);
        assert_eq!(h.consecutive_failures, 0);
    }
}

#[test]
fn safety_all_components_initially_unknown() {
    let wd = WatchdogSystem::default();
    let summary = wd.get_health_summary();

    for component in SystemComponent::all() {
        assert_eq!(
            summary[&component],
            HealthStatus::Unknown,
            "{component:?} should be Unknown initially"
        );
    }
}

#[test]
fn safety_component_metric_tracking() {
    let wd = WatchdogSystem::default();

    wd.add_component_metric(SystemComponent::RtThread, "jitter_us".to_string(), 42.5);
    wd.add_component_metric(SystemComponent::RtThread, "load_pct".to_string(), 85.0);

    let h = wd.get_component_health(SystemComponent::RtThread);
    if let Some(h) = h {
        assert_eq!(h.metrics.len(), 2);
        assert!((h.metrics["jitter_us"] - 42.5).abs() < f64::EPSILON);
        assert!((h.metrics["load_pct"] - 85.0).abs() < f64::EPSILON);
    }
}

#[test]
fn safety_get_all_component_health_returns_all() {
    let wd = WatchdogSystem::default();
    let all = wd.get_all_component_health();
    assert_eq!(all.len(), 6); // All SystemComponent variants
}

#[test]
fn safety_component_uptime_none_before_heartbeat() {
    let wd = WatchdogSystem::default();
    let uptime = wd.get_component_uptime(SystemComponent::RtThread);
    assert!(uptime.is_none());
}

#[test]
fn safety_component_uptime_some_after_heartbeat() {
    let wd = WatchdogSystem::default();
    wd.heartbeat(SystemComponent::RtThread);
    let uptime = wd.get_component_uptime(SystemComponent::RtThread);
    assert!(uptime.is_some());
}

#[test]
fn safety_health_status_display() {
    assert_eq!(HealthStatus::Healthy.to_string(), "Healthy");
    assert_eq!(HealthStatus::Degraded.to_string(), "Degraded");
    assert_eq!(HealthStatus::Faulted.to_string(), "Faulted");
    assert_eq!(HealthStatus::Unknown.to_string(), "Unknown");
}

#[test]
fn safety_fault_type_mapping_by_component() {
    let wd = WatchdogSystem::default();
    let triggered_faults = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let tf = Arc::clone(&triggered_faults);
    wd.add_fault_callback(move |ft, comp| {
        tf.lock().push((ft, comp.to_string()));
    });

    // RtThread → TimingViolation (need 5 failures to reach Faulted)
    for _ in 0..5 {
        wd.report_component_failure(SystemComponent::RtThread, None);
    }

    // HidCommunication → UsbStall
    for _ in 0..5 {
        wd.report_component_failure(SystemComponent::HidCommunication, None);
    }

    let faults = triggered_faults.lock();
    let has_timing = faults
        .iter()
        .any(|(ft, _)| *ft == FaultType::TimingViolation);
    let has_usb = faults.iter().any(|(ft, _)| *ft == FaultType::UsbStall);
    assert!(has_timing, "Expected TimingViolation from RtThread");
    assert!(has_usb, "Expected UsbStall from HidCommunication");
}

// ===========================================================================
// 8. Thread safety of watchdog operations
// ===========================================================================

#[test]
fn thread_safety_concurrent_plugin_executions() {
    let wd = Arc::new(system_with_timeout(100, 1000));
    let num_threads = 8;
    let iters_per_thread = 100;

    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let wd = Arc::clone(&wd);
            std::thread::spawn(move || {
                let plugin_id = format!("thread_plugin_{i}");
                for _ in 0..iters_per_thread {
                    wd.record_plugin_execution(&plugin_id, 50);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().ok();
    }

    // Each thread should have recorded iters_per_thread executions
    for i in 0..num_threads {
        let plugin_id = format!("thread_plugin_{i}");
        let stats = wd.get_plugin_stats(&plugin_id);
        assert!(stats.is_some());
        if let Some(s) = stats {
            assert_eq!(s.total_executions, iters_per_thread);
        }
    }
}

#[test]
fn thread_safety_concurrent_heartbeats() {
    let wd = Arc::new(WatchdogSystem::default());
    let components = [
        SystemComponent::RtThread,
        SystemComponent::HidCommunication,
        SystemComponent::TelemetryAdapter,
        SystemComponent::PluginHost,
    ];

    let handles: Vec<_> = components
        .iter()
        .map(|&comp| {
            let wd = Arc::clone(&wd);
            std::thread::spawn(move || {
                for _ in 0..100 {
                    wd.heartbeat(comp);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().ok();
    }

    // All heartbeat-ed components should be healthy
    for comp in &components {
        let health = wd.get_component_health(*comp);
        if let Some(h) = health {
            assert_eq!(h.status, HealthStatus::Healthy);
        }
    }
}

#[test]
fn thread_safety_concurrent_quarantine_and_release() {
    let wd = Arc::new(system_with_timeout(100, 2));

    // Writer thread: quarantine plugins
    let wd_writer = Arc::clone(&wd);
    let writer = std::thread::spawn(move || {
        for i in 0..20 {
            let id = format!("q_plugin_{i}");
            wd_writer.record_plugin_execution(&id, 200);
            wd_writer.record_plugin_execution(&id, 200);
        }
    });

    // Reader thread: check quarantine status
    let wd_reader = Arc::clone(&wd);
    let reader = std::thread::spawn(move || {
        for i in 0..20 {
            let id = format!("q_plugin_{i}");
            let _ = wd_reader.is_plugin_quarantined(&id);
            let _ = wd_reader.get_quarantined_plugins();
        }
    });

    writer.join().ok();
    reader.join().ok();
}

#[test]
fn thread_safety_concurrent_register_unregister() {
    let wd = Arc::new(system_with_timeout(100, 10));

    let wd1 = Arc::clone(&wd);
    let t1 = std::thread::spawn(move || {
        for i in 0..50 {
            wd1.register_plugin(&format!("reg_{i}"));
        }
    });

    let wd2 = Arc::clone(&wd);
    let t2 = std::thread::spawn(move || {
        for i in 50..100 {
            wd2.register_plugin(&format!("reg_{i}"));
        }
    });

    t1.join().ok();
    t2.join().ok();

    assert_eq!(wd.plugin_count(), 100);
}

#[test]
fn thread_safety_shared_plugin_name_concurrent_execution() {
    let wd = Arc::new(system_with_timeout(100, 10000));
    let num_threads: u64 = 4;
    let iters: u64 = 250;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let wd = Arc::clone(&wd);
            std::thread::spawn(move || {
                for _ in 0..iters {
                    wd.record_plugin_execution("shared", 50);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().ok();
    }

    let stats = wd.get_plugin_stats("shared");
    if let Some(s) = stats {
        assert_eq!(s.total_executions, num_threads * iters);
    }
}

// ===========================================================================
// 9. Plugin registration
// ===========================================================================

#[test]
fn registration_register_then_unregister() -> Result<(), Box<dyn std::error::Error>> {
    let wd = WatchdogSystem::default();

    wd.register_plugin("p1");
    wd.register_plugin("p2");
    assert_eq!(wd.plugin_count(), 2);

    wd.unregister_plugin("p1")?;
    assert_eq!(wd.plugin_count(), 1);
    assert!(wd.get_plugin_stats("p1").is_none());
    assert!(wd.get_plugin_stats("p2").is_some());
    Ok(())
}

#[test]
fn registration_unregister_unknown_returns_error() {
    let wd = WatchdogSystem::default();
    let result = wd.unregister_plugin("nonexistent");
    assert!(result.is_err());
}

#[test]
fn registration_duplicate_is_idempotent() {
    let wd = WatchdogSystem::default();
    wd.register_plugin("p1");
    wd.register_plugin("p1");
    assert_eq!(wd.plugin_count(), 1);
}

#[test]
fn registration_get_all_stats() {
    let wd = WatchdogSystem::default();
    wd.record_plugin_execution("a", 10);
    wd.record_plugin_execution("b", 20);

    let all = wd.get_all_plugin_stats();
    assert_eq!(all.len(), 2);
    assert!(all.contains_key("a"));
    assert!(all.contains_key("b"));
}

// ===========================================================================
// 10. Error types
// ===========================================================================

#[test]
fn error_display_contains_plugin_name() {
    let err = WatchdogError::plugin_not_found("my_plugin");
    assert!(err.to_string().contains("my_plugin"));

    let err = WatchdogError::not_quarantined("my_plugin");
    assert!(err.to_string().contains("my_plugin"));

    let err = WatchdogError::already_quarantined("my_plugin");
    assert!(err.to_string().contains("my_plugin"));
}

#[test]
fn error_display_contains_reason() {
    let err = WatchdogError::invalid_configuration("bad value");
    assert!(err.to_string().contains("bad value"));

    let err = WatchdogError::health_check_failed(SystemComponent::RtThread, "timeout");
    let msg = err.to_string();
    assert!(msg.contains("RT Thread"));
    assert!(msg.contains("timeout"));
}

#[test]
fn error_timeout_exceeded_format() {
    let err = WatchdogError::timeout_exceeded("test_context", Duration::from_millis(42));
    let msg = err.to_string();
    assert!(msg.contains("test_context"));
    assert!(msg.contains("42"));
}

// ===========================================================================
// 11. Debug and Display
// ===========================================================================

#[test]
fn watchdog_system_debug_format() {
    let wd = WatchdogSystem::default();
    let debug = format!("{wd:?}");
    assert!(debug.contains("WatchdogSystem"));
    assert!(debug.contains("plugin_count"));
}

#[test]
fn quarantine_reason_display() {
    assert_eq!(
        QuarantineReason::ConsecutiveTimeouts.to_string(),
        "Consecutive timeouts"
    );
    assert_eq!(QuarantineReason::Crash.to_string(), "Crash");
    assert_eq!(QuarantineReason::Manual.to_string(), "Manual");
    assert_eq!(QuarantineReason::Unknown.to_string(), "Unknown");
    assert_eq!(
        QuarantineReason::TimingViolation.to_string(),
        "Timing violation"
    );
}

// ===========================================================================
// 12. Property-based tests
// ===========================================================================

proptest! {
    #[test]
    fn prop_within_timeout_never_triggers(
        executions in prop::collection::vec(1_u64..=100, 1..200),
    ) {
        let wd = system_with_timeout(100, 5);
        for exec_time in &executions {
            let fault = wd.record_plugin_execution("prop_plugin", *exec_time);
            prop_assert!(fault.is_none(), "Should never quarantine within budget");
        }
        prop_assert!(!wd.is_plugin_quarantined("prop_plugin"));
    }

    #[test]
    fn prop_exceeding_timeout_always_triggers(
        max_timeouts in 1_u32..=10,
        excess in 101_u64..=500,
    ) {
        let wd = system_with_timeout(100, max_timeouts);
        let mut triggered = false;
        for _ in 0..max_timeouts {
            if let Some(FaultType::PluginOverrun) =
                wd.record_plugin_execution("prop_bad", excess)
            {
                triggered = true;
            }
        }
        prop_assert!(triggered, "Must quarantine after {} overruns", max_timeouts);
        prop_assert!(wd.is_plugin_quarantined("prop_bad"));
    }

    #[test]
    fn prop_consecutive_timeouts_accumulate(
        count in 1_u32..=20,
    ) {
        let wd = system_with_timeout(100, 100); // high threshold to avoid quarantine
        for _ in 0..count {
            wd.record_plugin_execution("acc_plugin", 200);
        }
        let stats = wd.get_plugin_stats("acc_plugin");
        prop_assert!(stats.is_some());
        if let Some(s) = stats {
            prop_assert_eq!(s.consecutive_timeouts, count);
            prop_assert_eq!(s.timeout_count, count);
            prop_assert_eq!(s.total_executions, u64::from(count));
        }
    }
}
