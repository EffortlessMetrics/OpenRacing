//! Real-time setup configuration.

/// Real-time setup configuration.
///
/// This struct defines the real-time parameters to apply to the scheduling thread
/// for optimal timing precision.
#[derive(Debug, Clone)]
pub struct RTSetup {
    /// Enable high-priority scheduling.
    ///
    /// On Windows: Sets thread to TIME_CRITICAL priority.
    /// On Linux: Sets thread to SCHED_FIFO with priority 80.
    pub high_priority: bool,

    /// Enable memory locking (prevent swapping).
    ///
    /// Locks all current and future memory pages to prevent page faults
    /// during real-time operation.
    pub lock_memory: bool,

    /// Disable power throttling.
    ///
    /// Requests the OS to maintain consistent CPU performance.
    pub disable_power_throttling: bool,

    /// CPU affinity mask (None = no affinity).
    ///
    /// Restricts the thread to run on specific CPU cores.
    /// Each bit represents a CPU core (bit 0 = core 0, etc.).
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

impl RTSetup {
    /// Create a new RTSetup with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a minimal RTSetup (no special configuration).
    pub fn minimal() -> Self {
        Self {
            high_priority: false,
            lock_memory: false,
            disable_power_throttling: false,
            cpu_affinity: None,
        }
    }

    /// Create RTSetup optimized for testing (less aggressive).
    pub fn testing() -> Self {
        Self {
            high_priority: false,
            lock_memory: false,
            disable_power_throttling: true,
            cpu_affinity: None,
        }
    }

    /// Set high priority.
    pub fn with_high_priority(mut self, enabled: bool) -> Self {
        self.high_priority = enabled;
        self
    }

    /// Set memory locking.
    pub fn with_lock_memory(mut self, enabled: bool) -> Self {
        self.lock_memory = enabled;
        self
    }

    /// Set power throttling.
    pub fn with_disable_power_throttling(mut self, enabled: bool) -> Self {
        self.disable_power_throttling = enabled;
        self
    }

    /// Set CPU affinity mask.
    pub fn with_cpu_affinity(mut self, mask: u64) -> Self {
        self.cpu_affinity = Some(mask);
        self
    }

    /// Check if any RT features are enabled.
    pub fn has_rt_features(&self) -> bool {
        self.high_priority
            || self.lock_memory
            || self.disable_power_throttling
            || self.cpu_affinity.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let setup = RTSetup::default();
        assert!(setup.high_priority);
        assert!(setup.lock_memory);
        assert!(setup.disable_power_throttling);
        assert!(setup.cpu_affinity.is_none());
    }

    #[test]
    fn test_minimal() {
        let setup = RTSetup::minimal();
        assert!(!setup.high_priority);
        assert!(!setup.lock_memory);
        assert!(!setup.disable_power_throttling);
        assert!(setup.cpu_affinity.is_none());
    }

    #[test]
    fn test_testing() {
        let setup = RTSetup::testing();
        assert!(!setup.high_priority);
        assert!(!setup.lock_memory);
        assert!(setup.disable_power_throttling);
    }

    #[test]
    fn test_builder_pattern() {
        let setup = RTSetup::new()
            .with_high_priority(false)
            .with_lock_memory(true)
            .with_cpu_affinity(0x0F);

        assert!(!setup.high_priority);
        assert!(setup.lock_memory);
        assert_eq!(setup.cpu_affinity, Some(0x0F));
    }

    #[test]
    fn test_has_rt_features() {
        let minimal = RTSetup::minimal();
        assert!(!minimal.has_rt_features());

        let with_affinity = RTSetup::minimal().with_cpu_affinity(1);
        assert!(with_affinity.has_rt_features());

        let default = RTSetup::default();
        assert!(default.has_rt_features());
    }
}
