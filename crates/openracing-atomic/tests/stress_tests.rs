//! Stress tests for openracing-atomic.
//!
//! Tests multi-threaded concurrent access, contention patterns, ordering
//! guarantees, and atomic consistency under load.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use openracing_atomic::{AtomicCounters, CounterSnapshot, StreamingStats};

#[cfg(feature = "queues")]
use openracing_atomic::queues::RTSampleQueues;

// ── Multi-threaded concurrent access patterns ───────────────────────────────

#[test]
fn stress_concurrent_mixed_operations() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads = 8_u64;
    let ops_per_thread = 50_000_u64;
    let barrier = Arc::new(Barrier::new(num_threads as usize));

    let handles: Vec<_> = (0..num_threads)
        .map(|tid| {
            let counters = Arc::clone(&counters);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..ops_per_thread {
                    counters.inc_tick();
                    if i % 100 == 0 {
                        counters.inc_missed_tick();
                    }
                    if i % 500 == 0 {
                        counters.inc_safety_event();
                    }
                    if tid % 2 == 0 && i % 50 == 0 {
                        counters.inc_profile_switch();
                    }
                    if i % 10 == 0 {
                        counters.inc_telemetry_received();
                    }
                    if i % 200 == 0 {
                        counters.inc_telemetry_lost();
                    }
                    counters.record_torque_saturation(i % 3 == 0);
                    if i % 1000 == 0 {
                        counters.inc_hid_write_error();
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().map_err(|_| "thread panicked")?;
    }

    let snap = counters.snapshot();
    assert_eq!(snap.total_ticks, num_threads * ops_per_thread);
    assert_eq!(snap.missed_ticks, num_threads * (ops_per_thread / 100));
    assert_eq!(snap.safety_events, num_threads * (ops_per_thread / 500));
    assert_eq!(snap.torque_saturation_samples, num_threads * ops_per_thread);
    assert_eq!(snap.hid_write_errors, num_threads * (ops_per_thread / 1000));
    Ok(())
}

#[test]
fn stress_concurrent_snapshot_during_writes() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let done = Arc::new(AtomicBool::new(false));
    let ops_per_writer = 100_000_u64;
    let num_writers = 4_u64;

    // Writer threads
    let writers: Vec<_> = (0..num_writers)
        .map(|_| {
            let counters = Arc::clone(&counters);
            let done = Arc::clone(&done);
            thread::spawn(move || {
                for _ in 0..ops_per_writer {
                    counters.inc_tick();
                    counters.inc_telemetry_received();
                }
                done.store(true, Ordering::Release);
            })
        })
        .collect();

    // Reader thread that takes snapshots while writes are happening
    let counters_r = Arc::clone(&counters);
    let done_r = Arc::clone(&done);
    let reader = thread::spawn(move || {
        let mut snapshot_count = 0_u64;
        let mut last_ticks = 0_u64;

        while !done_r.load(Ordering::Acquire) {
            let snap = counters_r.snapshot();
            // Ticks should be monotonically non-decreasing (relaxed ordering
            // doesn't guarantee this across threads, but on x86 it's effectively
            // true for loads of the same variable).
            // We only assert the value is within the expected total range.
            assert!(snap.total_ticks <= num_writers * ops_per_writer);
            // Individual counter reads are independent, so we just check they're
            // within expected bounds.
            assert!(snap.telemetry_packets_received <= num_writers * ops_per_writer);
            last_ticks = snap.total_ticks;
            snapshot_count += 1;
        }

        // After all writers are done, final snapshot should be exact
        // (give a small spin for visibility)
        thread::yield_now();
        (snapshot_count, last_ticks)
    });

    for w in writers {
        w.join().map_err(|_| "writer panicked")?;
    }

    let (_snap_count, _last) = reader.join().map_err(|_| "reader panicked")?;

    let final_snap = counters.snapshot();
    assert_eq!(final_snap.total_ticks, num_writers * ops_per_writer);
    assert_eq!(
        final_snap.telemetry_packets_received,
        num_writers * ops_per_writer
    );
    Ok(())
}

// ── Atomic operations under contention ──────────────────────────────────────

#[test]
fn stress_snapshot_and_reset_under_contention() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let total_increments = Arc::new(AtomicU64::new(0));
    let total_collected = Arc::new(AtomicU64::new(0));
    let num_writers = 4_u64;
    let ops_per_writer = 50_000_u64;
    let barrier = Arc::new(Barrier::new((num_writers + 1) as usize));

    // Writer threads
    let writers: Vec<_> = (0..num_writers)
        .map(|_| {
            let counters = Arc::clone(&counters);
            let total_increments = Arc::clone(&total_increments);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..ops_per_writer {
                    counters.inc_tick();
                    total_increments.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    // Collector thread that periodically resets
    let counters_c = Arc::clone(&counters);
    let total_collected_c = Arc::clone(&total_collected);
    let barrier_c = Arc::clone(&barrier);
    let collector = thread::spawn(move || {
        barrier_c.wait();
        let mut collected = 0_u64;
        // Keep collecting until we've seen enough
        loop {
            let snap = counters_c.snapshot_and_reset();
            collected += snap.total_ticks;
            total_collected_c.store(collected, Ordering::Relaxed);

            if collected >= num_writers * ops_per_writer {
                break;
            }
            thread::yield_now();
        }
        collected
    });

    for w in writers {
        w.join().map_err(|_| "writer panicked")?;
    }

    let collected = collector.join().map_err(|_| "collector panicked")?;

    // Also pick up any remaining after writers are done
    let remaining = counters.snapshot().total_ticks;
    let total = collected + remaining;

    // No increments should be lost: every inc_tick must show up in either
    // a snapshot_and_reset or the final snapshot
    assert_eq!(total, num_writers * ops_per_writer);
    Ok(())
}

#[test]
fn stress_torque_saturation_concurrent() -> Result<(), Box<dyn std::error::Error>> {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads = 8_u64;
    let samples_per_thread = 10_000_u64;
    let barrier = Arc::new(Barrier::new(num_threads as usize));

    let handles: Vec<_> = (0..num_threads)
        .map(|tid| {
            let counters = Arc::clone(&counters);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for i in 0..samples_per_thread {
                    // Even threads always saturated, odd threads never
                    counters.record_torque_saturation(tid % 2 == 0 || i % 4 == 0);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    let snap = counters.snapshot();
    assert_eq!(
        snap.torque_saturation_samples,
        num_threads * samples_per_thread
    );
    // Saturation count should be at least the contribution from even threads
    let even_threads = num_threads / 2;
    assert!(snap.torque_saturation_count >= even_threads * samples_per_thread);
    Ok(())
}

// ── Ordering guarantee tests ────────────────────────────────────────────────

#[test]
fn ordering_relaxed_counter_values_are_non_negative() -> Result<(), Box<dyn std::error::Error>> {
    // With Relaxed ordering, individual counter reads should always reflect
    // non-negative values (they start at 0 and only increment)
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new(5));

    let writers: Vec<_> = (0..4)
        .map(|_| {
            let c = Arc::clone(&counters);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                for _ in 0..100_000 {
                    c.inc_tick();
                    c.inc_missed_tick();
                }
            })
        })
        .collect();

    {
        let c = Arc::clone(&counters);
        let b = Arc::clone(&barrier);
        let reader = thread::spawn(move || {
            b.wait();
            for _ in 0..10_000 {
                let total = c.total_ticks();
                let missed = c.missed_ticks();
                // Each counter is independently valid (always >= 0, which is
                // trivially true for u64, but total should be <= expected max)
                assert!(total <= 4 * 100_000);
                assert!(missed <= 4 * 100_000);
            }
        });

        for w in writers {
            w.join().map_err(|_| "writer panicked")?;
        }
        reader.join().map_err(|_| "reader panicked")?;
    }
    Ok(())
}

#[test]
fn ordering_snapshot_fields_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that snapshot values never exceed the theoretical maximum
    let counters = Arc::new(AtomicCounters::new());
    let barrier = Arc::new(Barrier::new(3));
    let ops = 20_000_u64;

    let w1 = {
        let c = Arc::clone(&counters);
        let b = Arc::clone(&barrier);
        thread::spawn(move || {
            b.wait();
            for _ in 0..ops {
                c.inc_tick();
                c.inc_safety_event();
            }
        })
    };

    let w2 = {
        let c = Arc::clone(&counters);
        let b = Arc::clone(&barrier);
        thread::spawn(move || {
            b.wait();
            for _ in 0..ops {
                c.inc_tick();
                c.inc_profile_switch();
            }
        })
    };

    // Take many snapshots during the writes
    let reader = {
        let c = Arc::clone(&counters);
        thread::spawn(move || {
            for _ in 0..5_000 {
                let s = c.snapshot();
                // Each individual field should be within bounds
                assert!(s.total_ticks <= 2 * ops);
                assert!(s.safety_events <= ops);
                assert!(s.profile_switches <= ops);
            }
        })
    };

    barrier.wait();
    w1.join().map_err(|_| "w1 panicked")?;
    w2.join().map_err(|_| "w2 panicked")?;
    reader.join().map_err(|_| "reader panicked")?;
    Ok(())
}

// ── Property tests: atomic consistency ──────────────────────────────────────

use quickcheck_macros::quickcheck;

#[quickcheck]
fn prop_concurrent_inc_by_matches_sequential(amounts: Vec<u16>) -> bool {
    let counters = AtomicCounters::new();
    let expected: u64 = amounts.iter().map(|a| u64::from(*a)).sum();

    for a in &amounts {
        counters.inc_tick_by(u64::from(*a));
    }

    counters.total_ticks() == expected
}

#[quickcheck]
fn prop_with_values_roundtrips(
    ticks: u64,
    missed: u64,
    safety: u64,
    switches: u64,
    recv: u64,
    lost: u64,
    sat_samples: u64,
    sat_count: u64,
) -> bool {
    let snap = CounterSnapshot {
        total_ticks: ticks,
        missed_ticks: missed,
        safety_events: safety,
        profile_switches: switches,
        telemetry_packets_received: recv,
        telemetry_packets_lost: lost,
        torque_saturation_samples: sat_samples,
        torque_saturation_count: sat_count,
        hid_write_errors: 0,
    };

    let counters = AtomicCounters::with_values(snap);
    let result = counters.snapshot();
    result == snap
}

#[quickcheck]
fn prop_reset_always_zeroes(ticks: u64, missed: u64) -> bool {
    let counters = AtomicCounters::new();
    counters.inc_tick_by(ticks);
    counters.inc_missed_tick_by(missed);

    counters.reset();
    let snap = counters.snapshot();

    snap == CounterSnapshot::default()
}

#[quickcheck]
fn prop_streaming_stats_min_max_consistent(values: Vec<u64>) -> bool {
    if values.is_empty() {
        return true;
    }
    let mut stats = StreamingStats::new();
    for v in &values {
        stats.record(*v);
    }

    stats.count() == values.len() as u64
        && stats.min() <= stats.max()
        && values.contains(&stats.min())
        && values.contains(&stats.max())
}

#[quickcheck]
fn prop_streaming_stats_mean_is_bounded(values: Vec<u16>) -> bool {
    if values.is_empty() {
        return true;
    }
    let mut stats = StreamingStats::new();
    for v in &values {
        stats.record(u64::from(*v));
    }

    let mean = stats.mean();
    mean >= stats.min() as f64 && mean <= stats.max() as f64
}

// ── Queue stress tests ──────────────────────────────────────────────────────

#[cfg(feature = "queues")]
#[test]
fn stress_queue_concurrent_push_pop() -> Result<(), Box<dyn std::error::Error>> {
    let queues = Arc::new(RTSampleQueues::with_capacity(1_000));
    let total_items = 50_000_u64;
    let barrier = Arc::new(Barrier::new(2));

    let producer = {
        let q = Arc::clone(&queues);
        let b = Arc::clone(&barrier);
        thread::spawn(move || {
            b.wait();
            let mut pushed = 0_u64;
            for i in 0..total_items {
                if q.push_jitter(i).is_ok() {
                    pushed += 1;
                }
            }
            pushed
        })
    };

    let consumer = {
        let q = Arc::clone(&queues);
        let b = Arc::clone(&barrier);
        thread::spawn(move || {
            b.wait();
            let mut popped = 0_u64;
            let mut spins = 0_u32;
            loop {
                match q.pop_jitter() {
                    Some(_) => {
                        popped += 1;
                        spins = 0;
                    }
                    None => {
                        spins += 1;
                        if spins > 100_000 {
                            break;
                        }
                        thread::yield_now();
                    }
                }
            }
            popped
        })
    };

    let pushed = producer.join().map_err(|_| "producer panicked")?;
    let popped = consumer.join().map_err(|_| "consumer panicked")?;

    // Drain any remaining items
    let mut remaining = 0_u64;
    while queues.pop_jitter().is_some() {
        remaining += 1;
    }

    // No items should be created from thin air
    assert_eq!(popped + remaining, pushed);
    // Some items may have been dropped due to capacity limits
    assert!(pushed <= total_items);
    Ok(())
}

#[cfg(feature = "queues")]
#[test]
fn stress_queue_overflow_drops_gracefully() -> Result<(), Box<dyn std::error::Error>> {
    let capacity = 100;
    let queues = RTSampleQueues::with_capacity(capacity);

    // Fill the queue beyond capacity
    let mut pushed_ok = 0_usize;
    let mut pushed_err = 0_usize;
    for i in 0..1_000_u64 {
        match queues.push_jitter(i) {
            Ok(()) => pushed_ok += 1,
            Err(_) => pushed_err += 1,
        }
    }

    assert_eq!(pushed_ok, capacity);
    assert_eq!(pushed_err, 1_000 - capacity);

    // All pushed items should be retrievable
    let drained = queues.drain_jitter();
    assert_eq!(drained.len(), capacity);

    // Values should be the first `capacity` items (FIFO)
    for (i, val) in drained.iter().enumerate() {
        assert_eq!(*val, i as u64);
    }
    Ok(())
}

#[cfg(feature = "queues")]
#[test]
fn stress_queue_multi_producer() -> Result<(), Box<dyn std::error::Error>> {
    let queues = Arc::new(RTSampleQueues::with_capacity(100_000));
    let producers = 4_u64;
    let items_per_producer = 10_000_u64;
    let barrier = Arc::new(Barrier::new(producers as usize));

    let handles: Vec<_> = (0..producers)
        .map(|_| {
            let q = Arc::clone(&queues);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                let mut ok = 0_u64;
                for i in 0..items_per_producer {
                    if q.push_processing_time(i).is_ok() {
                        ok += 1;
                    }
                }
                ok
            })
        })
        .collect();

    let mut total_pushed = 0_u64;
    for h in handles {
        total_pushed += h.join().map_err(|_| "producer panicked")?;
    }

    let drained = queues.drain_processing_time();
    assert_eq!(drained.len() as u64, total_pushed);
    Ok(())
}
