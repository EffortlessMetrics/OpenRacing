//! IPC protocol compatibility tests.
//!
//! Validates version negotiation, feature discovery, message framing,
//! error response formatting, and transport-agnostic message routing.

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::{IpcError, IpcResult};
use openracing_ipc::server::{IpcConfig, IpcServer, is_version_compatible};
use openracing_ipc::transport::{TransportBuilder, TransportType};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Inline prost message for codec tests (avoids cross-crate dependency).
#[derive(Clone, PartialEq, prost::Message)]
struct TestDeviceId {
    #[prost(string, tag = "1")]
    id: String,
}

/// Larger inline prost message for size-limit tests.
#[derive(Clone, PartialEq, prost::Message)]
struct TestDeviceInfo {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, tag = "2")]
    name: String,
    #[prost(int32, tag = "3")]
    device_type: i32,
    #[prost(int32, tag = "5")]
    state: i32,
}

// ──────────────────────────────────────────────────────────────────────
// 1. Version negotiation handshake
// ──────────────────────────────────────────────────────────────────────

#[test]
fn version_same_is_compatible() {
    assert!(is_version_compatible("1.0.0", "1.0.0"));
}

#[test]
fn version_higher_minor_is_compatible() {
    assert!(is_version_compatible("1.2.0", "1.0.0"));
}

#[test]
fn version_higher_patch_is_compatible() {
    assert!(is_version_compatible("1.0.5", "1.0.0"));
}

#[test]
fn version_lower_minor_is_incompatible() {
    assert!(!is_version_compatible("1.0.0", "1.1.0"));
}

#[test]
fn version_different_major_is_incompatible() {
    assert!(!is_version_compatible("2.0.0", "1.0.0"));
    assert!(!is_version_compatible("0.9.0", "1.0.0"));
}

#[test]
fn version_malformed_string_is_incompatible() {
    assert!(!is_version_compatible("abc", "1.0.0"));
    assert!(!is_version_compatible("1.0", "1.0.0"));
    assert!(!is_version_compatible("", "1.0.0"));
}

#[tokio::test]
async fn negotiate_features_compatible_client() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "device_management".to_string(),
                "safety_control".to_string(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert!(result.enabled_features.contains(&"device_management".to_string()));
    assert!(result.enabled_features.contains(&"safety_control".to_string()));
    assert!(!result.server_version.is_empty());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn negotiate_features_incompatible_client_version() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("0.1.0", &["device_management".to_string()])
        .await?;

    assert!(!result.compatible);

    server.stop().await?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 2. Feature capability discovery
// ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn feature_discovery_returns_server_features() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features("1.0.0", &[]).await?;

    assert!(!result.supported_features.is_empty());
    assert!(result.supported_features.contains(&"device_management".to_string()));
    assert!(result.supported_features.contains(&"profile_management".to_string()));
    assert!(result.supported_features.contains(&"health_monitoring".to_string()));
    // No client features requested => none enabled
    assert!(result.enabled_features.is_empty());

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_discovery_unknown_feature_not_enabled() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("1.0.0", &["quantum_teleportation".to_string()])
        .await?;

    assert!(result.compatible);
    assert!(!result.enabled_features.contains(&"quantum_teleportation".to_string()));

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_registers_client() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    assert_eq!(server.client_count().await, 0);

    let _result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;

    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 3. Message framing correctness
// ──────────────────────────────────────────────────────────────────────

#[test]
fn message_header_roundtrip_all_types() -> TestResult {
    let types = [
        message_types::DEVICE,
        message_types::PROFILE,
        message_types::SAFETY,
        message_types::HEALTH,
        message_types::FEATURE_NEGOTIATION,
        message_types::GAME,
        message_types::TELEMETRY,
        message_types::DIAGNOSTIC,
    ];

    for &msg_type in &types {
        let header = MessageHeader::new(msg_type, 512, 7);
        let encoded = header.encode();
        assert_eq!(encoded.len(), MessageHeader::SIZE);
        let decoded = MessageHeader::decode(&encoded)?;
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.payload_len, 512);
        assert_eq!(decoded.sequence, 7);
    }
    Ok(())
}

#[test]
fn message_header_flags_bitmask() -> TestResult {
    let mut header = MessageHeader::new(message_types::DEVICE, 100, 1);
    header.set_flag(message_flags::COMPRESSED);
    header.set_flag(message_flags::IS_RESPONSE);
    header.set_flag(message_flags::STREAMING);

    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert!(decoded.has_flag(message_flags::COMPRESSED));
    assert!(decoded.has_flag(message_flags::IS_RESPONSE));
    assert!(decoded.has_flag(message_flags::STREAMING));
    assert!(!decoded.has_flag(message_flags::REQUIRES_ACK));
    assert!(!decoded.has_flag(message_flags::IS_ERROR));
    Ok(())
}

#[test]
fn message_header_decode_rejects_short_buffer() {
    let short_buf = [0u8; 4];
    let result = MessageHeader::decode(&short_buf);
    assert!(result.is_err());
}

#[test]
fn message_header_zero_payload_len() -> TestResult {
    let header = MessageHeader::new(message_types::SAFETY, 0, 0);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.payload_len, 0);
    assert_eq!(decoded.sequence, 0);
    Ok(())
}

#[test]
fn message_header_max_payload_len() -> TestResult {
    let header = MessageHeader::new(message_types::TELEMETRY, u32::MAX, u32::MAX);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.payload_len, u32::MAX);
    assert_eq!(decoded.sequence, u32::MAX);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 4. Error response formatting
// ──────────────────────────────────────────────────────────────────────

#[test]
fn error_display_contains_context() {
    let err = IpcError::ConnectionFailed("refused".to_string());
    let msg = format!("{}", err);
    assert!(msg.contains("refused"));

    let err = IpcError::VersionIncompatibility {
        client: "2.0.0".into(),
        server: "1.0.0".into(),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("2.0.0"));
    assert!(msg.contains("1.0.0"));
}

#[test]
fn error_recoverable_classification() {
    let recoverable_errors = [
        IpcError::ConnectionFailed("test".into()),
        IpcError::Timeout { timeout_ms: 5000 },
        IpcError::VersionIncompatibility {
            client: "1.0.0".into(),
            server: "2.0.0".into(),
        },
        IpcError::FeatureNegotiation("test".into()),
    ];

    for err in &recoverable_errors {
        assert!(err.is_recoverable(), "Expected recoverable: {}", err);
        assert!(!err.is_fatal(), "Expected not fatal: {}", err);
    }
}

#[test]
fn error_fatal_classification() {
    let fatal_errors = [
        IpcError::TransportInit("port in use".into()),
        IpcError::ServerNotRunning,
        IpcError::ShutdownRequested,
    ];

    for err in &fatal_errors {
        assert!(err.is_fatal(), "Expected fatal: {}", err);
        assert!(!err.is_recoverable(), "Expected not recoverable: {}", err);
    }
}

#[test]
fn error_convenience_constructors() {
    let timeout_err = IpcError::timeout(3000);
    assert!(matches!(timeout_err, IpcError::Timeout { timeout_ms: 3000 }));

    let limit_err = IpcError::connection_limit(50);
    assert!(matches!(
        limit_err,
        IpcError::ConnectionLimitExceeded { max: 50 }
    ));
}

// ──────────────────────────────────────────────────────────────────────
// 5. Transport-agnostic message routing
// ──────────────────────────────────────────────────────────────────────

#[test]
fn codec_encode_decode_proto_message() -> TestResult {
    let codec = MessageCodec::new();

    let msg = TestDeviceId {
        id: "dev-codec-test".into(),
    };

    let bytes: Vec<u8> = codec.encode(&msg)?;
    assert!(!bytes.is_empty());

    let decoded: TestDeviceId = codec.decode(&bytes)?;
    assert_eq!(decoded.id, "dev-codec-test");
    Ok(())
}

#[test]
fn codec_rejects_oversized_message() {
    let small_codec = MessageCodec::with_max_size(16);
    // A message that encodes to more than 16 bytes
    let big_msg = TestDeviceInfo {
        id: "a-very-long-device-identifier-that-exceeds-limit".into(),
        name: "This name is also quite long for our tiny codec".into(),
        device_type: 1,
        state: 1,
    };

    let result: IpcResult<Vec<u8>> = small_codec.encode(&big_msg);
    assert!(result.is_err());
}

#[test]
fn codec_rejects_empty_decode() {
    let codec = MessageCodec::new();
    let result: IpcResult<TestDeviceId> = codec.decode(&[]);
    assert!(result.is_err());
}

#[test]
fn transport_builder_produces_correct_config() {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp_with_address("0.0.0.0", 9999))
        .max_connections(25)
        .enable_acl(true)
        .build();

    assert_eq!(config.max_connections, 25);
    assert!(config.enable_acl);
    match config.transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "0.0.0.0");
            assert_eq!(port, 9999);
        }
        #[allow(unreachable_patterns)]
        _ => panic!("Expected TCP transport"),
    }
}

#[test]
fn ipc_config_builder_chain() {
    let config = IpcConfig::with_transport(TransportType::tcp())
        .max_connections(10)
        .health_buffer_size(200);

    assert_eq!(config.transport.max_connections, 10);
    assert_eq!(config.health_buffer_size, 200);
    assert_eq!(config.server_name, "openracing-ipc");
}
