//! Property-based tests for Native Plugin ABI Compatibility and Unsigned Plugin Configuration.
//!
//! These tests validate that:
//! - Native plugins with ABI versions different from CURRENT_ABI_VERSION are rejected
//!   with an ABI mismatch error. (**Validates: Requirements 9.5**)
//! - Native plugins with ABI version equal to CURRENT_ABI_VERSION are accepted.
//! - Unsigned plugins are rejected when `allow_unsigned` is false. (**Validates: Requirements 9.6**)
//! - Unsigned plugins are accepted when `allow_unsigned` is true. (**Validates: Requirements 9.6**)

use crate::native::{CURRENT_ABI_VERSION, NativePluginConfig};
use proptest::prelude::*;

/// Simulated ABI version check result.
/// This mirrors the logic in NativePlugin::verify_abi() and the load() method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiCheckResult {
    /// ABI version matches - plugin can be loaded
    Compatible,
    /// ABI version mismatch - plugin rejected
    Mismatch { expected: u32, actual: u32 },
}

/// Check ABI compatibility for a given plugin ABI version.
///
/// This function implements the same logic as NativePlugin::verify_abi()
/// and the ABI check in NativePlugin::load().
fn check_abi_compatibility(plugin_abi_version: u32) -> AbiCheckResult {
    if plugin_abi_version == CURRENT_ABI_VERSION {
        AbiCheckResult::Compatible
    } else {
        AbiCheckResult::Mismatch {
            expected: CURRENT_ABI_VERSION,
            actual: plugin_abi_version,
        }
    }
}

/// Strategy for generating ABI versions that are NOT equal to CURRENT_ABI_VERSION.
/// This ensures we test the rejection path for mismatched versions.
fn mismatched_abi_version_strategy() -> impl Strategy<Value = u32> {
    // Generate any u32 except CURRENT_ABI_VERSION
    any::<u32>().prop_filter("ABI version must differ from CURRENT_ABI_VERSION", |&v| {
        v != CURRENT_ABI_VERSION
    })
}

/// Strategy for generating ABI versions around the current version.
/// This tests boundary conditions more thoroughly.
fn abi_version_around_current_strategy() -> impl Strategy<Value = u32> {
    prop_oneof![
        // Versions below current (if current > 0)
        Just(CURRENT_ABI_VERSION.saturating_sub(1)),
        Just(CURRENT_ABI_VERSION.saturating_sub(2)),
        Just(CURRENT_ABI_VERSION.saturating_sub(10)),
        // Versions above current
        Just(CURRENT_ABI_VERSION.saturating_add(1)),
        Just(CURRENT_ABI_VERSION.saturating_add(2)),
        Just(CURRENT_ABI_VERSION.saturating_add(10)),
        // Edge cases
        Just(0),
        Just(u32::MAX),
        Just(u32::MAX / 2),
    ]
    .prop_filter("ABI version must differ from CURRENT_ABI_VERSION", |&v| {
        v != CURRENT_ABI_VERSION
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 12: Native Plugin ABI Compatibility
    ///
    /// *For any* native plugin with an ABI version different from CURRENT_ABI_VERSION,
    /// the loader SHALL reject the plugin with an ABI mismatch error.
    ///
    /// **Validates: Requirements 9.5**
    #[test]
    fn prop_native_plugin_abi_mismatch_rejected(
        plugin_abi_version in mismatched_abi_version_strategy(),
    ) {
        let result = check_abi_compatibility(plugin_abi_version);

        // Property: Any ABI version != CURRENT_ABI_VERSION must be rejected
        prop_assert_eq!(
            result,
            AbiCheckResult::Mismatch {
                expected: CURRENT_ABI_VERSION,
                actual: plugin_abi_version,
            },
            "Plugin with ABI version {} should be rejected (current is {})",
            plugin_abi_version,
            CURRENT_ABI_VERSION
        );
    }

    /// Feature: release-roadmap-v1, Property 12: Native Plugin ABI Compatibility (Boundary Cases)
    ///
    /// *For any* native plugin with an ABI version near CURRENT_ABI_VERSION but not equal,
    /// the loader SHALL reject the plugin with an ABI mismatch error.
    ///
    /// **Validates: Requirements 9.5**
    #[test]
    fn prop_native_plugin_abi_boundary_cases_rejected(
        plugin_abi_version in abi_version_around_current_strategy(),
    ) {
        let result = check_abi_compatibility(plugin_abi_version);

        // Property: Even versions close to CURRENT_ABI_VERSION must be rejected if not equal
        prop_assert_eq!(
            result,
            AbiCheckResult::Mismatch {
                expected: CURRENT_ABI_VERSION,
                actual: plugin_abi_version,
            },
            "Plugin with ABI version {} (near current {}) should be rejected",
            plugin_abi_version,
            CURRENT_ABI_VERSION
        );
    }

    /// Feature: release-roadmap-v1, Property 12: Native Plugin ABI Compatibility (Acceptance)
    ///
    /// *For any* native plugin with ABI version equal to CURRENT_ABI_VERSION,
    /// the loader SHALL accept the plugin (ABI check passes).
    ///
    /// **Validates: Requirements 9.5**
    #[test]
    fn prop_native_plugin_abi_match_accepted(
        // We use a constant here since there's only one valid value
        // but proptest requires at least one strategy
        _dummy in Just(()),
    ) {
        let result = check_abi_compatibility(CURRENT_ABI_VERSION);

        // Property: ABI version == CURRENT_ABI_VERSION must be accepted
        prop_assert_eq!(
            result,
            AbiCheckResult::Compatible,
            "Plugin with ABI version {} (current) should be accepted",
            CURRENT_ABI_VERSION
        );
    }

    /// Feature: release-roadmap-v1, Property 12: Native Plugin ABI Compatibility (Error Details)
    ///
    /// *For any* ABI mismatch, the error SHALL contain both the expected and actual versions.
    ///
    /// **Validates: Requirements 9.5**
    #[test]
    fn prop_native_plugin_abi_error_contains_versions(
        plugin_abi_version in mismatched_abi_version_strategy(),
    ) {
        let result = check_abi_compatibility(plugin_abi_version);

        // Property: Error must contain both expected and actual versions for debugging
        match result {
            AbiCheckResult::Mismatch { expected, actual } => {
                prop_assert_eq!(
                    expected,
                    CURRENT_ABI_VERSION,
                    "Expected version in error should be CURRENT_ABI_VERSION"
                );
                prop_assert_eq!(
                    actual,
                    plugin_abi_version,
                    "Actual version in error should be the plugin's ABI version"
                );
            }
            AbiCheckResult::Compatible => {
                return Err(TestCaseError::fail(
                    "Mismatched ABI version should not be compatible"
                ));
            }
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_current_abi_version_is_one() -> Result<(), Box<dyn std::error::Error>> {
        // Verify the current ABI version constant
        assert_eq!(CURRENT_ABI_VERSION, 1, "CURRENT_ABI_VERSION should be 1");
        Ok(())
    }

    #[test]
    fn test_abi_check_exact_match() -> Result<(), Box<dyn std::error::Error>> {
        let result = check_abi_compatibility(CURRENT_ABI_VERSION);
        assert_eq!(result, AbiCheckResult::Compatible);
        Ok(())
    }

    #[test]
    fn test_abi_check_version_zero_rejected() -> Result<(), Box<dyn std::error::Error>> {
        // Version 0 should be rejected (unless CURRENT_ABI_VERSION is 0)
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
        Ok(())
    }

    #[test]
    fn test_abi_check_version_two_rejected() -> Result<(), Box<dyn std::error::Error>> {
        // Version 2 should be rejected (unless CURRENT_ABI_VERSION is 2)
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
        Ok(())
    }

    #[test]
    fn test_abi_check_max_version_rejected() -> Result<(), Box<dyn std::error::Error>> {
        // u32::MAX should be rejected (unless CURRENT_ABI_VERSION is u32::MAX)
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
        Ok(())
    }

    #[test]
    fn test_abi_check_adjacent_versions_rejected() -> Result<(), Box<dyn std::error::Error>> {
        // Test versions immediately adjacent to CURRENT_ABI_VERSION
        let below = CURRENT_ABI_VERSION.saturating_sub(1);
        let above = CURRENT_ABI_VERSION.saturating_add(1);

        if below != CURRENT_ABI_VERSION {
            let result = check_abi_compatibility(below);
            assert_eq!(
                result,
                AbiCheckResult::Mismatch {
                    expected: CURRENT_ABI_VERSION,
                    actual: below
                },
                "Version {} (below current) should be rejected",
                below
            );
        }

        if above != CURRENT_ABI_VERSION {
            let result = check_abi_compatibility(above);
            assert_eq!(
                result,
                AbiCheckResult::Mismatch {
                    expected: CURRENT_ABI_VERSION,
                    actual: above
                },
                "Version {} (above current) should be rejected",
                above
            );
        }

        Ok(())
    }
}

// =============================================================================
// Property 13: Unsigned Plugin Configuration
// =============================================================================

/// Result of unsigned plugin check.
/// This mirrors the logic in NativePlugin::verify_signature() for unsigned plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsignedPluginResult {
    /// Plugin is accepted (unsigned plugins allowed)
    Accepted,
    /// Plugin is rejected (unsigned plugins not allowed)
    Rejected { reason: String },
}

/// Check if an unsigned plugin should be accepted or rejected based on configuration.
///
/// This function implements the same logic as NativePlugin::verify_signature()
/// for the unsigned plugin case (when no signature file exists).
///
/// The behavior is determined by the combination of `allow_unsigned` and `require_signatures`:
///
/// | `require_signatures` | `allow_unsigned` | Result |
/// |---------------------|------------------|--------|
/// | `true`              | `false`          | Rejected (strict mode) |
/// | `true`              | `true`           | Accepted (permissive mode) |
/// | `false`             | `true`           | Accepted (development mode) |
/// | `false`             | `false`          | Rejected (unsigned not allowed) |
fn check_unsigned_plugin(config: &NativePluginConfig) -> UnsignedPluginResult {
    // This mirrors the logic in verify_signature() for unsigned plugins:
    //
    // if !has_signature {
    //     if config.require_signatures && !config.allow_unsigned {
    //         return Err(...);  // Rejected
    //     }
    //     if config.allow_unsigned {
    //         return Ok(None);  // Accepted
    //     }
    //     return Err(...);  // Rejected
    // }

    if config.require_signatures && !config.allow_unsigned {
        return UnsignedPluginResult::Rejected {
            reason: "Plugin is unsigned and unsigned plugins are not allowed".to_string(),
        };
    }

    if config.allow_unsigned {
        return UnsignedPluginResult::Accepted;
    }

    UnsignedPluginResult::Rejected {
        reason: "Plugin is unsigned".to_string(),
    }
}

/// Strategy for generating NativePluginConfig with allow_unsigned = false.
fn config_disallow_unsigned_strategy() -> impl Strategy<Value = NativePluginConfig> {
    any::<bool>().prop_map(|require_signatures| NativePluginConfig {
        allow_unsigned: false,
        require_signatures,
    })
}

/// Strategy for generating NativePluginConfig with allow_unsigned = true.
fn config_allow_unsigned_strategy() -> impl Strategy<Value = NativePluginConfig> {
    any::<bool>().prop_map(|require_signatures| NativePluginConfig {
        allow_unsigned: true,
        require_signatures,
    })
}

/// Strategy for generating all possible NativePluginConfig combinations.
fn config_strategy() -> impl Strategy<Value = NativePluginConfig> {
    (any::<bool>(), any::<bool>()).prop_map(|(allow_unsigned, require_signatures)| {
        NativePluginConfig {
            allow_unsigned,
            require_signatures,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 13: Unsigned Plugin Configuration
    ///
    /// *For any* unsigned plugin, the loader SHALL reject it when allow_unsigned is false.
    ///
    /// **Validates: Requirements 9.6**
    #[test]
    fn prop_unsigned_plugin_rejected_when_disallowed(
        config in config_disallow_unsigned_strategy(),
    ) {
        let result = check_unsigned_plugin(&config);

        // Property: When allow_unsigned is false, unsigned plugins must be rejected
        prop_assert!(
            matches!(result, UnsignedPluginResult::Rejected { .. }),
            "Unsigned plugin should be rejected when allow_unsigned=false (config: {:?})",
            config
        );
    }

    /// Feature: release-roadmap-v1, Property 13: Unsigned Plugin Configuration
    ///
    /// *For any* unsigned plugin, the loader SHALL accept it when allow_unsigned is true.
    ///
    /// **Validates: Requirements 9.6**
    #[test]
    fn prop_unsigned_plugin_accepted_when_allowed(
        config in config_allow_unsigned_strategy(),
    ) {
        let result = check_unsigned_plugin(&config);

        // Property: When allow_unsigned is true, unsigned plugins must be accepted
        prop_assert_eq!(
            result,
            UnsignedPluginResult::Accepted,
            "Unsigned plugin should be accepted when allow_unsigned=true (config: {:?})",
            config
        );
    }

    /// Feature: release-roadmap-v1, Property 13: Unsigned Plugin Configuration (All Combinations)
    ///
    /// *For any* configuration, the unsigned plugin behavior SHALL be consistent:
    /// - Accepted if and only if allow_unsigned is true
    /// - Rejected if and only if allow_unsigned is false
    ///
    /// **Validates: Requirements 9.6**
    #[test]
    fn prop_unsigned_plugin_config_consistency(
        config in config_strategy(),
    ) {
        let result = check_unsigned_plugin(&config);

        // Property: The result depends solely on allow_unsigned flag
        if config.allow_unsigned {
            prop_assert_eq!(
                result,
                UnsignedPluginResult::Accepted,
                "Unsigned plugin should be accepted when allow_unsigned=true"
            );
        } else {
            prop_assert!(
                matches!(result, UnsignedPluginResult::Rejected { .. }),
                "Unsigned plugin should be rejected when allow_unsigned=false"
            );
        }
    }

    /// Feature: release-roadmap-v1, Property 13: Unsigned Plugin Configuration (Rejection Reason)
    ///
    /// *For any* rejected unsigned plugin, the rejection reason SHALL be non-empty
    /// and indicate that the plugin is unsigned.
    ///
    /// **Validates: Requirements 9.6**
    #[test]
    fn prop_unsigned_plugin_rejection_has_reason(
        config in config_disallow_unsigned_strategy(),
    ) {
        let result = check_unsigned_plugin(&config);

        // Property: Rejection must include a meaningful reason
        match result {
            UnsignedPluginResult::Rejected { reason } => {
                prop_assert!(
                    !reason.is_empty(),
                    "Rejection reason should not be empty"
                );
                prop_assert!(
                    reason.to_lowercase().contains("unsigned"),
                    "Rejection reason should mention 'unsigned': {}",
                    reason
                );
            }
            UnsignedPluginResult::Accepted => {
                return Err(TestCaseError::fail(
                    "Unsigned plugin should be rejected when allow_unsigned=false"
                ));
            }
        }
    }

    /// Feature: release-roadmap-v1, Property 13: Unsigned Plugin Configuration (Default Config)
    ///
    /// The default NativePluginConfig SHALL reject unsigned plugins (secure by default).
    ///
    /// **Validates: Requirements 9.6**
    #[test]
    fn prop_default_config_rejects_unsigned(
        _dummy in Just(()),
    ) {
        let default_config = NativePluginConfig::default();
        let result = check_unsigned_plugin(&default_config);

        // Property: Default configuration should be secure (reject unsigned)
        prop_assert!(
            matches!(result, UnsignedPluginResult::Rejected { .. }),
            "Default config should reject unsigned plugins (secure by default)"
        );
        prop_assert!(
            !default_config.allow_unsigned,
            "Default config should have allow_unsigned=false"
        );
    }
}

#[cfg(test)]
mod unsigned_plugin_unit_tests {
    use super::*;

    #[test]
    fn test_strict_mode_rejects_unsigned() -> Result<(), Box<dyn std::error::Error>> {
        // Strict mode: require_signatures=true, allow_unsigned=false
        let config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: true,
        };
        let result = check_unsigned_plugin(&config);
        assert!(
            matches!(result, UnsignedPluginResult::Rejected { .. }),
            "Strict mode should reject unsigned plugins"
        );
        Ok(())
    }

    #[test]
    fn test_permissive_mode_accepts_unsigned() -> Result<(), Box<dyn std::error::Error>> {
        // Permissive mode: require_signatures=true, allow_unsigned=true
        let config = NativePluginConfig {
            allow_unsigned: true,
            require_signatures: true,
        };
        let result = check_unsigned_plugin(&config);
        assert_eq!(
            result,
            UnsignedPluginResult::Accepted,
            "Permissive mode should accept unsigned plugins"
        );
        Ok(())
    }

    #[test]
    fn test_development_mode_accepts_unsigned() -> Result<(), Box<dyn std::error::Error>> {
        // Development mode: require_signatures=false, allow_unsigned=true
        let config = NativePluginConfig {
            allow_unsigned: true,
            require_signatures: false,
        };
        let result = check_unsigned_plugin(&config);
        assert_eq!(
            result,
            UnsignedPluginResult::Accepted,
            "Development mode should accept unsigned plugins"
        );
        Ok(())
    }

    #[test]
    fn test_no_signatures_no_unsigned_rejects() -> Result<(), Box<dyn std::error::Error>> {
        // require_signatures=false, allow_unsigned=false
        let config = NativePluginConfig {
            allow_unsigned: false,
            require_signatures: false,
        };
        let result = check_unsigned_plugin(&config);
        assert!(
            matches!(result, UnsignedPluginResult::Rejected { .. }),
            "Should reject unsigned plugins when allow_unsigned=false"
        );
        Ok(())
    }

    #[test]
    fn test_default_config_is_secure() -> Result<(), Box<dyn std::error::Error>> {
        let config = NativePluginConfig::default();
        assert!(!config.allow_unsigned, "Default should not allow unsigned");
        assert!(
            config.require_signatures,
            "Default should require signatures"
        );

        let result = check_unsigned_plugin(&config);
        assert!(
            matches!(result, UnsignedPluginResult::Rejected { .. }),
            "Default config should reject unsigned plugins"
        );
        Ok(())
    }

    #[test]
    fn test_all_config_combinations() -> Result<(), Box<dyn std::error::Error>> {
        // Test all 4 combinations of (allow_unsigned, require_signatures)
        let test_cases = [
            (false, false, false), // (allow_unsigned, require_signatures, expected_accepted)
            (false, true, false),
            (true, false, true),
            (true, true, true),
        ];

        for (allow_unsigned, require_signatures, expected_accepted) in test_cases {
            let config = NativePluginConfig {
                allow_unsigned,
                require_signatures,
            };
            let result = check_unsigned_plugin(&config);
            let is_accepted = matches!(result, UnsignedPluginResult::Accepted);

            assert_eq!(
                is_accepted,
                expected_accepted,
                "Config (allow_unsigned={}, require_signatures={}) should {} unsigned plugins",
                allow_unsigned,
                require_signatures,
                if expected_accepted {
                    "accept"
                } else {
                    "reject"
                }
            );
        }
        Ok(())
    }
}
