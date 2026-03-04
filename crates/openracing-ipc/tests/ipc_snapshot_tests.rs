//! Extended snapshot tests for IPC message types, health events,
//! server states, and error classification.
//!
//! Complements `snapshot_tests.rs` by covering handler DTOs,
//! health event types, server states, and error recoverability/fatality
//! classification matrices.

use openracing_ipc::codec::{MessageHeader, message_flags, message_types};
use openracing_ipc::error::IpcError;
use openracing_ipc::server::{HealthEventType, ServerState};
use openracing_ipc::transport::TransportType;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Health event type Display snapshots (all 9 variants)
// =========================================================================

#[test]
fn snapshot_health_event_type_all_variants() -> Result<(), BoxErr> {
    let variants = [
        (HealthEventType::Connected, "Connected"),
        (HealthEventType::Disconnected, "Disconnected"),
        (HealthEventType::Fault, "Fault"),
        (HealthEventType::FaultCleared, "FaultCleared"),
        (HealthEventType::TemperatureWarning, "TemperatureWarning"),
        (HealthEventType::TemperatureCritical, "TemperatureCritical"),
        (HealthEventType::ProfileChanged, "ProfileChanged"),
        (HealthEventType::HighTorqueEnabled, "HighTorqueEnabled"),
        (HealthEventType::EmergencyStop, "EmergencyStop"),
    ];
    let formatted: Vec<String> = variants
        .iter()
        .map(|(v, label)| format!("{label}: repr={}", *v as i32))
        .collect();
    insta::assert_snapshot!("health_event_type_variants", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// Server state Display snapshots (all 4 variants)
// =========================================================================

#[test]
fn snapshot_server_state_all_variants() -> Result<(), BoxErr> {
    let variants = [
        ServerState::Stopped,
        ServerState::Starting,
        ServerState::Running,
        ServerState::ShuttingDown,
    ];
    let formatted: Vec<String> = variants.iter().map(|v| format!("{v:?}")).collect();
    insta::assert_snapshot!("server_state_variants", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// Message type constants
// =========================================================================

#[test]
fn snapshot_message_type_constants() -> Result<(), BoxErr> {
    let types = [
        ("DEVICE", message_types::DEVICE),
        ("PROFILE", message_types::PROFILE),
        ("SAFETY", message_types::SAFETY),
        ("HEALTH", message_types::HEALTH),
        ("FEATURE_NEGOTIATION", message_types::FEATURE_NEGOTIATION),
        ("GAME", message_types::GAME),
        ("TELEMETRY", message_types::TELEMETRY),
        ("DIAGNOSTIC", message_types::DIAGNOSTIC),
    ];
    let formatted: Vec<String> = types
        .iter()
        .map(|(name, val)| format!("{name}: 0x{val:04x}"))
        .collect();
    insta::assert_snapshot!("message_type_constants", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// Message flag constants
// =========================================================================

#[test]
fn snapshot_message_flag_constants() -> Result<(), BoxErr> {
    let flags = [
        ("COMPRESSED", message_flags::COMPRESSED),
        ("REQUIRES_ACK", message_flags::REQUIRES_ACK),
        ("IS_RESPONSE", message_flags::IS_RESPONSE),
        ("IS_ERROR", message_flags::IS_ERROR),
        ("STREAMING", message_flags::STREAMING),
    ];
    let formatted: Vec<String> = flags
        .iter()
        .map(|(name, val)| format!("{name}: 0x{val:04x}"))
        .collect();
    insta::assert_snapshot!("message_flag_constants", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// Message header with combined flags
// =========================================================================

#[test]
fn snapshot_header_all_flags_combined() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::TELEMETRY, 512, 99);
    header.set_flag(message_flags::COMPRESSED);
    header.set_flag(message_flags::REQUIRES_ACK);
    header.set_flag(message_flags::IS_RESPONSE);
    header.set_flag(message_flags::IS_ERROR);
    header.set_flag(message_flags::STREAMING);
    let bytes = header.encode();
    insta::assert_snapshot!("header_all_flags", format!("{bytes:02x?}"));
    Ok(())
}

#[test]
fn snapshot_header_diagnostic_message() -> Result<(), BoxErr> {
    let header = MessageHeader::new(message_types::DIAGNOSTIC, 4096, 1000);
    let bytes = header.encode();
    insta::assert_snapshot!("header_diagnostic", format!("{bytes:02x?}"));
    Ok(())
}

// =========================================================================
// Error recoverability classification matrix
// =========================================================================

#[test]
fn snapshot_error_recoverability_matrix() -> Result<(), BoxErr> {
    let errors: Vec<(&str, IpcError)> = vec![
        ("TransportInit", IpcError::TransportInit("test".into())),
        (
            "ConnectionFailed",
            IpcError::ConnectionFailed("test".into()),
        ),
        ("EncodingFailed", IpcError::EncodingFailed("test".into())),
        ("DecodingFailed", IpcError::DecodingFailed("test".into())),
        (
            "VersionIncompatibility",
            IpcError::VersionIncompatibility {
                client: "0.9".into(),
                server: "1.0".into(),
            },
        ),
        (
            "FeatureNegotiation",
            IpcError::FeatureNegotiation("test".into()),
        ),
        ("ServerNotRunning", IpcError::ServerNotRunning),
        (
            "ConnectionLimitExceeded",
            IpcError::ConnectionLimitExceeded { max: 100 },
        ),
        ("Timeout", IpcError::Timeout { timeout_ms: 5000 }),
        ("Grpc", IpcError::Grpc("test".into())),
        ("InvalidConfig", IpcError::InvalidConfig("test".into())),
        (
            "PlatformNotSupported",
            IpcError::PlatformNotSupported("test".into()),
        ),
        ("ShutdownRequested", IpcError::ShutdownRequested),
    ];
    let formatted: Vec<String> = errors
        .iter()
        .map(|(label, err)| {
            format!(
                "{label}: recoverable={}, fatal={}",
                err.is_recoverable(),
                err.is_fatal()
            )
        })
        .collect();
    insta::assert_snapshot!("error_recoverability_matrix", formatted.join("\n"));
    Ok(())
}

// =========================================================================
// Transport type descriptions
// =========================================================================

#[test]
fn snapshot_transport_tcp_description() -> Result<(), BoxErr> {
    let transport = TransportType::tcp();
    insta::assert_snapshot!("transport_tcp_description", transport.description());
    Ok(())
}

#[test]
fn snapshot_transport_tcp_custom_description() -> Result<(), BoxErr> {
    let transport = TransportType::tcp_with_address("10.0.0.1", 9999);
    insta::assert_snapshot!("transport_tcp_custom_description", transport.description());
    Ok(())
}

#[cfg(windows)]
#[test]
fn snapshot_transport_named_pipe_description() -> Result<(), BoxErr> {
    let transport = TransportType::named_pipe(r"\\.\pipe\openracing-test");
    insta::assert_snapshot!("transport_named_pipe_description", transport.description());
    Ok(())
}

#[test]
fn snapshot_transport_platform_default_description() -> Result<(), BoxErr> {
    let transport = TransportType::platform_default();
    insta::assert_snapshot!(
        "transport_platform_default_description",
        transport.description()
    );
    Ok(())
}

// =========================================================================
// Error Debug format snapshots
// =========================================================================

#[test]
fn snapshot_error_debug_transport_init() -> Result<(), BoxErr> {
    let err = IpcError::TransportInit("bind failed on port 50051".into());
    insta::assert_snapshot!("error_debug_transport_init", format!("{err:?}"));
    Ok(())
}

#[test]
fn snapshot_error_debug_version_incompatibility() -> Result<(), BoxErr> {
    let err = IpcError::VersionIncompatibility {
        client: "0.8.0".into(),
        server: "1.0.0".into(),
    };
    insta::assert_snapshot!("error_debug_version_incompatibility", format!("{err:?}"));
    Ok(())
}

#[test]
fn snapshot_error_debug_connection_limit() -> Result<(), BoxErr> {
    let err = IpcError::ConnectionLimitExceeded { max: 200 };
    insta::assert_snapshot!("error_debug_connection_limit", format!("{err:?}"));
    Ok(())
}

#[test]
fn snapshot_error_debug_timeout() -> Result<(), BoxErr> {
    let err = IpcError::Timeout { timeout_ms: 30000 };
    insta::assert_snapshot!("error_debug_timeout", format!("{err:?}"));
    Ok(())
}

// =========================================================================
// Protocol version constants
// =========================================================================

#[test]
fn snapshot_protocol_constants() -> Result<(), BoxErr> {
    let formatted = format!(
        "PROTOCOL_VERSION: {}\nMIN_CLIENT_VERSION: {}\nDEFAULT_TCP_PORT: {}",
        openracing_ipc::PROTOCOL_VERSION,
        openracing_ipc::MIN_CLIENT_VERSION,
        openracing_ipc::DEFAULT_TCP_PORT,
    );
    insta::assert_snapshot!("protocol_constants", formatted);
    Ok(())
}
