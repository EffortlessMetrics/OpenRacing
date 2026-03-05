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
    /// Plugin has an older minor version within the same major – forward-compatible.
    ForwardCompatible {
        /// Host ABI version.
        host: u32,
        /// Plugin ABI version.
        plugin: u32,
    },
    /// ABI version mismatch - plugin rejected.
    Mismatch {
        /// Expected ABI version.
        expected: u32,
        /// Actual ABI version from plugin.
        actual: u32,
    },
}

impl AbiCheckResult {
    /// Returns `true` when the plugin can safely be loaded.
    #[must_use]
    pub fn is_loadable(&self) -> bool {
        matches!(
            self,
            AbiCheckResult::Compatible | AbiCheckResult::ForwardCompatible { .. }
        )
    }
}

/// Check ABI compatibility for a given plugin ABI version.
///
/// Native plugins use a simple incrementing version number and require
/// an exact match.  For forward-compatible version negotiation using
/// the packed major.minor scheme, see [`check_abi_compatibility_packed`].
///
/// # Arguments
///
/// * `plugin_abi_version` - The ABI version reported by the plugin.
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

/// Check ABI compatibility using packed major.minor version format.
///
/// This allows forward-compatible loading: a host with version X.Y can
/// load a plugin with version X.Z where Z ≤ Y (same major, older minor).
///
/// Use this for plugin headers that encode their version with
/// [`openracing_plugin_abi::abi_version_pack`].
pub fn check_abi_compatibility_packed(host_version: u32, plugin_version: u32) -> AbiCheckResult {
    if host_version == plugin_version {
        AbiCheckResult::Compatible
    } else if openracing_plugin_abi::is_abi_compatible(host_version, plugin_version) {
        AbiCheckResult::ForwardCompatible {
            host: host_version,
            plugin: plugin_version,
        }
    } else {
        AbiCheckResult::Mismatch {
            expected: host_version,
            actual: plugin_version,
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
        assert!(result.is_loadable());
    }

    #[test]
    fn test_incompatible_version_zero() {
        if CURRENT_ABI_VERSION != 0 {
            let result = check_abi_compatibility(0);
            assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
            assert!(!result.is_loadable());
        }
    }

    #[test]
    fn test_incompatible_version_two() {
        if CURRENT_ABI_VERSION != 2 {
            let result = check_abi_compatibility(2);
            assert!(!result.is_loadable());
        }
    }

    #[test]
    fn test_incompatible_max_version() {
        if CURRENT_ABI_VERSION != u32::MAX {
            let result = check_abi_compatibility(u32::MAX);
            assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
            assert!(!result.is_loadable());
        }
    }

    #[test]
    fn test_adjacent_versions_rejected() {
        let below = CURRENT_ABI_VERSION.saturating_sub(1);
        let above = CURRENT_ABI_VERSION.saturating_add(1);

        if below != CURRENT_ABI_VERSION {
            let result = check_abi_compatibility(below);
            assert!(!result.is_loadable());
        }

        if above != CURRENT_ABI_VERSION {
            let result = check_abi_compatibility(above);
            assert!(!result.is_loadable());
        }
    }

    #[test]
    fn test_packed_exact_match_compatible() {
        let v = openracing_plugin_abi::PLUG_ABI_VERSION;
        let result = check_abi_compatibility_packed(v, v);
        assert_eq!(result, AbiCheckResult::Compatible);
        assert!(result.is_loadable());
    }

    #[test]
    fn test_packed_forward_compatible_older_minor() {
        let host = openracing_plugin_abi::abi_version_pack(1, 3);
        let plugin = openracing_plugin_abi::abi_version_pack(1, 0);
        let result = check_abi_compatibility_packed(host, plugin);
        assert!(
            matches!(result, AbiCheckResult::ForwardCompatible { .. }),
            "older minor should be forward-compatible"
        );
        assert!(result.is_loadable());
    }

    #[test]
    fn test_packed_rejects_newer_minor() {
        let host = openracing_plugin_abi::abi_version_pack(1, 0);
        let plugin = openracing_plugin_abi::abi_version_pack(1, 2);
        let result = check_abi_compatibility_packed(host, plugin);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
        assert!(!result.is_loadable());
    }

    #[test]
    fn test_packed_rejects_different_major() {
        let host = openracing_plugin_abi::abi_version_pack(1, 0);
        let plugin = openracing_plugin_abi::abi_version_pack(2, 0);
        let result = check_abi_compatibility_packed(host, plugin);
        assert!(matches!(result, AbiCheckResult::Mismatch { .. }));
        assert!(!result.is_loadable());
    }
}
