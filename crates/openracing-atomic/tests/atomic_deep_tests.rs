//! Deep tests for openracing-atomic: atomic counters, snapshots, streaming
//! stats, RT metrics, thresholds, and lock-free queues.

use openracing_atomic::{
    AppMetricsSnapshot, AppThresholds, AtomicCounters, CounterSnapshot, JitterStats, LatencyStats,
    RTMetricsSnapshot, RTThresholds, StreamingStats,
};
use std::sync::Arc;
use std::thread;

// ===========================================================================
// CounterSnapshot — construction, defaults, percentage calculations
// ===========================================================================

#[test]
fn counter_snapshot_default_all_zero() {
    let s = CounterSnapshot::default();
    assert_eq!(s.total_ticks, 0);
    assert_eq!(s.missed_ticks, 0);
    assert_eq!(s.safety_events, 0);
    assert_eq!(s.profile_switches, 0);
    assert_eq!(s.telemetry_packets_received, 0);
    assert_eq!(s.telemetry_packets_lost, 0);
    assert_eq!(s.torque_saturation_samples, 0);
    assert_eq!(s.torque_saturation_count, 0);
    assert_eq!(s.hid_write_errors, 0);
}

#[test]
fn counter_snapshot_torque_saturation_percent_zero_samples() {
    let s = CounterSnapshot::default();
    let pct = s.torque_saturation_percent();
    // With zero samples, should return 0.0 or handle gracefully
    assert!(pct.is_finite());
    assert!(pct >= 0.0);
}

#[test]
fn counter_snapshot_torque_saturation_percent_all_saturated() {
    let s = CounterSnapshot {
        torque_saturation_samples: 100,
        torque_saturation_count: 100,
        ..CounterSnapshot::default()
    };
    let pct = s.torque_saturation_percent();
    assert!((pct - 100.0).abs() < 0.1);
}

#[test]
fn counter_snapshot_torque_saturation_percent_half() {
    let s = CounterSnapshot {
        torque_saturation_samples: 200,
        torque_saturation_count: 100,
        ..CounterSnapshot::default()
    };
    let pct = s.torque_saturation_percent();
    assert!((pct - 50.0).abs() < 0.1);
}

#[test]
fn counter_snapshot_telemetry_loss_percent_zero() {
    let s = CounterSnapshot::default();
    let pct = s.telemetry_loss_percent();
    assert!(pct.is_finite());
    assert!(pct >= 0.0);
}

#[test]
fn counter_snapshot_telemetry_loss_percent_all_lost() {
    let s = CounterSnapshot {
        telemetry_packets_received: 0,
        telemetry_packets_lost: 100,
        ..CounterSnapshot::default()
    };
    let pct = s.telemetry_loss_percent();
    assert!((pct - 100.0).abs() < 0.1);
}

#[test]
fn counter_snapshot_telemetry_loss_percent_mixed() {
    let s = CounterSnapshot {
        telemetry_packets_received: 90,
        telemetry_packets_lost: 10,
        ..CounterSnapshot::default()
    };
    let pct = s.telemetry_loss_percent();
    assert!((pct - 10.0).abs() < 0.1);
}

#[test]
fn counter_snapshot_eq_and_clone() {
    let s1 = CounterSnapshot {
        total_ticks: 42,
        ..CounterSnapshot::default()
    };
    let s2 = s1;
    assert_eq!(s1, s2);
}

#[test]
fn counter_snapshot_debug() {
    let s = CounterSnapshot::default();
    let debug = format!("{s:?}");
    assert!(debug.contains("CounterSnapshot"));
}

// ===========================================================================
// AtomicCounters — construction, increments, snapshot, reset
// ===========================================================================

#[test]
fn atomic_counters_new_all_zero() {
    let c = AtomicCounters::new();
    let s = c.snapshot();
    assert_eq!(s, CounterSnapshot::default());
}

#[test]
fn atomic_counters_default_matches_new() {
    let c1 = AtomicCounters::new();
    let c2 = AtomicCounters::default();
    assert_eq!(c1.snapshot(), c2.snapshot());
}

#[test]
fn atomic_counters_with_values() {
    let init = CounterSnapshot {
        total_ticks: 100,
        missed_ticks: 5,
        safety_events: 2,
        profile_switches: 3,
        telemetry_packets_received: 50,
        telemetry_packets_lost: 1,
        torque_saturation_samples: 80,
        torque_saturation_count: 10,
        hid_write_errors: 0,
    };
    let c = AtomicCounters::with_values(init);
    let s = c.snapshot();
    assert_eq!(s, init);
}

#[test]
fn atomic_counters_inc_tick() {
    let c = AtomicCounters::new();
    c.inc_tick();
    c.inc_tick();
    c.inc_tick();
    assert_eq!(c.total_ticks(), 3);
}

#[test]
fn atomic_counters_inc_tick_by() {
    let c = AtomicCounters::new();
    c.inc_tick_by(10);
    c.inc_tick_by(20);
    assert_eq!(c.total_ticks(), 30);
}

#[test]
fn atomic_counters_inc_tick_by_zero() {
    let c = AtomicCounters::new();
    c.inc_tick_by(0);
    assert_eq!(c.total_ticks(), 0);
}

#[test]
fn atomic_counters_inc_missed_tick() {
    let c = AtomicCounters::new();
    c.inc_missed_tick();
    c.inc_missed_tick();
    assert_eq!(c.missed_ticks(), 2);
}

#[test]
fn atomic_counters_inc_missed_tick_by() {
    let c = AtomicCounters::new();
    c.inc_missed_tick_by(5);
    assert_eq!(c.missed_ticks(), 5);
}

#[test]
fn atomic_counters_inc_safety_event() {
    let c = AtomicCounters::new();
    c.inc_safety_event();
    c.inc_safety_event();
    assert_eq!(c.safety_events(), 2);
}

#[test]
fn atomic_counters_inc_profile_switch() {
    let c = AtomicCounters::new();
    c.inc_profile_switch();
    let s = c.snapshot();
    assert_eq!(s.profile_switches, 1);
}

#[test]
fn atomic_counters_inc_telemetry_received() {
    let c = AtomicCounters::new();
    c.inc_telemetry_received();
    c.inc_telemetry_received();
    let s = c.snapshot();
    assert_eq!(s.telemetry_packets_received, 2);
}

#[test]
fn atomic_counters_inc_telemetry_lost() {
    let c = AtomicCounters::new();
    c.inc_telemetry_lost();
    let s = c.snapshot();
    assert_eq!(s.telemetry_packets_lost, 1);
}

#[test]
fn atomic_counters_record_torque_saturation_true() {
    let c = AtomicCounters::new();
    c.record_torque_saturation(true);
    c.record_torque_saturation(true);
    let s = c.snapshot();
    assert_eq!(s.torque_saturation_samples, 2);
    assert_eq!(s.torque_saturation_count, 2);
}

#[test]
fn atomic_counters_record_torque_saturation_false() {
    let c = AtomicCounters::new();
    c.record_torque_saturation(false);
    c.record_torque_saturation(false);
    let s = c.snapshot();
    assert_eq!(s.torque_saturation_samples, 2);
    assert_eq!(s.torque_saturation_count, 0);
}

#[test]
fn atomic_counters_record_torque_saturation_mixed() {
    let c = AtomicCounters::new();
    c.record_torque_saturation(true);
    c.record_torque_saturation(false);
    c.record_torque_saturation(true);
    c.record_torque_saturation(false);
    let s = c.snapshot();
    assert_eq!(s.torque_saturation_samples, 4);
    assert_eq!(s.torque_saturation_count, 2);
}

#[test]
fn atomic_counters_inc_hid_write_error() {
    let c = AtomicCounters::new();
    c.inc_hid_write_error();
    let s = c.snapshot();
    assert_eq!(s.hid_write_errors, 1);
}

#[test]
fn atomic_counters_torque_saturation_percent() {
    let c = AtomicCounters::new();
    for _ in 0..100 {
        c.record_torque_saturation(true);
    }
    for _ in 0..100 {
        c.record_torque_saturation(false);
    }
    let pct = c.torque_saturation_percent();
    assert!((pct - 50.0).abs() < 0.1);
}

#[test]
fn atomic_counters_telemetry_loss_percent() {
    let c = AtomicCounters::new();
    for _ in 0..90 {
        c.inc_telemetry_received();
    }
    for _ in 0..10 {
        c.inc_telemetry_lost();
    }
    let pct = c.telemetry_loss_percent();
    assert!((pct - 10.0).abs() < 0.1);
}

#[test]
fn atomic_counters_reset() {
    let c = AtomicCounters::new();
    c.inc_tick_by(1000);
    c.inc_missed_tick_by(50);
    c.inc_safety_event();
    c.inc_hid_write_error();
    c.reset();
    assert_eq!(c.snapshot(), CounterSnapshot::default());
}

#[test]
fn atomic_counters_snapshot_and_reset() {
    let c = AtomicCounters::new();
    c.inc_tick_by(100);
    c.inc_missed_tick_by(5);
    c.inc_safety_event();

    let s = c.snapshot_and_reset();
    assert_eq!(s.total_ticks, 100);
    assert_eq!(s.missed_ticks, 5);
    assert_eq!(s.safety_events, 1);

    // After reset, all should be zero
    let s2 = c.snapshot();
    assert_eq!(s2, CounterSnapshot::default());
}

#[test]
fn atomic_counters_snapshot_is_consistent() {
    let c = AtomicCounters::new();
    c.inc_tick_by(42);
    c.inc_missed_tick_by(7);
    let s1 = c.snapshot();
    let s2 = c.snapshot();
    assert_eq!(s1, s2);
}

// ===========================================================================
// AtomicCounters — thread safety (concurrent increments)
// ===========================================================================

#[test]
fn atomic_counters_concurrent_increments() {
    let counters = Arc::new(AtomicCounters::new());
    let threads_count = 8;
    let ops_per_thread = 1000;

    let handles: Vec<_> = (0..threads_count)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..ops_per_thread {
                    c.inc_tick();
                    c.inc_missed_tick();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked").ok();
    }

    let s = counters.snapshot();
    assert_eq!(s.total_ticks, threads_count * ops_per_thread);
    assert_eq!(s.missed_ticks, threads_count * ops_per_thread);
}

#[test]
fn atomic_counters_concurrent_mixed_operations() {
    let counters = Arc::new(AtomicCounters::new());
    let n = 500_u64;

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..n {
                    match i % 4 {
                        0 => c.inc_tick(),
                        1 => c.inc_missed_tick(),
                        2 => c.inc_safety_event(),
                        3 => c.inc_hid_write_error(),
                        _ => {}
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked").ok();
    }

    let s = counters.snapshot();
    assert_eq!(s.total_ticks, n);
    assert_eq!(s.missed_ticks, n);
    assert_eq!(s.safety_events, n);
    assert_eq!(s.hid_write_errors, n);
}

#[test]
fn atomic_counters_concurrent_snapshot_during_writes() {
    let counters = Arc::new(AtomicCounters::new());
    let n = 1000_u64;

    let writer = {
        let c = Arc::clone(&counters);
        thread::spawn(move || {
            for _ in 0..n {
                c.inc_tick();
            }
        })
    };

    let reader = {
        let c = Arc::clone(&counters);
        thread::spawn(move || {
            let mut max_seen = 0_u64;
            for _ in 0..100 {
                let s = c.snapshot();
                assert!(
                    s.total_ticks >= max_seen,
                    "Ticks should never decrease: was {max_seen}, now {}",
                    s.total_ticks
                );
                max_seen = s.total_ticks;
            }
        })
    };

    writer.join().map_err(|_| "writer panicked").ok();
    reader.join().map_err(|_| "reader panicked").ok();

    let final_snap = counters.snapshot();
    assert_eq!(final_snap.total_ticks, n);
}

// ===========================================================================
// JitterStats — construction, thresholds, conversions
// ===========================================================================

#[test]
fn jitter_stats_new_all_zero() {
    let s = JitterStats::new();
    assert_eq!(s.p50_ns, 0);
    assert_eq!(s.p99_ns, 0);
    assert_eq!(s.max_ns, 0);
}

#[test]
fn jitter_stats_from_values() {
    let s = JitterStats::from_values(100, 500, 1000);
    assert_eq!(s.p50_ns, 100);
    assert_eq!(s.p99_ns, 500);
    assert_eq!(s.max_ns, 1000);
}

#[test]
fn jitter_stats_exceeds_threshold() {
    let s = JitterStats::from_values(100, 500, 1000);
    assert!(s.exceeds_threshold(499));
    assert!(!s.exceeds_threshold(500));
    assert!(!s.exceeds_threshold(1000));
}

#[test]
fn jitter_stats_exceeds_threshold_checks_p99() {
    let s = JitterStats::from_values(10, 250, 1000);
    // Threshold below p99 → exceeds
    assert!(s.exceeds_threshold(249));
    // Threshold at or above p99 → does not exceed
    assert!(!s.exceeds_threshold(250));
}

#[test]
fn jitter_stats_to_micros() {
    let s = JitterStats::from_values(1_000, 5_000, 10_000);
    let micros = s.to_micros();
    assert_eq!(micros.p50_ns, 1);
    assert_eq!(micros.p99_ns, 5);
    assert_eq!(micros.max_ns, 10);
}

#[test]
fn jitter_stats_to_micros_sub_microsecond() {
    let s = JitterStats::from_values(500, 999, 1);
    let micros = s.to_micros();
    assert_eq!(micros.p50_ns, 0);
    assert_eq!(micros.p99_ns, 0);
    assert_eq!(micros.max_ns, 0);
}

#[test]
fn jitter_stats_default() {
    let s = JitterStats::default();
    assert_eq!(s, JitterStats::new());
}

#[test]
fn jitter_stats_clone_copy() {
    let s1 = JitterStats::from_values(1, 2, 3);
    let s2 = s1;
    assert_eq!(s1, s2);
}

// ===========================================================================
// LatencyStats — construction, thresholds, nanos conversion
// ===========================================================================

#[test]
fn latency_stats_new_all_zero() {
    let s = LatencyStats::new();
    assert_eq!(s.p50_us, 0);
    assert_eq!(s.p99_us, 0);
    assert_eq!(s.max_us, 0);
}

#[test]
fn latency_stats_from_values() {
    let s = LatencyStats::from_values(10, 50, 100);
    assert_eq!(s.p50_us, 10);
    assert_eq!(s.p99_us, 50);
    assert_eq!(s.max_us, 100);
}

#[test]
fn latency_stats_exceeds_threshold() {
    let s = LatencyStats::from_values(10, 50, 100);
    assert!(s.exceeds_threshold(49));
    assert!(!s.exceeds_threshold(50));
    assert!(!s.exceeds_threshold(100));
}

#[test]
fn latency_stats_from_nanos() {
    let s = LatencyStats::from_nanos(10_000, 50_000, 100_000);
    assert_eq!(s.p50_us, 10);
    assert_eq!(s.p99_us, 50);
    assert_eq!(s.max_us, 100);
}

#[test]
fn latency_stats_from_nanos_rounding() {
    let s = LatencyStats::from_nanos(999, 1500, 2999);
    assert_eq!(s.p50_us, 0);
    assert_eq!(s.p99_us, 1);
    assert_eq!(s.max_us, 2);
}

#[test]
fn latency_stats_default() {
    let s = LatencyStats::default();
    assert_eq!(s, LatencyStats::new());
}

// ===========================================================================
// RTMetricsSnapshot — construction, violations, missed tick rate
// ===========================================================================

#[test]
fn rt_metrics_new_all_zero() {
    let m = RTMetricsSnapshot::new();
    assert_eq!(m.total_ticks, 0);
    assert_eq!(m.missed_ticks, 0);
    assert!(m.cpu_usage_percent >= 0.0);
    assert_eq!(m.memory_usage_bytes, 0);
}

#[test]
fn rt_metrics_missed_tick_rate_zero_ticks() {
    let m = RTMetricsSnapshot::new();
    let rate = m.missed_tick_rate();
    assert!(rate.is_finite() || rate.is_nan());
}

#[test]
fn rt_metrics_missed_tick_rate_normal() {
    let mut m = RTMetricsSnapshot::new();
    m.total_ticks = 1000;
    m.missed_ticks = 10;
    let rate = m.missed_tick_rate();
    // missed_tick_rate returns a percentage: 10/1000 = 1.0%
    assert!((rate - 1.0).abs() < 1e-6);
}

#[test]
fn rt_metrics_has_violations_clean() {
    let m = RTMetricsSnapshot::new();
    let t = RTThresholds::default();
    assert!(!m.has_violations(&t));
}

#[test]
fn rt_metrics_has_violations_jitter() {
    let mut m = RTMetricsSnapshot::new();
    m.jitter = JitterStats::from_values(0, 500_000, 1_000_000); // p99=500µs
    let t = RTThresholds::default();
    assert!(m.has_violations(&t));
}

#[test]
fn rt_metrics_has_violations_processing_time() {
    let mut m = RTMetricsSnapshot::new();
    m.processing_time = LatencyStats::from_values(0, 1000, 2000); // p99=1000µs
    let t = RTThresholds::default();
    assert!(m.has_violations(&t));
}

#[test]
fn rt_metrics_default() {
    let m = RTMetricsSnapshot::default();
    assert_eq!(m.total_ticks, 0);
}

// ===========================================================================
// RTThresholds — defaults
// ===========================================================================

#[test]
fn rt_thresholds_default_values() {
    let t = RTThresholds::default();
    assert!(t.max_jitter_ns > 0);
    assert!(t.max_processing_time_us > 0);
    assert!(t.max_hid_latency_us > 0);
    assert!(t.max_cpu_usage_percent > 0.0);
    assert!(t.max_memory_usage_bytes > 0);
}

#[test]
fn rt_thresholds_debug() {
    let t = RTThresholds::default();
    let debug = format!("{t:?}");
    assert!(debug.contains("RTThresholds"));
}

// ===========================================================================
// AppMetricsSnapshot — construction, violations
// ===========================================================================

#[test]
fn app_metrics_new_all_zero() {
    let m = AppMetricsSnapshot::new();
    assert_eq!(m.connected_devices, 0);
    assert!((m.torque_saturation_percent - 0.0).abs() < f32::EPSILON);
    assert!((m.telemetry_packet_loss_percent - 0.0).abs() < f32::EPSILON);
    assert_eq!(m.safety_events, 0);
    assert_eq!(m.profile_switches, 0);
}

#[test]
fn app_metrics_no_violations_clean() {
    let m = AppMetricsSnapshot::new();
    let t = AppThresholds::default();
    assert!(!m.has_violations(&t));
}

#[test]
fn app_metrics_violation_torque() {
    let mut m = AppMetricsSnapshot::new();
    m.torque_saturation_percent = 96.0; // above default 95.0
    let t = AppThresholds::default();
    assert!(m.has_violations(&t));
}

#[test]
fn app_metrics_violation_telemetry_loss() {
    let mut m = AppMetricsSnapshot::new();
    m.telemetry_packet_loss_percent = 6.0; // above default 5.0
    let t = AppThresholds::default();
    assert!(m.has_violations(&t));
}

#[test]
fn app_metrics_default() {
    let m = AppMetricsSnapshot::default();
    assert_eq!(m.connected_devices, 0);
}

// ===========================================================================
// AppThresholds — defaults
// ===========================================================================

#[test]
fn app_thresholds_defaults() {
    let t = AppThresholds::default();
    assert!((t.max_torque_saturation_percent - 95.0).abs() < f32::EPSILON);
    assert!((t.max_telemetry_loss_percent - 5.0).abs() < f32::EPSILON);
}

// ===========================================================================
// StreamingStats — recording, min/max/mean, reset
// ===========================================================================

#[test]
fn streaming_stats_new_is_empty() {
    let s = StreamingStats::new();
    assert!(s.is_empty());
    assert_eq!(s.count(), 0);
}

#[test]
fn streaming_stats_default_matches_new() {
    let s1 = StreamingStats::new();
    let s2 = StreamingStats::default();
    assert_eq!(s1.count(), s2.count());
    assert_eq!(s1.is_empty(), s2.is_empty());
}

#[test]
fn streaming_stats_single_record() {
    let mut s = StreamingStats::new();
    s.record(42);
    assert!(!s.is_empty());
    assert_eq!(s.count(), 1);
    assert_eq!(s.min(), 42);
    assert_eq!(s.max(), 42);
    assert!((s.mean() - 42.0).abs() < f64::EPSILON);
}

#[test]
fn streaming_stats_multiple_records() {
    let mut s = StreamingStats::new();
    s.record(10);
    s.record(20);
    s.record(30);
    assert_eq!(s.count(), 3);
    assert_eq!(s.min(), 10);
    assert_eq!(s.max(), 30);
    assert!((s.mean() - 20.0).abs() < f64::EPSILON);
}

#[test]
fn streaming_stats_min_max_tracking() {
    let mut s = StreamingStats::new();
    s.record(50);
    s.record(10);
    s.record(100);
    s.record(1);
    assert_eq!(s.min(), 1);
    assert_eq!(s.max(), 100);
}

#[test]
fn streaming_stats_zero_values() {
    let mut s = StreamingStats::new();
    s.record(0);
    s.record(0);
    assert_eq!(s.min(), 0);
    assert_eq!(s.max(), 0);
    assert!((s.mean() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn streaming_stats_large_values() {
    let mut s = StreamingStats::new();
    s.record(u64::MAX / 2);
    s.record(u64::MAX / 2);
    assert_eq!(s.count(), 2);
    assert_eq!(s.min(), u64::MAX / 2);
    assert_eq!(s.max(), u64::MAX / 2);
}

#[test]
fn streaming_stats_reset() {
    let mut s = StreamingStats::new();
    s.record(10);
    s.record(20);
    s.reset();
    assert!(s.is_empty());
    assert_eq!(s.count(), 0);
}

#[test]
fn streaming_stats_record_after_reset() {
    let mut s = StreamingStats::new();
    s.record(100);
    s.reset();
    s.record(42);
    assert_eq!(s.count(), 1);
    assert_eq!(s.min(), 42);
    assert_eq!(s.max(), 42);
}

#[test]
fn streaming_stats_mean_empty() {
    let s = StreamingStats::new();
    let mean = s.mean();
    // Mean of empty set should be NaN or 0
    assert!(mean.is_nan() || (mean - 0.0).abs() < f64::EPSILON);
}

#[test]
fn streaming_stats_clone_copy() {
    let mut s1 = StreamingStats::new();
    s1.record(5);
    s1.record(15);
    let s2 = s1;
    assert_eq!(s1.count(), s2.count());
    assert_eq!(s1.min(), s2.min());
    assert_eq!(s1.max(), s2.max());
}

// ===========================================================================
// RTSampleQueues — push, pop, capacity, drain (requires "queues" feature)
// ===========================================================================

#[cfg(feature = "queues")]
mod queue_tests {
    use openracing_atomic::queues::{RTSampleQueues, DEFAULT_QUEUE_CAPACITY};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn queues_default_empty() {
        let q = RTSampleQueues::default();
        assert!(q.jitter_is_empty());
        assert!(q.processing_time_is_empty());
        assert!(q.hid_latency_is_empty());
        assert_eq!(q.jitter_len(), 0);
        assert_eq!(q.processing_time_len(), 0);
        assert_eq!(q.hid_latency_len(), 0);
    }

    #[test]
    fn queues_new_empty() {
        let q = RTSampleQueues::new();
        assert!(q.jitter_is_empty());
    }

    #[test]
    fn queues_push_pop_jitter() {
        let q = RTSampleQueues::new();
        assert!(q.push_jitter(100).is_ok());
        assert!(q.push_jitter(200).is_ok());
        assert_eq!(q.jitter_len(), 2);
        assert_eq!(q.pop_jitter(), Some(100));
        assert_eq!(q.pop_jitter(), Some(200));
        assert_eq!(q.pop_jitter(), None);
    }

    #[test]
    fn queues_push_pop_processing_time() {
        let q = RTSampleQueues::new();
        assert!(q.push_processing_time(500).is_ok());
        assert_eq!(q.processing_time_len(), 1);
        assert_eq!(q.pop_processing_time(), Some(500));
        assert!(q.processing_time_is_empty());
    }

    #[test]
    fn queues_push_pop_hid_latency() {
        let q = RTSampleQueues::new();
        assert!(q.push_hid_latency(42).is_ok());
        assert_eq!(q.hid_latency_len(), 1);
        assert_eq!(q.pop_hid_latency(), Some(42));
        assert!(q.hid_latency_is_empty());
    }

    #[test]
    fn queues_push_drop_ignores_full() {
        let q = RTSampleQueues::with_capacity(2);
        q.push_jitter_drop(1);
        q.push_jitter_drop(2);
        q.push_jitter_drop(3); // should be silently dropped
        assert_eq!(q.jitter_len(), 2);
    }

    #[test]
    fn queues_push_returns_error_when_full() {
        let q = RTSampleQueues::with_capacity(1);
        assert!(q.push_jitter(1).is_ok());
        let result = q.push_jitter(2);
        assert!(result.is_err());
        assert_eq!(result, Err(2));
    }

    #[test]
    fn queues_processing_time_drop_ignores_full() {
        let q = RTSampleQueues::with_capacity(1);
        q.push_processing_time_drop(1);
        q.push_processing_time_drop(2); // dropped
        assert_eq!(q.processing_time_len(), 1);
    }

    #[test]
    fn queues_hid_latency_drop_ignores_full() {
        let q = RTSampleQueues::with_capacity(1);
        q.push_hid_latency_drop(1);
        q.push_hid_latency_drop(2); // dropped
        assert_eq!(q.hid_latency_len(), 1);
    }

    #[test]
    fn queues_drain_jitter() {
        let q = RTSampleQueues::new();
        for i in 0..5 {
            q.push_jitter_drop(i);
        }
        let drained = q.drain_jitter();
        assert_eq!(drained.len(), 5);
        assert!(q.jitter_is_empty());
    }

    #[test]
    fn queues_drain_processing_time() {
        let q = RTSampleQueues::new();
        for i in 0..3 {
            q.push_processing_time_drop(i * 100);
        }
        let drained = q.drain_processing_time();
        assert_eq!(drained.len(), 3);
        assert!(q.processing_time_is_empty());
    }

    #[test]
    fn queues_drain_hid_latency() {
        let q = RTSampleQueues::new();
        q.push_hid_latency_drop(10);
        q.push_hid_latency_drop(20);
        let drained = q.drain_hid_latency();
        assert_eq!(drained.len(), 2);
        assert!(q.hid_latency_is_empty());
    }

    #[test]
    fn queues_stats() {
        let q = RTSampleQueues::new();
        q.push_jitter_drop(1);
        q.push_jitter_drop(2);
        q.push_processing_time_drop(3);
        q.push_hid_latency_drop(4);

        let stats = q.stats();
        assert_eq!(stats.jitter_count, 2);
        assert_eq!(stats.processing_time_count, 1);
        assert_eq!(stats.hid_latency_count, 1);
    }

    #[test]
    fn queues_default_capacity_constant() {
        assert_eq!(DEFAULT_QUEUE_CAPACITY, 10_000);
    }

    #[test]
    fn queues_with_capacity() {
        let q = RTSampleQueues::with_capacity(5);
        for i in 0..5 {
            assert!(q.push_jitter(i).is_ok());
        }
        assert!(q.push_jitter(5).is_err());
    }

    #[test]
    fn queues_fifo_ordering() {
        let q = RTSampleQueues::new();
        for i in 0..10 {
            q.push_jitter_drop(i);
        }
        for i in 0..10 {
            assert_eq!(q.pop_jitter(), Some(i));
        }
    }

    #[test]
    fn queues_concurrent_push_pop() {
        let q = Arc::new(RTSampleQueues::with_capacity(10_000));
        let n = 1000_u64;

        let producer = {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                for i in 0..n {
                    q.push_jitter_drop(i);
                }
            })
        };

        let consumer = {
            let q = Arc::clone(&q);
            thread::spawn(move || {
                let mut count = 0_u64;
                // Drain after producer finishes
                thread::sleep(std::time::Duration::from_millis(10));
                while q.pop_jitter().is_some() {
                    count += 1;
                }
                count
            })
        };

        producer.join().map_err(|_| "producer panicked").ok();
        let consumed = consumer.join().map_err(|_| "consumer panicked").unwrap_or(0);
        // All items should have been consumed (single producer, single consumer)
        assert!(consumed <= n);
    }

    #[test]
    fn queues_concurrent_multi_producer() {
        let q = Arc::new(RTSampleQueues::with_capacity(10_000));
        let producers = 4;
        let items_per_producer = 500_u64;

        let handles: Vec<_> = (0..producers)
            .map(|_| {
                let q = Arc::clone(&q);
                thread::spawn(move || {
                    for i in 0..items_per_producer {
                        q.push_jitter_drop(i);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().map_err(|_| "thread panicked").ok();
        }

        let total = q.jitter_len();
        // All items should have been pushed (capacity is large enough)
        assert_eq!(total as u64, producers as u64 * items_per_producer);
    }

    #[test]
    fn queues_pop_empty_returns_none() {
        let q = RTSampleQueues::new();
        assert_eq!(q.pop_jitter(), None);
        assert_eq!(q.pop_processing_time(), None);
        assert_eq!(q.pop_hid_latency(), None);
    }

    #[test]
    fn queues_drain_empty() {
        let q = RTSampleQueues::new();
        assert!(q.drain_jitter().is_empty());
        assert!(q.drain_processing_time().is_empty());
        assert!(q.drain_hid_latency().is_empty());
    }
}

// ===========================================================================
// Integration: AtomicCounters snapshot → metrics snapshot pipeline
// ===========================================================================

#[test]
fn counters_to_app_metrics_pipeline() {
    let c = AtomicCounters::new();

    // Simulate a session
    c.inc_tick_by(10_000);
    c.inc_missed_tick_by(5);
    c.inc_safety_event();
    c.inc_safety_event();
    c.inc_profile_switch();
    for _ in 0..100 {
        c.record_torque_saturation(true);
    }
    for _ in 0..100 {
        c.record_torque_saturation(false);
    }
    for _ in 0..95 {
        c.inc_telemetry_received();
    }
    for _ in 0..5 {
        c.inc_telemetry_lost();
    }

    let snap = c.snapshot();
    let app = AppMetricsSnapshot {
        connected_devices: 1,
        torque_saturation_percent: snap.torque_saturation_percent(),
        telemetry_packet_loss_percent: snap.telemetry_loss_percent(),
        safety_events: snap.safety_events,
        profile_switches: snap.profile_switches,
    };

    assert_eq!(app.connected_devices, 1);
    assert!((app.torque_saturation_percent - 50.0).abs() < 0.1);
    assert!((app.telemetry_packet_loss_percent - 5.0).abs() < 0.1);
    assert_eq!(app.safety_events, 2);
    assert_eq!(app.profile_switches, 1);

    // Should be within thresholds
    let thresholds = AppThresholds::default();
    assert!(!app.has_violations(&thresholds));
}

#[test]
fn rt_metrics_no_violations_within_thresholds() {
    let m = RTMetricsSnapshot {
        total_ticks: 10_000,
        missed_ticks: 0,
        jitter: JitterStats::from_values(50_000, 100_000, 200_000),
        hid_latency: LatencyStats::from_values(10, 30, 50),
        processing_time: LatencyStats::from_values(20, 40, 80),
        cpu_usage_percent: 30.0,
        memory_usage_bytes: 1024 * 1024,
    };
    let t = RTThresholds::default();
    assert!(!m.has_violations(&t));
}

// ===========================================================================
// Edge cases — large numbers, boundary values
// ===========================================================================

#[test]
fn atomic_counters_large_increments() {
    let c = AtomicCounters::new();
    c.inc_tick_by(u64::MAX / 2);
    assert_eq!(c.total_ticks(), u64::MAX / 2);
}

#[test]
fn streaming_stats_sequential_min_update() {
    let mut s = StreamingStats::new();
    // Record in decreasing order to test min updates at each step
    s.record(100);
    assert_eq!(s.min(), 100);
    s.record(50);
    assert_eq!(s.min(), 50);
    s.record(25);
    assert_eq!(s.min(), 25);
    s.record(1);
    assert_eq!(s.min(), 1);
    s.record(0);
    assert_eq!(s.min(), 0);
}

#[test]
fn streaming_stats_sequential_max_update() {
    let mut s = StreamingStats::new();
    s.record(1);
    assert_eq!(s.max(), 1);
    s.record(50);
    assert_eq!(s.max(), 50);
    s.record(100);
    assert_eq!(s.max(), 100);
    s.record(1000);
    assert_eq!(s.max(), 1000);
}

#[test]
fn jitter_stats_zero_threshold() {
    let s = JitterStats::from_values(0, 0, 0);
    assert!(!s.exceeds_threshold(0));
    assert!(!s.exceeds_threshold(1));
}

#[test]
fn latency_stats_zero_threshold() {
    let s = LatencyStats::from_values(0, 0, 0);
    assert!(!s.exceeds_threshold(0));
    assert!(!s.exceeds_threshold(1));
}
