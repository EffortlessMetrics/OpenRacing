//! Property-based tests for IPC version negotiation.
//!
//! Uses proptest to verify invariants:
//! - Serialize/deserialize roundtrips always succeed
//! - Version negotiation always converges to a valid state
//! - Feature flag combinations produce valid results

use proptest::prelude::*;

use openracing_ipc::version::{FeatureFlags, ProtocolVersion, VersionInfo, VersionNegotiator};

// =========================================================================
// Arbitrary type generators
// =========================================================================

fn arb_protocol_version() -> impl Strategy<Value = ProtocolVersion> {
    (0u16..=1000u16, 0u16..=1000u16, 0u16..=1000u16)
        .prop_map(|(major, minor, patch)| ProtocolVersion::new(major, minor, patch))
}

fn arb_feature_flags() -> impl Strategy<Value = FeatureFlags> {
    any::<u64>().prop_map(FeatureFlags::from_bits)
}

fn arb_version_info() -> impl Strategy<Value = VersionInfo> {
    (
        arb_protocol_version(),
        arb_feature_flags(),
        arb_protocol_version(),
    )
        .prop_map(|(ver, feat, min)| VersionInfo::new(ver, feat, min))
}

// =========================================================================
// Roundtrip properties
// =========================================================================

proptest! {
    #[test]
    fn prop_protocol_version_roundtrip(
        major in 0u16..=u16::MAX,
        minor in 0u16..=u16::MAX,
        patch in 0u16..=u16::MAX,
    ) {
        let v = ProtocolVersion::new(major, minor, patch);
        let bytes = v.to_bytes();
        let decoded = ProtocolVersion::from_bytes(&bytes)
            .map_err(|e| TestCaseError::fail(format!("decode failed: {e}")))?;
        prop_assert_eq!(v, decoded);
    }

    #[test]
    fn prop_feature_flags_roundtrip(bits in any::<u64>()) {
        let flags = FeatureFlags::from_bits(bits);
        let bytes = flags.to_bytes();
        let decoded = FeatureFlags::from_bytes(&bytes)
            .map_err(|e| TestCaseError::fail(format!("decode failed: {e}")))?;
        prop_assert_eq!(flags, decoded);
    }

    #[test]
    fn prop_version_info_roundtrip(info in arb_version_info()) {
        let bytes = info.to_bytes();
        let decoded = VersionInfo::from_bytes(&bytes)
            .map_err(|e| TestCaseError::fail(format!("decode failed: {e}")))?;
        prop_assert_eq!(info, decoded);
    }

    // =========================================================================
    // Version negotiation always converges
    // =========================================================================

    #[test]
    fn prop_negotiation_always_returns_result(
        server_ver in arb_protocol_version(),
        client_ver in arb_protocol_version(),
        server_flags in arb_feature_flags(),
        client_flags in arb_feature_flags(),
    ) {
        // min_supported = server version for simplicity
        let min = ProtocolVersion::new(server_ver.major(), 0, 0);
        let negotiator = VersionNegotiator::with_features(server_ver, min, server_flags);
        let result = negotiator.negotiate(&client_ver, client_flags)
            .map_err(|e| TestCaseError::fail(format!("negotiate failed: {e}")))?;

        // Result must always have valid versions
        prop_assert_eq!(result.server_version, server_ver);
        prop_assert_eq!(result.client_version, client_ver);

        // If compatible, rejection_reason must be None
        if result.compatible {
            prop_assert!(result.rejection_reason.is_none());
        } else {
            prop_assert!(result.rejection_reason.is_some());
        }
    }

    #[test]
    fn prop_negotiation_major_mismatch_always_incompatible(
        server_major in 0u16..=100u16,
        client_major in 0u16..=100u16,
        minor in 0u16..=50u16,
        patch in 0u16..=50u16,
        flags in arb_feature_flags(),
    ) {
        prop_assume!(server_major != client_major);
        let server = ProtocolVersion::new(server_major, minor, patch);
        let client = ProtocolVersion::new(client_major, minor, patch);
        let min = ProtocolVersion::new(server_major, 0, 0);
        let negotiator = VersionNegotiator::new(server, min);
        let result = negotiator.negotiate(&client, flags)
            .map_err(|e| TestCaseError::fail(format!("negotiate failed: {e}")))?;

        prop_assert!(!result.compatible, "Different major versions must be incompatible");
        prop_assert!(
            result.rejection_reason.as_deref().unwrap_or("").contains("Major version mismatch"),
            "Rejection reason should mention major version mismatch"
        );
    }

    #[test]
    fn prop_same_version_always_compatible(
        major in 0u16..=100u16,
        minor in 0u16..=100u16,
        patch in 0u16..=100u16,
        flags in arb_feature_flags(),
    ) {
        let ver = ProtocolVersion::new(major, minor, patch);
        let negotiator = VersionNegotiator::new(ver, ver);
        let result = negotiator.negotiate(&ver, flags)
            .map_err(|e| TestCaseError::fail(format!("negotiate failed: {e}")))?;

        prop_assert!(result.compatible, "Same version must always be compatible");
        prop_assert_eq!(result.effective_version, ver);
    }

    // =========================================================================
    // Feature flag combination invariants
    // =========================================================================

    #[test]
    fn prop_negotiated_features_are_subset_of_both(
        server_flags in arb_feature_flags(),
        client_flags in arb_feature_flags(),
    ) {
        let ver = ProtocolVersion::new(1, 0, 0);
        let negotiator = VersionNegotiator::with_features(ver, ver, server_flags);
        let result = negotiator.negotiate(&ver, client_flags)
            .map_err(|e| TestCaseError::fail(format!("negotiate failed: {e}")))?;

        // Negotiated = intersection, so must be subset of both
        let negotiated = result.negotiated_features;
        prop_assert!(
            server_flags.contains(negotiated),
            "Negotiated features 0x{:x} must be subset of server 0x{:x}",
            negotiated.bits(),
            server_flags.bits()
        );
        prop_assert!(
            client_flags.contains(negotiated),
            "Negotiated features 0x{:x} must be subset of client 0x{:x}",
            negotiated.bits(),
            client_flags.bits()
        );
    }

    #[test]
    fn prop_feature_flags_bitwise_ops_consistent(
        a_bits in any::<u64>(),
        b_bits in any::<u64>(),
    ) {
        let a = FeatureFlags::from_bits(a_bits);
        let b = FeatureFlags::from_bits(b_bits);

        // OR union
        let union = a | b;
        prop_assert_eq!(union.bits(), a_bits | b_bits);

        // AND intersection
        let intersection = a & b;
        prop_assert_eq!(intersection.bits(), a_bits & b_bits);

        // Intersection is subset of both
        prop_assert!(a.contains(intersection));
        prop_assert!(b.contains(intersection));
    }

    #[test]
    fn prop_feature_flags_contains_is_reflexive(bits in any::<u64>()) {
        let flags = FeatureFlags::from_bits(bits);
        prop_assert!(flags.contains(flags), "Flags must contain themselves");
    }

    #[test]
    fn prop_feature_flags_none_contained_by_all(bits in any::<u64>()) {
        let flags = FeatureFlags::from_bits(bits);
        prop_assert!(
            flags.contains(FeatureFlags::NONE),
            "Every flag set contains NONE"
        );
    }

    // =========================================================================
    // Effective version invariants
    // =========================================================================

    #[test]
    fn prop_effective_version_is_minimum_when_compatible(
        server_minor in 0u16..=100u16,
        server_patch in 0u16..=100u16,
        client_minor in 0u16..=100u16,
        client_patch in 0u16..=100u16,
    ) {
        let server = ProtocolVersion::new(1, server_minor, server_patch);
        let client = ProtocolVersion::new(1, client_minor, client_patch);
        let min = ProtocolVersion::new(1, 0, 0);

        let negotiator = VersionNegotiator::new(server, min);
        let result = negotiator.negotiate(&client, FeatureFlags::NONE)
            .map_err(|e| TestCaseError::fail(format!("negotiate failed: {e}")))?;

        if result.compatible {
            let expected_effective = if client < server { client } else { server };
            prop_assert_eq!(
                result.effective_version, expected_effective,
                "Effective version must be the minimum of client and server"
            );
        }
    }

    // =========================================================================
    // Version parsing roundtrips
    // =========================================================================

    #[test]
    fn prop_version_parse_display_roundtrip(
        major in 0u16..=999u16,
        minor in 0u16..=999u16,
        patch in 0u16..=999u16,
    ) {
        let v = ProtocolVersion::new(major, minor, patch);
        let s = format!("{v}");
        let parsed = ProtocolVersion::parse(&s)
            .map_err(|e| TestCaseError::fail(format!("parse failed: {e}")))?;
        prop_assert_eq!(v, parsed);
    }

    // =========================================================================
    // Ordering consistency
    // =========================================================================

    #[test]
    fn prop_version_ordering_consistent_with_compatibility(
        a in arb_protocol_version(),
        b in arb_protocol_version(),
    ) {
        // If a > b and same major, then a is_compatible_with(b)
        if a.major() == b.major() && a >= b {
            prop_assert!(
                a.is_compatible_with(&b),
                "v{a} >= v{b} with same major should be compatible"
            );
        }
    }
}
