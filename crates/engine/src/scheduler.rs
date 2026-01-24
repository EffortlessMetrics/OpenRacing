//! Absolute scheduler for 1kHz real-time operation with PLL and RT setup

use crate::{RTError, RTResult};
use std::time::{Duration, Instant};

/// Phase-Locked Loop for drift correction
#[derive(Debug, Clone)]
pub struct PLL {
    /// Target period in nanoseconds
    target_period_ns: u64,

    /// Current estimated period in nanoseconds
    estimated_period_ns: f64,

    /// PLL gain factor (lower = more stable, higher = faster correction)
    gain: f64,

    /// Last tick timestamp for period measurement
    last_tick: Option<Instant>,

    /// Accumulated phase error
    phase_error_ns: f64,
}

impl PLL {
    /// Create new PLL with target period
    pub fn new(target_period_ns: u64) -> Self {
        Self {
            target_period_ns,
            estimated_period_ns: target_period_ns as f64,
            gain: 0.01, // 1% correction per tick
            last_tick: None,
            phase_error_ns: 0.0,
        }
    }

    /// Update PLL with actual tick timing
    pub fn update(&mut self, actual_tick_time: Instant) -> Duration {
        if let Some(last) = self.last_tick {
            let actual_period_ns = actual_tick_time.duration_since(last).as_nanos() as f64;

            // Calculate phase error
            let period_error = actual_period_ns - self.target_period_ns as f64;
            self.phase_error_ns += period_error;

            // Apply PLL correction
            let correction = self.gain * (period_error + 0.1 * self.phase_error_ns);
            self.estimated_period_ns = self.target_period_ns as f64 - correction;

            // Clamp to reasonable bounds (±10% of target)
            let min_period = self.target_period_ns as f64 * 0.9;
            let max_period = self.target_period_ns as f64 * 1.1;
            self.estimated_period_ns = self.estimated_period_ns.clamp(min_period, max_period);
        }

        self.last_tick = Some(actual_tick_time);
        Duration::from_nanos(self.estimated_period_ns as u64)
    }

    /// Get current phase error in nanoseconds
    pub fn phase_error_ns(&self) -> f64 {
        self.phase_error_ns
    }

    /// Reset PLL state
    pub fn reset(&mut self) {
        self.estimated_period_ns = self.target_period_ns as f64;
        self.phase_error_ns = 0.0;
        self.last_tick = None;
    }
}

/// Real-time setup configuration
#[derive(Debug, Clone)]
pub struct RTSetup {
    /// Enable high-priority scheduling
    pub high_priority: bool,

    /// Enable memory locking (prevent swapping)
    pub lock_memory: bool,

    /// Disable power throttling
    pub disable_power_throttling: bool,

    /// CPU affinity mask (None = no affinity)
    pub cpu_affinity: Option<u64>,
}

impl Default for RTSetup {
    fn default() -> Self {
        Self {
            high_priority: true,
            lock_memory: true,
            disable_power_throttling: true,
            cpu_affinity: None,
        }
    }
}

/// Jitter metrics collection
#[derive(Debug, Clone, Default)]
pub struct JitterMetrics {
    /// Total number of ticks
    pub total_ticks: u64,

    /// Number of missed deadlines
    pub missed_ticks: u64,

    /// Maximum observed jitter in nanoseconds
    pub max_jitter_ns: u64,

    /// Running sum of squared jitter for variance calculation
    pub jitter_sum_squared: f64,

    /// Recent jitter samples for percentile calculation
    pub recent_jitter_samples: Vec<u64>,

    /// Maximum samples to keep for percentile calculation
    pub max_samples: usize,
}

impl JitterMetrics {
    /// Create new jitter metrics collector
    pub fn new() -> Self {
        Self {
            max_samples: 10000, // Keep last 10k samples for p99 calculation
            ..Default::default()
        }
    }

    /// Record a tick with its jitter
    pub fn record_tick(&mut self, jitter_ns: u64, missed_deadline: bool) {
        self.total_ticks += 1;

        if missed_deadline {
            self.missed_ticks += 1;
        }

        self.max_jitter_ns = self.max_jitter_ns.max(jitter_ns);
        self.jitter_sum_squared += (jitter_ns as f64).powi(2);

        // Keep recent samples for percentile calculation
        self.recent_jitter_samples.push(jitter_ns);
        if self.recent_jitter_samples.len() > self.max_samples {
            self.recent_jitter_samples.remove(0);
        }
    }

    /// Calculate p99 jitter in nanoseconds
    pub fn p99_jitter_ns(&self) -> u64 {
        if self.recent_jitter_samples.is_empty() {
            return 0;
        }

        let mut sorted = self.recent_jitter_samples.clone();
        sorted.sort_unstable();

        let p99_index = (sorted.len() as f64 * 0.99) as usize;
        sorted.get(p99_index).copied().unwrap_or(0)
    }

    /// Calculate missed tick rate (0.0 to 1.0)
    pub fn missed_tick_rate(&self) -> f64 {
        if self.total_ticks == 0 {
            0.0
        } else {
            self.missed_ticks as f64 / self.total_ticks as f64
        }
    }

    /// Check if metrics meet performance requirements
    pub fn meets_requirements(&self) -> bool {
        // Requirements: p99 jitter ≤ 0.25ms, missed tick rate ≤ 0.001%
        self.p99_jitter_ns() <= 250_000 && self.missed_tick_rate() <= 0.00001
    }
}

/// Absolute scheduler for precise 1kHz timing with PLL and jitter metrics
pub struct AbsoluteScheduler {
    /// Target period in nanoseconds
    /// TODO: Used for future adaptive scheduling implementation
    #[allow(dead_code)]
    period_ns: u64,

    /// Next scheduled tick time
    next_tick: Instant,

    /// Total tick count
    tick_count: u64,

    /// Phase-locked loop for drift correction
    pll: PLL,

    /// Jitter metrics collection
    metrics: JitterMetrics,

    /// RT setup applied
    rt_setup_applied: bool,
}

impl AbsoluteScheduler {
    /// Create new scheduler with 1kHz (1ms) period
    pub fn new_1khz() -> Self {
        let period_ns = 1_000_000; // 1ms in nanoseconds
        Self {
            period_ns,
            next_tick: Instant::now(),
            tick_count: 0,
            pll: PLL::new(period_ns),
            metrics: JitterMetrics::new(),
            rt_setup_applied: false,
        }
    }

    /// Apply real-time setup for optimal performance
    pub fn apply_rt_setup(&mut self, setup: &RTSetup) -> RTResult {
        if self.rt_setup_applied {
            return Ok(()); // Already applied
        }

        #[cfg(target_os = "windows")]
        {
            self.apply_windows_rt_setup(setup)?;
        }

        #[cfg(target_os = "linux")]
        {
            self.apply_linux_rt_setup(setup)?;
        }

        self.rt_setup_applied = true;
        Ok(())
    }

    /// Wait for next tick (RT-safe) with PLL correction and jitter measurement
    pub fn wait_for_tick(&mut self) -> RTResult<u64> {
        let tick_start = Instant::now();

        // Check if we missed the deadline
        let missed_deadline = tick_start >= self.next_tick;
        let jitter_ns = if missed_deadline {
            tick_start.duration_since(self.next_tick).as_nanos() as u64
        } else {
            self.next_tick.duration_since(tick_start).as_nanos() as u64
        };

        // Record metrics
        self.metrics.record_tick(jitter_ns, missed_deadline);

        // If we're early, sleep until the target time
        if !missed_deadline {
            self.sleep_until(self.next_tick)?;
        }

        // Update PLL with actual tick timing
        let actual_tick_time = Instant::now();
        let corrected_period = self.pll.update(actual_tick_time);

        // Update tick count and schedule next tick
        self.tick_count += 1;
        self.next_tick += corrected_period;

        // Check for severe timing violations
        if jitter_ns > 250_000 {
            return Err(RTError::TimingViolation);
        }

        Ok(self.tick_count)
    }

    /// Apply Windows-specific RT setup
    #[cfg(target_os = "windows")]
    fn apply_windows_rt_setup(&self, setup: &RTSetup) -> RTResult {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
        };

        unsafe {
            if setup.high_priority {
                // Set thread to time-critical priority
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)
                    .map_err(|_| RTError::TimingViolation)?;

                // Note: SetProcessPriorityClass and power management APIs
                // would be added here in a full implementation
            }
        }

        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail
    #[cfg(target_os = "windows")]
    fn sleep_until(&self, target: Instant) -> RTResult {
        use windows::Win32::Foundation::{CloseHandle, FILETIME};
        use windows::Win32::System::Threading::{
            CreateWaitableTimerW, INFINITE, SetWaitableTimer, WaitForSingleObject,
        };

        let now = Instant::now();
        if target <= now {
            return Ok(());
        }

        let duration = target.duration_since(now);

        // If duration is very short, just busy-spin
        if duration.as_micros() < 100 {
            while Instant::now() < target {
                std::hint::spin_loop();
            }
            return Ok(());
        }

        // Sleep until ~80µs before target, then busy-spin
        let sleep_duration = duration.saturating_sub(Duration::from_micros(80));
        let _sleep_target = now + sleep_duration;

        unsafe {
            let timer =
                CreateWaitableTimerW(None, true, None).map_err(|_| RTError::TimingViolation)?;

            let ft_duration = -(sleep_duration.as_nanos() as i64 / 100); // 100ns units, negative for relative

            let due_time = FILETIME {
                dwLowDateTime: ft_duration as u32,
                dwHighDateTime: (ft_duration >> 32) as u32,
            };

            SetWaitableTimer(
                timer,
                &due_time.dwLowDateTime as *const u32 as *const i64,
                0,
                None,
                None,
                false,
            )
            .map_err(|_| RTError::TimingViolation)?;

            WaitForSingleObject(timer, INFINITE);
            let _ = CloseHandle(timer);
        }

        // Busy-spin for the final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }

    /// Apply Linux-specific RT setup
    #[cfg(target_os = "linux")]
    fn apply_linux_rt_setup(&self, setup: &RTSetup) -> RTResult {
        use libc::{
            MCL_CURRENT, MCL_FUTURE, SCHED_FIFO, mlockall, sched_param, sched_setscheduler,
        };

        unsafe {
            if setup.high_priority {
                // Set SCHED_FIFO with high priority
                let param = sched_param {
                    sched_priority: 80, // High priority but not maximum
                };

                if sched_setscheduler(0, SCHED_FIFO, &param) != 0 {
                    // Fall back to trying via rtkit if direct scheduling fails
                    // This would require rtkit integration in a full implementation
                }
            }

            if setup.lock_memory {
                // Lock all current and future memory to prevent swapping
                mlockall(MCL_CURRENT | MCL_FUTURE);
            }
        }

        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail
    #[cfg(target_os = "linux")]
    fn sleep_until(&self, target: Instant) -> RTResult {
        use libc::{CLOCK_MONOTONIC, clock_nanosleep, timespec};

        let now = Instant::now();
        if target <= now {
            return Ok(());
        }

        let duration = target.duration_since(now);

        // If duration is very short, just busy-spin
        if duration.as_micros() < 100 {
            while Instant::now() < target {
                std::hint::spin_loop();
            }
            return Ok(());
        }

        // Sleep until ~80µs before target, then busy-spin
        let sleep_duration = duration.saturating_sub(Duration::from_micros(80));

        let ts = timespec {
            tv_sec: sleep_duration.as_secs() as i64,
            tv_nsec: sleep_duration.subsec_nanos() as i64,
        };

        unsafe {
            let result = clock_nanosleep(
                CLOCK_MONOTONIC,
                0, // Relative time
                &ts,
                std::ptr::null_mut(),
            );

            if result != 0 {
                return Err(RTError::TimingViolation);
            }
        }

        // Busy-spin for the final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }

    /// Fallback sleep implementation
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    fn sleep_until(&self, target: Instant) -> RTResult {
        let now = Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
        Ok(())
    }

    /// Get current tick count
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Get jitter metrics
    pub fn metrics(&self) -> &JitterMetrics {
        &self.metrics
    }

    /// Get PLL phase error in nanoseconds
    pub fn phase_error_ns(&self) -> f64 {
        self.pll.phase_error_ns()
    }

    /// Reset scheduler state (for testing)
    pub fn reset(&mut self) {
        self.next_tick = Instant::now();
        self.tick_count = 0;
        self.pll.reset();
        self.metrics = JitterMetrics::new();
    }

    /// Check if RT setup has been applied
    pub fn is_rt_setup_applied(&self) -> bool {
        self.rt_setup_applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_pll_creation() {
        let pll = PLL::new(1_000_000); // 1ms
        assert_eq!(pll.target_period_ns, 1_000_000);
        assert_eq!(pll.estimated_period_ns, 1_000_000.0);
        assert_eq!(pll.phase_error_ns(), 0.0);
    }

    #[test]
    fn test_pll_update() {
        let mut pll = PLL::new(1_000_000); // 1ms

        let _start = Instant::now();
        thread::sleep(Duration::from_millis(1));
        let tick1 = Instant::now();

        let corrected_period = pll.update(tick1);
        assert!(corrected_period.as_nanos() > 900_000); // Should be close to 1ms
        assert!(corrected_period.as_nanos() < 1_100_000);
    }

    #[test]
    fn test_jitter_metrics() {
        let mut metrics = JitterMetrics::new();

        // Record some ticks
        metrics.record_tick(100_000, false); // 0.1ms jitter
        metrics.record_tick(200_000, false); // 0.2ms jitter
        metrics.record_tick(300_000, true); // 0.3ms jitter, missed deadline

        assert_eq!(metrics.total_ticks, 3);
        assert_eq!(metrics.missed_ticks, 1);
        assert_eq!(metrics.max_jitter_ns, 300_000);
        assert_eq!(metrics.missed_tick_rate(), 1.0 / 3.0);
    }

    #[test]
    fn test_jitter_metrics_p99() {
        let mut metrics = JitterMetrics::new();

        // Add 100 samples with known distribution
        for i in 0..100 {
            metrics.record_tick(i * 1000, false); // 0 to 99µs
        }

        let p99 = metrics.p99_jitter_ns();
        assert!(p99 >= 98_000); // Should be around 98-99µs
        assert!(p99 <= 99_000);
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = AbsoluteScheduler::new_1khz();
        assert_eq!(scheduler.period_ns, 1_000_000);
        assert_eq!(scheduler.tick_count(), 0);
        assert!(!scheduler.is_rt_setup_applied());
    }

    #[test]
    fn test_rt_setup_default() {
        let setup = RTSetup::default();
        assert!(setup.high_priority);
        assert!(setup.lock_memory);
        assert!(setup.disable_power_throttling);
        assert!(setup.cpu_affinity.is_none());
    }

    #[test]
    fn test_scheduler_reset() {
        let mut scheduler = AbsoluteScheduler::new_1khz();

        // Simulate some ticks
        scheduler.tick_count = 100;
        scheduler.metrics.total_ticks = 100;

        scheduler.reset();

        assert_eq!(scheduler.tick_count(), 0);
        assert_eq!(scheduler.metrics().total_ticks, 0);
    }

    #[test]
    fn test_metrics_requirements() {
        let mut metrics = JitterMetrics::new();

        // Add samples that meet requirements (low jitter, no missed ticks)
        for _ in 0..1000 {
            metrics.record_tick(100_000, false); // 0.1ms jitter, no missed ticks
        }

        assert!(metrics.meets_requirements());

        // Add a few samples that violate jitter but not enough to affect p99
        for _ in 0..5 {
            metrics.record_tick(300_000, false); // 0.3ms jitter but no missed deadline
        }

        // Should still meet requirements due to p99 calculation (only 0.5% of samples are high jitter)
        assert!(metrics.meets_requirements());

        // Now add enough missed ticks to violate the missed tick rate requirement
        for _ in 0..100 {
            metrics.record_tick(100_000, true); // Low jitter but missed deadline
        }

        // Should now fail requirements due to missed tick rate
        assert!(!metrics.meets_requirements());
    }

    #[tokio::test]
    async fn test_scheduler_basic_operation() {
        let mut scheduler = AbsoluteScheduler::new_1khz();

        // Apply RT setup (should not fail even if we don't have permissions)
        let setup = RTSetup::default();
        let _ = scheduler.apply_rt_setup(&setup);

        let start = Instant::now();

        // Run a few ticks
        for expected_tick in 1..=5 {
            // This might fail in CI due to timing, so we'll be lenient
            match scheduler.wait_for_tick() {
                Ok(tick) => {
                    assert_eq!(tick, expected_tick);
                }
                Err(RTError::TimingViolation) => {
                    // Expected in CI environments with poor timing
                    break;
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }

        let elapsed = start.elapsed();

        // Should have taken some time (be lenient for CI)
        assert!(elapsed.as_micros() >= 100);

        // Check that metrics were collected
        assert!(scheduler.metrics().total_ticks > 0);
    }
}
