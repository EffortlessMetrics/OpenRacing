//! Allocation tracking for CI assertions
//!
//! This module provides allocation tracking capabilities to ensure
//! the RT path remains zero-allocation after pipeline compilation.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};

/// Global allocation counter for tracking heap allocations
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Custom allocator that tracks allocations
pub struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOCATION_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        ALLOCATION_COUNT.fetch_sub(1, Ordering::Relaxed);
        ALLOCATION_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() && new_size > layout.size() {
            ALLOCATION_BYTES.fetch_add(new_size - layout.size(), Ordering::Relaxed);
        } else if !new_ptr.is_null() && new_size < layout.size() {
            ALLOCATION_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
        }
        new_ptr
    }
}

/// Allocation tracking guard that resets counters on creation
/// and provides access to allocation counts
pub struct AllocationGuard {
    start_count: usize,
    start_bytes: usize,
}

impl AllocationGuard {
    /// Create a new allocation guard and reset counters
    pub fn new() -> Self {
        let start_count = ALLOCATION_COUNT.load(Ordering::Relaxed);
        let start_bytes = ALLOCATION_BYTES.load(Ordering::Relaxed);
        
        Self {
            start_count,
            start_bytes,
        }
    }

    /// Get the number of allocations since guard creation
    pub fn allocations_since_start(&self) -> usize {
        ALLOCATION_COUNT.load(Ordering::Relaxed).saturating_sub(self.start_count)
    }

    /// Get the number of bytes allocated since guard creation
    pub fn bytes_allocated_since_start(&self) -> usize {
        ALLOCATION_BYTES.load(Ordering::Relaxed).saturating_sub(self.start_bytes)
    }

    /// Get the current total allocation count
    pub fn total_allocations(&self) -> usize {
        ALLOCATION_COUNT.load(Ordering::Relaxed)
    }

    /// Get the current total allocated bytes
    pub fn total_bytes(&self) -> usize {
        ALLOCATION_BYTES.load(Ordering::Relaxed)
    }

    /// Reset allocation counters to zero
    pub fn reset_counters(&self) {
        ALLOCATION_COUNT.store(0, Ordering::Relaxed);
        ALLOCATION_BYTES.store(0, Ordering::Relaxed);
    }
}

/// Get current allocation count (for compatibility)
pub fn get_allocation_count() -> usize {
    ALLOCATION_COUNT.load(Ordering::Relaxed)
}

/// Get current allocated bytes
pub fn get_allocated_bytes() -> usize {
    ALLOCATION_BYTES.load(Ordering::Relaxed)
}

/// Create an allocation tracking guard
pub fn track() -> AllocationGuard {
    AllocationGuard::new()
}

/// Assert that no allocations occurred since the guard was created
#[macro_export]
macro_rules! assert_zero_alloc {
    ($guard:expr) => {
        let allocs = $guard.allocations_since_start();
        let bytes = $guard.bytes_allocated_since_start();
        if allocs > 0 {
            panic!(
                "RT path allocation violation: {} allocations ({} bytes) detected. \
                 This violates the zero-allocation requirement for the hot path.",
                allocs, bytes
            );
        }
    };
    ($guard:expr, $msg:expr) => {
        let allocs = $guard.allocations_since_start();
        let bytes = $guard.bytes_allocated_since_start();
        if allocs > 0 {
            panic!(
                "{}: RT path allocation violation: {} allocations ({} bytes) detected. \
                 This violates the zero-allocation requirement for the hot path.",
                $msg, allocs, bytes
            );
        }
    };
}

/// CI-specific assertion that can be used in automated testing
/// This macro will cause CI to fail if allocations are detected on the hot path
#[macro_export]
macro_rules! ci_assert_zero_alloc {
    ($guard:expr, $context:expr) => {
        let allocs = $guard.allocations_since_start();
        let bytes = $guard.bytes_allocated_since_start();
        if allocs > 0 {
            eprintln!("CI FAILURE: RT path allocation detected in {}", $context);
            eprintln!("Allocations: {}, Bytes: {}", allocs, bytes);
            eprintln!("This violates the zero-allocation requirement for real-time code paths.");
            std::process::exit(1);
        }
    };
}

/// Benchmark allocation tracking for performance testing
pub struct AllocationBenchmark {
    guard: AllocationGuard,
    context: String,
}

impl AllocationBenchmark {
    pub fn new(context: String) -> Self {
        Self {
            guard: AllocationGuard::new(),
            context,
        }
    }
    
    pub fn finish(self) -> AllocationReport {
        AllocationReport {
            context: self.context,
            allocations: self.guard.allocations_since_start(),
            bytes: self.guard.bytes_allocated_since_start(),
        }
    }
}

/// Report of allocation activity during a benchmark
#[derive(Debug, Clone)]
pub struct AllocationReport {
    pub context: String,
    pub allocations: usize,
    pub bytes: usize,
}

impl AllocationReport {
    pub fn assert_zero_alloc(&self) {
        if self.allocations > 0 {
            panic!(
                "Allocation violation in {}: {} allocations ({} bytes)",
                self.context, self.allocations, self.bytes
            );
        }
    }
    
    pub fn print_summary(&self) {
        if self.allocations > 0 {
            println!(
                "⚠️  {} allocated {} times ({} bytes)",
                self.context, self.allocations, self.bytes
            );
        } else {
            println!("✅ {} - zero allocations", self.context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec::Vec;

    #[test]
    fn test_allocation_tracking_basic() {
        let guard = track();
        
        // This should cause allocations
        let _vec: Vec<i32> = vec![1, 2, 3, 4, 5];
        
        // Should have recorded allocations
        assert!(guard.allocations_since_start() > 0);
        assert!(guard.bytes_allocated_since_start() > 0);
    }

    #[test]
    fn test_allocation_guard_reset() {
        let guard = track();
        
        // Cause some allocations
        let _vec: Vec<i32> = vec![1, 2, 3];
        assert!(guard.allocations_since_start() > 0);
        
        // Reset and check
        guard.reset_counters();
        let new_guard = track();
        assert_eq!(new_guard.allocations_since_start(), 0);
    }

    #[test]
    fn test_zero_alloc_macro() {
        let guard = track();
        
        // No allocations - should pass
        let x = 42;
        let _y = x + 1;
        
        assert_zero_alloc!(guard);
    }

    #[test]
    #[should_panic(expected = "Expected zero allocations")]
    fn test_zero_alloc_macro_fails() {
        let guard = track();
        
        // Cause allocation - should fail
        let _vec: Vec<i32> = vec![1, 2, 3];
        
        assert_zero_alloc!(guard);
    }
}