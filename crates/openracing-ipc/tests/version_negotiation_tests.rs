//! Feature negotiation tests for the IPC version handshake.
//!
//! Tests client capability discovery, graceful degradation when features
//! aren't available, and clear error messages for version mismatches.

use openracing_ipc::version::{
    FeatureFlags, NegotiationResult, ProtocolVersion, VersionInfo, VersionNegotiator,
};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Client capability discovery
// =========================================================================

#[test]
fn client_discovers_server_capabilities_via_negotiation() -> Result<(), BoxErr> {
    let negotiator = VersionNegotiator::with_features(
        ProtocolVersion::new(1, 2, 0),
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::DEVICE_MANAGEMENT
            | FeatureFlags::SAFETY_CONTROL
            | FeatureFlags::HEALTH_MONITORING,
    );

    // Client requests all features
    let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::ALL_V1)?;

    assert!(result.compatible);
    // Only the server-supported features are negotiated
    assert!(
        result
            .negotiated_features
            .contains(FeatureFlags::DEVICE_MANAGEMENT)
    );
    assert!(
        result
            .negotiated_features
            .contains(FeatureFlags::SAFETY_CONTROL)
    );
    assert!(
        result
            .negotiated_features
            .contains(FeatureFlags::HEALTH_MONITORING)
    );
    // Features the server doesn't support are excluded
    assert!(
        !result
            .negotiated_features
            .contains(FeatureFlags::PROFILE_MANAGEMENT)
    );
    assert!(!result.negotiated_features.contains(FeatureFlags::TELEMETRY));
    Ok(())
}

#[test]
fn client_with_no_features_gets_empty_negotiation() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));

    let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::NONE)?;

    assert!(result.compatible);
    assert!(result.negotiated_features.is_empty());
    assert_eq!(result.negotiated_features.count(), 0);
    Ok(())
}

#[test]
fn server_with_no_features_results_in_empty_negotiation() -> Result<(), BoxErr> {
    let negotiator = VersionNegotiator::with_features(
        ProtocolVersion::new(1, 0, 0),
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::NONE,
    );

    let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::ALL_V1)?;

    assert!(result.compatible);
    assert!(result.negotiated_features.is_empty());
    Ok(())
}

// =========================================================================
// Graceful degradation
// =========================================================================

#[test]
fn graceful_degradation_partial_feature_support() -> Result<(), BoxErr> {
    // Server only supports device management and health
    let negotiator = VersionNegotiator::with_features(
        ProtocolVersion::new(1, 1, 0),
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::HEALTH_MONITORING,
    );

    // Client wants everything
    let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::ALL_V1)?;

    assert!(result.compatible);
    // Client can use the features that were negotiated
    assert!(
        result
            .negotiated_features
            .contains(FeatureFlags::DEVICE_MANAGEMENT)
    );
    assert!(
        result
            .negotiated_features
            .contains(FeatureFlags::HEALTH_MONITORING)
    );
    // Client knows these aren't available and can gracefully degrade
    assert!(!result.negotiated_features.contains(FeatureFlags::TELEMETRY));
    assert!(
        !result
            .negotiated_features
            .contains(FeatureFlags::GAME_INTEGRATION)
    );
    assert_eq!(result.negotiated_features.count(), 2);
    Ok(())
}

#[test]
fn graceful_degradation_newer_client_with_older_server() -> Result<(), BoxErr> {
    // Server is v1.1, client is v1.3
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 1, 0), ProtocolVersion::new(1, 0, 0));

    let result = negotiator.negotiate(&ProtocolVersion::new(1, 3, 0), FeatureFlags::ALL_V1)?;

    assert!(result.compatible);
    // Effective version is the lower (server's 1.1.0)
    assert_eq!(result.effective_version, ProtocolVersion::new(1, 1, 0));
    Ok(())
}

#[test]
fn graceful_degradation_older_client_with_newer_server() -> Result<(), BoxErr> {
    // Server is v1.3, client is v1.1
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 3, 0), ProtocolVersion::new(1, 0, 0));

    let result = negotiator.negotiate(
        &ProtocolVersion::new(1, 1, 0),
        FeatureFlags::DEVICE_MANAGEMENT,
    )?;

    assert!(result.compatible);
    // Effective version is the lower (client's 1.1.0)
    assert_eq!(result.effective_version, ProtocolVersion::new(1, 1, 0));
    Ok(())
}

// =========================================================================
// Version mismatch error messages
// =========================================================================

#[test]
fn error_message_major_version_mismatch() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));

    let result = negotiator.negotiate(&ProtocolVersion::new(2, 0, 0), FeatureFlags::ALL_V1)?;

    assert!(!result.compatible);
    let reason = result.rejection_reason.as_deref().unwrap_or("(no reason)");
    assert!(
        reason.contains("Major version mismatch"),
        "Error message should describe major version mismatch, got: {reason}"
    );
    assert!(
        reason.contains("2.0.0"),
        "Error message should mention client version, got: {reason}"
    );
    assert!(
        reason.contains("1.0.0"),
        "Error message should mention server version, got: {reason}"
    );
    Ok(())
}

#[test]
fn error_message_below_minimum_version() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 5, 0), ProtocolVersion::new(1, 2, 0));

    let result = negotiator.negotiate(&ProtocolVersion::new(1, 1, 0), FeatureFlags::ALL_V1)?;

    assert!(!result.compatible);
    let reason = result.rejection_reason.as_deref().unwrap_or("(no reason)");
    assert!(
        reason.contains("below minimum"),
        "Error should mention 'below minimum', got: {reason}"
    );
    assert!(
        reason.contains("please upgrade") || reason.contains("upgrade"),
        "Error should suggest upgrading, got: {reason}"
    );
    Ok(())
}

#[test]
fn error_message_server_too_old_for_client() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));

    let client_info = VersionInfo::new(
        ProtocolVersion::new(1, 3, 0),
        FeatureFlags::ALL_V1,
        ProtocolVersion::new(1, 2, 0), // client requires server >= 1.2.0
    );

    let result = negotiator.negotiate_info(&client_info)?;

    assert!(!result.compatible);
    let reason = result.rejection_reason.as_deref().unwrap_or("(no reason)");
    assert!(
        reason.contains("client requires a newer server"),
        "Error should mention client needs newer server, got: {reason}"
    );
    Ok(())
}

#[test]
fn compatible_result_has_no_rejection_reason() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));

    let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), FeatureFlags::ALL_V1)?;

    assert!(result.compatible);
    assert!(
        result.rejection_reason.is_none(),
        "Compatible result should have no rejection reason"
    );
    Ok(())
}

// =========================================================================
// Negotiation result inspection
// =========================================================================

#[test]
fn negotiation_result_contains_both_versions() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 2, 0), ProtocolVersion::new(1, 0, 0));

    let result = negotiator.negotiate(
        &ProtocolVersion::new(1, 1, 0),
        FeatureFlags::DEVICE_MANAGEMENT,
    )?;

    assert_eq!(result.server_version, ProtocolVersion::new(1, 2, 0));
    assert_eq!(result.client_version, ProtocolVersion::new(1, 1, 0));
    Ok(())
}

#[test]
fn negotiate_info_full_handshake() -> Result<(), BoxErr> {
    let negotiator = VersionNegotiator::with_features(
        ProtocolVersion::new(1, 2, 0),
        ProtocolVersion::new(1, 0, 0),
        FeatureFlags::ALL_V1,
    );

    let client_info = VersionInfo::new(
        ProtocolVersion::new(1, 1, 0),
        FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::TELEMETRY,
        ProtocolVersion::new(1, 0, 0),
    );

    let result = negotiator.negotiate_info(&client_info)?;

    assert!(result.compatible);
    assert_eq!(result.effective_version, ProtocolVersion::new(1, 1, 0));
    assert!(
        result
            .negotiated_features
            .contains(FeatureFlags::DEVICE_MANAGEMENT)
    );
    assert!(result.negotiated_features.contains(FeatureFlags::TELEMETRY));
    assert_eq!(result.negotiated_features.count(), 2);
    Ok(())
}

// =========================================================================
// Edge cases
// =========================================================================

#[test]
fn negotiation_with_zero_version() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(0, 0, 1), ProtocolVersion::new(0, 0, 0));

    let result = negotiator.negotiate(
        &ProtocolVersion::new(0, 0, 0),
        FeatureFlags::DEVICE_MANAGEMENT,
    )?;

    assert!(result.compatible);
    assert_eq!(result.effective_version, ProtocolVersion::new(0, 0, 0));
    Ok(())
}

#[test]
fn negotiation_preserves_feature_names() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));

    let client_features = FeatureFlags::DEVICE_MANAGEMENT | FeatureFlags::SAFETY_CONTROL;
    let result = negotiator.negotiate(&ProtocolVersion::new(1, 0, 0), client_features)?;

    let names = result.negotiated_features.names();
    assert!(names.contains(&"device_management"));
    assert!(names.contains(&"safety_control"));
    Ok(())
}

#[test]
fn negotiation_result_serializable() -> Result<(), BoxErr> {
    let negotiator =
        VersionNegotiator::new(ProtocolVersion::new(1, 0, 0), ProtocolVersion::new(1, 0, 0));
    let result = negotiator.negotiate(
        &ProtocolVersion::new(1, 0, 0),
        FeatureFlags::DEVICE_MANAGEMENT,
    )?;

    let json = serde_json::to_string(&result)?;
    let deserialized: NegotiationResult = serde_json::from_str(&json)?;
    assert_eq!(deserialized.compatible, result.compatible);
    assert_eq!(deserialized.effective_version, result.effective_version);
    assert_eq!(deserialized.negotiated_features, result.negotiated_features);
    Ok(())
}
