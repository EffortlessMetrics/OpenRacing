//! Property-based tests for native plugin ABI compatibility.

use openracing_native_plugin::{AbiCheckResult, CURRENT_ABI_VERSION, check_abi_compatibility};
use proptest::prelude::*;

fn mismatched_abi_version_strategy() -> impl Strategy<Value = u32> {
    any::<u32>().prop_filter("ABI version must differ from CURRENT_ABI_VERSION", |&v| {
        v != CURRENT_ABI_VERSION
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_abi_mismatch_rejected(plugin_abi_version in mismatched_abi_version_strategy()) {
        let result = check_abi_compatibility(plugin_abi_version);

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

    #[test]
    fn prop_abi_match_accepted(_dummy in Just(())) {
        let result = check_abi_compatibility(CURRENT_ABI_VERSION);

        prop_assert_eq!(
            result,
            AbiCheckResult::Compatible,
            "Plugin with ABI version {} (current) should be accepted",
            CURRENT_ABI_VERSION
        );
    }

    #[test]
    fn prop_abi_error_contains_versions(plugin_abi_version in mismatched_abi_version_strategy()) {
        let result = check_abi_compatibility(plugin_abi_version);

        match result {
            AbiCheckResult::Mismatch { expected, actual } => {
                prop_assert_eq!(expected, CURRENT_ABI_VERSION);
                prop_assert_eq!(actual, plugin_abi_version);
            }
            AbiCheckResult::Compatible => {
                return Err(TestCaseError::fail("Mismatched ABI version should not be compatible"));
            }
        }
    }
}
