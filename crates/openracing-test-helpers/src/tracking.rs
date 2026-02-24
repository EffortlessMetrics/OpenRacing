//! Allocation tracking for RT safety tests.
//!
//! This module provides utilities to verify that code paths don't allocate
//! on the heap, which is critical for real-time safety.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
    static ALLOCATION_BYTES: Cell<usize> = const { Cell::new(0) };
    static TRACKING_ENABLED: Cell<bool> = const { Cell::new(false) };
}

pub struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && TRACKING_ENABLED.with(|e| e.get()) {
            ALLOCATION_COUNT.with(|count| {
                count.set(count.get().saturating_add(1));
            });
            ALLOCATION_BYTES.with(|bytes| {
                bytes.set(bytes.get().saturating_add(layout.size()));
            });
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() && TRACKING_ENABLED.with(|e| e.get()) && new_size > layout.size() {
            ALLOCATION_BYTES.with(|bytes| {
                bytes.set(bytes.get().saturating_add(new_size - layout.size()));
            });
        }
        new_ptr
    }
}

pub struct AllocationGuard {
    start_count: usize,
    start_bytes: usize,
    _private: (),
}

impl AllocationGuard {
    pub fn new() -> Self {
        TRACKING_ENABLED.with(|e| e.set(true));
        Self {
            start_count: ALLOCATION_COUNT.with(|c| c.get()),
            start_bytes: ALLOCATION_BYTES.with(|b| b.get()),
            _private: (),
        }
    }

    pub fn allocations(&self) -> usize {
        ALLOCATION_COUNT
            .with(|count| count.get())
            .saturating_sub(self.start_count)
    }

    pub fn bytes(&self) -> usize {
        ALLOCATION_BYTES
            .with(|bytes| bytes.get())
            .saturating_sub(self.start_bytes)
    }

    pub fn has_allocations(&self) -> bool {
        self.allocations() > 0
    }

    pub fn reset(&self) {
        ALLOCATION_COUNT.with(|c| c.set(0));
        ALLOCATION_BYTES.with(|b| b.set(0));
    }
}

impl Default for AllocationGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AllocationGuard {
    fn drop(&mut self) {
        TRACKING_ENABLED.with(|e| e.set(false));
    }
}

pub fn track() -> AllocationGuard {
    AllocationGuard::new()
}

#[macro_export]
macro_rules! assert_rt_safe {
    ($guard:expr) => {
        let _guard = &$guard;
        let allocs = _guard.allocations();
        let bytes = _guard.bytes();
        if allocs > 0 {
            panic!(
                "RT path allocation violation: {} allocations ({} bytes)\n\
                 This violates the zero-allocation requirement for real-time code.\n\
                 Location: {}:{}",
                allocs,
                bytes,
                file!(),
                line!()
            );
        }
    };
    ($guard:expr, $context:expr) => {
        let _guard = &$guard;
        let allocs = _guard.allocations();
        let bytes = _guard.bytes();
        if allocs > 0 {
            panic!(
                "RT path allocation violation in '{}': {} allocations ({} bytes)\n\
                 This violates the zero-allocation requirement for real-time code.\n\
                 Location: {}:{}",
                $context,
                allocs,
                bytes,
                file!(),
                line!()
            );
        }
    };
}

#[macro_export]
macro_rules! ci_assert_rt_safe {
    ($guard:expr, $context:expr) => {
        let _guard = &$guard;
        let allocs = _guard.allocations();
        let bytes = _guard.bytes();
        if allocs > 0 {
            eprintln!("CI FAILURE: RT path allocation detected in '{}'", $context);
            eprintln!("  Allocations: {}", allocs);
            eprintln!("  Bytes: {}", bytes);
            eprintln!("This violates the zero-allocation requirement for real-time code.");
            std::process::exit(1);
        }
    };
}

pub struct AllocationReport {
    pub allocations: usize,
    pub bytes: usize,
    pub context: String,
}

impl AllocationReport {
    pub fn new(context: impl Into<String>) -> Self {
        Self {
            allocations: 0,
            bytes: 0,
            context: context.into(),
        }
    }

    pub fn assert_zero(&self) -> &Self {
        if self.allocations > 0 {
            panic!(
                "Allocation violation in '{}': {} allocations ({} bytes)",
                self.context, self.allocations, self.bytes
            );
        }
        self
    }

    pub fn is_zero(&self) -> bool {
        self.allocations == 0
    }
}

impl std::fmt::Display for AllocationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.allocations > 0 {
            write!(
                f,
                "⚠️  {} allocated {} times ({} bytes)",
                self.context, self.allocations, self.bytes
            )
        } else {
            write!(f, "✅ {} - zero allocations", self.context)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_no_allocations() {
        let guard = track();
        let x = 42;
        let _y = x + 1;
        assert_rt_safe!(guard);
    }

    #[test]
    #[should_panic(expected = "RT path allocation violation")]
    fn test_guard_with_allocations() {
        let guard = track();
        let _vec: Vec<i32> = vec![1, 2, 3];
        assert_rt_safe!(guard);
    }

    #[test]
    fn test_guard_allocations_count() {
        let guard = track();
        let _vec: Vec<i32> = vec![1, 2, 3, 4, 5];
        assert!(guard.allocations() > 0);
        assert!(guard.bytes() > 0);
    }

    #[test]
    fn test_guard_has_allocations() {
        let guard = track();
        assert!(!guard.has_allocations());
        let _vec: Vec<i32> = vec![1, 2, 3];
        assert!(guard.has_allocations());
    }

    #[test]
    fn test_allocation_report() {
        let report = AllocationReport::new("test");
        assert!(report.is_zero());
        report.assert_zero();
    }

    #[test]
    #[should_panic(expected = "Allocation violation")]
    fn test_allocation_report_assert() {
        let report = AllocationReport {
            allocations: 1,
            bytes: 100,
            context: "test".to_string(),
        };
        report.assert_zero();
    }

    #[test]
    fn test_allocation_report_display() {
        let zero = AllocationReport::new("zero");
        assert!(zero.to_string().contains("zero allocations"));

        let nonzero = AllocationReport {
            allocations: 3,
            bytes: 256,
            context: "nonzero".to_string(),
        };
        let s = nonzero.to_string();
        assert!(s.contains("3 times"));
        assert!(s.contains("256 bytes"));
    }

    #[test]
    fn test_nested_guards() {
        let guard1 = track();
        let guard2 = track();
        let _x = 1;
        assert_rt_safe!(guard2);
        assert_rt_safe!(guard1);
    }
}
