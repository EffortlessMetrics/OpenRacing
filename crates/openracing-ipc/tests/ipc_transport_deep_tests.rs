//! Deep tests for IPC transport implementations.
//!
//! Covers:
//! - Named pipe transport (Windows)
//! - Unix socket transport (Linux)
//! - gRPC/TCP transport
//! - Transport auto-selection per platform
//! - Connection timeout handling
//! - Reconnection logic
//! - Message serialization/deserialization
//! - Large message handling
//! - Concurrent connections

use std::collections::HashMap;
use std::time::{Duration, Instant};

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::{IpcError, IpcResult};
use openracing_ipc::server::{
    ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, ServerState,
    is_version_compatible,
};
use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};
use openracing_ipc::{DEFAULT_TCP_PORT, MIN_CLIENT_VERSION, PROTOCOL_VERSION};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// Named Pipe transport (Windows)
// =========================================================================

#[cfg(windows)]
#[test]
fn named_pipe_transport_creation() -> Result<(), BoxErr> {
    let transport = TransportType::named_pipe(r"\\.\pipe\openracing-test");
    match transport {
        TransportType::NamedPipe { pipe_name } => {
            assert_eq!(pipe_name, r"\\.\pipe\openracing-test");
        }
        _ => return Err("Expected NamedPipe transport variant".into()),
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn named_pipe_description_contains_pipe_name() -> Result<(), BoxErr> {
    let transport = TransportType::named_pipe(r"\\.\pipe\my-wheel");
    let desc = transport.description();
    assert!(
        desc.contains("Named pipe"),
        "Description should mention 'Named pipe', got: {desc}"
    );
    assert!(
        desc.contains(r"\\.\pipe\my-wheel"),
        "Description should contain pipe name, got: {desc}"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn named_pipe_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportType::named_pipe(r"\\.\pipe\openracing-serde");
    let json = serde_json::to_string(&original)?;
    let deserialized: TransportType = serde_json::from_str(&json)?;
    match deserialized {
        TransportType::NamedPipe { pipe_name } => {
            assert_eq!(pipe_name, r"\\.\pipe\openracing-serde");
        }
        _ => return Err("Expected NamedPipe after deserialization".into()),
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn named_pipe_in_transport_config() -> Result<(), BoxErr> {
    let config = TransportBuilder::new()
        .transport(TransportType::named_pipe(r"\\.\pipe\or-config-test"))
        .max_connections(25)
        .build();
    match &config.transport {
        TransportType::NamedPipe { pipe_name } => {
            assert_eq!(pipe_name, r"\\.\pipe\or-config-test");
        }
        _ => return Err("Expected NamedPipe in config".into()),
    }
    assert_eq!(config.max_connections, 25);
    Ok(())
}

// =========================================================================
// Unix socket transport (Linux/macOS)
// =========================================================================

#[cfg(unix)]
#[test]
fn unix_socket_transport_creation() -> Result<(), BoxErr> {
    let transport = TransportType::unix_socket("/tmp/openracing-test.sock");
    match transport {
        TransportType::UnixSocket { socket_path } => {
            assert_eq!(
                socket_path,
                std::path::PathBuf::from("/tmp/openracing-test.sock")
            );
        }
        _ => return Err("Expected UnixSocket transport variant".into()),
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_description_contains_path() -> Result<(), BoxErr> {
    let transport = TransportType::unix_socket("/run/user/1000/openracing.sock");
    let desc = transport.description();
    assert!(
        desc.contains("Unix socket"),
        "Description should mention 'Unix socket', got: {desc}"
    );
    assert!(
        desc.contains("openracing.sock"),
        "Description should contain socket path, got: {desc}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportType::unix_socket("/tmp/openracing-serde.sock");
    let json = serde_json::to_string(&original)?;
    let deserialized: TransportType = serde_json::from_str(&json)?;
    match deserialized {
        TransportType::UnixSocket { socket_path } => {
            assert_eq!(
                socket_path,
                std::path::PathBuf::from("/tmp/openracing-serde.sock")
            );
        }
        _ => return Err("Expected UnixSocket after deserialization".into()),
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_socket_in_transport_config() -> Result<(), BoxErr> {
    let config = TransportBuilder::new()
        .transport(TransportType::unix_socket("/tmp/or-config.sock"))
        .max_connections(10)
        .build();
    match &config.transport {
        TransportType::UnixSocket { socket_path } => {
            assert_eq!(
                socket_path,
                &std::path::PathBuf::from("/tmp/or-config.sock")
            );
        }
        _ => return Err("Expected UnixSocket in config".into()),
    }
    assert_eq!(config.max_connections, 10);
    Ok(())
}

// =========================================================================
// gRPC / TCP transport
// =========================================================================

#[test]
fn tcp_transport_default_address_and_port() -> Result<(), BoxErr> {
    let transport = TransportType::tcp();
    match transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "127.0.0.1");
            assert_eq!(port, DEFAULT_TCP_PORT);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("Expected Tcp transport variant".into()),
    }
    Ok(())
}

#[test]
fn tcp_transport_custom_address() -> Result<(), BoxErr> {
    let transport = TransportType::tcp_with_address("0.0.0.0", 9999);
    match transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "0.0.0.0");
            assert_eq!(port, 9999);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("Expected Tcp transport variant".into()),
    }
    Ok(())
}

#[test]
fn tcp_description_contains_address_and_port() -> Result<(), BoxErr> {
    let transport = TransportType::tcp_with_address("10.0.0.5", 7777);
    let desc = transport.description();
    assert!(desc.contains("TCP"), "Should mention TCP, got: {desc}");
    assert!(
        desc.contains("10.0.0.5"),
        "Should contain address, got: {desc}"
    );
    assert!(desc.contains("7777"), "Should contain port, got: {desc}");
    Ok(())
}

#[test]
fn tcp_transport_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportType::tcp_with_address("192.168.0.1", 4040);
    let json = serde_json::to_string(&original)?;
    let deserialized: TransportType = serde_json::from_str(&json)?;
    match deserialized {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "192.168.0.1");
            assert_eq!(port, 4040);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("Expected Tcp after deserialization".into()),
    }
    Ok(())
}

#[test]
fn grpc_default_port_constant() {
    assert_eq!(DEFAULT_TCP_PORT, 50051);
}

// =========================================================================
// Transport auto-selection per platform
// =========================================================================

#[test]
fn platform_default_returns_correct_variant() -> Result<(), BoxErr> {
    let transport = TransportType::platform_default();

    #[cfg(windows)]
    match &transport {
        TransportType::NamedPipe { pipe_name } => {
            assert!(
                pipe_name.contains("openracing"),
                "Platform default pipe name should contain 'openracing', got: {pipe_name}"
            );
        }
        _ => return Err("Expected NamedPipe on Windows".into()),
    }

    #[cfg(unix)]
    match &transport {
        TransportType::UnixSocket { socket_path } => {
            let path_str = socket_path.to_string_lossy();
            assert!(
                path_str.contains("openracing"),
                "Platform default socket path should contain 'openracing', got: {path_str}"
            );
        }
        _ => return Err("Expected UnixSocket on Unix".into()),
    }

    // Ensure description is non-empty regardless of platform
    assert!(
        !transport.description().is_empty(),
        "Platform default description should not be empty"
    );
    Ok(())
}

#[test]
fn transport_type_default_matches_platform_default() -> Result<(), BoxErr> {
    let default_transport = TransportType::default();
    let platform_transport = TransportType::platform_default();
    // Both should produce the same description
    assert_eq!(default_transport.description(), platform_transport.description());
    Ok(())
}

#[test]
fn transport_config_default_uses_platform_transport() {
    let config = TransportConfig::default();
    let desc = config.transport.description();
    // Platform default should be something concrete
    assert!(!desc.is_empty());
}

// =========================================================================
// Connection timeout handling
// =========================================================================

#[test]
fn connection_timeout_default_is_30s() {
    let config = TransportConfig::default();
    assert_eq!(config.connection_timeout, Duration::from_secs(30));
}

#[test]
fn connection_timeout_custom_via_builder() -> Result<(), BoxErr> {
    let config = TransportBuilder::new()
        .connection_timeout(Duration::from_millis(500))
        .build();
    assert_eq!(config.connection_timeout, Duration::from_millis(500));
    Ok(())
}

#[test]
fn timeout_error_contains_duration() {
    let err = IpcError::timeout(5000);
    let msg = err.to_string();
    assert!(
        msg.contains("5000"),
        "Timeout error should contain timeout ms, got: {msg}"
    );
}

#[test]
fn timeout_error_is_recoverable() {
    let err = IpcError::timeout(3000);
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
}

#[test]
fn negotiation_timeout_config() {
    let config = IpcConfig::default();
    assert_eq!(config.negotiation_timeout, Duration::from_secs(5));
}

// =========================================================================
// Reconnection logic
// =========================================================================

#[tokio::test]
async fn server_can_restart_after_stop() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());

    server.start().await?;
    assert_eq!(server.state().await, ServerState::Running);

    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);

    // Restart
    server.start().await?;
    assert_eq!(server.state().await, ServerState::Running);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn double_start_returns_error() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.start().await;
    assert!(result.is_err(), "Double start should return an error");

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn stop_when_already_stopped_is_ok() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    // Stop without start should succeed (no-op)
    let result = server.stop().await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn client_registration_and_unregistration() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let client = ClientInfo {
        id: "reconnect-client-1".to_string(),
        connected_at: Instant::now(),
        version: PROTOCOL_VERSION.to_string(),
        features: vec!["device_management".to_string()],
        peer_info: PeerInfo::default(),
    };

    server.register_client(client).await;
    assert_eq!(server.client_count().await, 1);

    server.unregister_client("reconnect-client-1").await;
    assert_eq!(server.client_count().await, 0);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn stop_clears_all_clients() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    for i in 0..5 {
        let client = ClientInfo {
            id: format!("client-{i}"),
            connected_at: Instant::now(),
            version: PROTOCOL_VERSION.to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;
    }
    assert_eq!(server.client_count().await, 5);

    server.stop().await?;
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

// =========================================================================
// Message serialization / deserialization
// =========================================================================

#[test]
fn message_header_roundtrip_all_types() -> Result<(), BoxErr> {
    let all_types = [
        message_types::DEVICE,
        message_types::PROFILE,
        message_types::SAFETY,
        message_types::HEALTH,
        message_types::FEATURE_NEGOTIATION,
        message_types::GAME,
        message_types::TELEMETRY,
        message_types::DIAGNOSTIC,
    ];

    for &msg_type in &all_types {
        let header = MessageHeader::new(msg_type, 512, 99);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)?;
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.payload_len, 512);
        assert_eq!(decoded.sequence, 99);
    }
    Ok(())
}

#[test]
fn message_header_flags_roundtrip() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::DEVICE, 100, 1);
    header.set_flag(message_flags::COMPRESSED);
    header.set_flag(message_flags::REQUIRES_ACK);
    header.set_flag(message_flags::STREAMING);

    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert!(decoded.has_flag(message_flags::COMPRESSED));
    assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
    assert!(decoded.has_flag(message_flags::STREAMING));
    assert!(!decoded.has_flag(message_flags::IS_RESPONSE));
    assert!(!decoded.has_flag(message_flags::IS_ERROR));
    Ok(())
}

#[test]
fn message_header_zero_values() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0, 0, 0);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.message_type, 0);
    assert_eq!(decoded.payload_len, 0);
    assert_eq!(decoded.sequence, 0);
    assert_eq!(decoded.flags, 0);
    Ok(())
}

#[test]
fn message_header_max_values() -> Result<(), BoxErr> {
    let header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.message_type, u16::MAX);
    assert_eq!(decoded.payload_len, u32::MAX);
    assert_eq!(decoded.sequence, u32::MAX);
    Ok(())
}

#[test]
fn codec_encode_decode_protobuf_message() -> Result<(), BoxErr> {
    // Use a simple prost::Message: prost_types::Duration
    let original = prost_types::Duration {
        seconds: 42,
        nanos: 123_456_789,
    };

    let codec = MessageCodec::new();
    let encoded: Vec<u8> = MessageEncoder::encode(&codec, &original)?;
    let decoded: prost_types::Duration = MessageDecoder::decode(&codec, &encoded)?;

    assert_eq!(decoded.seconds, 42);
    assert_eq!(decoded.nanos, 123_456_789);
    Ok(())
}

#[test]
fn codec_encode_to_buffer_reuses_allocation() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let msg = prost_types::Duration {
        seconds: 10,
        nanos: 0,
    };

    let mut buffer = Vec::with_capacity(128);
    MessageEncoder::encode_to_buffer(&codec, &msg, &mut buffer)?;

    let decoded: prost_types::Duration = MessageDecoder::decode(&codec, &buffer)?;
    assert_eq!(decoded.seconds, 10);
    Ok(())
}

// =========================================================================
// Large message handling
// =========================================================================

#[test]
fn codec_rejects_message_exceeding_max_size() {
    let codec = MessageCodec::with_max_size(64);
    // Create a large payload by making a Duration — its encoding is small,
    // but we can test the validation path via decode with oversized bytes
    let big_bytes = vec![0u8; 128];
    let result: IpcResult<prost_types::Duration> = MessageDecoder::decode(&codec, &big_bytes);
    assert!(result.is_err());
}

#[test]
fn codec_rejects_zero_length_message() {
    let codec = MessageCodec::new();
    let empty: &[u8] = &[];
    let result: IpcResult<prost_types::Duration> = MessageDecoder::decode(&codec, empty);
    assert!(result.is_err());
}

#[test]
fn codec_accepts_message_at_exact_max_size() -> Result<(), BoxErr> {
    // prost_types::Duration encoded is small (~4 bytes); create a codec that fits
    let msg = prost_types::Duration {
        seconds: 1,
        nanos: 0,
    };
    let codec_big = MessageCodec::new();
    let encoded = MessageEncoder::encode(&codec_big, &msg)?;
    let exact_codec = MessageCodec::with_max_size(encoded.len());
    let decoded: prost_types::Duration = MessageDecoder::decode(&exact_codec, &encoded)?;
    assert_eq!(decoded.seconds, 1);
    Ok(())
}

#[test]
fn default_max_message_size_is_16mb() {
    let codec = MessageCodec::new();
    assert_eq!(codec.max_message_size(), 16 * 1024 * 1024);
}

#[test]
fn codec_is_valid_size_boundaries() {
    let codec = MessageCodec::with_max_size(1000);
    assert!(!codec.is_valid_size(0));
    assert!(codec.is_valid_size(1));
    assert!(codec.is_valid_size(999));
    assert!(codec.is_valid_size(1000));
    assert!(!codec.is_valid_size(1001));
}

// =========================================================================
// Concurrent connections
// =========================================================================

#[tokio::test]
async fn concurrent_client_registration() -> Result<(), BoxErr> {
    let server = std::sync::Arc::new(IpcServer::new(IpcConfig::default()));
    server.start().await?;

    let mut handles = vec![];
    for i in 0..20 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            let client = ClientInfo {
                id: format!("concurrent-client-{i}"),
                connected_at: Instant::now(),
                version: PROTOCOL_VERSION.to_string(),
                features: vec!["device_management".to_string()],
                peer_info: PeerInfo::default(),
            };
            srv.register_client(client).await;
        }));
    }

    for handle in handles {
        handle.await?;
    }

    assert_eq!(server.client_count().await, 20);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_feature_negotiation() -> Result<(), BoxErr> {
    let server = std::sync::Arc::new(IpcServer::new(IpcConfig::default()));
    server.start().await?;

    let mut handles = vec![];
    for _ in 0..10 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            srv.negotiate_features("1.0.0", &["device_management".to_string()])
                .await
        }));
    }

    for handle in handles {
        let result = handle.await?;
        let negotiation = result?;
        assert!(negotiation.compatible);
    }

    // Each negotiation registers a client
    assert_eq!(server.client_count().await, 10);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_register_and_unregister() -> Result<(), BoxErr> {
    let server = std::sync::Arc::new(IpcServer::new(IpcConfig::default()));
    server.start().await?;

    // Register 10 clients
    for i in 0..10 {
        let client = ClientInfo {
            id: format!("churn-client-{i}"),
            connected_at: Instant::now(),
            version: PROTOCOL_VERSION.to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;
    }

    // Concurrently unregister first 5 while registering 5 more
    let mut handles = vec![];
    for i in 0..5 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            srv.unregister_client(&format!("churn-client-{i}")).await;
        }));
    }
    for i in 10..15 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            let client = ClientInfo {
                id: format!("churn-client-{i}"),
                connected_at: Instant::now(),
                version: PROTOCOL_VERSION.to_string(),
                features: vec![],
                peer_info: PeerInfo::default(),
            };
            srv.register_client(client).await;
        }));
    }

    for handle in handles {
        handle.await?;
    }

    // 10 original - 5 removed + 5 new = 10
    assert_eq!(server.client_count().await, 10);
    server.stop().await?;
    Ok(())
}

// =========================================================================
// Health event broadcasting
// =========================================================================

#[tokio::test]
async fn health_event_broadcast_and_receive() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx = server.subscribe_health();

    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "dev-001".to_string(),
        event_type: HealthEventType::Connected,
        message: "Device online".to_string(),
        metadata: HashMap::new(),
    };
    server.broadcast_health_event(event);

    let received = rx.try_recv();
    assert!(received.is_ok());
    Ok(())
}

#[tokio::test]
async fn health_event_multiple_subscribers() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx1 = server.subscribe_health();
    let mut rx2 = server.subscribe_health();

    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "dev-multi".to_string(),
        event_type: HealthEventType::Fault,
        message: "Overcurrent".to_string(),
        metadata: HashMap::new(),
    };
    server.broadcast_health_event(event);

    assert!(rx1.try_recv().is_ok());
    assert!(rx2.try_recv().is_ok());
    Ok(())
}

// =========================================================================
// Error classification
// =========================================================================

#[test]
fn connection_failed_error_is_recoverable() {
    let err = IpcError::ConnectionFailed("network timeout".to_string());
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
}

#[test]
fn transport_init_error_is_fatal() {
    let err = IpcError::TransportInit("port in use".to_string());
    assert!(err.is_fatal());
    assert!(!err.is_recoverable());
}

#[test]
fn connection_limit_error_display() {
    let err = IpcError::connection_limit(100);
    let msg = err.to_string();
    assert!(
        msg.contains("100"),
        "Connection limit error should contain the limit, got: {msg}"
    );
}

#[test]
fn version_incompatibility_error_is_recoverable() {
    let err = IpcError::VersionIncompatibility {
        client: "0.9.0".to_string(),
        server: "1.0.0".to_string(),
    };
    assert!(err.is_recoverable());
}

#[test]
fn shutdown_requested_error_is_fatal() {
    let err = IpcError::ShutdownRequested;
    assert!(err.is_fatal());
}

// =========================================================================
// Version compatibility
// =========================================================================

#[test]
fn version_compat_same_version() {
    assert!(is_version_compatible("1.0.0", "1.0.0"));
}

#[test]
fn version_compat_higher_minor() {
    assert!(is_version_compatible("1.2.0", "1.0.0"));
}

#[test]
fn version_compat_higher_patch() {
    assert!(is_version_compatible("1.0.5", "1.0.0"));
}

#[test]
fn version_incompat_different_major() {
    assert!(!is_version_compatible("2.0.0", "1.0.0"));
}

#[test]
fn version_incompat_lower_minor() {
    assert!(!is_version_compatible("1.0.0", "1.1.0"));
}

#[test]
fn version_incompat_malformed() {
    assert!(!is_version_compatible("abc", "1.0.0"));
    assert!(!is_version_compatible("1.0", "1.0.0"));
}

// =========================================================================
// Feature negotiation
// =========================================================================

#[tokio::test]
async fn feature_negotiation_compatible_client() -> Result<(), BoxErr> {
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
    assert_eq!(result.server_version, PROTOCOL_VERSION);
    assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_incompatible_client() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("0.1.0", &["device_management".to_string()])
        .await?;

    assert!(!result.compatible);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_unknown_features_filtered() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "device_management".to_string(),
                "nonexistent_feature".to_string(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert!(result.enabled_features.contains(&"device_management".to_string()));
    assert!(!result.enabled_features.contains(&"nonexistent_feature".to_string()));

    server.stop().await?;
    Ok(())
}

// =========================================================================
// IPC config builder
// =========================================================================

#[test]
fn ipc_config_default_server_name() {
    let config = IpcConfig::default();
    assert_eq!(config.server_name, "openracing-ipc");
}

#[test]
fn ipc_config_with_transport_overrides() {
    let config = IpcConfig::with_transport(TransportType::tcp_with_address("0.0.0.0", 8080));
    match &config.transport.transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "0.0.0.0");
            assert_eq!(*port, 8080);
        }
        #[allow(unreachable_patterns)]
        _ => panic!("Expected Tcp transport"),
    }
}

#[test]
fn ipc_config_builder_chain() {
    let config = IpcConfig::default()
        .max_connections(75)
        .health_buffer_size(2000);

    assert_eq!(config.transport.max_connections, 75);
    assert_eq!(config.health_buffer_size, 2000);
}

// =========================================================================
// Transport builder validation
// =========================================================================

#[test]
fn transport_builder_defaults() {
    let config = TransportBuilder::new().build();
    assert_eq!(config.max_connections, 100);
    assert!(!config.enable_acl);
    assert_eq!(config.recv_buffer_size, 64 * 1024);
    assert_eq!(config.send_buffer_size, 64 * 1024);
}

#[test]
fn transport_builder_acl_toggle() {
    let config = TransportBuilder::new().enable_acl(true).build();
    assert!(config.enable_acl);

    let config2 = TransportBuilder::new().enable_acl(false).build();
    assert!(!config2.enable_acl);
}

#[test]
fn transport_config_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportBuilder::new()
        .transport(TransportType::tcp_with_address("10.0.0.1", 6060))
        .max_connections(42)
        .connection_timeout(Duration::from_secs(15))
        .enable_acl(true)
        .build();

    let json = serde_json::to_string(&original)?;
    let deserialized: TransportConfig = serde_json::from_str(&json)?;

    assert_eq!(deserialized.max_connections, 42);
    assert_eq!(deserialized.connection_timeout, Duration::from_secs(15));
    assert!(deserialized.enable_acl);
    Ok(())
}
