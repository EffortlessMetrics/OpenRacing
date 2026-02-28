//! Fallback platform implementation for non-Windows, non-Linux systems.

use crate::error::RTResult;
use crate::rt_setup::RTSetup;
use std::time::Instant;

/// Fallback sleep implementation using standard library.
pub struct PlatformSleep;

impl PlatformSleep {
    /// Create new platform sleep instance.
    pub fn new() -> Self {
        Self
    }

    /// Apply RT setup (no-op for fallback).
    pub fn apply_rt_setup(&mut self, _setup: &RTSetup) -> RTResult {
        Ok(())
    }

    /// Fallback sleep using standard thread::sleep.
    pub fn sleep_until(&mut self, target: Instant) -> RTResult {
        let now = Instant::now();
        if target > now {
            std::thread::sleep(target - now);
        }
        Ok(())
    }
}

impl Default for PlatformSleep {
    fn default() -> Self {
        Self::new()
    }
}
