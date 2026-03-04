//! Snapshot tests for IPC message serialization formats using `insta`.
//!
//! These tests lock down the wire-format / JSON representation of
//! IPC configuration types and error messages so that accidental
//! format changes are caught in review.

use openracing_ipc::codec::{MessageHeader, message_flags, message_types};
use openracing_ipc::error::IpcError;
use openracing_ipc::server::IpcConfig;
use openracing_ipc::transport::{TransportBuilder, TransportType};

use std::time::Duration;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Snapshot: IPC configuration serialization
// =========================================================================

#[test]
fn snapshot_ipc_config_default_json() -> Result<(), BoxErr> {
    // Use explicit TCP transport to keep snapshot platform-independent.
    let config = IpcConfig::with_transport(TransportType::tcp());
    let json = serde_json::to_string_pretty(&config)?;
    insta::assert_snapshot!("ipc_config_default", json);
    Ok(())
}

#[test]
fn snapshot_transport_config_default_json() -> Result<(), BoxErr> {
    // Use explicit TCP transport to keep snapshot platform-independent.
    let config = TransportBuilder::new()
        .transport(TransportType::tcp())
        .build();
    let json = serde_json::to_string_pretty(&config)?;
    insta::assert_snapshot!("transport_config_default", json);
    Ok(())
}

#[test]
fn snapshot_transport_type_tcp_json() -> Result<(), BoxErr> {
    let transport = TransportType::tcp();
    let json = serde_json::to_string_pretty(&transport)?;
    insta::assert_snapshot!("transport_type_tcp", json);
    Ok(())
}

#[test]
fn snapshot_transport_type_tcp_custom_json() -> Result<(), BoxErr> {
    let transport = TransportType::tcp_with_address("192.168.1.100", 9090);
    let json = serde_json::to_string_pretty(&transport)?;
    insta::assert_snapshot!("transport_type_tcp_custom", json);
    Ok(())
}

#[cfg(windows)]
#[test]
fn snapshot_transport_type_named_pipe_json() -> Result<(), BoxErr> {
    let transport = TransportType::named_pipe(r"\\.\pipe\openracing");
    let json = serde_json::to_string_pretty(&transport)?;
    insta::assert_snapshot!("transport_type_named_pipe", json);
    Ok(())
}

#[test]
fn snapshot_ipc_config_custom_json() -> Result<(), BoxErr> {
    let config = IpcConfig::with_transport(TransportType::tcp())
        .max_connections(50)
        .health_buffer_size(2048);
    let json = serde_json::to_string_pretty(&config)?;
    insta::assert_snapshot!("ipc_config_custom", json);
    Ok(())
}

#[test]
fn snapshot_transport_builder_config_json() -> Result<(), BoxErr> {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp_with_address("0.0.0.0", 8080))
        .max_connections(200)
        .connection_timeout(Duration::from_secs(60))
        .enable_acl(true)
        .build();
    let json = serde_json::to_string_pretty(&config)?;
    insta::assert_snapshot!("transport_builder_config", json);
    Ok(())
}

// =========================================================================
// Snapshot: message header wire format
// =========================================================================

#[test]
fn snapshot_message_header_device() -> Result<(), BoxErr> {
    let header = MessageHeader::new(message_types::DEVICE, 256, 1);
    let bytes = header.encode();
    insta::assert_snapshot!("header_device", format!("{bytes:02x?}"));
    Ok(())
}

#[test]
fn snapshot_message_header_health_with_flags() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::HEALTH, 1024, 42);
    header.set_flag(message_flags::STREAMING);
    header.set_flag(message_flags::REQUIRES_ACK);
    let bytes = header.encode();
    insta::assert_snapshot!("header_health_flagged", format!("{bytes:02x?}"));
    Ok(())
}

#[test]
fn snapshot_message_header_zeroed() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0, 0, 0);
    let bytes = header.encode();
    insta::assert_snapshot!("header_zeroed", format!("{bytes:02x?}"));
    Ok(())
}

#[test]
fn snapshot_message_header_max_values() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
    header.flags = u16::MAX;
    let bytes = header.encode();
    insta::assert_snapshot!("header_max_values", format!("{bytes:02x?}"));
    Ok(())
}

// =========================================================================
// Snapshot: error messages
// =========================================================================

#[test]
fn snapshot_error_transport_init() -> Result<(), BoxErr> {
    let err = IpcError::TransportInit("failed to bind port 50051".into());
    insta::assert_snapshot!("error_transport_init", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_connection_failed() -> Result<(), BoxErr> {
    let err = IpcError::ConnectionFailed("connection refused".into());
    insta::assert_snapshot!("error_connection_failed", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_encoding_failed() -> Result<(), BoxErr> {
    let err = IpcError::EncodingFailed("message size 20000000 exceeds maximum 16777216".into());
    insta::assert_snapshot!("error_encoding_failed", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_decoding_failed() -> Result<(), BoxErr> {
    let err = IpcError::DecodingFailed("invalid wire type".into());
    insta::assert_snapshot!("error_decoding_failed", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_version_incompatibility() -> Result<(), BoxErr> {
    let err = IpcError::VersionIncompatibility {
        client: "0.9.0".into(),
        server: "1.0.0".into(),
    };
    insta::assert_snapshot!("error_version_incompatibility", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_feature_negotiation() -> Result<(), BoxErr> {
    let err = IpcError::FeatureNegotiation("unsupported feature: turbo_mode".into());
    insta::assert_snapshot!("error_feature_negotiation", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_server_not_running() -> Result<(), BoxErr> {
    let err = IpcError::ServerNotRunning;
    insta::assert_snapshot!("error_server_not_running", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_connection_limit() -> Result<(), BoxErr> {
    let err = IpcError::ConnectionLimitExceeded { max: 100 };
    insta::assert_snapshot!("error_connection_limit", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_timeout() -> Result<(), BoxErr> {
    let err = IpcError::Timeout { timeout_ms: 5000 };
    insta::assert_snapshot!("error_timeout", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_grpc() -> Result<(), BoxErr> {
    let err = IpcError::Grpc("status: UNAVAILABLE, message: connection reset".into());
    insta::assert_snapshot!("error_grpc", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_invalid_config() -> Result<(), BoxErr> {
    let err = IpcError::InvalidConfig("max_connections must be > 0".into());
    insta::assert_snapshot!("error_invalid_config", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_platform_not_supported() -> Result<(), BoxErr> {
    let err = IpcError::PlatformNotSupported("wasm32".into());
    insta::assert_snapshot!("error_platform_not_supported", format!("{err}"));
    Ok(())
}

#[test]
fn snapshot_error_shutdown() -> Result<(), BoxErr> {
    let err = IpcError::ShutdownRequested;
    insta::assert_snapshot!("error_shutdown", format!("{err}"));
    Ok(())
}
