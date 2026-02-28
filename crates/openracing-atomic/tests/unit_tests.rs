//! Unit tests for openracing-atomic.
//!
//! These tests cover basic functionality of all types in the crate.

use openracing_atomic::{
    AppMetricsSnapshot, AppThresholds, AtomicCounters, CounterSnapshot, JitterStats, LatencyStats,
    RTMetricsSnapshot, RTThresholds, StreamingStats,
};

#[test]
fn test_atomic_counters_default() {
    let counters = AtomicCounters::default();
    let snapshot = counters.snapshot();
    assert_eq!(snapshot.total_ticks, 0);
}

#[test]
fn test_atomic_counters_all_operations() {
    let counters = AtomicCounters::new();

    counters.inc_tick();
    counters.inc_tick();
    counters.inc_tick_by(7);
    counters.inc_missed_tick();
    counters.inc_missed_tick_by(3);
    counters.inc_safety_event();
    counters.inc_profile_switch();
    counters.inc_telemetry_received();
    counters.inc_telemetry_lost();
    counters.record_torque_saturation(true);
    counters.record_torque_saturation(false);
    counters.record_torque_saturation(true);
    counters.inc_hid_write_error();

    let snapshot = counters.snapshot();
    assert_eq!(snapshot.total_ticks, 9);
    assert_eq!(snapshot.missed_ticks, 4);
    assert_eq!(snapshot.safety_events, 1);
    assert_eq!(snapshot.profile_switches, 1);
    assert_eq!(snapshot.telemetry_packets_received, 1);
    assert_eq!(snapshot.telemetry_packets_lost, 1);
    assert_eq!(snapshot.torque_saturation_samples, 3);
    assert_eq!(snapshot.torque_saturation_count, 2);
    assert_eq!(snapshot.hid_write_errors, 1);
}

#[test]
fn test_counter_snapshot_default() {
    let snapshot = CounterSnapshot::default();
    assert_eq!(snapshot.total_ticks, 0);
    assert_eq!(snapshot.missed_ticks, 0);
}

#[test]
fn test_atomic_counters_reset() {
    let counters = AtomicCounters::new();
    counters.inc_tick();
    counters.inc_missed_tick();

    counters.reset();

    let snapshot = counters.snapshot();
    assert_eq!(snapshot.total_ticks, 0);
    assert_eq!(snapshot.missed_ticks, 0);
}

#[test]
fn test_atomic_counters_snapshot_and_reset() {
    let counters = AtomicCounters::new();
    counters.inc_tick();
    counters.inc_tick();
    counters.inc_missed_tick();

    let first = counters.snapshot_and_reset();
    assert_eq!(first.total_ticks, 2);
    assert_eq!(first.missed_ticks, 1);

    let second = counters.snapshot();
    assert_eq!(second.total_ticks, 0);
    assert_eq!(second.missed_ticks, 0);
}

#[test]
fn test_torque_saturation_percent_empty() {
    let counters = AtomicCounters::new();
    assert_eq!(counters.torque_saturation_percent(), 0.0);
}

#[test]
fn test_torque_saturation_percent_calculated() {
    let counters = AtomicCounters::new();

    for _ in 0..75 {
        counters.record_torque_saturation(false);
    }
    for _ in 0..25 {
        counters.record_torque_saturation(true);
    }

    let pct = counters.torque_saturation_percent();
    assert!((pct - 25.0).abs() < f32::EPSILON);
}

#[test]
fn test_telemetry_loss_percent_empty() {
    let counters = AtomicCounters::new();
    assert_eq!(counters.telemetry_loss_percent(), 0.0);
}

#[test]
fn test_telemetry_loss_percent_calculated() {
    let counters = AtomicCounters::new();

    for _ in 0..98 {
        counters.inc_telemetry_received();
    }
    for _ in 0..2 {
        counters.inc_telemetry_lost();
    }

    let pct = counters.telemetry_loss_percent();
    assert!((pct - 2.0).abs() < f32::EPSILON);
}

#[test]
fn test_jitter_stats_new() {
    let stats = JitterStats::new();
    assert_eq!(stats.p50_ns, 0);
    assert_eq!(stats.p99_ns, 0);
    assert_eq!(stats.max_ns, 0);
}

#[test]
fn test_jitter_stats_from_values() {
    let stats = JitterStats::from_values(100, 200, 500);
    assert_eq!(stats.p50_ns, 100);
    assert_eq!(stats.p99_ns, 200);
    assert_eq!(stats.max_ns, 500);
}

#[test]
fn test_jitter_stats_exceeds_threshold() {
    let stats = JitterStats::from_values(100, 300, 500);

    assert!(stats.exceeds_threshold(250));
    assert!(!stats.exceeds_threshold(300));
    assert!(!stats.exceeds_threshold(500));
}

#[test]
fn test_jitter_stats_to_micros() {
    let stats = JitterStats::from_values(1_000_000, 2_000_000, 5_000_000);
    let micros = stats.to_micros();

    assert_eq!(micros.p50_ns, 1_000);
    assert_eq!(micros.p99_ns, 2_000);
    assert_eq!(micros.max_ns, 5_000);
}

#[test]
fn test_latency_stats_new() {
    let stats = LatencyStats::new();
    assert_eq!(stats.p50_us, 0);
    assert_eq!(stats.p99_us, 0);
    assert_eq!(stats.max_us, 0);
}

#[test]
fn test_latency_stats_from_values() {
    let stats = LatencyStats::from_values(50, 150, 300);
    assert_eq!(stats.p50_us, 50);
    assert_eq!(stats.p99_us, 150);
    assert_eq!(stats.max_us, 300);
}

#[test]
fn test_latency_stats_from_nanos() {
    let stats = LatencyStats::from_nanos(50_000, 150_000, 300_000);
    assert_eq!(stats.p50_us, 50);
    assert_eq!(stats.p99_us, 150);
    assert_eq!(stats.max_us, 300);
}

#[test]
fn test_latency_stats_exceeds_threshold() {
    let stats = LatencyStats::from_values(50, 150, 300);

    assert!(stats.exceeds_threshold(100));
    assert!(!stats.exceeds_threshold(150));
    assert!(!stats.exceeds_threshold(200));
}

#[test]
fn test_rt_metrics_snapshot_default() {
    let metrics = RTMetricsSnapshot::default();
    assert_eq!(metrics.total_ticks, 0);
    assert_eq!(metrics.missed_ticks, 0);
    assert_eq!(metrics.cpu_usage_percent, 0.0);
}

#[test]
fn test_rt_metrics_missed_tick_rate() {
    let mut metrics = RTMetricsSnapshot::new();
    assert_eq!(metrics.missed_tick_rate(), 0.0);

    metrics.total_ticks = 10000;
    metrics.missed_ticks = 1;

    let rate = metrics.missed_tick_rate();
    assert!((rate - 0.01).abs() < f64::EPSILON);
}

#[test]
fn test_rt_metrics_has_violations() {
    let thresholds = RTThresholds::default();
    let mut metrics = RTMetricsSnapshot::new();

    assert!(!metrics.has_violations(&thresholds));

    metrics.jitter.p99_ns = thresholds.max_jitter_ns + 1;
    assert!(metrics.has_violations(&thresholds));
}

#[test]
fn test_rt_thresholds_default() {
    let thresholds = RTThresholds::default();

    assert_eq!(thresholds.max_jitter_ns, 250_000);
    assert_eq!(thresholds.max_processing_time_us, 200);
    assert_eq!(thresholds.max_hid_latency_us, 300);
}

#[test]
fn test_app_metrics_snapshot_default() {
    let metrics = AppMetricsSnapshot::default();
    assert_eq!(metrics.connected_devices, 0);
    assert_eq!(metrics.torque_saturation_percent, 0.0);
}

#[test]
fn test_app_metrics_has_violations() {
    let thresholds = AppThresholds::default();
    let mut metrics = AppMetricsSnapshot::new();

    assert!(!metrics.has_violations(&thresholds));

    metrics.torque_saturation_percent = thresholds.max_torque_saturation_percent + 1.0;
    assert!(metrics.has_violations(&thresholds));
}

#[test]
fn test_app_thresholds_default() {
    let thresholds = AppThresholds::default();

    assert_eq!(thresholds.max_torque_saturation_percent, 95.0);
    assert_eq!(thresholds.max_telemetry_loss_percent, 5.0);
}

#[test]
fn test_streaming_stats_empty() {
    let stats = StreamingStats::new();
    assert!(stats.is_empty());
    assert_eq!(stats.count(), 0);
    assert_eq!(stats.mean(), 0.0);
}

#[test]
fn test_streaming_stats_record() {
    let mut stats = StreamingStats::new();

    stats.record(10);
    stats.record(20);
    stats.record(30);
    stats.record(40);
    stats.record(50);

    assert!(!stats.is_empty());
    assert_eq!(stats.count(), 5);
    assert_eq!(stats.min(), 10);
    assert_eq!(stats.max(), 50);
    assert!((stats.mean() - 30.0).abs() < f64::EPSILON);
}

#[test]
fn test_streaming_stats_reset() {
    let mut stats = StreamingStats::new();

    stats.record(100);
    stats.record(200);
    assert!(!stats.is_empty());

    stats.reset();
    assert!(stats.is_empty());
}

#[test]
fn test_streaming_stats_saturating_add() {
    let mut stats = StreamingStats::new();

    stats.record(u64::MAX);
    stats.record(1);

    assert_eq!(stats.count(), 2);
}

#[test]
fn test_atomic_counters_with_values() {
    let initial = CounterSnapshot {
        total_ticks: 1000,
        missed_ticks: 5,
        safety_events: 10,
        profile_switches: 2,
        telemetry_packets_received: 500,
        telemetry_packets_lost: 5,
        torque_saturation_samples: 100,
        torque_saturation_count: 10,
        hid_write_errors: 0,
    };

    let counters = AtomicCounters::with_values(initial);
    let snapshot = counters.snapshot();

    assert_eq!(snapshot.total_ticks, 1000);
    assert_eq!(snapshot.missed_ticks, 5);
    assert_eq!(snapshot.safety_events, 10);
}

#[test]
fn test_counter_snapshot_methods() {
    let snapshot = CounterSnapshot {
        total_ticks: 100,
        missed_ticks: 5,
        safety_events: 0,
        profile_switches: 0,
        telemetry_packets_received: 90,
        telemetry_packets_lost: 10,
        torque_saturation_samples: 100,
        torque_saturation_count: 25,
        hid_write_errors: 0,
    };

    let torque_pct = snapshot.torque_saturation_percent();
    assert!((torque_pct - 25.0).abs() < f32::EPSILON);

    let loss_pct = snapshot.telemetry_loss_percent();
    assert!((loss_pct - 10.0).abs() < f32::EPSILON);
}
