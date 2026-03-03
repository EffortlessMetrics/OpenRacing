//! Deep tests for the openracing-telemetry-streams crate.
//!
//! Covers stream creation/lifecycle, subscriber management (buffer ops),
//! back-pressure handling (eviction), and stream filtering (processing).

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

// ---------------------------------------------------------------------------
// Stream filtering — RateCounter
// ---------------------------------------------------------------------------

#[test]
fn rate_counter_starts_at_zero() -> TestResult {
    let counter = RateCounter::new(Duration::from_secs(1));
    // Rate at start is 0
    assert_eq!(counter.rate(), 0.0);
    Ok(())
}

#[test]
fn rate_counter_increment_and_rate() -> TestResult {
    let mut counter = RateCounter::new(Duration::from_secs(1));

    for _ in 0..10 {
        counter.increment();
    }

    // Sleep a tiny bit so elapsed > 0
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

// ---------------------------------------------------------------------------
// Subscriber management — thread safety of TelemetryBuffer
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

    // We may not read all 50 due to timing, but we should read at least some
    // and no panics/deadlocks should occur
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
