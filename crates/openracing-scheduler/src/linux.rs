//! Linux-specific platform implementation.

use crate::error::{RTError, RTResult};
use crate::rt_setup::RTSetup;
use core::time::Duration;
use libc::{
    CLOCK_MONOTONIC, MCL_CURRENT, MCL_FUTURE, SCHED_FIFO, clock_nanosleep, mlockall, sched_param,
    sched_setscheduler, timespec,
};
use std::time::Instant;

/// Linux-specific sleep implementation.
pub struct PlatformSleep;

impl PlatformSleep {
    /// Create new platform sleep instance.
    pub fn new() -> Self {
        Self
    }

    /// Apply Linux-specific RT setup.
    pub fn apply_rt_setup(&mut self, setup: &RTSetup) -> RTResult {
        unsafe {
            if setup.high_priority {
                let param = sched_param { sched_priority: 80 };

                if sched_setscheduler(0, SCHED_FIFO, &param) != 0 {
                    // Non-fatal: may fail without CAP_SYS_NICE
                }
            }

            if setup.lock_memory {
                mlockall(MCL_CURRENT | MCL_FUTURE);
            }
        }

        Ok(())
    }

    /// Platform-specific high-precision sleep with busy-spin tail.
    ///
    /// Uses clock_nanosleep for the bulk of the sleep, then busy-spins
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

        let ts = timespec {
            tv_sec: sleep_duration.as_secs() as i64,
            tv_nsec: sleep_duration.subsec_nanos() as i64,
        };

        unsafe {
            let result = clock_nanosleep(CLOCK_MONOTONIC, 0, &ts, std::ptr::null_mut());

            if result != 0 {
                return Err(RTError::TimingViolation);
            }
        }

        // Busy-spin for final precision
        while Instant::now() < target {
            std::hint::spin_loop();
        }

        Ok(())
    }
}

impl Default for PlatformSleep {
    fn default() -> Self {
        Self::new()
    }
}
