//! Resource limits configuration for WASM plugins.
//!
//! This module provides configuration for limiting plugin resource consumption,
//! including memory, fuel (instruction count), and instance limits.

use std::time::Duration;

/// Resource limits for WASM plugins.
///
/// These limits ensure plugins cannot consume excessive system resources.
/// All limits are enforced by the wasmtime runtime.
///
/// # Security Considerations
///
/// - Memory limits prevent plugins from consuming excessive RAM
/// - Fuel limits prevent infinite loops and denial-of-service attacks
/// - Instance limits prevent resource exhaustion from too many plugins
/// - Table element limits prevent certain memory exploits
///
/// # Example
///
/// ```
/// use openracing_wasm_runtime::ResourceLimits;
///
/// let limits = ResourceLimits::default()
///     .with_memory(8 * 1024 * 1024)   // 8MB memory
///     .with_fuel(5_000_000)           // 5M instructions per call
///     .with_max_instances(16);        // Max 16 plugins
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    /// Maximum memory in bytes a plugin can allocate.
    ///
    /// Default: 16MB (16 * 1024 * 1024)
    ///
    /// This limit prevents plugins from consuming excessive memory.
    /// Memory is allocated in 64KB pages by WASM, so the actual
    /// allocation will be rounded up to the nearest page boundary.
    pub max_memory_bytes: usize,

    /// Maximum fuel (instruction count) per call.
    ///
    /// Default: 10_000_000 (~10M instructions)
    ///
    /// Each WASM instruction consumes fuel. When fuel runs out,
    /// execution is interrupted. This prevents infinite loops
    /// and denial-of-service attacks.
    ///
    /// Rough timing estimates:
    /// - 10M instructions ≈ 10-100ms on modern hardware
    /// - 1M instructions ≈ 1-10ms on modern hardware
    pub max_fuel: u64,

    /// Maximum number of table elements.
    ///
    /// Default: 10_000
    ///
    /// This limits the size of the function table, which can
    /// be used for indirect calls. Large tables can be used
    /// for certain exploits.
    pub max_table_elements: u32,

    /// Maximum number of plugin instances.
    ///
    /// Default: 32
    ///
    /// This limits the total number of plugins that can be loaded
    /// simultaneously. Each plugin instance consumes resources
    /// (memory, file handles, etc.).
    pub max_instances: usize,

    /// Maximum execution time per call.
    ///
    /// Default: None (no timeout)
    ///
    /// When set, this provides a hard timeout for plugin execution.
    /// This is in addition to fuel-based limits and provides a
    /// safety net for edge cases.
    pub max_execution_time: Option<Duration>,

    /// Enable epoch interruption.
    ///
    /// Default: true
    ///
    /// When enabled, the runtime can interrupt long-running plugins
    /// by incrementing the epoch. This is useful for implementing
    /// cancellation or timeouts.
    pub epoch_interruption: bool,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 16 * 1024 * 1024,
            max_fuel: 10_000_000,
            max_table_elements: 10_000,
            max_instances: 32,
            max_execution_time: None,
            epoch_interruption: true,
        }
    }
}

impl ResourceLimits {
    /// Create new resource limits with custom values.
    ///
    /// # Arguments
    ///
    /// * `max_memory_bytes` - Maximum memory in bytes
    /// * `max_fuel` - Maximum fuel (instruction count) per call
    /// * `max_table_elements` - Maximum table elements
    /// * `max_instances` - Maximum number of plugin instances
    #[must_use]
    pub fn new(
        max_memory_bytes: usize,
        max_fuel: u64,
        max_table_elements: u32,
        max_instances: usize,
    ) -> Self {
        Self {
            max_memory_bytes,
            max_fuel,
            max_table_elements,
            max_instances,
            max_execution_time: None,
            epoch_interruption: true,
        }
    }

    /// Create resource limits with a specific memory limit.
    #[must_use]
    pub fn with_memory(mut self, max_memory_bytes: usize) -> Self {
        self.max_memory_bytes = max_memory_bytes;
        self
    }

    /// Create resource limits with a specific fuel limit.
    #[must_use]
    pub fn with_fuel(mut self, max_fuel: u64) -> Self {
        self.max_fuel = max_fuel;
        self
    }

    /// Create resource limits with a specific table elements limit.
    #[must_use]
    pub fn with_table_elements(mut self, max_table_elements: u32) -> Self {
        self.max_table_elements = max_table_elements;
        self
    }

    /// Create resource limits with a specific max instances limit.
    #[must_use]
    pub fn with_max_instances(mut self, max_instances: usize) -> Self {
        self.max_instances = max_instances;
        self
    }

    /// Create resource limits with a specific execution timeout.
    #[must_use]
    pub fn with_execution_time(mut self, max_execution_time: Duration) -> Self {
        self.max_execution_time = Some(max_execution_time);
        self
    }

    /// Create resource limits with epoch interruption enabled/disabled.
    #[must_use]
    pub fn with_epoch_interruption(mut self, enabled: bool) -> Self {
        self.epoch_interruption = enabled;
        self
    }

    /// Create conservative limits for untrusted plugins.
    ///
    /// These limits are suitable for plugins from untrusted sources:
    /// - 4MB memory
    /// - 1M instructions per call
    /// - 8 instances maximum
    /// - 1 second execution timeout
    #[must_use]
    pub fn conservative() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024,
            max_fuel: 1_000_000,
            max_table_elements: 1_000,
            max_instances: 8,
            max_execution_time: Some(Duration::from_secs(1)),
            epoch_interruption: true,
        }
    }

    /// Create generous limits for trusted plugins.
    ///
    /// These limits are suitable for trusted plugins:
    /// - 64MB memory
    /// - 50M instructions per call
    /// - 128 instances maximum
    /// - No execution timeout
    #[must_use]
    pub fn generous() -> Self {
        Self {
            max_memory_bytes: 64 * 1024 * 1024,
            max_fuel: 50_000_000,
            max_table_elements: 50_000,
            max_instances: 128,
            max_execution_time: None,
            epoch_interruption: true,
        }
    }

    /// Validate that the limits are reasonable.
    ///
    /// # Errors
    ///
    /// Returns an error if any limit is unreasonably small or large.
    pub fn validate(&self) -> Result<(), String> {
        const MIN_MEMORY: usize = 64 * 1024;
        const MAX_MEMORY: usize = 4 * 1024 * 1024 * 1024;
        const MIN_FUEL: u64 = 1000;
        const MAX_FUEL: u64 = 10_000_000_000;

        if self.max_memory_bytes < MIN_MEMORY {
            return Err(format!(
                "Memory limit {} is below minimum {}",
                self.max_memory_bytes, MIN_MEMORY
            ));
        }

        if self.max_memory_bytes > MAX_MEMORY {
            return Err(format!(
                "Memory limit {} exceeds maximum {}",
                self.max_memory_bytes, MAX_MEMORY
            ));
        }

        if self.max_fuel < MIN_FUEL {
            return Err(format!(
                "Fuel limit {} is below minimum {}",
                self.max_fuel, MIN_FUEL
            ));
        }

        if self.max_fuel > MAX_FUEL {
            return Err(format!(
                "Fuel limit {} exceeds maximum {}",
                self.max_fuel, MAX_FUEL
            ));
        }

        if self.max_instances == 0 {
            return Err("Max instances must be at least 1".to_string());
        }

        if self.max_instances > 1000 {
            return Err(format!(
                "Max instances {} exceeds reasonable limit 1000",
                self.max_instances
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 10_000_000);
        assert_eq!(limits.max_table_elements, 10_000);
        assert_eq!(limits.max_instances, 32);
        assert!(limits.max_execution_time.is_none());
        assert!(limits.epoch_interruption);
    }

    #[test]
    fn test_builder_pattern() {
        let limits = ResourceLimits::default()
            .with_memory(32 * 1024 * 1024)
            .with_fuel(20_000_000)
            .with_table_elements(20_000)
            .with_max_instances(64)
            .with_execution_time(Duration::from_millis(100))
            .with_epoch_interruption(false);

        assert_eq!(limits.max_memory_bytes, 32 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 20_000_000);
        assert_eq!(limits.max_table_elements, 20_000);
        assert_eq!(limits.max_instances, 64);
        assert_eq!(limits.max_execution_time, Some(Duration::from_millis(100)));
        assert!(!limits.epoch_interruption);
    }

    #[test]
    fn test_conservative_limits() {
        let limits = ResourceLimits::conservative();
        assert_eq!(limits.max_memory_bytes, 4 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 1_000_000);
        assert_eq!(limits.max_instances, 8);
        assert!(limits.max_execution_time.is_some());
    }

    #[test]
    fn test_generous_limits() {
        let limits = ResourceLimits::generous();
        assert_eq!(limits.max_memory_bytes, 64 * 1024 * 1024);
        assert_eq!(limits.max_fuel, 50_000_000);
        assert_eq!(limits.max_instances, 128);
        assert!(limits.max_execution_time.is_none());
    }

    #[test]
    fn test_validate_valid_limits() {
        let limits = ResourceLimits::default();
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_validate_memory_too_small() {
        let limits = ResourceLimits::default().with_memory(1024);
        assert!(limits.validate().is_err());
    }

    #[test]
    fn test_validate_fuel_too_small() {
        let limits = ResourceLimits::default().with_fuel(100);
        assert!(limits.validate().is_err());
    }

    #[test]
    fn test_validate_zero_instances() {
        let limits = ResourceLimits::default().with_max_instances(0);
        assert!(limits.validate().is_err());
    }
}
