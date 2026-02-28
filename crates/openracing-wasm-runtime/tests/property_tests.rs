//! Property-based tests for WASM runtime resource limits.

use openracing_wasm_runtime::ResourceLimits;
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_resource_limits_validation_memory(
        memory in 64 * 1024usize..=4 * 1024 * 1024 * 1024,
    ) {
        let limits = ResourceLimits::default().with_memory(memory);
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_resource_limits_validation_fuel(
        fuel in 1000u64..=10_000_000_000,
    ) {
        let limits = ResourceLimits::default().with_fuel(fuel);
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_resource_limits_validation_instances(
        instances in 1usize..=1000,
    ) {
        let limits = ResourceLimits::default().with_max_instances(instances);
        prop_assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_resource_limits_invalid_memory(
        memory in 0usize..64 * 1024,
    ) {
        let limits = ResourceLimits::default().with_memory(memory);
        prop_assert!(limits.validate().is_err());
    }

    #[test]
    fn test_resource_limits_invalid_fuel(
        fuel in 0u64..1000,
    ) {
        let limits = ResourceLimits::default().with_fuel(fuel);
        prop_assert!(limits.validate().is_err());
    }

    #[test]
    fn test_resource_limits_builder_preserves_other_values(
        memory in 1024 * 1024usize..=64 * 1024 * 1024,
        fuel in 1_000_000u64..=50_000_000,
        instances in 8usize..=64,
    ) {
        let limits = ResourceLimits::default()
            .with_memory(memory)
            .with_fuel(fuel)
            .with_max_instances(instances);

        prop_assert_eq!(limits.max_memory_bytes, memory);
        prop_assert_eq!(limits.max_fuel, fuel);
        prop_assert_eq!(limits.max_instances, instances);
    }

    #[test]
    fn test_resource_limits_clone(
        memory in 1024 * 1024usize..=64 * 1024 * 1024,
        fuel in 1_000_000u64..=50_000_000,
    ) {
        let limits = ResourceLimits::default()
            .with_memory(memory)
            .with_fuel(fuel);

        let cloned = limits;

        prop_assert_eq!(limits.max_memory_bytes, cloned.max_memory_bytes);
        prop_assert_eq!(limits.max_fuel, cloned.max_fuel);
    }
}
