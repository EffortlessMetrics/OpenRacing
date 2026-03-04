//! Deep tests for the openracing-telemetry-streams crate.
//!
//! Covers stream creation/lifecycle, multi-consumer broadcasting,
//! back-pressure handling (eviction), stream filtering (processing),
//! and performance under load.

use openracing_telemetry_streams::{
    MovingAverage, RateCounter, RateLimiter, RingBuffer, StreamError, TelemetryBuffer,
};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

// ---------------------------------------------------------------------------
// Stream creation and lifecycle — TelemetryBuffer
// ---------------------------------------------------------------------------

#[test]
fn telemetry_buffer_creation_empty() -> TestResult {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::new(10);
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert!(buf.pop().is_none());
    assert!(buf.latest().is_none());
    assert!(buf.oldest().is_none());
    Ok(())
}

#[test]
fn telemetry_buffer_default_capacity() -> TestResult {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::default();
    // Default capacity is 1000; verify by filling
    for i in 0..1000 {
        buf.push(i);
    }
    assert_eq!(buf.len(), 1000);
    // Adding one more should evict oldest
    buf.push(1000);
    assert_eq!(buf.len(), 1000);
    assert_eq!(buf.oldest(), Some(1));
    Ok(())
}

#[test]
fn telemetry_buffer_push_pop_fifo_order() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    buf.push(10);
    buf.push(20);
    buf.push(30);

    assert_eq!(buf.pop(), Some(10));
    assert_eq!(buf.pop(), Some(20));
    assert_eq!(buf.pop(), Some(30));
    assert!(buf.pop().is_none());
    Ok(())
}

#[test]
fn telemetry_buffer_latest_and_oldest() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    buf.push(1);
    buf.push(2);
    buf.push(3);

    assert_eq!(buf.oldest(), Some(1));
    assert_eq!(buf.latest(), Some(3));
    Ok(())
}

#[test]
fn telemetry_buffer_clear() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    buf.push(1);
    buf.push(2);
    buf.push(3);
    assert_eq!(buf.len(), 3);

    buf.clear();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    Ok(())
}

#[test]
fn telemetry_buffer_iter_clones_contents() -> TestResult {
    let buf = TelemetryBuffer::new(5);
    buf.push(10);
    buf.push(20);
    buf.push(30);

    let items: Vec<i32> = buf.iter().collect();
    assert_eq!(items, vec![10, 20, 30]);

    // Original buffer should be unchanged
    assert_eq!(buf.len(), 3);
    Ok(())
}

#[test]
fn telemetry_buffer_push_after_pop_and_clear_cycle() -> TestResult {
    let buf = TelemetryBuffer::new(3);
    buf.push(1);
    buf.push(2);
    assert_eq!(buf.pop(), Some(1));

    buf.clear();
    assert!(buf.is_empty());

    buf.push(10);
    buf.push(20);
    assert_eq!(buf.oldest(), Some(10));
    assert_eq!(buf.latest(), Some(20));
    assert_eq!(buf.len(), 2);
    Ok(())
}

#[test]
fn telemetry_buffer_latest_oldest_single_element() -> TestResult {
    let buf = TelemetryBuffer::new(10);
    buf.push(42);
    assert_eq!(buf.latest(), Some(42));
    assert_eq!(buf.oldest(), Some(42));
    Ok(())
}

// ---------------------------------------------------------------------------
// Back-pressure handling — eviction on overflow
// ---------------------------------------------------------------------------

#[test]
fn telemetry_buffer_evicts_oldest_on_overflow() -> TestResult {
    let buf = TelemetryBuffer::new(3);
    buf.push(1);
    buf.push(2);
    buf.push(3);
    // Buffer full: [1, 2, 3]

    buf.push(4);
    // Should evict 1: [2, 3, 4]
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.oldest(), Some(2));
    assert_eq!(buf.latest(), Some(4));

    buf.push(5);
    // Should evict 2: [3, 4, 5]
    assert_eq!(buf.oldest(), Some(3));
    assert_eq!(buf.latest(), Some(5));
    Ok(())
}

#[test]
fn telemetry_buffer_capacity_one() -> TestResult {
    let buf = TelemetryBuffer::new(1);
    buf.push(42);
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.latest(), Some(42));

    buf.push(99);
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.latest(), Some(99));
    assert_eq!(buf.pop(), Some(99));
    Ok(())
}

#[test]
fn telemetry_buffer_continuous_eviction_stress() -> TestResult {
    let buf = TelemetryBuffer::new(2);
    for i in 0..1000 {
        buf.push(i);
    }
    assert_eq!(buf.len(), 2);
    assert_eq!(buf.oldest(), Some(998));
    assert_eq!(buf.latest(), Some(999));
    Ok(())
}

// ---------------------------------------------------------------------------
// RingBuffer — creation, lifecycle, overflow
// ---------------------------------------------------------------------------

#[test]
fn ring_buffer_creation_empty() -> TestResult {
    let rb: RingBuffer<i32> = RingBuffer::new(5);
    assert!(rb.is_empty());
    assert!(!rb.is_full());
    assert_eq!(rb.len(), 0);
    assert_eq!(rb.capacity(), 5);
    Ok(())
}

#[test]
fn ring_buffer_write_read_fifo() -> TestResult {
    let mut rb = RingBuffer::new(3);
    assert!(rb.write(10).is_none()); // No eviction
    assert!(rb.write(20).is_none());
    assert!(rb.write(30).is_none());

    assert!(rb.is_full());
    assert_eq!(rb.read(), Some(10));
    assert_eq!(rb.read(), Some(20));
    assert_eq!(rb.read(), Some(30));
    assert!(rb.is_empty());
    Ok(())
}

#[test]
fn ring_buffer_overflow_evicts_oldest() -> TestResult {
    let mut rb = RingBuffer::new(2);
    rb.write(1);
    rb.write(2);
    // Full: [1, 2]

    let evicted = rb.write(3);
    assert_eq!(evicted, Some(1)); // 1 was evicted
    assert_eq!(rb.len(), 2);
    assert_eq!(rb.read(), Some(2));
    assert_eq!(rb.read(), Some(3));
    Ok(())
}

#[test]
fn ring_buffer_clear_resets_state() -> TestResult {
    let mut rb = RingBuffer::new(5);
    rb.write(1);
    rb.write(2);
    rb.write(3);
    assert_eq!(rb.len(), 3);

    rb.clear();
    assert!(rb.is_empty());
    assert_eq!(rb.len(), 0);
    assert!(rb.read().is_none());
    Ok(())
}

#[test]
fn ring_buffer_read_from_empty_returns_none() -> TestResult {
    let mut rb: RingBuffer<i32> = RingBuffer::new(3);
    assert!(rb.read().is_none());
    Ok(())
}

#[test]
fn ring_buffer_wraparound_correctness() -> TestResult {
    let mut rb = RingBuffer::new(3);

    // Fill and drain twice to test wraparound
    for cycle in 0..2 {
        let base = cycle * 10;
        rb.write(base + 1);
        rb.write(base + 2);
        rb.write(base + 3);

        assert_eq!(rb.read(), Some(base + 1));
        assert_eq!(rb.read(), Some(base + 2));
        assert_eq!(rb.read(), Some(base + 3));
        assert!(rb.is_empty());
    }
    Ok(())
}

#[test]
fn ring_buffer_interleaved_write_read() -> TestResult {
    let mut rb = RingBuffer::new(4);
    rb.write(1);
    rb.write(2);
    assert_eq!(rb.read(), Some(1));
    rb.write(3);
    rb.write(4);
    assert_eq!(rb.len(), 3);
    assert_eq!(rb.read(), Some(2));
    assert_eq!(rb.read(), Some(3));
    assert_eq!(rb.read(), Some(4));
    assert!(rb.is_empty());
    Ok(())
}

#[test]
fn ring_buffer_capacity_one() -> TestResult {
    let mut rb = RingBuffer::new(1);
    assert!(rb.write(10).is_none());
    assert!(rb.is_full());
    let evicted = rb.write(20);
    assert_eq!(evicted, Some(10));
    assert_eq!(rb.read(), Some(20));
    assert!(rb.is_empty());
    Ok(())
}

// ---------------------------------------------------------------------------
// Stream filtering — MovingAverage
// ---------------------------------------------------------------------------

#[test]
fn moving_average_empty_returns_zero() -> TestResult {
    let avg = MovingAverage::new(5);
    assert_eq!(avg.average(), 0.0);
    Ok(())
}

#[test]
fn moving_average_single_value() -> TestResult {
    let mut avg = MovingAverage::new(5);
    avg.push(42.0);
    assert!((avg.average() - 42.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_within_window() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    assert!((avg.average() - 2.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_slides_window() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    // Window: [1, 2, 3], avg = 2

    avg.push(4.0);
    // Window: [2, 3, 4], avg = 3
    assert!((avg.average() - 3.0).abs() < 0.001);

    avg.push(5.0);
    // Window: [3, 4, 5], avg = 4
    assert!((avg.average() - 4.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_reset() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(10.0);
    avg.push(20.0);
    avg.push(30.0);
    assert!((avg.average() - 20.0).abs() < 0.001);

    avg.reset();
    assert_eq!(avg.average(), 0.0);

    avg.push(5.0);
    assert!((avg.average() - 5.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_negative_values() -> TestResult {
    let mut avg = MovingAverage::new(3);
    avg.push(-10.0);
    avg.push(-20.0);
    avg.push(-30.0);
    assert!((avg.average() - (-20.0)).abs() < 0.001);

    avg.push(0.0);
    // Window: [-20, -30, 0], avg ≈ -16.67
    let expected = (-20.0 + -30.0 + 0.0) / 3.0;
    assert!((avg.average() - expected).abs() < 0.01);
    Ok(())
}

#[test]
fn moving_average_window_size_one() -> TestResult {
    let mut avg = MovingAverage::new(1);
    avg.push(10.0);
    assert!((avg.average() - 10.0).abs() < 0.001);
    avg.push(20.0);
    assert!((avg.average() - 20.0).abs() < 0.001);
    avg.push(30.0);
    assert!((avg.average() - 30.0).abs() < 0.001);
    Ok(())
}

#[test]
fn moving_average_large_window_partially_filled() -> TestResult {
    let mut avg = MovingAverage::new(100);
    avg.push(50.0);
    avg.push(50.0);
    // Only 2 values in window of 100 → average still 50
    assert!((avg.average() - 50.0).abs() < 0.001);
    Ok(())
}

// ---------------------------------------------------------------------------
// Stream filtering — RateLimiter
// ---------------------------------------------------------------------------

#[test]
fn rate_limiter_first_update_always_passes() -> TestResult {
    let mut limiter = RateLimiter::new(100.0);
    assert!(limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_immediate_second_call_rejected() -> TestResult {
    let mut limiter = RateLimiter::new(100.0);
    assert!(limiter.should_update());
    assert!(!limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_reset_allows_immediate_update() -> TestResult {
    let mut limiter = RateLimiter::new(100.0);
    assert!(limiter.should_update());
    assert!(!limiter.should_update());

    limiter.reset();
    assert!(limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_after_delay_passes() -> TestResult {
    let mut limiter = RateLimiter::new(50.0); // 50 Hz = 20ms interval
    assert!(limiter.should_update());

    std::thread::sleep(Duration::from_millis(25));
    assert!(limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_low_rate_long_interval() -> TestResult {
    let mut limiter = RateLimiter::new(1.0); // 1 Hz = 1s interval
    assert!(limiter.should_update());
    // Immediately after, should be rejected
    assert!(!limiter.should_update());
    Ok(())
}

#[test]
fn rate_limiter_multiple_resets() -> TestResult {
    let mut limiter = RateLimiter::new(100.0);
    for _ in 0..5 {
        limiter.reset();
        assert!(limiter.should_update());
        assert!(!limiter.should_update());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Stream filtering — RateCounter
// ---------------------------------------------------------------------------

#[test]
fn rate_counter_starts_at_zero() -> TestResult {
    let counter = RateCounter::new(Duration::from_secs(1));
    assert_eq!(counter.rate(), 0.0);
    Ok(())
}

#[test]
fn rate_counter_increment_and_rate() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));

    for _ in 0..10 {
        counter.increment();
    }

    std::thread::sleep(Duration::from_millis(10));

    let rate = counter.rate();
    assert!(rate > 0.0, "rate should be positive after increments");
    Ok(())
}

#[test]
fn rate_counter_reset_clears_count() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    counter.increment();
    counter.increment();

    std::thread::sleep(Duration::from_millis(10));
    assert!(counter.rate() > 0.0);

    counter.reset();
    assert_eq!(counter.rate(), 0.0);
    Ok(())
}

#[test]
fn rate_counter_multiple_resets() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    for round in 0..3 {
        for _ in 0..5 {
            counter.increment();
        }
        std::thread::sleep(Duration::from_millis(10));
        assert!(
            counter.rate() > 0.0,
            "rate should be positive in round {}",
            round
        );
        counter.reset();
        assert_eq!(counter.rate(), 0.0);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[test]
fn stream_error_display() -> TestResult {
    let err = StreamError::BufferOverflow;
    assert_eq!(format!("{err}"), "Buffer overflow");

    let err = StreamError::StreamClosed;
    assert_eq!(format!("{err}"), "Stream closed");

    let err = StreamError::ProcessingError("parse failed".into());
    let msg = format!("{err}");
    assert!(msg.contains("parse failed"));
    Ok(())
}

#[test]
fn stream_error_debug_contains_variant_name() -> TestResult {
    let err = StreamError::BufferOverflow;
    assert!(format!("{err:?}").contains("BufferOverflow"));

    let err = StreamError::StreamClosed;
    assert!(format!("{err:?}").contains("StreamClosed"));

    let err = StreamError::ProcessingError("ctx".into());
    let dbg = format!("{err:?}");
    assert!(dbg.contains("ProcessingError"));
    assert!(dbg.contains("ctx"));
    Ok(())
}

#[test]
fn stream_error_is_std_error() -> TestResult {
    let err: Box<dyn std::error::Error> = Box::new(StreamError::BufferOverflow);
    assert!(!err.to_string().is_empty());
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-consumer broadcasting — thread safety of TelemetryBuffer
// ---------------------------------------------------------------------------

#[test]
fn telemetry_buffer_thread_safe_push_pop() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(100));

    let buf_writer = std::sync::Arc::clone(&buf);
    let writer = std::thread::spawn(move || {
        for i in 0..50 {
            buf_writer.push(i);
        }
    });

    let buf_reader = std::sync::Arc::clone(&buf);
    let reader = std::thread::spawn(move || {
        let mut count = 0;
        // Give writer time, then drain
        std::thread::sleep(Duration::from_millis(10));
        while buf_reader.pop().is_some() {
            count += 1;
        }
        count
    });

    writer.join().map_err(|_| "writer thread panicked")?;
    let read_count = reader.join().map_err(|_| "reader thread panicked")?;

    // We may not read all 50 due to timing, but no panics/deadlocks
    assert!(read_count <= 50);
    Ok(())
}

#[test]
fn telemetry_buffer_multiple_writers() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(200));

    let handles: Vec<_> = (0..4)
        .map(|thread_id| {
            let buf_clone = std::sync::Arc::clone(&buf);
            std::thread::spawn(move || {
                for i in 0..25 {
                    buf_clone.push(thread_id * 100 + i);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    // All 100 items should be in the buffer (capacity 200)
    assert_eq!(buf.len(), 100);
    Ok(())
}

#[test]
fn telemetry_buffer_multiple_readers_via_iter() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(100));

    // Pre-fill the buffer
    for i in 0..50 {
        buf.push(i);
    }

    // Multiple reader threads snapshot via iter() concurrently
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let buf_clone = std::sync::Arc::clone(&buf);
            std::thread::spawn(move || {
                let snapshot: Vec<i32> = buf_clone.iter().collect();
                snapshot.len()
            })
        })
        .collect();

    for r in readers {
        let count = r.join().map_err(|_| "reader thread panicked")?;
        // Each reader should see the same 50 items (iter is non-destructive)
        assert_eq!(count, 50);
    }

    // Buffer should still have all items since iter() clones
    assert_eq!(buf.len(), 50);
    Ok(())
}

#[test]
fn telemetry_buffer_concurrent_writers_and_readers() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(500));
    let num_writers = 4;
    let items_per_writer = 50;

    // Spawn writers
    let writers: Vec<_> = (0..num_writers)
        .map(|tid| {
            let buf_clone = std::sync::Arc::clone(&buf);
            std::thread::spawn(move || {
                for i in 0..items_per_writer {
                    buf_clone.push(tid * 1000 + i);
                }
            })
        })
        .collect();

    // Spawn readers that poll via latest()
    let readers: Vec<_> = (0..2)
        .map(|_| {
            let buf_clone = std::sync::Arc::clone(&buf);
            std::thread::spawn(move || {
                let mut seen_count = 0u32;
                for _ in 0..100 {
                    if buf_clone.latest().is_some() {
                        seen_count += 1;
                    }
                    std::thread::yield_now();
                }
                seen_count
            })
        })
        .collect();

    for w in writers {
        w.join().map_err(|_| "writer panicked")?;
    }
    for r in readers {
        let _seen = r.join().map_err(|_| "reader panicked")?;
        // No assertion on exact count; the test passes if no deadlock/panic occurs
    }

    // Buffer should have all items (200 < capacity 500)
    assert_eq!(buf.len(), (num_writers * items_per_writer) as usize);
    Ok(())
}

#[test]
fn telemetry_buffer_broadcast_snapshot_consistency() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(100));

    // Fill with sequential values
    for i in 0..100 {
        buf.push(i);
    }

    // Multiple threads take snapshots; each should see a consistent ordered view
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let buf_clone = std::sync::Arc::clone(&buf);
            std::thread::spawn(move || {
                let snapshot: Vec<i32> = buf_clone.iter().collect();
                // Verify ordering within snapshot
                let is_ordered = snapshot.windows(2).all(|w| w[0] < w[1]);
                (snapshot.len(), is_ordered)
            })
        })
        .collect();

    for r in readers {
        let (len, is_ordered) = r.join().map_err(|_| "reader panicked")?;
        assert_eq!(len, 100);
        assert!(is_ordered, "snapshot should be ordered");
    }
    Ok(())
}

#[test]
fn telemetry_buffer_writer_during_reader_iter() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(1000));

    // Pre-fill
    for i in 0..500 {
        buf.push(i);
    }

    let buf_reader = std::sync::Arc::clone(&buf);
    let reader = std::thread::spawn(move || {
        let snapshot: Vec<i32> = buf_reader.iter().collect();
        snapshot.len()
    });

    // Writer pushes concurrently while reader is iterating
    let buf_writer = std::sync::Arc::clone(&buf);
    let writer = std::thread::spawn(move || {
        for i in 500..600 {
            buf_writer.push(i);
        }
    });

    writer.join().map_err(|_| "writer panicked")?;
    let read_len = reader.join().map_err(|_| "reader panicked")?;

    // Reader got a snapshot of at least 500 items (the pre-filled ones)
    assert!(read_len >= 500);
    Ok(())
}

// ---------------------------------------------------------------------------
// Performance under load
// ---------------------------------------------------------------------------

#[test]
fn telemetry_buffer_high_throughput_single_thread() -> TestResult {
    let buf = TelemetryBuffer::new(10_000);
    let count = 100_000;

    let start = std::time::Instant::now();
    for i in 0..count {
        buf.push(i);
    }
    let elapsed = start.elapsed();

    // Buffer should contain only last 10_000 items
    assert_eq!(buf.len(), 10_000);
    assert_eq!(buf.oldest(), Some(count - 10_000));
    assert_eq!(buf.latest(), Some(count - 1));

    // Sanity: should complete in reasonable time (< 5s for 100k ops)
    assert!(
        elapsed < Duration::from_secs(5),
        "100k pushes took {:?}, expected < 5s",
        elapsed
    );
    Ok(())
}

#[test]
fn ring_buffer_high_throughput_write() -> TestResult {
    let mut rb = RingBuffer::new(1024);
    let count = 100_000;

    let start = std::time::Instant::now();
    for i in 0..count {
        rb.write(i);
    }
    let elapsed = start.elapsed();

    assert!(rb.is_full());
    assert_eq!(rb.len(), 1024);

    // Read back last 1024 values
    let first_expected = count - 1024;
    let first_read = rb.read().ok_or("expected value")?;
    assert_eq!(first_read, first_expected);

    assert!(
        elapsed < Duration::from_secs(5),
        "100k ring writes took {:?}, expected < 5s",
        elapsed
    );
    Ok(())
}

#[test]
fn moving_average_high_throughput() -> TestResult {
    let mut avg = MovingAverage::new(64);
    let count = 100_000;

    let start = std::time::Instant::now();
    for i in 0..count {
        avg.push(i as f32);
    }
    let elapsed = start.elapsed();

    // Average of last 64 values: (99936 + 99937 + ... + 99999) / 64
    let expected = (count as f32 - 1.0) - (64.0 - 1.0) / 2.0;
    assert!(
        (avg.average() - expected).abs() < 1.0,
        "avg={}, expected≈{}",
        avg.average(),
        expected
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "100k MA pushes took {:?}",
        elapsed
    );
    Ok(())
}

#[test]
fn telemetry_buffer_concurrent_load_stress() -> TestResult {
    let buf = std::sync::Arc::new(TelemetryBuffer::new(5000));
    let num_threads = 8;
    let items_per_thread = 10_000;

    let start = std::time::Instant::now();

    let handles: Vec<_> = (0..num_threads)
        .map(|tid| {
            let buf_clone = std::sync::Arc::clone(&buf);
            std::thread::spawn(move || {
                for i in 0..items_per_thread {
                    buf_clone.push(tid * items_per_thread + i);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().map_err(|_| "thread panicked")?;
    }

    let elapsed = start.elapsed();

    // Buffer capped at 5000
    assert_eq!(buf.len(), 5000);
    assert!(
        elapsed < Duration::from_secs(10),
        "concurrent load took {:?}",
        elapsed
    );
    Ok(())
}

#[test]
fn rate_counter_high_frequency_increments() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    let count = 100_000u64;

    for _ in 0..count {
        counter.increment();
    }

    std::thread::sleep(Duration::from_millis(10));

    let rate = counter.rate();
    assert!(
        rate > 0.0,
        "rate should be positive after {} increments",
        count
    );
    Ok(())
}

#[test]
fn ring_buffer_rapid_write_read_cycles() -> TestResult {
    let mut rb = RingBuffer::new(16);
    let cycles = 10_000;

    for cycle in 0..cycles {
        let base = cycle * 4;
        rb.write(base);
        rb.write(base + 1);
        rb.write(base + 2);
        rb.write(base + 3);

        let v0 = rb.read().ok_or("expected value")?;
        let v1 = rb.read().ok_or("expected value")?;
        assert!(v0 < v1, "values should be ordered");
    }
    Ok(())
}
