//! Prelude module for common engine types
//!
//! This module provides a convenient way to import the most commonly used
//! types from the racing wheel engine.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Extension trait for Mutex that provides panic-on-poison locking.
///
/// This is used to avoid `unwrap()` calls on mutex locks while still panicking
/// on poisoned mutexes (which indicate a previous thread panic - a fatal error).
pub trait MutexExt<T> {
    /// Lock the mutex, panicking if it was poisoned.
    fn lock_or_panic(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    #[allow(clippy::panic)]
    fn lock_or_panic(&self) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(g) => g,
            Err(e) => panic!("mutex poisoned: {e}"),
        }
    }
}

/// Extension trait for RwLock that provides panic-on-poison locking.
pub trait RwLockExt<T> {
    /// Read-lock the RwLock, panicking if it was poisoned.
    fn read_or_panic(&self) -> RwLockReadGuard<'_, T>;
    /// Write-lock the RwLock, panicking if it was poisoned.
    fn write_or_panic(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    #[allow(clippy::panic)]
    fn read_or_panic(&self) -> RwLockReadGuard<'_, T> {
        match self.read() {
            Ok(g) => g,
            Err(e) => panic!("rwlock poisoned (read): {e}"),
        }
    }

    #[allow(clippy::panic)]
    fn write_or_panic(&self) -> RwLockWriteGuard<'_, T> {
        match self.write() {
            Ok(g) => g,
            Err(e) => panic!("rwlock poisoned (write): {e}"),
        }
    }
}

// Core RT types (canonical exports)
pub use crate::rt::{FFBMode, Frame, PerformanceMetrics, RTError, RTResult};

// Engine types
pub use crate::engine::{BlackboxFrame, Engine, EngineConfig, GameInput};

// Device and port types
pub use crate::device::{DeviceInfo, TelemetryData, VirtualDevice};
pub use crate::ports::{HidDevice, NormalizedTelemetry, TelemetryFlags};

// FFB capability negotiation
pub use crate::ffb::{
    CapabilityNegotiator, GameCompatibility, ModeSelectionPolicy, NegotiationResult,
};

// Test harness for development
#[cfg(test)]
pub use crate::test_harness::{RTLoopTestHarness, TestHarnessConfig, TestResult};

// Scheduler and RT setup
pub use crate::scheduler::{JitterMetrics, PLL, RTSetup};
