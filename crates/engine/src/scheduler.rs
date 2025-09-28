//! Absolute scheduler for 1kHz real-time operation

use std::time::{Duration, Instant};
use crate::{RTResult, RTError};

/// Absolute scheduler for precise 1kHz timing
pub struct AbsoluteScheduler {
    period_ns: u64,
    next_tick: Instant,
    tick_count: u64,
}

impl AbsoluteScheduler {
    /// Create new scheduler with 1kHz (1ms) period
    pub fn new_1khz() -> Self {
        Self {
            period_ns: 1_000_000, // 1ms in nanoseconds
            next_tick: Instant::now(),
            tick_count: 0,
        }
    }

    /// Wait for next tick (RT-safe)
    pub fn wait_for_tick(&mut self) -> RTResult<u64> {
        let now = Instant::now();
        
        if now < self.next_tick {
            // Sleep until next tick
            self.sleep_until(self.next_tick)?;
        } else if now.duration_since(self.next_tick).as_nanos() > 250_000 {
            // Missed deadline by more than 0.25ms
            return Err(RTError::TimingViolation);
        }
        
        self.tick_count += 1;
        self.next_tick += Duration::from_nanos(self.period_ns);
        
        Ok(self.tick_count)
    }

    /// Platform-specific high-precision sleep
    #[cfg(target_os = "windows")]
    fn sleep_until(&self, target: Instant) -> RTResult {
        use std::ptr;
        use windows::Win32::System::Threading::{
            CreateWaitableTimerW, SetWaitableTimer, WaitForSingleObject,
            CloseHandle, INFINITE,
        };
        use windows::Win32::Foundation::{HANDLE, FILETIME};

        unsafe {
            let timer = CreateWaitableTimerW(ptr::null(), true, None)
                .map_err(|_| RTError::TimingViolation)?;

            let duration = target.duration_since(Instant::now());
            let ft_duration = -(duration.as_nanos() as i64 / 100); // 100ns units, negative for relative
            
            let mut due_time = FILETIME {
                dwLowDateTime: ft_duration as u32,
                dwHighDateTime: (ft_duration >> 32) as u32,
            };

            SetWaitableTimer(timer, &due_time, 0, None, ptr::null(), false)
                .map_err(|_| RTError::TimingViolation)?;

            WaitForSingleObject(timer, INFINITE);
            CloseHandle(timer);
        }

        Ok(())
    }

    /// Platform-specific high-precision sleep
    #[cfg(target_os = "linux")]
    fn sleep_until(&self, target: Instant) -> RTResult {
        use libc::{clock_nanosleep, timespec, CLOCK_MONOTONIC, TIMER_ABSTIME};

        let duration = target.duration_since(Instant::now());
        let ts = timespec {
            tv_sec: duration.as_secs() as i64,
            tv_nsec: duration.subsec_nanos() as i64,
        };

        unsafe {
            let result = clock_nanosleep(
                CLOCK_MONOTONIC,
                TIMER_ABSTIME,
                &ts,
                std::ptr::null_mut(),
            );
            
            if result != 0 {
                return Err(RTError::TimingViolation);
            }
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
}