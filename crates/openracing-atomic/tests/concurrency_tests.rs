//! Concurrency tests for openracing-atomic.
//!
//! These tests verify thread-safety properties of atomic operations.

use std::sync::Arc;
use std::thread;

use openracing_atomic::AtomicCounters;

#[test]
fn test_concurrent_increment_single_counter() {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads: u64 = 8;
    let increments_per_thread: u64 = 10_000;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    counters.inc_tick();
                }
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let expected = num_threads * increments_per_thread;
    assert_eq!(counters.total_ticks(), expected);
}

#[test]
fn test_concurrent_increment_all_counters() {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads: u64 = 4;
    let iterations_per_thread: u64 = 5_000;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..iterations_per_thread {
                    counters.inc_tick();
                    if i % 10 == 0 {
                        counters.inc_missed_tick();
                    }
                    if thread_id % 2 == 0 {
                        counters.inc_safety_event();
                    }
                    counters.record_torque_saturation(i % 3 == 0);
                    counters.inc_hid_write_error();
                }
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let snapshot = counters.snapshot();

    let total_expected = num_threads * iterations_per_thread;
    assert_eq!(snapshot.total_ticks, total_expected);

    let missed_expected = total_expected / 10;
    assert_eq!(snapshot.missed_ticks, missed_expected);

    let safety_expected = total_expected / 2;
    assert_eq!(snapshot.safety_events, safety_expected);

    let hid_errors_expected = total_expected;
    assert_eq!(snapshot.hid_write_errors, hid_errors_expected);

    assert_eq!(
        snapshot.torque_saturation_samples,
        num_threads * iterations_per_thread
    );
}

#[test]
fn test_concurrent_snapshot_and_increment() {
    let counters = Arc::new(AtomicCounters::new());
    let num_writer_threads: u64 = 4;
    let num_reader_threads: u64 = 2;
    let iterations_per_thread: u64 = 10_000;

    let writer_handles: Vec<_> = (0..num_writer_threads)
        .map(|_| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..iterations_per_thread {
                    counters.inc_tick();
                    if i % 100 == 0 {
                        counters.inc_missed_tick();
                    }
                }
            })
        })
        .collect();

    let reader_handles: Vec<_> = (0..num_reader_threads)
        .map(|_| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..100 {
                    let _ = counters.snapshot();
                    thread::yield_now();
                }
            })
        })
        .collect();

    for handle in writer_handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    for handle in reader_handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let final_snapshot = counters.snapshot();
    let total_expected = num_writer_threads * iterations_per_thread;
    assert_eq!(final_snapshot.total_ticks, total_expected);
}

#[test]
fn test_concurrent_snapshot_and_reset() {
    let counters = Arc::new(AtomicCounters::new());
    let num_writer_threads: u64 = 4;
    let num_collector_threads: u64 = 2;
    let iterations_per_thread: u64 = 5_000;

    let writer_handles: Vec<_> = (0..num_writer_threads)
        .map(|_| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..iterations_per_thread {
                    counters.inc_tick();
                }
            })
        })
        .collect();

    let collector_handles: Vec<_> = (0..num_collector_threads)
        .map(|_| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                let mut total_collected = 0u64;
                for _ in 0..50 {
                    let snapshot = counters.snapshot_and_reset();
                    total_collected += snapshot.total_ticks;
                    thread::yield_now();
                }
                total_collected
            })
        })
        .collect();

    for handle in writer_handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let collected: u64 = collector_handles
        .into_iter()
        .map(|h| match h.join() {
            Ok(val) => val,
            Err(_) => panic!("collector thread panicked"),
        })
        .sum();

    let remaining = counters.total_ticks();

    let total_expected = num_writer_threads * iterations_per_thread;
    assert_eq!(collected + remaining, total_expected);
}

#[test]
fn test_concurrent_torque_saturation() {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads: u64 = 4;
    let samples_per_thread: u64 = 10_000;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..samples_per_thread {
                    let is_saturated = (thread_id + i) % 4 == 0;
                    counters.record_torque_saturation(is_saturated);
                }
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let snapshot = counters.snapshot();
    let expected_samples = num_threads * samples_per_thread;
    assert_eq!(snapshot.torque_saturation_samples, expected_samples);

    let expected_saturated = expected_samples / 4;
    let tolerance = expected_saturated / 10;
    assert!(
        snapshot.torque_saturation_count >= expected_saturated - tolerance
            && snapshot.torque_saturation_count <= expected_saturated + tolerance
    );
}

#[test]
fn test_concurrent_telemetry_tracking() {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads: u64 = 8;
    let packets_per_thread: u64 = 5_000;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for i in 0..packets_per_thread {
                    if (thread_id + i) % 20 == 0 {
                        counters.inc_telemetry_lost();
                    } else {
                        counters.inc_telemetry_received();
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let pct = counters.telemetry_loss_percent();
    assert!((0.0..=100.0).contains(&pct));

    let expected_loss = (num_threads * packets_per_thread) / 20;
    let expected_pct = (expected_loss as f32 / (num_threads * packets_per_thread) as f32) * 100.0;
    let tolerance = 1.0;

    assert!((pct - expected_pct).abs() < tolerance);
}

#[test]
fn test_stress_counter_overflow() {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads: u64 = 8;
    let iterations = 100_000;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..iterations {
                    counters.inc_tick_by(u64::MAX / 100);
                }
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let _ = counters.total_ticks();
}

#[test]
fn test_concurrent_independent_counters() {
    let counters = Arc::new(AtomicCounters::new());
    let num_threads: u64 = 4;
    let iterations: u64 = 10_000;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let counters = Arc::clone(&counters);
            thread::spawn(move || {
                for _ in 0..iterations {
                    match thread_id {
                        0 => counters.inc_tick(),
                        1 => counters.inc_missed_tick(),
                        2 => counters.inc_safety_event(),
                        3 => counters.inc_profile_switch(),
                        _ => unreachable!(),
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        assert!(handle.join().is_ok(), "thread panicked unexpectedly");
    }

    let snapshot = counters.snapshot();
    assert_eq!(snapshot.total_ticks, iterations);
    assert_eq!(snapshot.missed_ticks, iterations);
    assert_eq!(snapshot.safety_events, iterations);
    assert_eq!(snapshot.profile_switches, iterations);
}

#[cfg(feature = "queues")]
mod queue_tests {
    use openracing_atomic::queues::RTSampleQueues;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_queue_push_pop() {
        let queues = Arc::new(RTSampleQueues::with_capacity(1000));
        let num_producers: u64 = 4;
        let num_consumers: u64 = 2;
        let samples_per_producer: u64 = 1000;

        let producer_handles: Vec<_> = (0..num_producers)
            .map(|producer_id| {
                let queues = Arc::clone(&queues);
                thread::spawn(move || {
                    for i in 0..samples_per_producer {
                        let value = producer_id * 10000 + i;
                        queues.push_jitter_drop(value);
                    }
                })
            })
            .collect();

        let consumer_handles: Vec<_> = (0..num_consumers)
            .map(|_| {
                let queues = Arc::clone(&queues);
                thread::spawn(move || {
                    let mut count = 0u64;
                    for _ in 0..samples_per_producer {
                        while queues.pop_jitter().is_some() {
                            count += 1;
                        }
                        thread::yield_now();
                    }
                    count
                })
            })
            .collect();

        for handle in producer_handles {
            assert!(handle.join().is_ok(), "thread panicked unexpectedly");
        }

        let consumed: u64 = consumer_handles
            .into_iter()
            .map(|h| match h.join() {
                Ok(val) => val,
                Err(_) => panic!("consumer thread panicked"),
            })
            .sum();

        let remaining = queues.jitter_len() as u64;

        assert!(consumed + remaining > 0);
    }

    #[test]
    fn test_queue_overflow_drops_samples() {
        let capacity = 100;
        let queues = Arc::new(RTSampleQueues::with_capacity(capacity));
        let num_threads: u64 = 4;
        let samples_per_thread: u64 = 1000;

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let queues = Arc::clone(&queues);
                thread::spawn(move || {
                    for i in 0..samples_per_thread {
                        queues.push_jitter_drop(i);
                    }
                })
            })
            .collect();

        for handle in handles {
            assert!(handle.join().is_ok(), "thread panicked unexpectedly");
        }

        let queue_len = queues.jitter_len();
        assert!(queue_len <= capacity);
    }
}
