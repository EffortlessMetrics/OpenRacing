//! Windows-specific platform implementation.

use crate::error::{RTError, RTResult};
use crate::rt_setup::RTSetup;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::{
    CreateWaitableTimerW, GetCurrentThread, INFINITE, SetThreadPriority, SetWaitableTimer,
    THREAD_PRIORITY_TIME_CRITICAL, WaitForSingleObject,
};

/// Windows-specific sleep implementation.
pub struct PlatformSleep {
    timer_handle: Option<HANDLE>,
}

impl PlatformSleep {
    /// Create new platform sleep instance.
    pub fn new() -> Self {
        Self { timer_handle: None }
    }

    /// Apply Windows-specific RT setup.
    pub fn apply_rt_setup(&mut self, setup: &RTSetup) -> RTResult {
        if setup.high_priority {
            unsafe {
                SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL)
                    .map_err(|_| RTError::RTSetupFailed)?;
            }
        }
        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail.
    ///
    /// Uses a waitable timer for the bulk of the sleep, then busy-spins
    /// for the final ~80 microseconds to achieve precise timing.
    pub fn sleep_until(&mut self, target: Instant) -> RTResult {
        let now = Instant::now();
        if target <= now {
            return Ok(());
        }

        let duration = target.duration_since(now);

        // For very short durations, just busy-spin
        if duration.as_micros() < 100 {
            while Instant::now() < target {
                std::hint::spin_loop();
            }
            return Ok(());
        }

        // Sleep until ~80µs before target, then busy-spin
        let sleep_duration = duration.saturating_sub(Duration::from_micros(80));

        let timer = self.get_or_create_timer()?;
        let due_time = relative_due_time_100ns(sleep_duration);

        unsafe {
            SetWaitableTimer(timer, &due_time, 0, None, None, false)
                .map_err(|_| RTError::TimingViolation)?;
            WaitForSingleObject(timer, INFINITE);
        }

        // Busy-spin for final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }

    fn get_or_create_timer(&mut self) -> RTResult<HANDLE> {
        if let Some(handle) = self.timer_handle {
            return Ok(handle);
        }

        let timer =
            unsafe { CreateWaitableTimerW(None, true, None).map_err(|_| RTError::RTSetupFailed)? };
        self.timer_handle = Some(timer);
        Ok(timer)
    }
}

impl Drop for PlatformSleep {
    fn drop(&mut self) {
        if let Some(handle) = self.timer_handle.take() {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
    }
}

/// Convert duration to relative due time in 100ns units for waitable timer.
fn relative_due_time_100ns(duration: Duration) -> i64 {
    let ticks_100ns = (duration.as_nanos() / 100).min(i64::MAX as u128) as i64;
    -ticks_100ns.max(1)
}

impl Default for PlatformSleep {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_sleep_creation() {
        let sleep = PlatformSleep::new();
        assert!(sleep.timer_handle.is_none());
    }

    #[test]
    fn test_relative_due_time() {
        let duration = Duration::from_micros(1000);
        let due = relative_due_time_100ns(duration);
        assert!(due < 0); // Negative for relative time
    }
}
