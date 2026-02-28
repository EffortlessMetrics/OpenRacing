//! Property-based tests for openracing-atomic using quickcheck.
//!
//! These tests verify invariants and properties that should hold for all inputs.

use openracing_atomic::{
    AtomicCounters, JitterStats, LatencyStats, RTMetricsSnapshot, StreamingStats,
};
use quickcheck_macros::quickcheck;

#[quickcheck]
fn prop_counter_increment_is_monotonic(incs: u8) -> bool {
    let counters = AtomicCounters::new();

    for _ in 0..incs {
        counters.inc_tick();
    }

    counters.total_ticks() == u64::from(incs)
}

#[quickcheck]
fn prop_counter_add_is_monotonic(base: u64, add: u64) -> bool {
    let counters = AtomicCounters::new();
    counters.inc_tick_by(base);
    counters.inc_tick_by(add);

    let result = counters.total_ticks();
    let expected = base.wrapping_add(add);
    result == expected
}

#[quickcheck]
fn prop_snapshot_and_reset_clears_counters(ticks: u64, missed: u64) -> bool {
    let counters = AtomicCounters::new();

    counters.inc_tick_by(ticks);
    counters.inc_missed_tick_by(missed);

    let first = counters.snapshot_and_reset();
    let second = counters.snapshot();

    first.total_ticks == ticks
        && first.missed_ticks == missed
        && second.total_ticks == 0
        && second.missed_ticks == 0
}

#[quickcheck]
fn prop_torque_saturation_never_exceeds_100(saturated: u8, not_saturated: u8) -> bool {
    let counters = AtomicCounters::new();

    for _ in 0..saturated {
        counters.record_torque_saturation(true);
    }
    for _ in 0..not_saturated {
        counters.record_torque_saturation(false);
    }

    let pct = counters.torque_saturation_percent();
    (0.0..=100.0).contains(&pct)
}

#[quickcheck]
fn prop_telemetry_loss_never_exceeds_100(received: u8, lost: u8) -> bool {
    let counters = AtomicCounters::new();

    for _ in 0..received {
        counters.inc_telemetry_received();
    }
    for _ in 0..lost {
        counters.inc_telemetry_lost();
    }

    let pct = counters.telemetry_loss_percent();
    (0.0..=100.0).contains(&pct)
}

#[quickcheck]
fn prop_jitter_stats_percentiles_ordered(p50: u64, p99: u64, max: u64) -> bool {
    let stats = JitterStats::from_values(p50, p99, max);

    stats.p50_ns <= stats.p99_ns && stats.p99_ns <= stats.max_ns || stats.p50_ns == p50
}

#[quickcheck]
fn prop_latency_stats_percentiles_ordered(p50: u64, p99: u64, max: u64) -> bool {
    let stats = LatencyStats::from_values(p50, p99, max);

    stats.p50_us <= stats.p99_us && stats.p99_us <= stats.max_us || stats.p50_us == p50
}

#[quickcheck]
fn prop_streaming_stats_min_max(samples: Vec<u64>) -> bool {
    if samples.is_empty() {
        return true;
    }

    let mut stats = StreamingStats::new();
    let Some(&expected_min) = samples.iter().min() else {
        return true;
    };
    let Some(&expected_max) = samples.iter().max() else {
        return true;
    };

    for &s in &samples {
        stats.record(s);
    }

    stats.min() == expected_min && stats.max() == expected_max
}

#[quickcheck]
fn prop_streaming_stats_count(samples: Vec<u64>) -> bool {
    let mut stats = StreamingStats::new();

    for &s in &samples {
        stats.record(s);
    }

    stats.count() == samples.len() as u64
}

#[quickcheck]
fn prop_streaming_stats_mean(samples: Vec<u64>) -> bool {
    if samples.is_empty() {
        return true;
    }

    let mut stats = StreamingStats::new();
    for &s in &samples {
        stats.record(s);
    }

    let mean = stats.mean();
    mean.is_finite() && mean >= 0.0
}

#[quickcheck]
fn prop_threshold_exceeded_consistency(jitter_p99: u64, threshold: u64) -> bool {
    let stats = JitterStats::from_values(0, jitter_p99, jitter_p99);
    let exceeds = stats.exceeds_threshold(threshold);

    (jitter_p99 > threshold) == exceeds
}

#[quickcheck]
fn prop_rt_metrics_missed_tick_rate(total: u64, missed: u64) -> bool {
    let total = total.max(missed);
    let mut metrics = RTMetricsSnapshot::new();
    metrics.total_ticks = total;
    metrics.missed_ticks = missed;

    let rate = metrics.missed_tick_rate();

    if total == 0 {
        rate == 0.0
    } else {
        let expected = (missed as f64 / total as f64) * 100.0;
        (rate - expected).abs() < f64::EPSILON && (0.0..=100.0).contains(&rate)
    }
}

#[quickcheck]
fn prop_counter_operations_are_independent(ticks: u8, missed: u8, safety: u8, profile: u8) -> bool {
    let counters = AtomicCounters::new();

    for _ in 0..ticks {
        counters.inc_tick();
    }
    for _ in 0..missed {
        counters.inc_missed_tick();
    }
    for _ in 0..safety {
        counters.inc_safety_event();
    }
    for _ in 0..profile {
        counters.inc_profile_switch();
    }

    let snapshot = counters.snapshot();
    snapshot.total_ticks == u64::from(ticks)
        && snapshot.missed_ticks == u64::from(missed)
        && snapshot.safety_events == u64::from(safety)
        && snapshot.profile_switches == u64::from(profile)
}
