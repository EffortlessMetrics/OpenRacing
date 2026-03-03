use openracing_telemetry_streams::*;

use std::time::Duration;

// ── TelemetryBuffer: Creation and Management ────────────────────────────────

#[test]
fn telemetry_buffer_new_is_empty() {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::new(10);
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
}

#[test]
fn telemetry_buffer_default_is_empty() {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::default();
    assert!(buf.is_empty());
}

#[test]
fn telemetry_buffer_push_pop_fifo() {
    let buf = TelemetryBuffer::new(10);
    buf.push(1);
    buf.push(2);
    buf.push(3);
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.pop(), Some(1));
    assert_eq!(buf.pop(), Some(2));
    assert_eq!(buf.pop(), Some(3));
    assert_eq!(buf.pop(), None);
}

#[test]
fn telemetry_buffer_latest_and_oldest() {
    let buf = TelemetryBuffer::new(5);
    buf.push(10);
    buf.push(20);
    buf.push(30);
    assert_eq!(buf.oldest(), Some(10));
    assert_eq!(buf.latest(), Some(30));
}

#[test]
fn telemetry_buffer_latest_oldest_on_empty() {
    let buf: TelemetryBuffer<i32> = TelemetryBuffer::new(5);
    assert_eq!(buf.latest(), None);
    assert_eq!(buf.oldest(), None);
}

#[test]
fn telemetry_buffer_clear_empties_buffer() {
    let buf = TelemetryBuffer::new(5);
    buf.push(1);
    buf.push(2);
    buf.clear();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.pop(), None);
}

#[test]
fn telemetry_buffer_iter_collects_all() {
    let buf = TelemetryBuffer::new(5);
    buf.push(10);
    buf.push(20);
    buf.push(30);
    let items: Vec<i32> = buf.iter().collect();
    assert_eq!(items, vec![10, 20, 30]);
}

// ── TelemetryBuffer: Backpressure (eviction) ────────────────────────────────

#[test]
fn telemetry_buffer_evicts_oldest_when_full() {
    let buf = TelemetryBuffer::new(3);
    buf.push(1);
    buf.push(2);
    buf.push(3);
    buf.push(4); // evicts 1
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.oldest(), Some(2));
    assert_eq!(buf.latest(), Some(4));
}

#[test]
fn telemetry_buffer_continuous_eviction() {
    let buf = TelemetryBuffer::new(2);
    for i in 0..100 {
        buf.push(i);
    }
    assert_eq!(buf.len(), 2);
    assert_eq!(buf.oldest(), Some(98));
    assert_eq!(buf.latest(), Some(99));
}

#[test]
fn telemetry_buffer_size_one_always_has_latest() {
    let buf = TelemetryBuffer::new(1);
    buf.push(42);
    assert_eq!(buf.latest(), Some(42));
    buf.push(99);
    assert_eq!(buf.len(), 1);
    assert_eq!(buf.latest(), Some(99));
}

// ── RingBuffer: Data Flow ───────────────────────────────────────────────────

#[test]
fn ring_buffer_initial_state() {
    let buf: RingBuffer<i32> = RingBuffer::new(5);
    assert!(buf.is_empty());
    assert!(!buf.is_full());
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.capacity(), 5);
}

#[test]
fn ring_buffer_write_and_read() {
    let mut buf = RingBuffer::new(5);
    assert_eq!(buf.write(10), None);
    assert_eq!(buf.write(20), None);
    assert_eq!(buf.read(), Some(10));
    assert_eq!(buf.read(), Some(20));
    assert_eq!(buf.read(), None);
}

#[test]
fn ring_buffer_full_state() {
    let mut buf = RingBuffer::new(3);
    buf.write(1);
    buf.write(2);
    buf.write(3);
    assert!(buf.is_full());
    assert_eq!(buf.len(), 3);
}

#[test]
fn ring_buffer_overflow_evicts_oldest() {
    let mut buf = RingBuffer::new(3);
    buf.write(1);
    buf.write(2);
    buf.write(3);
    let evicted = buf.write(4);
    assert_eq!(evicted, Some(1));
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.read(), Some(2));
    assert_eq!(buf.read(), Some(3));
    assert_eq!(buf.read(), Some(4));
}

#[test]
fn ring_buffer_clear_resets() {
    let mut buf = RingBuffer::new(5);
    buf.write(1);
    buf.write(2);
    buf.clear();
    assert!(buf.is_empty());
    assert_eq!(buf.read(), None);
}

#[test]
fn ring_buffer_wrap_around_multiple_cycles() {
    let mut buf = RingBuffer::new(3);
    // Fill first cycle
    buf.write(1);
    buf.write(2);
    buf.write(3);
    // Second cycle: evicts 1, 2
    buf.write(4);
    buf.write(5);
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.read(), Some(3));
    assert_eq!(buf.read(), Some(4));
    assert_eq!(buf.read(), Some(5));
}

#[test]
fn ring_buffer_interleaved_read_write() {
    let mut buf = RingBuffer::new(3);
    buf.write(1);
    buf.write(2);
    assert_eq!(buf.read(), Some(1));
    buf.write(3);
    buf.write(4);
    assert_eq!(buf.len(), 3);
    assert_eq!(buf.read(), Some(2));
    assert_eq!(buf.read(), Some(3));
    assert_eq!(buf.read(), Some(4));
    assert!(buf.is_empty());
}

#[test]
fn ring_buffer_read_empty_returns_none() {
    let mut buf: RingBuffer<i32> = RingBuffer::new(5);
    assert_eq!(buf.read(), None);
}

// ── MovingAverage ───────────────────────────────────────────────────────────

#[test]
fn moving_average_empty_returns_zero() {
    let avg = MovingAverage::new(5);
    assert!(avg.average().abs() < f32::EPSILON);
}

#[test]
fn moving_average_single_value() {
    let mut avg = MovingAverage::new(5);
    avg.push(10.0);
    assert!((avg.average() - 10.0).abs() < 0.001);
}

#[test]
fn moving_average_full_window() {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    // avg = (1+2+3)/3 = 2.0
    assert!((avg.average() - 2.0).abs() < 0.001);
}

#[test]
fn moving_average_window_slides() {
    let mut avg = MovingAverage::new(3);
    avg.push(1.0);
    avg.push(2.0);
    avg.push(3.0);
    avg.push(4.0); // window is now [2, 3, 4]
    assert!((avg.average() - 3.0).abs() < 0.001);
}

#[test]
fn moving_average_reset_clears() {
    let mut avg = MovingAverage::new(3);
    avg.push(10.0);
    avg.push(20.0);
    avg.reset();
    assert!(avg.average().abs() < f32::EPSILON);
}

// ── RateLimiter ─────────────────────────────────────────────────────────────

#[test]
fn rate_limiter_first_call_allowed() {
    let mut limiter = RateLimiter::new(10.0);
    assert!(limiter.should_update());
}

#[test]
fn rate_limiter_immediate_second_call_blocked() {
    let mut limiter = RateLimiter::new(10.0);
    assert!(limiter.should_update());
    assert!(!limiter.should_update());
}

#[test]
fn rate_limiter_allows_after_interval() {
    let mut limiter = RateLimiter::new(50.0); // 20ms interval
    assert!(limiter.should_update());
    std::thread::sleep(Duration::from_millis(30));
    assert!(limiter.should_update());
}

#[test]
fn rate_limiter_reset_allows_immediate_update() {
    let mut limiter = RateLimiter::new(10.0);
    assert!(limiter.should_update());
    limiter.reset();
    assert!(limiter.should_update());
}

// ── RateCounter ─────────────────────────────────────────────────────────────

#[test]
fn rate_counter_initial_rate_zero() {
    let counter = RateCounter::new(Duration::from_secs(1));
    assert!(counter.rate().abs() < f64::EPSILON);
}

#[test]
fn rate_counter_increments_tracked() {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    for _ in 0..10 {
        counter.increment();
    }
    std::thread::sleep(Duration::from_millis(50));
    let rate = counter.rate();
    assert!(rate > 0.0, "rate should be positive after increments");
}

#[test]
fn rate_counter_reset_clears() {
    let mut counter = RateCounter::new(Duration::from_secs(1));
    counter.increment();
    counter.increment();
    counter.reset();
    assert!(counter.rate().abs() < f64::EPSILON);
}

// ── StreamError ─────────────────────────────────────────────────────────────

#[test]
fn stream_error_display_buffer_overflow() {
    let err = StreamError::BufferOverflow;
    assert_eq!(err.to_string(), "Buffer overflow");
}

#[test]
fn stream_error_display_stream_closed() {
    let err = StreamError::StreamClosed;
    assert_eq!(err.to_string(), "Stream closed");
}

#[test]
fn stream_error_display_processing_error() {
    let err = StreamError::ProcessingError("bad data".to_string());
    assert!(err.to_string().contains("bad data"));
}

#[test]
fn stream_result_ok() {
    let val: StreamResult<u32> = Ok(42);
    assert!(val.is_ok());
}

#[test]
fn stream_result_err() {
    let val: StreamResult<u32> = Err(StreamError::StreamClosed);
    assert!(val.is_err());
}
