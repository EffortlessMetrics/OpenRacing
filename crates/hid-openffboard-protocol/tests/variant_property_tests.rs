//! Property-based tests for OpenFFBoard device variant consistency.
//!
//! Verifies PID consistency, VID correctness, and variant name invariants
//! across all known OpenFFBoard device variants.

use proptest::prelude::*;
use racing_wheel_hid_openffboard_protocol::{
    is_openffboard_product, OpenFFBoardVariant, OPENFFBOARD_PRODUCT_ID,
    OPENFFBOARD_PRODUCT_ID_ALT, OPENFFBOARD_VENDOR_ID,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Every variant's product_id must be recognised by is_openffboard_product.
    #[test]
    fn prop_variant_pid_recognised(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        prop_assert!(
            is_openffboard_product(variant.product_id()),
            "variant {:?} PID {:#06X} must be recognised",
            variant,
            variant.product_id()
        );
    }

    /// Every variant's product_id must match the corresponding constant.
    #[test]
    fn prop_variant_pid_matches_constant(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        let expected = match variant {
            OpenFFBoardVariant::Main => OPENFFBOARD_PRODUCT_ID,
            OpenFFBoardVariant::Alternate => OPENFFBOARD_PRODUCT_ID_ALT,
        };
        prop_assert_eq!(
            variant.product_id(),
            expected,
            "variant {:?} PID must match constant",
            variant
        );
    }

    /// All variant PIDs must be unique.
    #[test]
    fn prop_variant_pids_unique(
        i in 0usize..2usize,
        j in 0usize..2usize,
    ) {
        if i != j {
            let a = OpenFFBoardVariant::ALL[i];
            let b = OpenFFBoardVariant::ALL[j];
            prop_assert_ne!(
                a.product_id(),
                b.product_id(),
                "variants {:?} and {:?} must have distinct PIDs",
                a,
                b
            );
        }
    }

    /// Every variant must report the correct VID (0x1209).
    #[test]
    fn prop_variant_vid_is_pid_codes(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        prop_assert_eq!(
            variant.vendor_id(),
            OPENFFBOARD_VENDOR_ID,
            "variant {:?} VID must equal OPENFFBOARD_VENDOR_ID",
            variant
        );
    }

    /// Every variant's VID must be 0x1209 (pid.codes open hardware).
    #[test]
    fn prop_variant_vid_exact_value(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        prop_assert_eq!(
            variant.vendor_id(),
            0x1209,
            "variant {:?} VID must be 0x1209",
            variant
        );
    }

    /// Every variant name must be non-empty.
    #[test]
    fn prop_variant_name_non_empty(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        prop_assert!(
            !variant.name().is_empty(),
            "variant {:?} must have a non-empty name",
            variant
        );
    }

    /// Every variant name must be valid UTF-8 and contain no control characters.
    #[test]
    fn prop_variant_name_printable(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        let name = variant.name();
        for ch in name.chars() {
            prop_assert!(
                !ch.is_control(),
                "variant {:?} name contains control char {:?}",
                variant,
                ch
            );
        }
    }

    /// Variant product_id must be non-zero (valid USB PID).
    #[test]
    fn prop_variant_pid_nonzero(idx in 0usize..2usize) {
        let variant = OpenFFBoardVariant::ALL[idx];
        prop_assert!(
            variant.product_id() != 0,
            "variant {:?} PID must not be zero",
            variant
        );
    }
}
