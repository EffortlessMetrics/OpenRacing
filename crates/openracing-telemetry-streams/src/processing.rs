//! Telemetry processing utilities

use std::collections::VecDeque;

pub struct MovingAverage {
    window: VecDeque<f32>,
    window_size: usize,
    sum: f32,
}

impl MovingAverage {
    pub fn new(window_size: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(window_size),
            window_size,
            sum: 0.0,
        }
    }

    pub fn push(&mut self, value: f32) {
        if self.window.len() == self.window_size {
            if let Some(old) = self.window.pop_front() {
                self.sum -= old;
            }
        }

        self.window.push_back(value);
        self.sum += value;
    }

    pub fn average(&self) -> f32 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.sum / self.window.len() as f32
    }

    pub fn reset(&mut self) {
        self.window.clear();
        self.sum = 0.0;
    }
}

pub struct RateLimiter {
    min_interval: Duration,
    last_update: Option<Instant>,
}

impl RateLimiter {
    pub fn new(rate_hz: f32) -> Self {
        let min_interval = Duration::from_secs_f64(1.0 / rate_hz as f64);

        Self {
            min_interval,
            last_update: None,
        }
    }

    pub fn should_update(&mut self) -> bool {
        let now = Instant::now();

        if let Some(last) = self.last_update {
            if now.duration_since(last) < self.min_interval {
                return false;
            }
        }

        self.last_update = Some(now);
        true
    }

    pub fn reset(&mut self) {
        self.last_update = None;
    }
}

pub struct RateCounter {
    count: u64,
    window: Duration,
    start: Instant,
}

impl RateCounter {
    pub fn new(window: Duration) -> Self {
        Self {
            count: 0,
            window,
            start: Instant::now(),
        }
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }

    pub fn rate(&self) -> f64 {
        let elapsed = self.start.elapsed();
        if elapsed.is_zero() {
            return 0.0;
        }

        let window_secs = self.window.as_secs_f64();
        let elapsed_secs = elapsed.as_secs_f64();

        (self.count as f64 / elapsed_secs) * window_secs
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.start = Instant::now();
    }
}

use std::time::Instant;

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_moving_average() {
        let mut avg = MovingAverage::new(3);

        avg.push(1.0);
        assert!((avg.average() - 1.0).abs() < 0.01);

        avg.push(2.0);
        avg.push(3.0);
        assert!((avg.average() - 2.0).abs() < 0.01);

        avg.push(4.0);
        assert!((avg.average() - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(100.0);

        assert!(limiter.should_update());
        assert!(!limiter.should_update());

        thread::sleep(Duration::from_millis(20));

        assert!(limiter.should_update());
    }

    #[test]
    fn test_rate_counter() {
        let mut counter = RateCounter::new(Duration::from_secs(1));

        for _ in 0..10 {
            counter.increment();
        }

        thread::sleep(Duration::from_millis(100));

        let rate = counter.rate();
        assert!(rate > 0.0);
    }
}
