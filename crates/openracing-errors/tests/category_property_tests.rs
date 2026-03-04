#![allow(clippy::redundant_closure)]
//! Extended property-based tests for error category classification.
//!
//! These tests verify invariants across randomized inputs:
//! - Category classification is consistent
//! - Severity ordering is a total order
//! - From conversions preserve category
//! - RT error code round-trips
//! - Display is never empty

use openracing_errors::{
    OpenRacingError,
    common::{ErrorCategory, ErrorContext, ErrorSeverity},
    device::DeviceError,
    rt::RTError,
    validation::ValidationError,
};
use proptest::prelude::*;

/// Strategy that produces an arbitrary RTError variant.
fn arb_rt_error() -> impl Strategy<Value = RTError> {
    prop_oneof![
        Just(RTError::DeviceDisconnected),
        Just(RTError::TorqueLimit),
        Just(RTError::PipelineFault),
        Just(RTError::TimingViolation),
        Just(RTError::RTSetupFailed),
        Just(RTError::InvalidConfig),
        Just(RTError::SafetyInterlock),
        Just(RTError::BufferOverflow),
        Just(RTError::DeadlineMissed),
        Just(RTError::ResourceUnavailable),
    ]
}

/// Strategy that produces an arbitrary DeviceError variant.
fn arb_device_error() -> impl Strategy<Value = DeviceError> {
    prop_oneof![
        "[a-z0-9_-]{1,20}".prop_map(|s| DeviceError::not_found(s)),
        "[a-z0-9_-]{1,20}".prop_map(|s| DeviceError::disconnected(s)),
        "[a-z0-9_-]{1,20}".prop_map(|s| DeviceError::ConnectionFailed(s)),
        ("[a-z0-9_-]{1,20}", "[a-z ]{1,30}").prop_map(|(d, m)| DeviceError::CommunicationError {
            device: d,
            message: m
        }),
        "[a-z ]{1,30}".prop_map(|s| DeviceError::HidError(s)),
        ("[a-z0-9_-]{1,20}", 1usize..=1024, 1usize..=1024).prop_map(|(d, e, a)| {
            DeviceError::InvalidResponse {
                device: d,
                expected: e,
                actual: a,
            }
        }),
        ("[a-z0-9_-]{1,20}", 1u64..=60000).prop_map(|(d, t)| DeviceError::timeout(d, t)),
        (any::<u16>(), any::<u16>()).prop_map(|(v, p)| DeviceError::unsupported(v, p)),
        "[a-z0-9_-]{1,20}".prop_map(|s| DeviceError::Busy(s)),
        "[a-z0-9_-]{1,20}".prop_map(|s| DeviceError::PermissionDenied(s)),
    ]
}

/// Strategy that produces an arbitrary ValidationError variant.
fn arb_validation_error() -> impl Strategy<Value = ValidationError> {
    prop_oneof![
        "[a-z_]{1,20}".prop_map(|f| ValidationError::required(f)),
        ("[a-z_]{1,20}", "[a-z ]{1,30}").prop_map(|(f, r)| ValidationError::invalid_format(f, r)),
        ("[a-z_]{1,20}", 1usize..=1000, 1usize..=1000)
            .prop_map(|(f, a, m)| ValidationError::too_long(f, a, m)),
        ("[a-z_]{1,20}", 0usize..=100, 1usize..=100)
            .prop_map(|(f, a, m)| ValidationError::too_short(f, a, m)),
        "[a-z ]{1,50}".prop_map(|s| ValidationError::constraint(s)),
        "[a-z ]{1,50}".prop_map(|s| ValidationError::custom(s)),
    ]
}

/// Strategy that produces an arbitrary OpenRacingError.
fn arb_openracing_error() -> impl Strategy<Value = OpenRacingError> {
    prop_oneof![
        arb_rt_error().prop_map(OpenRacingError::from),
        arb_device_error().prop_map(OpenRacingError::from),
        arb_validation_error().prop_map(OpenRacingError::from),
        "[a-z ]{1,30}".prop_map(|s| OpenRacingError::config(s)),
        "[a-z ]{1,30}".prop_map(|s| OpenRacingError::other(s)),
    ]
}

proptest! {
    // -----------------------------------------------------------------------
    // Category classification invariants
    // -----------------------------------------------------------------------

    #[test]
    fn rt_error_always_categorised_as_rt(rt in arb_rt_error()) {
        let ore: OpenRacingError = rt.into();
        prop_assert_eq!(ore.category(), ErrorCategory::RT);
    }

    #[test]
    fn device_error_always_categorised_as_device(de in arb_device_error()) {
        let ore: OpenRacingError = de.into();
        prop_assert_eq!(ore.category(), ErrorCategory::Device);
    }

    #[test]
    fn validation_error_always_categorised_as_validation(ve in arb_validation_error()) {
        let ore: OpenRacingError = ve.into();
        prop_assert_eq!(ore.category(), ErrorCategory::Validation);
    }

    #[test]
    fn config_error_always_config(msg in "[a-z ]{1,50}") {
        let ore = OpenRacingError::config(msg);
        prop_assert_eq!(ore.category(), ErrorCategory::Config);
    }

    #[test]
    fn other_error_always_other(msg in "[a-z ]{1,50}") {
        let ore = OpenRacingError::other(msg);
        prop_assert_eq!(ore.category(), ErrorCategory::Other);
    }

    // -----------------------------------------------------------------------
    // Display is never empty
    // -----------------------------------------------------------------------

    #[test]
    fn openracing_error_display_never_empty(ore in arb_openracing_error()) {
        prop_assert!(!ore.to_string().is_empty());
    }

    #[test]
    fn rt_error_display_never_empty(rt in arb_rt_error()) {
        prop_assert!(!rt.to_string().is_empty());
    }

    #[test]
    fn device_error_display_never_empty(de in arb_device_error()) {
        prop_assert!(!de.to_string().is_empty());
    }

    #[test]
    fn validation_error_display_never_empty(ve in arb_validation_error()) {
        prop_assert!(!ve.to_string().is_empty());
    }

    // -----------------------------------------------------------------------
    // Severity is always valid
    // -----------------------------------------------------------------------

    #[test]
    fn severity_always_in_range(ore in arb_openracing_error()) {
        let sev = ore.severity();
        prop_assert!(matches!(
            sev,
            ErrorSeverity::Info | ErrorSeverity::Warning | ErrorSeverity::Error | ErrorSeverity::Critical
        ));
    }

    // -----------------------------------------------------------------------
    // RT error code round-trip (full u8 range)
    // -----------------------------------------------------------------------

    #[test]
    fn rt_code_roundtrip_or_none(code in any::<u8>()) {
        match RTError::from_code(code) {
            Some(err) => prop_assert_eq!(err.code(), code),
            None => prop_assert!(code == 0 || code > 10),
        }
    }

    // -----------------------------------------------------------------------
    // Severity ordering consistency
    // -----------------------------------------------------------------------

    #[test]
    fn severity_transitivity(a in 0u8..=3, b in 0u8..=3, c in 0u8..=3) {
        let sev = |x: u8| match x {
            0 => ErrorSeverity::Info,
            1 => ErrorSeverity::Warning,
            2 => ErrorSeverity::Error,
            _ => ErrorSeverity::Critical,
        };
        let sa = sev(a);
        let sb = sev(b);
        let sc = sev(c);
        // Transitivity: if a <= b and b <= c then a <= c
        if sa <= sb && sb <= sc {
            prop_assert!(sa <= sc);
        }
    }

    // -----------------------------------------------------------------------
    // Recoverability is consistent with severity
    // -----------------------------------------------------------------------

    #[test]
    fn recoverability_consistent_with_severity(ore in arb_openracing_error()) {
        let sev = ore.severity();
        if sev < ErrorSeverity::Critical {
            prop_assert!(ore.is_recoverable());
        } else {
            prop_assert!(!ore.is_recoverable());
        }
    }

    // -----------------------------------------------------------------------
    // RT error safety action implies critical or error severity
    // -----------------------------------------------------------------------

    #[test]
    fn safety_action_implies_critical(rt in arb_rt_error()) {
        if rt.requires_safety_action() {
            prop_assert_eq!(rt.severity(), ErrorSeverity::Critical);
        }
    }

    // -----------------------------------------------------------------------
    // ErrorContext preserves all fields
    // -----------------------------------------------------------------------

    #[test]
    fn error_context_preserves_operation(op in "[a-z_]{1,20}") {
        let ctx = ErrorContext::new(&op);
        prop_assert!(ctx.to_string().contains(&op));
    }

    #[test]
    fn error_context_preserves_kv_pairs(
        op in "[a-z_]{1,10}",
        key in "[a-z_]{1,10}",
        val in "[a-z0-9]{1,10}"
    ) {
        let ctx = ErrorContext::new(&op).with(&key, &val);
        let msg = ctx.to_string();
        prop_assert!(msg.contains(&op));
        prop_assert!(msg.contains(&key));
        prop_assert!(msg.contains(&val));
    }
}
