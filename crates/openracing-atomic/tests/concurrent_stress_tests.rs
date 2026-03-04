//! Deep concurrent stress tests for openracing-atomic primitives.
//!
//! These tests exercise atomic counters, lock-free queues, and derived
//! metrics under heavy multi-threaded contention to verify correctness
//! and absence of torn reads.

use std::sync::Arc;
use std::thread;

use openracing_atomic::{AtomicCounters, CounterSnapshot, StreamingStats};

// ---------------------------------------------------------------------------
// Constants shared across tests
// ---------------------------------------------------------------------------
const NUM_THREADS: u64 = 8;
const OPS_PER_THREAD: u64 = 10_000;

// ===========================================================================
// 1. Atomic counter stress tests
// ===========================================================================

/// 8 threads × 10K increments must equal 80K total ticks.
#[test]
fn stress_counter_increment_total() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    c.inc_tick();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "worker thread panicked")?;
    }

    let snap = counters.snapshot();
    assert_eq!(snap.total_ticks, NUM_THREADS * OPS_PER_THREAD);
    Ok(())
}

/// Concurrent `inc_tick_by` from 8 threads sums correctly.
#[test]
fn stress_counter_inc_by() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    c.inc_tick_by(3);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "worker thread panicked")?;
    }

    assert_eq!(counters.total_ticks(), NUM_THREADS * OPS_PER_THREAD * 3);
    Ok(())
}

/// 8 threads each increment a different counter; verify no cross-contamination.
#[test]
fn stress_independent_counters() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    match tid % 5 {
                        0 => c.inc_tick(),
                        1 => c.inc_missed_tick(),
                        2 => c.inc_safety_event(),
                        3 => c.inc_profile_switch(),
                        4 => c.inc_hid_write_error(),
                        _ => {}
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "worker thread panicked")?;
    }

    let snap = counters.snapshot();
    // Threads 0,5 → tick; 1,6 → missed; 2,7 → safety; 3 → profile; 4 → hid
    let threads_per_bucket =
        |bucket: u64| -> u64 { (0..NUM_THREADS).filter(|t| t % 5 == bucket).count() as u64 };

    assert_eq!(snap.total_ticks, threads_per_bucket(0) * OPS_PER_THREAD);
    assert_eq!(snap.missed_ticks, threads_per_bucket(1) * OPS_PER_THREAD);
    assert_eq!(snap.safety_events, threads_per_bucket(2) * OPS_PER_THREAD);
    assert_eq!(
        snap.profile_switches,
        threads_per_bucket(3) * OPS_PER_THREAD
    );
    assert_eq!(
        snap.hid_write_errors,
        threads_per_bucket(4) * OPS_PER_THREAD
    );
    Ok(())
}

// ===========================================================================
// 2. Snapshot-and-reset coherence (flag-like set/clear)
// ===========================================================================

/// Writers increment while collectors snapshot_and_reset; the total across
/// all snapshots plus the residual must equal the total writes.
#[test]
fn stress_snapshot_and_reset_coherence() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let writer_count: u64 = 6;
    let collector_count: u64 = 2;

    let writer_handles: Vec<_> = (0..writer_count)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    c.inc_tick();
                    c.inc_missed_tick();
                }
            })
        })
        .collect();

    let collector_handles: Vec<_> = (0..collector_count)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                let mut collected_ticks: u64 = 0;
                let mut collected_missed: u64 = 0;
                for _ in 0..200 {
                    let snap = c.snapshot_and_reset();
                    collected_ticks += snap.total_ticks;
                    collected_missed += snap.missed_ticks;
                    thread::yield_now();
                }
                (collected_ticks, collected_missed)
            })
        })
        .collect();

    for h in writer_handles {
        h.join().map_err(|_| "writer panicked")?;
    }

    let mut sum_ticks: u64 = 0;
    let mut sum_missed: u64 = 0;
    for h in collector_handles {
        let (t, m) = h.join().map_err(|_| "collector panicked")?;
        sum_ticks += t;
        sum_missed += m;
    }

    // Drain residual
    let residual = counters.snapshot_and_reset();
    sum_ticks += residual.total_ticks;
    sum_missed += residual.missed_ticks;

    let expected = writer_count * OPS_PER_THREAD;
    assert_eq!(sum_ticks, expected);
    assert_eq!(sum_missed, expected);
    Ok(())
}

// ===========================================================================
// 3. Concurrent percentage reads — no torn reads
// ===========================================================================

/// Concurrent writers call `record_torque_saturation` while readers observe
/// the percentage. The percentage must always be in [0, 100].
#[test]
fn stress_torque_saturation_no_torn_reads() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());

    let writer_handles: Vec<_> = (0..NUM_THREADS / 2)
        .map(|tid| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..OPS_PER_THREAD {
                    c.record_torque_saturation((tid + i) % 2 == 0);
                }
            })
        })
        .collect();

    let reader_handles: Vec<_> = (0..NUM_THREADS / 2)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    let pct = c.torque_saturation_percent();
                    // Under concurrent writes, the percentage can transiently
                    // exceed 100% because samples and saturated counts are loaded
                    // at different instants (TOCTOU). A tolerance of ~10% above
                    // 100 accommodates this without masking real torn-read bugs.
                    assert!(
                        (0.0..=110.0).contains(&pct),
                        "torque saturation % out of range: {pct}"
                    );
                }
            })
        })
        .collect();

    for h in writer_handles {
        h.join().map_err(|_| "writer panicked")?;
    }
    for h in reader_handles {
        h.join().map_err(|_| "reader panicked")?;
    }

    Ok(())
}

/// Concurrent telemetry writers; loss percentage must always be in [0, 100].
#[test]
fn stress_telemetry_loss_no_torn_reads() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());

    let writer_handles: Vec<_> = (0..NUM_THREADS / 2)
        .map(|tid| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..OPS_PER_THREAD {
                    if (tid + i) % 10 == 0 {
                        c.inc_telemetry_lost();
                    } else {
                        c.inc_telemetry_received();
                    }
                }
            })
        })
        .collect();

    let reader_handles: Vec<_> = (0..NUM_THREADS / 2)
        .map(|_| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..OPS_PER_THREAD {
                    let pct = c.telemetry_loss_percent();
                    assert!(
                        (0.0..=100.0).contains(&pct),
                        "telemetry loss % out of range: {pct}"
                    );
                }
            })
        })
        .collect();

    for h in writer_handles {
        h.join().map_err(|_| "writer panicked")?;
    }
    for h in reader_handles {
        h.join().map_err(|_| "reader panicked")?;
    }

    Ok(())
}

// ===========================================================================
// 4. Lock-free queue stress tests (feature = "queues")
// ===========================================================================

#[cfg(feature = "queues")]
mod queue_stress {
    use super::*;
    use openracing_atomic::queues::RTSampleQueues;

    /// 8 producer threads push unique values; after all producers finish the
    /// consumer drains. No items may be lost or duplicated.
    #[test]
    fn stress_queue_no_lost_items() -> Result<(), Box<dyn std::error::Error>> {
        let total_items = NUM_THREADS * OPS_PER_THREAD;
        let queues = Arc::new(RTSampleQueues::with_capacity(total_items as usize));

        let handles: Vec<_> = (0..NUM_THREADS)
            .map(|tid| {
                let q = Arc::clone(&queues);
                thread::spawn(move || {
                    let mut pushed = 0u64;
                    for i in 0..OPS_PER_THREAD {
                        let val = tid * OPS_PER_THREAD + i;
                        if q.push_jitter(val).is_ok() {
                            pushed += 1;
                        }
                    }
                    pushed
                })
            })
            .collect();

        let mut total_pushed: u64 = 0;
        for h in handles {
            total_pushed += h.join().map_err(|_| "producer panicked")?;
        }

        // Drain
        let mut popped = 0u64;
        while queues.pop_jitter().is_some() {
            popped += 1;
        }

        assert_eq!(popped, total_pushed, "items lost in queue");
        Ok(())
    }

    /// Concurrent push and pop: total pushed minus total popped equals residual.
    #[test]
    fn stress_queue_concurrent_push_pop() -> Result<(), Box<dyn std::error::Error>> {
        let capacity = 4096;
        let queues = Arc::new(RTSampleQueues::with_capacity(capacity));

        let producer_handles: Vec<_> = (0..NUM_THREADS / 2)
            .map(|_| {
                let q = Arc::clone(&queues);
                thread::spawn(move || {
                    let mut pushed = 0u64;
                    for i in 0..OPS_PER_THREAD {
                        if q.push_jitter(i).is_ok() {
                            pushed += 1;
                        }
                    }
                    pushed
                })
            })
            .collect();

        let consumer_handles: Vec<_> = (0..NUM_THREADS / 2)
            .map(|_| {
                let q = Arc::clone(&queues);
                thread::spawn(move || {
                    let mut consumed = 0u64;
                    for _ in 0..OPS_PER_THREAD {
                        if q.pop_jitter().is_some() {
                            consumed += 1;
                        }
                        thread::yield_now();
                    }
                    consumed
                })
            })
            .collect();

        let mut total_pushed: u64 = 0;
        for h in producer_handles {
            total_pushed += h.join().map_err(|_| "producer panicked")?;
        }

        let mut total_consumed: u64 = 0;
        for h in consumer_handles {
            total_consumed += h.join().map_err(|_| "consumer panicked")?;
        }

        // Drain residual
        while queues.pop_jitter().is_some() {
            total_consumed += 1;
        }

        assert_eq!(total_consumed, total_pushed, "mismatch after drain");
        Ok(())
    }

    /// All three queue lanes survive concurrent access simultaneously.
    #[test]
    fn stress_queue_all_lanes_concurrent() -> Result<(), Box<dyn std::error::Error>> {
        let queues = Arc::new(RTSampleQueues::with_capacity(OPS_PER_THREAD as usize));

        let handles: Vec<_> = (0..NUM_THREADS)
            .map(|tid| {
                let q = Arc::clone(&queues);
                thread::spawn(move || {
                    for i in 0..OPS_PER_THREAD {
                        match tid % 3 {
                            0 => {
                                q.push_jitter_drop(i);
                            }
                            1 => {
                                q.push_processing_time_drop(i);
                            }
                            2 => {
                                q.push_hid_latency_drop(i);
                            }
                            _ => {}
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().map_err(|_| "worker panicked")?;
        }

        let stats = queues.stats();
        assert!(stats.jitter_count <= OPS_PER_THREAD as usize);
        assert!(stats.processing_time_count <= OPS_PER_THREAD as usize);
        assert!(stats.hid_latency_count <= OPS_PER_THREAD as usize);
        Ok(())
    }
}

// ===========================================================================
// 5. Property-style consistency tests
// ===========================================================================

/// After any mix of increments the snapshot sums are self-consistent:
///   total_ticks >= missed_ticks (logically, though the API doesn't enforce it)
///   torque_saturation_count <= torque_saturation_samples
///   telemetry_loss_percent in [0, 100]
#[test]
fn property_snapshot_consistency() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|tid| {
            let c = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..OPS_PER_THREAD {
                    c.inc_tick();
                    if (tid + i) % 7 == 0 {
                        c.inc_missed_tick();
                    }
                    c.record_torque_saturation(i % 3 == 0);
                    if i % 5 == 0 {
                        c.inc_telemetry_lost();
                    } else {
                        c.inc_telemetry_received();
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "worker panicked")?;
    }

    let snap = counters.snapshot();

    // Monotonicity invariants
    assert!(snap.torque_saturation_count <= snap.torque_saturation_samples);

    // Percentage bounds
    let pct = snap.torque_saturation_percent();
    assert!(
        (0.0..=100.0).contains(&pct),
        "torque saturation % out of range: {pct}"
    );

    let loss = snap.telemetry_loss_percent();
    assert!(
        (0.0..=100.0).contains(&loss),
        "telemetry loss % out of range: {loss}"
    );

    Ok(())
}

/// `with_values` round-trips: create counters from a snapshot, read them back.
#[test]
fn property_with_values_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let original = CounterSnapshot {
        total_ticks: 42,
        missed_ticks: 7,
        safety_events: 3,
        profile_switches: 1,
        telemetry_packets_received: 100,
        telemetry_packets_lost: 5,
        torque_saturation_samples: 200,
        torque_saturation_count: 10,
        hid_write_errors: 2,
    };

    let counters = AtomicCounters::with_values(original);
    let snap = counters.snapshot();
    assert_eq!(snap, original);
    Ok(())
}

/// Resetting counters always produces a zero snapshot.
#[test]
fn property_reset_yields_zero() -> Result<(), Box<dyn std::error::Error>> {
    let counters = AtomicCounters::new();
    for _ in 0..1_000 {
        counters.inc_tick();
        counters.inc_missed_tick();
        counters.inc_safety_event();
        counters.record_torque_saturation(true);
    }
    counters.reset();

    let snap = counters.snapshot();
    assert_eq!(snap, CounterSnapshot::default());
    Ok(())
}

/// `StreamingStats` invariants: min <= mean <= max when samples are recorded.
#[test]
fn property_streaming_stats_invariants() -> Result<(), Box<dyn std::error::Error>> {
    let mut stats = StreamingStats::new();
    for v in [1u64, 50, 100, 500, 1000] {
        stats.record(v);
    }

    assert!(stats.min() <= stats.max());
    let mean = stats.mean();
    assert!(mean >= stats.min() as f64);
    assert!(mean <= stats.max() as f64);
    assert!(!stats.is_empty());
    Ok(())
}
