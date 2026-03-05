//! Comprehensive correctness tests for openracing-atomic primitives.
//!
//! Covers concurrent correctness, ordering guarantees, edge cases,
//! queue overflow/underflow, FIFO ordering, wrap-around, and
//! property-based testing for arbitrary operation sequences.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use openracing_atomic::{
    AtomicCounters, CounterSnapshot, JitterStats, LatencyStats, StreamingStats,
};

#[cfg(feature = "queues")]
use openracing_atomic::queues::{DEFAULT_QUEUE_CAPACITY, RTSampleQueues};

// ============================================================================
// 1. AtomicCounters — concurrent correctness
// ============================================================================

/// All threads start simultaneously via barrier; total must be exact.
#[test]
fn counter_barrier_synchronized_increments() -> Result<(), Box<dyn std::error::Error>> {
    let threads: u64 = 16;
    let ops: u64 = 20_000;
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new(threads as usize));

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..ops {
                    c.inc_tick();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    assert_eq!(counters.total_ticks(), threads * ops);
    Ok(())
}

/// Multiple collectors calling `snapshot_and_reset` concurrently must
/// conserve the total: sum of all collected values + residual == total written.
#[test]
fn counter_multi_collector_conservation() -> Result<(), Box<dyn std::error::Error>> {
    let writers: u64 = 4;
    let collectors: u64 = 4;
    let ops: u64 = 50_000;
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new((writers + collectors) as usize));

    let writer_handles: Vec<_> = (0..writers)
        .map(|_| {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..ops {
                    c.inc_tick();
                }
            })
        })
        .collect();

    let collector_handles: Vec<_> = (0..collectors)
        .map(|_| {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || -> u64 {
                b.wait();
                let mut collected = 0u64;
                for _ in 0..500 {
                    collected += c.snapshot_and_reset().total_ticks;
                    thread::yield_now();
                }
                collected
            })
        })
        .collect();

    for h in writer_handles {
        h.join().map_err(|_| "writer panicked")?;
    }

    let mut total_collected: u64 = 0;
    for h in collector_handles {
        total_collected += h.join().map_err(|_| "collector panicked")?;
    }

    let residual = counters.snapshot().total_ticks;
    assert_eq!(
        total_collected + residual,
        writers * ops,
        "increments lost in multi-collector scenario"
    );
    Ok(())
}

/// Each counter field is independent: concurrent writes to different counters
/// must not interfere.
#[test]
fn counter_field_isolation() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let ops: u64 = 30_000;
    let barrier = Arc::new(Barrier::new(9));

    let spawn_counter = |f: fn(&AtomicCounters)| {
        let c = Arc::clone(&counters);
        let b = Arc::clone(&barrier);
        thread::spawn(move || {
            b.wait();
            for _ in 0..ops {
                f(&c);
            }
        })
    };

    let handles = vec![
        spawn_counter(AtomicCounters::inc_tick),
        spawn_counter(AtomicCounters::inc_missed_tick),
        spawn_counter(AtomicCounters::inc_safety_event),
        spawn_counter(AtomicCounters::inc_profile_switch),
        spawn_counter(AtomicCounters::inc_telemetry_received),
        spawn_counter(AtomicCounters::inc_telemetry_lost),
        spawn_counter(AtomicCounters::inc_hid_write_error),
        {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..ops {
                    c.record_torque_saturation(true);
                }
            })
        },
        {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..ops {
                    c.record_torque_saturation(false);
                }
            })
        },
    ];

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    let snap = counters.snapshot();
    assert_eq!(snap.total_ticks, ops);
    assert_eq!(snap.missed_ticks, ops);
    assert_eq!(snap.safety_events, ops);
    assert_eq!(snap.profile_switches, ops);
    assert_eq!(snap.telemetry_packets_received, ops);
    assert_eq!(snap.telemetry_packets_lost, ops);
    assert_eq!(snap.hid_write_errors, ops);
    // Two threads each do ops saturation recordings
    assert_eq!(snap.torque_saturation_samples, 2 * ops);
    assert_eq!(snap.torque_saturation_count, ops);
    Ok(())
}

/// Interleaved `inc_tick` and `snapshot_and_reset` from the same thread
/// must not lose any increments.
#[test]
fn counter_single_thread_reset_conservation() -> Result<(), Box<dyn std::error::Error>> {
    let counters = AtomicCounters::new();
    let mut total = 0u64;
    let iters = 10_000u64;

    for _ in 0..iters {
        counters.inc_tick();
        counters.inc_tick();
        total += counters.snapshot_and_reset().total_ticks;
    }
    total += counters.snapshot().total_ticks;

    assert_eq!(total, 2 * iters);
    Ok(())
}

// ============================================================================
// 2. Ordering guarantees
// ============================================================================

/// Under Relaxed ordering, snapshot reads never see negative (impossible for u64)
/// or values beyond the maximum written. Validated with concurrent readers/writers.
#[test]
fn ordering_snapshot_values_bounded_during_writes() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let done = Arc::new(AtomicBool::new(false));
    let max_ops = 100_000u64;
    let writers = 4u64;

    let writer_handles: Vec<_> = (0..writers)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..max_ops {
                    c.inc_tick();
                    c.inc_missed_tick();
                    c.inc_safety_event();
                }
            })
        })
        .collect();

    let done_flag = Arc::clone(&done);
    let reader_counters = Arc::clone(&counters);
    let reader = thread::spawn(move || {
        let ceiling = writers * max_ops;
        while !done_flag.load(Ordering::Acquire) {
            let s = reader_counters.snapshot();
            assert!(s.total_ticks <= ceiling, "ticks exceeded ceiling");
            assert!(s.missed_ticks <= ceiling, "missed exceeded ceiling");
            assert!(s.safety_events <= ceiling, "safety exceeded ceiling");
        }
    });

    for h in writer_handles {
        h.join().map_err(|_| "writer panicked")?;
    }
    done.store(true, Ordering::Release);
    reader.join().map_err(|_| "reader panicked")?;

    let final_snap = counters.snapshot();
    assert_eq!(final_snap.total_ticks, writers * max_ops);
    Ok(())
}

/// Verify that `inc_tick_by(0)` is a no-op and does not corrupt the counter.
#[test]
fn ordering_increment_by_zero_is_noop() -> Result<(), Box<dyn std::error::Error>> {
    let counters = AtomicCounters::new();
    counters.inc_tick_by(42);
    counters.inc_tick_by(0);
    counters.inc_missed_tick_by(0);
    assert_eq!(counters.total_ticks(), 42);
    assert_eq!(counters.missed_ticks(), 0);
    Ok(())
}

/// Verify that wrapping (u64 overflow) behaves correctly via `fetch_add`.
#[test]
fn ordering_counter_wrapping_on_overflow() -> Result<(), Box<dyn std::error::Error>> {
    let counters = AtomicCounters::new();
    counters.inc_tick_by(u64::MAX);
    counters.inc_tick();
    // fetch_add wraps on overflow
    assert_eq!(counters.total_ticks(), 0);

    counters.inc_tick();
    assert_eq!(counters.total_ticks(), 1);
    Ok(())
}

/// `snapshot()` and `snapshot_and_reset()` must be self-consistent on a
/// quiescent (no concurrent writers) counter.
#[test]
fn ordering_quiescent_snapshot_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let counters = AtomicCounters::new();
    for _ in 0..100 {
        counters.inc_tick();
        counters.inc_missed_tick();
        counters.record_torque_saturation(true);
    }

    let snap1 = counters.snapshot();
    let snap2 = counters.snapshot();
    assert_eq!(
        snap1, snap2,
        "repeated snapshots differ on quiescent counter"
    );

    let snap3 = counters.snapshot_and_reset();
    assert_eq!(snap3, snap1, "snapshot_and_reset differs from snapshot");

    let snap4 = counters.snapshot();
    assert_eq!(snap4, CounterSnapshot::default(), "reset did not zero");
    Ok(())
}

// ============================================================================
// 3. CounterSnapshot edge cases
// ============================================================================

#[test]
fn snapshot_torque_saturation_percent_zero_samples() -> Result<(), Box<dyn std::error::Error>> {
    let snap = CounterSnapshot::default();
    assert!((snap.torque_saturation_percent() - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn snapshot_torque_saturation_percent_all_saturated() -> Result<(), Box<dyn std::error::Error>> {
    let snap = CounterSnapshot {
        torque_saturation_samples: 1000,
        torque_saturation_count: 1000,
        ..CounterSnapshot::default()
    };
    assert!((snap.torque_saturation_percent() - 100.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn snapshot_telemetry_loss_zero_packets() -> Result<(), Box<dyn std::error::Error>> {
    let snap = CounterSnapshot::default();
    assert!((snap.telemetry_loss_percent() - 0.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn snapshot_telemetry_loss_all_lost() -> Result<(), Box<dyn std::error::Error>> {
    let snap = CounterSnapshot {
        telemetry_packets_received: 0,
        telemetry_packets_lost: 500,
        ..CounterSnapshot::default()
    };
    assert!((snap.telemetry_loss_percent() - 100.0).abs() < f32::EPSILON);
    Ok(())
}

#[test]
fn snapshot_telemetry_loss_saturating_total() -> Result<(), Box<dyn std::error::Error>> {
    // When received + lost would overflow u64, saturating_add caps at u64::MAX
    let snap = CounterSnapshot {
        telemetry_packets_received: u64::MAX,
        telemetry_packets_lost: 1,
        ..CounterSnapshot::default()
    };
    let pct = snap.telemetry_loss_percent();
    assert!(pct.is_finite() && pct >= 0.0);
    Ok(())
}

// ============================================================================
// 4. StreamingStats edge cases
// ============================================================================

#[test]
fn streaming_stats_single_element() -> Result<(), Box<dyn std::error::Error>> {
    let mut stats = StreamingStats::new();
    stats.record(42);
    assert_eq!(stats.count(), 1);
    assert_eq!(stats.min(), 42);
    assert_eq!(stats.max(), 42);
    assert!((stats.mean() - 42.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn streaming_stats_all_same_values() -> Result<(), Box<dyn std::error::Error>> {
    let mut stats = StreamingStats::new();
    for _ in 0..100 {
        stats.record(7);
    }
    assert_eq!(stats.min(), 7);
    assert_eq!(stats.max(), 7);
    assert!((stats.mean() - 7.0).abs() < f64::EPSILON);
    Ok(())
}

#[test]
fn streaming_stats_min_then_max() -> Result<(), Box<dyn std::error::Error>> {
    let mut stats = StreamingStats::new();
    stats.record(0);
    stats.record(u64::MAX);
    assert_eq!(stats.min(), 0);
    assert_eq!(stats.max(), u64::MAX);
    // Sum saturates at u64::MAX, so mean is capped
    assert!(stats.mean().is_finite());
    Ok(())
}

#[test]
fn streaming_stats_reset_then_reuse() -> Result<(), Box<dyn std::error::Error>> {
    let mut stats = StreamingStats::new();
    stats.record(100);
    stats.record(200);
    stats.reset();

    assert!(stats.is_empty());
    assert_eq!(stats.count(), 0);
    assert_eq!(stats.min(), u64::MAX);
    assert_eq!(stats.max(), 0);

    stats.record(50);
    assert_eq!(stats.min(), 50);
    assert_eq!(stats.max(), 50);
    assert_eq!(stats.count(), 1);
    Ok(())
}

#[test]
fn streaming_stats_large_count_saturating() -> Result<(), Box<dyn std::error::Error>> {
    let mut stats = StreamingStats::new();
    // Record u64::MAX twice; both count and sum use saturating_add
    stats.record(u64::MAX);
    stats.record(u64::MAX);
    assert_eq!(stats.count(), 2);
    // sum saturates at u64::MAX
    let mean = stats.mean();
    assert!(mean.is_finite() && mean > 0.0);
    Ok(())
}

// ============================================================================
// 5. JitterStats / LatencyStats edge cases
// ============================================================================

#[test]
fn jitter_stats_zero_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let stats = JitterStats::from_values(0, 0, 0);
    assert!(!stats.exceeds_threshold(0));
    assert!(!stats.exceeds_threshold(1));
    Ok(())
}

#[test]
fn jitter_stats_max_values() -> Result<(), Box<dyn std::error::Error>> {
    let stats = JitterStats::from_values(u64::MAX, u64::MAX, u64::MAX);
    assert!(stats.exceeds_threshold(u64::MAX - 1));
    assert!(!stats.exceeds_threshold(u64::MAX));
    Ok(())
}

#[test]
fn jitter_to_micros_rounding() -> Result<(), Box<dyn std::error::Error>> {
    let stats = JitterStats::from_values(999, 1001, 500_500);
    let micros = stats.to_micros();
    // Integer division truncates
    assert_eq!(micros.p50_ns, 0);
    assert_eq!(micros.p99_ns, 1);
    assert_eq!(micros.max_ns, 500);
    Ok(())
}

#[test]
fn latency_from_nanos_rounding() -> Result<(), Box<dyn std::error::Error>> {
    let stats = LatencyStats::from_nanos(999, 1500, 2999);
    assert_eq!(stats.p50_us, 0);
    assert_eq!(stats.p99_us, 1);
    assert_eq!(stats.max_us, 2);
    Ok(())
}

#[test]
fn latency_threshold_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let stats = LatencyStats::from_values(10, 200, 300);
    // Equal to threshold is not exceeded
    assert!(!stats.exceeds_threshold(200));
    assert!(stats.exceeds_threshold(199));
    Ok(())
}

// ============================================================================
// 6. Queue correctness tests (feature = "queues")
// ============================================================================

#[cfg(feature = "queues")]
mod queue_correctness {
    use super::*;

    #[test]
    fn queue_default_capacity() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::new();
        assert_eq!(DEFAULT_QUEUE_CAPACITY, 10_000);
        // Should accept at least DEFAULT_QUEUE_CAPACITY items
        for i in 0..DEFAULT_QUEUE_CAPACITY as u64 {
            queues
                .push_jitter(i)
                .map_err(|_| "push failed before capacity")?;
        }
        assert_eq!(queues.jitter_len(), DEFAULT_QUEUE_CAPACITY);
        Ok(())
    }

    /// Verify strict FIFO ordering on a single-threaded push/pop sequence.
    #[test]
    fn queue_fifo_ordering_single_thread() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(100);
        for i in 0..100u64 {
            queues.push_jitter(i).map_err(|_| "push failed")?;
        }
        for i in 0..100u64 {
            let val = queues.pop_jitter().ok_or("unexpected empty queue")?;
            assert_eq!(val, i, "FIFO order violated at index {i}");
        }
        assert!(queues.pop_jitter().is_none());
        Ok(())
    }

    /// All three queue lanes maintain independent FIFO order.
    #[test]
    fn queue_all_lanes_independent_fifo() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(50);

        for i in 0..50u64 {
            queues
                .push_jitter(i * 10)
                .map_err(|_| "jitter push failed")?;
            queues
                .push_processing_time(i * 20)
                .map_err(|_| "processing push failed")?;
            queues
                .push_hid_latency(i * 30)
                .map_err(|_| "hid push failed")?;
        }

        for i in 0..50u64 {
            let j = queues.pop_jitter().ok_or("jitter pop failed")?;
            let p = queues
                .pop_processing_time()
                .ok_or("processing pop failed")?;
            let h = queues.pop_hid_latency().ok_or("hid pop failed")?;
            assert_eq!(j, i * 10);
            assert_eq!(p, i * 20);
            assert_eq!(h, i * 30);
        }
        Ok(())
    }

    /// Capacity-1 queue: push one, pop one, repeat.
    #[test]
    fn queue_capacity_one() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(1);

        // Push one
        queues.push_jitter(42).map_err(|_| "first push failed")?;
        // Second push must fail (queue full)
        assert!(queues.push_jitter(99).is_err());

        // Pop should give us the first value
        let val = queues.pop_jitter().ok_or("pop failed")?;
        assert_eq!(val, 42);
        assert!(queues.pop_jitter().is_none());

        // After pop, we can push again
        queues
            .push_jitter(100)
            .map_err(|_| "push after pop failed")?;
        let val = queues.pop_jitter().ok_or("second pop failed")?;
        assert_eq!(val, 100);
        Ok(())
    }

    /// Overflow: once full, pushes fail with Err containing the rejected value.
    #[test]
    fn queue_overflow_returns_rejected_value() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(3);
        queues.push_jitter(10).map_err(|_| "push 1 failed")?;
        queues.push_jitter(20).map_err(|_| "push 2 failed")?;
        queues.push_jitter(30).map_err(|_| "push 3 failed")?;

        let err = queues.push_jitter(40);
        assert!(err.is_err());
        // The rejected value is returned in the Err variant
        assert_eq!(err.err(), Some(40));
        Ok(())
    }

    /// Underflow: popping from empty queue returns None.
    #[test]
    fn queue_underflow_returns_none() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(10);
        assert!(queues.pop_jitter().is_none());
        assert!(queues.pop_processing_time().is_none());
        assert!(queues.pop_hid_latency().is_none());
        Ok(())
    }

    /// Empty/length checks are consistent.
    #[test]
    fn queue_empty_and_length_consistency() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(5);

        assert!(queues.jitter_is_empty());
        assert_eq!(queues.jitter_len(), 0);

        queues.push_jitter(1).map_err(|_| "push failed")?;
        assert!(!queues.jitter_is_empty());
        assert_eq!(queues.jitter_len(), 1);

        queues.push_jitter(2).map_err(|_| "push failed")?;
        assert_eq!(queues.jitter_len(), 2);

        let _ = queues.pop_jitter();
        assert_eq!(queues.jitter_len(), 1);

        let _ = queues.pop_jitter();
        assert!(queues.jitter_is_empty());
        assert_eq!(queues.jitter_len(), 0);
        Ok(())
    }

    /// `push_*_drop` methods silently drop on overflow.
    #[test]
    fn queue_push_drop_variants_silent_overflow() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(2);
        queues.push_jitter_drop(1);
        queues.push_jitter_drop(2);
        queues.push_jitter_drop(3); // silently dropped

        assert_eq!(queues.jitter_len(), 2);
        let v1 = queues.pop_jitter().ok_or("pop 1 failed")?;
        let v2 = queues.pop_jitter().ok_or("pop 2 failed")?;
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        assert!(queues.pop_jitter().is_none());
        Ok(())
    }

    /// Drain returns all items in FIFO order and empties the queue.
    #[cfg(feature = "std")]
    #[test]
    fn queue_drain_fifo_and_empties() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(10);
        for i in 0..5u64 {
            queues.push_jitter(i).map_err(|_| "push failed")?;
            queues
                .push_processing_time(i * 10)
                .map_err(|_| "push failed")?;
            queues
                .push_hid_latency(i * 100)
                .map_err(|_| "push failed")?;
        }

        let jitter_drained = queues.drain_jitter();
        let proc_drained = queues.drain_processing_time();
        let hid_drained = queues.drain_hid_latency();

        assert_eq!(jitter_drained, vec![0, 1, 2, 3, 4]);
        assert_eq!(proc_drained, vec![0, 10, 20, 30, 40]);
        assert_eq!(hid_drained, vec![0, 100, 200, 300, 400]);

        assert!(queues.jitter_is_empty());
        assert!(queues.processing_time_is_empty());
        assert!(queues.hid_latency_is_empty());
        Ok(())
    }

    /// QueueStats reflects actual counts across all three lanes.
    #[test]
    fn queue_stats_accuracy() -> Result<(), Box<dyn std::error::Error>> {
        let queues = RTSampleQueues::with_capacity(100);
        for _ in 0..3 {
            queues.push_jitter(0).map_err(|_| "push failed")?;
        }
        for _ in 0..7 {
            queues.push_processing_time(0).map_err(|_| "push failed")?;
        }
        for _ in 0..5 {
            queues.push_hid_latency(0).map_err(|_| "push failed")?;
        }

        let stats = queues.stats();
        assert_eq!(stats.jitter_count, 3);
        assert_eq!(stats.processing_time_count, 7);
        assert_eq!(stats.hid_latency_count, 5);
        Ok(())
    }

    // ── Queue concurrent correctness ────────────────────────────────────────

    /// Producer-consumer with barrier: total pushed == total popped + residual.
    #[test]
    fn queue_concurrent_producer_consumer_conservation() -> Result<(), Box<dyn std::error::Error>> {
        let cap = 2048;
        let queues = Arc::new(RTSampleQueues::with_capacity(cap));
        let producers = 4u64;
        let items_per = 10_000u64;
        let barrier = Arc::new(Barrier::new((producers + 2) as usize));
        let done = Arc::new(AtomicBool::new(false));

        // Track total pushed with atomic counter
        let total_pushed = Arc::new(AtomicU64::new(0));

        let producer_handles: Vec<_> = (0..producers)
            .map(|_| {
                let q = Arc::clone(&queues);
                let b = Arc::clone(&barrier);
                let pushed = Arc::clone(&total_pushed);
                thread::spawn(move || {
                    b.wait();
                    for i in 0..items_per {
                        if q.push_jitter(i).is_ok() {
                            pushed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        let total_popped = Arc::new(AtomicU64::new(0));
        let consumer_handles: Vec<_> = (0..2u64)
            .map(|_| {
                let q = Arc::clone(&queues);
                let b = Arc::clone(&barrier);
                let popped = Arc::clone(&total_popped);
                let d = Arc::clone(&done);
                thread::spawn(move || {
                    b.wait();
                    loop {
                        match q.pop_jitter() {
                            Some(_) => {
                                popped.fetch_add(1, Ordering::Relaxed);
                            }
                            None if d.load(Ordering::Acquire) => break,
                            None => thread::yield_now(),
                        }
                    }
                })
            })
            .collect();

        for h in producer_handles {
            h.join().map_err(|_| "producer panicked")?;
        }
        done.store(true, Ordering::Release);

        for h in consumer_handles {
            h.join().map_err(|_| "consumer panicked")?;
        }

        let pushed = total_pushed.load(Ordering::Relaxed);
        let mut popped = total_popped.load(Ordering::Relaxed);

        // Drain residual
        while queues.pop_jitter().is_some() {
            popped += 1;
        }

        assert_eq!(
            popped, pushed,
            "conservation violated: pushed={pushed}, popped={popped}"
        );
        Ok(())
    }

    /// Multiple producers, single-threaded drain: no items created from thin air.
    #[test]
    fn queue_multi_producer_no_phantom_items() -> Result<(), Box<dyn std::error::Error>> {
        let queues = Arc::new(RTSampleQueues::with_capacity(100_000));
        let threads = 8u64;
        let items = 5_000u64;
        let barrier = Arc::new(Barrier::new(threads as usize));

        let handles: Vec<_> = (0..threads)
            .map(|tid| {
                let q = Arc::clone(&queues);
                let b = Arc::clone(&barrier);
                thread::spawn(move || -> u64 {
                    b.wait();
                    let mut count = 0u64;
                    for i in 0..items {
                        let val = tid * items + i;
                        if q.push_processing_time(val).is_ok() {
                            count += 1;
                        }
                    }
                    count
                })
            })
            .collect();

        let mut total_pushed = 0u64;
        for h in handles {
            total_pushed += h.join().map_err(|_| "producer panicked")?;
        }

        let drained = queues.drain_processing_time();
        assert_eq!(drained.len() as u64, total_pushed, "phantom items detected");
        Ok(())
    }

    /// Wrap-around test: push to full, drain, push again, verify FIFO.
    #[test]
    fn queue_wrap_around_fifo() -> Result<(), Box<dyn std::error::Error>> {
        let cap = 8;
        let queues = RTSampleQueues::with_capacity(cap);

        // Fill completely
        for i in 0..cap as u64 {
            queues.push_hid_latency(i).map_err(|_| "push failed")?;
        }
        assert!(queues.push_hid_latency(99).is_err());

        // Drain half
        for i in 0..4u64 {
            let v = queues.pop_hid_latency().ok_or("pop failed")?;
            assert_eq!(v, i);
        }

        // Push 4 more (these wrap around internal ring buffer)
        for i in 100..104u64 {
            queues.push_hid_latency(i).map_err(|_| "wrap push failed")?;
        }

        // Pop remaining: should be [4,5,6,7,100,101,102,103]
        let expected = [4u64, 5, 6, 7, 100, 101, 102, 103];
        for &exp in &expected {
            let v = queues.pop_hid_latency().ok_or("pop after wrap failed")?;
            assert_eq!(v, exp, "FIFO broken after wrap-around");
        }
        assert!(queues.pop_hid_latency().is_none());
        Ok(())
    }

    /// Stress: all three lanes under concurrent writes, verify no cross-lane
    /// contamination by checking counts.
    #[test]
    fn queue_concurrent_lane_isolation() -> Result<(), Box<dyn std::error::Error>> {
        let queues = Arc::new(RTSampleQueues::with_capacity(50_000));
        let items = 10_000u64;
        let barrier = Arc::new(Barrier::new(3));

        let jitter_handle = {
            let q = Arc::clone(&queues);
            let b = Arc::clone(&barrier);
            thread::spawn(move || -> u64 {
                b.wait();
                let mut count = 0u64;
                for i in 0..items {
                    if q.push_jitter(i).is_ok() {
                        count += 1;
                    }
                }
                count
            })
        };

        let proc_handle = {
            let q = Arc::clone(&queues);
            let b = Arc::clone(&barrier);
            thread::spawn(move || -> u64 {
                b.wait();
                let mut count = 0u64;
                for i in 0..items {
                    if q.push_processing_time(i).is_ok() {
                        count += 1;
                    }
                }
                count
            })
        };

        let hid_handle = {
            let q = Arc::clone(&queues);
            let b = Arc::clone(&barrier);
            thread::spawn(move || -> u64 {
                b.wait();
                let mut count = 0u64;
                for i in 0..items {
                    if q.push_hid_latency(i).is_ok() {
                        count += 1;
                    }
                }
                count
            })
        };

        let jitter_pushed = jitter_handle.join().map_err(|_| "jitter panicked")?;
        let proc_pushed = proc_handle.join().map_err(|_| "proc panicked")?;
        let hid_pushed = hid_handle.join().map_err(|_| "hid panicked")?;

        let stats = queues.stats();
        assert_eq!(stats.jitter_count, jitter_pushed as usize);
        assert_eq!(stats.processing_time_count, proc_pushed as usize);
        assert_eq!(stats.hid_latency_count, hid_pushed as usize);
        Ok(())
    }
}

// ============================================================================
// 7. Property-based tests (quickcheck) for arbitrary operation sequences
// ============================================================================

use quickcheck_macros::quickcheck;

/// For any sequence of increments, snapshot_and_reset captures all of them.
#[quickcheck]
fn prop_snapshot_reset_captures_all(ops: Vec<u16>) -> bool {
    let counters = AtomicCounters::new();
    let mut expected = 0u64;
    for &op in &ops {
        let amount = u64::from(op);
        counters.inc_tick_by(amount);
        expected += amount;
    }
    let snap = counters.snapshot_and_reset();
    snap.total_ticks == expected && counters.total_ticks() == 0
}

/// For any combination of saturated/not-saturated calls, the ratio is correct.
#[quickcheck]
fn prop_torque_saturation_ratio(saturated: u8, not_saturated: u8) -> bool {
    let counters = AtomicCounters::new();
    for _ in 0..saturated {
        counters.record_torque_saturation(true);
    }
    for _ in 0..not_saturated {
        counters.record_torque_saturation(false);
    }

    let snap = counters.snapshot();
    let total = u64::from(saturated) + u64::from(not_saturated);
    snap.torque_saturation_samples == total && snap.torque_saturation_count == u64::from(saturated)
}

/// `with_values` followed by `snapshot_and_reset` must return the original
/// and leave counters at zero.
#[quickcheck]
fn prop_with_values_then_reset_roundtrip(
    ticks: u64,
    missed: u64,
    safety: u64,
    hid_errors: u64,
) -> bool {
    let snap = CounterSnapshot {
        total_ticks: ticks,
        missed_ticks: missed,
        safety_events: safety,
        profile_switches: 0,
        telemetry_packets_received: 0,
        telemetry_packets_lost: 0,
        torque_saturation_samples: 0,
        torque_saturation_count: 0,
        hid_write_errors: hid_errors,
    };

    let counters = AtomicCounters::with_values(snap);
    let got = counters.snapshot_and_reset();
    let after = counters.snapshot();

    got == snap && after == CounterSnapshot::default()
}

/// `StreamingStats` with any sequence: mean is always between min and max (or 0).
#[quickcheck]
fn prop_streaming_stats_mean_bounded(values: Vec<u16>) -> bool {
    if values.is_empty() {
        return true;
    }
    let mut stats = StreamingStats::new();
    for &v in &values {
        stats.record(u64::from(v));
    }
    let mean = stats.mean();
    mean >= stats.min() as f64 && mean <= stats.max() as f64
}

/// Interleaved inc_tick and snapshot_and_reset: total is conserved.
#[quickcheck]
fn prop_interleaved_inc_reset_conservation(pattern: Vec<u8>) -> bool {
    let counters = AtomicCounters::new();
    let mut total_collected = 0u64;

    for &op in &pattern {
        if op % 3 == 0 {
            total_collected += counters.snapshot_and_reset().total_ticks;
        } else {
            counters.inc_tick();
        }
    }
    total_collected += counters.snapshot().total_ticks;

    let expected_ticks: u64 = pattern.iter().filter(|&&op| op % 3 != 0).count() as u64;
    total_collected == expected_ticks
}

/// Queue FIFO ordering property: for any sequence of values pushed into a
/// queue with sufficient capacity, pop order matches push order.
#[cfg(feature = "queues")]
#[quickcheck]
fn prop_queue_fifo_order(values: Vec<u16>) -> bool {
    if values.is_empty() {
        return true;
    }
    let queues = RTSampleQueues::with_capacity(values.len());
    for &v in &values {
        if queues.push_jitter(u64::from(v)).is_err() {
            return false;
        }
    }
    for &v in &values {
        match queues.pop_jitter() {
            Some(got) if got == u64::from(v) => {}
            _ => return false,
        }
    }
    queues.pop_jitter().is_none()
}

/// Queue overflow: items beyond capacity are rejected, existing items preserved.
#[cfg(feature = "queues")]
#[quickcheck]
fn prop_queue_overflow_preserves_existing(values: Vec<u8>) -> bool {
    if values.is_empty() {
        return true;
    }
    let cap = values.len().min(50);
    let queues = RTSampleQueues::with_capacity(cap);

    let mut pushed = Vec::new();
    for &v in &values {
        if queues.push_processing_time(u64::from(v)).is_ok() {
            pushed.push(u64::from(v));
        }
    }

    assert!(pushed.len() <= cap);

    // Pop and verify order
    for &expected in &pushed {
        match queues.pop_processing_time() {
            Some(got) if got == expected => {}
            _ => return false,
        }
    }
    queues.pop_processing_time().is_none()
}

// ============================================================================
// 8. Memory ordering stress tests
// ============================================================================

/// High-contention test: all threads hammer the same counter simultaneously,
/// verifying the final value is exact despite maximum contention.
#[test]
fn ordering_stress_max_contention() -> Result<(), Box<dyn std::error::Error>> {
    let threads = 32u64;
    let ops = 10_000u64;
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new(threads as usize));

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..ops {
                    c.inc_tick();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    assert_eq!(counters.total_ticks(), threads * ops);
    Ok(())
}

/// Mixed read/write stress: readers constantly take snapshots while writers
/// increment; verify the final snapshot after all writes complete is exact.
#[test]
fn ordering_stress_mixed_read_write() -> Result<(), Box<dyn std::error::Error>> {
    let writers = 8u64;
    let readers = 4u64;
    let ops = 50_000u64;
    let counters = Arc::new(AtomicCounters::new());
    let done = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new((writers + readers) as usize));

    let writer_handles: Vec<_> = (0..writers)
        .map(|_| {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..ops {
                    c.inc_tick();
                    c.inc_missed_tick();
                }
            })
        })
        .collect();

    let reader_handles: Vec<_> = (0..readers)
        .map(|_| {
            let c = Arc::clone(&counters);
            let d = Arc::clone(&done);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                let mut reads = 0u64;
                while !d.load(Ordering::Acquire) {
                    let s = c.snapshot();
                    // Bounds check
                    assert!(s.total_ticks <= writers * ops);
                    assert!(s.missed_ticks <= writers * ops);
                    reads += 1;
                }
                reads
            })
        })
        .collect();

    for h in writer_handles {
        h.join().map_err(|_| "writer panicked")?;
    }
    done.store(true, Ordering::Release);

    for h in reader_handles {
        let _reads = h.join().map_err(|_| "reader panicked")?;
    }

    let final_snap = counters.snapshot();
    assert_eq!(final_snap.total_ticks, writers * ops);
    assert_eq!(final_snap.missed_ticks, writers * ops);
    Ok(())
}

/// Concurrent percentage calculations during writes must produce finite,
/// non-negative results (no NaN/Inf from torn reads).
#[test]
fn ordering_stress_percentage_no_nan() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let done = Arc::new(AtomicBool::new(false));
    let ops = 50_000u64;

    let writer = {
        let c = Arc::clone(&counters);
        thread::spawn(move || {
            for i in 0..ops {
                c.record_torque_saturation(i % 2 == 0);
                c.inc_telemetry_received();
                if i % 10 == 0 {
                    c.inc_telemetry_lost();
                }
            }
        })
    };

    let reader = {
        let c = Arc::clone(&counters);
        let d = Arc::clone(&done);
        thread::spawn(move || {
            while !d.load(Ordering::Acquire) {
                let torque = c.torque_saturation_percent();
                let telemetry = c.telemetry_loss_percent();
                assert!(
                    torque.is_finite() && torque >= 0.0,
                    "invalid torque pct: {torque}"
                );
                assert!(
                    telemetry.is_finite() && telemetry >= 0.0,
                    "invalid telemetry pct: {telemetry}"
                );
            }
        })
    };

    writer.join().map_err(|_| "writer panicked")?;
    done.store(true, Ordering::Release);
    reader.join().map_err(|_| "reader panicked")?;
    Ok(())
}
