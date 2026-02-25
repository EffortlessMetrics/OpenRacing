//! ABI version checking for native plugins.

/// Current native plugin ABI version.
///
/// This version must match the plugin's ABI version for successful loading.
/// Increment this when making breaking changes to the plugin ABI.
pub const CURRENT_ABI_VERSION: u32 = 1;

/// Result of ABI compatibility check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiCheckResult {
    /// ABI version matches - plugin can be loaded.
    Compatible,
    /// ABI version mismatch - plugin rejected.
    Mismatch {
        /// Expected ABI version.
        expected: u32,
        /// Actual ABI version from plugin.
        actual: u32,
    },
}

/// Check ABI compatibility for a given plugin ABI version.
///
/// # Arguments
///
/// * `plugin_abi_version` - The ABI version reported by the plugin.
///
/// # Returns
///
/// * `AbiCheckResult::Compatible` if versions match.
/// * `AbiCheckResult::Mismatch` if versions don't match.
///
/// # Example
///
/// ```
/// use openracing_native_plugin::abi_check::{check_abi_compatibility, CURRENT_ABI_VERSION, AbiCheckResult};
///
/// let result = check_abi_compatibility(CURRENT_ABI_VERSION);
/// assert_eq!(result, AbiCheckResult::Compatible);
///
/// let result = check_abi_compatibility(999);
/// assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
/// ```
pub fn check_abi_compatibility(plugin_abi_version: u32) -> AbiCheckResult {
    if plugin_abi_version == CURRENT_ABI_VERSION {
        AbiCheckResult::Compatible
    } else {
        AbiCheckResult::Mismatch {
            expected: CURRENT_ABI_VERSION,
            actual: plugin_abi_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_abi_version_is_one() {
        assert_eq!(CURRENT_ABI_VERSION, 1);
    }

    #[test]
    fn test_compatible_version() {
        let result = check_abi_compatibility(CURRENT_ABI_VERSION);
        assert_eq!(result, AbiCheckResult::Compatible);
    }

    #[test]
    fn test_incompatible_version_zero() {
        if CURRENT_ABI_VERSION != 0 {
            let result = check_abi_compatibility(0);
            assert_eq!(
                result,
                AbiCheckResult::Mismatch {
                    expected: CURRENT_ABI_VERSION,
                    actual: 0
                }
            );
        }
    }

    #[test]
    fn test_incompatible_version_two() {
        if CURRENT_ABI_VERSION != 2 {
            let result = check_abi_compatibility(2);
            assert_eq!(
                result,
                AbiCheckResult::Mismatch {
                    expected: CURRENT_ABI_VERSION,
                    actual: 2
                }
            );
        }
    }

    #[test]
    fn test_incompatible_max_version() {
        if CURRENT_ABI_VERSION != u32::MAX {
            let result = check_abi_compatibility(u32::MAX);
            assert_eq!(
                result,
                AbiCheckResult::Mismatch {
                    expected: CURRENT_ABI_VERSION,
                    actual: u32::MAX
                }
            );
        }
    }

    #[test]
    fn test_adjacent_versions_rejected() {
        let below = CURRENT_ABI_VERSION.saturating_sub(1);
        let above = CURRENT_ABI_VERSION.saturating_add(1);

        if below != CURRENT_ABI_VERSION {
            let result = check_abi_compatibility(below);
            assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
        }

        if above != CURRENT_ABI_VERSION {
            let result = check_abi_compatibility(above);
            assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
        }
    }
}
