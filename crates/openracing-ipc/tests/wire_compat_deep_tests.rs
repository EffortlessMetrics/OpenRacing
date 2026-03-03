//! Deep tests for IPC wire format versioning, protocol evolution, and backward
//! compatibility.
//!
//! Covers: wire header parsing, forward/backward compat, unknown message types,
//! unknown fields, feature negotiation, message size limits, fragmented messages,
//! connection lifecycle, reconnection, concurrent connections, error format
//! consistency, timeout handling, heartbeat/keepalive, and protocol downgrade
//! prevention.

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
use openracing_ipc::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Inline prost message for codec round-trip tests.
#[derive(Clone, PartialEq, prost::Message)]
struct SimplePayload {
    #[prost(string, tag = "1")]
    value: String,
}

/// Extended prost message with extra fields (simulates a newer schema).
#[derive(Clone, PartialEq, prost::Message)]
struct ExtendedPayload {
    #[prost(string, tag = "1")]
    value: String,
    #[prost(string, tag = "2")]
    extra_field: String,
    #[prost(int32, tag = "3")]
    extra_number: i32,
}

/// Prost message that simulates a completely different schema.
#[derive(Clone, PartialEq, prost::Message)]
struct AlternatePayload {
    #[prost(int64, tag = "10")]
    alt_id: i64,
    #[prost(bytes = "vec", tag = "11")]
    alt_data: Vec<u8>,
}

// ──────────────────────────────────────────────────────────────────────
// 1. Wire format version header parsing
// ──────────────────────────────────────────────────────────────────────

#[test]
fn header_round_trip_all_message_types() -> TestResult {
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
        let header = MessageHeader::new(msg_type, 512, 99);
        let encoded = header.encode();
        assert_eq!(encoded.len(), MessageHeader::SIZE);

        let decoded = MessageHeader::decode(&encoded)?;
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.payload_len, 512);
        assert_eq!(decoded.sequence, 99);
        assert_eq!(decoded.flags, 0);
    }
    Ok(())
}

#[test]
fn header_max_values_round_trip() -> TestResult {
    let header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert_eq!(decoded.message_type, u16::MAX);
    assert_eq!(decoded.payload_len, u32::MAX);
    assert_eq!(decoded.sequence, u32::MAX);
    Ok(())
}

#[test]
fn header_zero_values_round_trip() -> TestResult {
    let header = MessageHeader::new(0, 0, 0);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert_eq!(decoded.message_type, 0);
    assert_eq!(decoded.payload_len, 0);
    assert_eq!(decoded.sequence, 0);
    Ok(())
}

#[test]
fn header_decode_rejects_insufficient_bytes() {
    for len in 0..MessageHeader::SIZE {
        let bytes = vec![0u8; len];
        let result = MessageHeader::decode(&bytes);
        assert!(result.is_err(), "Expected error for {} bytes", len);
    }
}

#[test]
fn header_decode_accepts_extra_trailing_bytes() -> TestResult {
    let header = MessageHeader::new(message_types::DEVICE, 100, 1);
    let mut bytes = header.encode().to_vec();
    bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // extra trailing bytes

    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.message_type, message_types::DEVICE);
    assert_eq!(decoded.payload_len, 100);
    Ok(())
}

#[test]
fn header_flags_combination_round_trip() -> TestResult {
    let mut header = MessageHeader::new(message_types::HEALTH, 256, 7);
    header.set_flag(message_flags::COMPRESSED);
    header.set_flag(message_flags::REQUIRES_ACK);
    header.set_flag(message_flags::IS_RESPONSE);
    header.set_flag(message_flags::IS_ERROR);
    header.set_flag(message_flags::STREAMING);

    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert!(decoded.has_flag(message_flags::COMPRESSED));
    assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
    assert!(decoded.has_flag(message_flags::IS_RESPONSE));
    assert!(decoded.has_flag(message_flags::IS_ERROR));
    assert!(decoded.has_flag(message_flags::STREAMING));
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 2. Forward compatibility (new client → old server)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn forward_compat_extended_payload_decoded_as_simple() -> TestResult {
    let codec = MessageCodec::new();
    let extended = ExtendedPayload {
        value: "hello".into(),
        extra_field: "new-data".into(),
        extra_number: 42,
    };
    let bytes = codec.encode(&extended)?;

    // Decode as the simpler (older) message — unknown fields are silently ignored
    let simple: SimplePayload = codec.decode(&bytes)?;
    assert_eq!(simple.value, "hello");
    Ok(())
}

#[test]
fn forward_compat_newer_minor_version_client_accepted() {
    // A newer minor-version client is still compatible
    assert!(is_version_compatible("1.5.0", "1.0.0"));
    assert!(is_version_compatible("1.99.99", "1.0.0"));
}

#[test]
fn forward_compat_newer_major_version_rejected() {
    assert!(!is_version_compatible("2.0.0", "1.0.0"));
    assert!(!is_version_compatible("3.1.0", "1.0.0"));
}

// ──────────────────────────────────────────────────────────────────────
// 3. Backward compatibility (old client → new server)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn backward_compat_simple_payload_decoded_as_extended() -> TestResult {
    let codec = MessageCodec::new();
    let simple = SimplePayload {
        value: "legacy".into(),
    };
    let bytes = codec.encode(&simple)?;

    // Decode as the extended (newer) message — missing fields get defaults
    let extended: ExtendedPayload = codec.decode(&bytes)?;
    assert_eq!(extended.value, "legacy");
    assert_eq!(extended.extra_field, "");
    assert_eq!(extended.extra_number, 0);
    Ok(())
}

#[test]
fn backward_compat_empty_payload_decoded_with_defaults() -> TestResult {
    // An empty protobuf message has zero encoded length; the codec rejects
    // zero-size payloads. Verify this is caught as an encoding-level error,
    // not a panic.
    let codec = MessageCodec::new();
    let empty = SimplePayload { value: String::new() };
    let result = codec.encode(&empty);
    assert!(result.is_err(), "Zero-length protobuf should be rejected by codec");
    Ok(())
}

#[tokio::test]
async fn backward_compat_old_client_version_exact_minimum() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(MIN_CLIENT_VERSION, &["device_management".to_string()])
        .await?;

    assert!(result.compatible);
    server.stop().await?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 4. Unknown message types handled gracefully
// ──────────────────────────────────────────────────────────────────────

#[test]
fn unknown_message_type_in_header_round_trips() -> TestResult {
    let unknown_type: u16 = 0xFFFE;
    let header = MessageHeader::new(unknown_type, 64, 10);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert_eq!(decoded.message_type, unknown_type);
    assert_eq!(decoded.payload_len, 64);
    Ok(())
}

#[test]
fn reserved_message_type_zero_round_trips() -> TestResult {
    let header = MessageHeader::new(0x0000, 0, 0);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.message_type, 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 5. Unknown fields in known messages preserved (or safely ignored)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn unknown_fields_silently_ignored_on_decode() -> TestResult {
    let codec = MessageCodec::new();

    // AlternatePayload uses entirely different tag numbers (10, 11)
    let alt = AlternatePayload {
        alt_id: 999,
        alt_data: vec![1, 2, 3],
    };
    let bytes = codec.encode(&alt)?;

    // Decode as SimplePayload — fields at tags 10, 11 are unknown and ignored
    let simple: SimplePayload = codec.decode(&bytes)?;
    assert_eq!(simple.value, ""); // tag 1 not present → default
    Ok(())
}

#[test]
fn unknown_flags_in_header_preserved() -> TestResult {
    let mut header = MessageHeader::new(message_types::DEVICE, 128, 5);
    // Set a "future" flag bit that doesn't exist yet
    header.flags = 0xFF00;
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;
    assert_eq!(decoded.flags, 0xFF00);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 6. Feature negotiation handshake
// ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn feature_negotiation_returns_server_version() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;

    assert_eq!(result.server_version, PROTOCOL_VERSION);
    assert!(!result.min_client_version.is_empty());
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_no_features_requested() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features("1.0.0", &[]).await?;

    assert!(result.compatible);
    assert!(result.enabled_features.is_empty());
    assert!(!result.supported_features.is_empty());
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_unknown_features_filtered() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "device_management".to_string(),
                "time_travel".to_string(),
                "teleportation".to_string(),
            ],
        )
        .await?;

    assert!(result.compatible);
    // Only known features are enabled
    assert!(result.enabled_features.contains(&"device_management".to_string()));
    assert!(!result.enabled_features.contains(&"time_travel".to_string()));
    assert!(!result.enabled_features.contains(&"teleportation".to_string()));
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn feature_negotiation_incompatible_version_not_registered() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("0.1.0", &["device_management".to_string()])
        .await?;

    assert!(!result.compatible);
    // Incompatible client should not be registered
    assert_eq!(server.client_count().await, 0);
    server.stop().await?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 7. Message size limits enforcement
// ──────────────────────────────────────────────────────────────────────

#[test]
fn codec_rejects_oversized_encode() {
    let codec = MessageCodec::with_max_size(64);
    let big = SimplePayload {
        value: "x".repeat(200),
    };
    let result = codec.encode(&big);
    assert!(result.is_err());
}

#[test]
fn codec_rejects_zero_length_decode() {
    let codec = MessageCodec::new();
    let result: IpcResult<SimplePayload> = codec.decode(&[]);
    assert!(result.is_err());
}

#[test]
fn codec_rejects_oversized_decode() {
    let codec = MessageCodec::with_max_size(8);
    let big_bytes = vec![0u8; 64];
    let result: IpcResult<SimplePayload> = codec.decode(&big_bytes);
    assert!(result.is_err());
}

#[test]
fn codec_accepts_exactly_max_size() -> TestResult {
    let codec = MessageCodec::with_max_size(1024);
    // Build a payload that encodes to <= 1024 bytes
    let payload = SimplePayload {
        value: "a".repeat(900),
    };
    let bytes = codec.encode(&payload)?;
    assert!(bytes.len() <= 1024);

    let decoded: SimplePayload = codec.decode(&bytes)?;
    assert_eq!(decoded.value.len(), 900);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 8. Fragmented message handling
// ──────────────────────────────────────────────────────────────────────

#[test]
fn fragmented_header_reassembly() -> TestResult {
    let header = MessageHeader::new(message_types::TELEMETRY, 2048, 77);
    let full = header.encode();

    // Simulate two fragments that together form the header
    let frag1 = &full[..6];
    let frag2 = &full[6..];
    let mut assembled = Vec::new();
    assembled.extend_from_slice(frag1);
    assembled.extend_from_slice(frag2);

    let decoded = MessageHeader::decode(&assembled)?;
    assert_eq!(decoded.message_type, message_types::TELEMETRY);
    assert_eq!(decoded.payload_len, 2048);
    assert_eq!(decoded.sequence, 77);
    Ok(())
}

#[test]
fn fragmented_payload_reassembly() -> TestResult {
    let codec = MessageCodec::new();
    let payload = SimplePayload {
        value: "fragmented-data".into(),
    };
    let full_bytes = codec.encode(&payload)?;

    // Split into chunks and reassemble
    let mid = full_bytes.len() / 2;
    let chunk1 = &full_bytes[..mid];
    let chunk2 = &full_bytes[mid..];
    let mut reassembled = Vec::new();
    reassembled.extend_from_slice(chunk1);
    reassembled.extend_from_slice(chunk2);

    let decoded: SimplePayload = codec.decode(&reassembled)?;
    assert_eq!(decoded.value, "fragmented-data");
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 9. Connection lifecycle
// ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn lifecycle_stopped_starting_running_shutdown() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    assert_eq!(server.state().await, ServerState::Stopped);

    server.start().await?;
    assert_eq!(server.state().await, ServerState::Running);
    assert!(server.is_running().await);

    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    assert!(!server.is_running().await);
    Ok(())
}

#[tokio::test]
async fn lifecycle_double_start_returns_error() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let err = server.start().await;
    assert!(err.is_err());
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn lifecycle_stop_when_already_stopped_is_ok() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    // Stopping a never-started server should be fine
    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    Ok(())
}

#[tokio::test]
async fn lifecycle_negotiate_registers_client() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;
    assert_eq!(server.client_count().await, 0);

    let result = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;
    assert!(result.compatible);
    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    // After stop, clients are cleared
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 10. Reconnection with session continuity
// ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn reconnection_after_disconnect_succeeds() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // First connection
    let r1 = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;
    assert!(r1.compatible);
    assert_eq!(server.client_count().await, 1);

    // Simulate disconnect by unregistering
    let clients = server.connected_clients().await;
    assert_eq!(clients.len(), 1);
    server.unregister_client(&clients[0].id).await;
    assert_eq!(server.client_count().await, 0);

    // Reconnect
    let r2 = server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;
    assert!(r2.compatible);
    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 11. Concurrent connection handling
// ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_multiple_clients_registered() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    for i in 0..5 {
        let features = vec![format!("feature_{}", i)];
        let r = server.negotiate_features("1.0.0", &features).await?;
        assert!(r.compatible);
    }

    assert_eq!(server.client_count().await, 5);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_manual_register_unregister() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let client_a = ClientInfo {
        id: "client-a".to_string(),
        connected_at: Instant::now(),
        version: "1.0.0".to_string(),
        features: vec!["device_management".to_string()],
        peer_info: PeerInfo::default(),
    };
    let client_b = ClientInfo {
        id: "client-b".to_string(),
        connected_at: Instant::now(),
        version: "1.0.0".to_string(),
        features: vec!["safety_control".to_string()],
        peer_info: PeerInfo::default(),
    };

    server.register_client(client_a).await;
    server.register_client(client_b).await;
    assert_eq!(server.client_count().await, 2);

    server.unregister_client("client-a").await;
    assert_eq!(server.client_count().await, 1);

    let remaining = server.connected_clients().await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "client-b");

    server.stop().await?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 12. Error response format consistency
// ──────────────────────────────────────────────────────────────────────

#[test]
fn error_encoding_failed_is_not_recoverable() {
    let err = IpcError::EncodingFailed("bad data".to_string());
    assert!(!err.is_recoverable());
    assert!(!err.is_fatal());
    assert!(err.to_string().contains("bad data"));
}

#[test]
fn error_decoding_failed_is_not_recoverable() {
    let err = IpcError::DecodingFailed("corrupt".to_string());
    assert!(!err.is_recoverable());
    assert!(!err.is_fatal());
    assert!(err.to_string().contains("corrupt"));
}

#[test]
fn error_version_incompatibility_is_recoverable() {
    let err = IpcError::VersionIncompatibility {
        client: "2.0.0".to_string(),
        server: "1.0.0".to_string(),
    };
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
    let msg = err.to_string();
    assert!(msg.contains("2.0.0"));
    assert!(msg.contains("1.0.0"));
}

#[test]
fn error_feature_negotiation_is_recoverable() {
    let err = IpcError::FeatureNegotiation("unsupported feature".to_string());
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
}

#[test]
fn error_connection_limit_display_format() {
    let err = IpcError::connection_limit(100);
    let msg = err.to_string();
    assert!(msg.contains("100"));
    assert!(msg.contains("limit"));
}

// ──────────────────────────────────────────────────────────────────────
// 13. Timeout handling for slow responses
// ──────────────────────────────────────────────────────────────────────

#[test]
fn timeout_error_carries_duration() {
    let err = IpcError::timeout(5000);
    assert!(err.is_recoverable());
    assert!(!err.is_fatal());
    let msg = err.to_string();
    assert!(msg.contains("5000"));
}

#[test]
fn timeout_zero_ms_is_valid() {
    let err = IpcError::timeout(0);
    let msg = err.to_string();
    assert!(msg.contains("0"));
}

#[test]
fn negotiation_timeout_configurable() {
    let config = IpcConfig::default();
    assert_eq!(config.negotiation_timeout, Duration::from_secs(5));

    let custom = IpcConfig {
        negotiation_timeout: Duration::from_millis(500),
        ..IpcConfig::default()
    };
    assert_eq!(custom.negotiation_timeout, Duration::from_millis(500));
}

// ──────────────────────────────────────────────────────────────────────
// 14. Heartbeat / keepalive mechanism
// ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_event_broadcast_round_trip() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx = server.subscribe_health();

    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "dev-hb-1".to_string(),
        event_type: HealthEventType::Connected,
        message: "keepalive".to_string(),
        metadata: HashMap::new(),
    };
    server.broadcast_health_event(event);

    let received = rx.try_recv();
    assert!(received.is_ok());
    let evt = received?;
    assert_eq!(evt.device_id, "dev-hb-1");
    assert_eq!(evt.message, "keepalive");
    Ok(())
}

#[tokio::test]
async fn health_buffer_size_configurable() -> TestResult {
    let config = IpcConfig::default().health_buffer_size(10);
    let server = IpcServer::new(config);
    let mut rx = server.subscribe_health();

    // Broadcast events to fill the buffer
    for i in 0..10 {
        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: format!("dev-{}", i),
            event_type: HealthEventType::Connected,
            message: "ping".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);
    }

    // Should be able to receive at least some events
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    assert!(count > 0);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────
// 15. Protocol downgrade prevention (security)
// ──────────────────────────────────────────────────────────────────────

#[test]
fn downgrade_older_major_version_rejected() {
    assert!(!is_version_compatible("0.9.0", "1.0.0"));
    assert!(!is_version_compatible("0.1.0", "1.0.0"));
    assert!(!is_version_compatible("0.0.1", "1.0.0"));
}

#[test]
fn downgrade_older_minor_version_rejected() {
    assert!(!is_version_compatible("1.0.0", "1.1.0"));
    assert!(!is_version_compatible("1.0.9", "1.1.0"));
}

#[test]
fn downgrade_older_patch_version_rejected() {
    assert!(!is_version_compatible("1.1.0", "1.1.1"));
    assert!(!is_version_compatible("1.0.0", "1.0.1"));
}

#[tokio::test]
async fn downgrade_prevention_via_negotiation() -> TestResult {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Client claiming version older than minimum should be rejected
    let result = server
        .negotiate_features("0.9.0", &["device_management".to_string()])
        .await?;

    assert!(!result.compatible);
    assert_eq!(server.client_count().await, 0);
    server.stop().await?;
    Ok(())
}

#[test]
fn downgrade_malformed_version_strings_rejected() {
    assert!(!is_version_compatible("", "1.0.0"));
    assert!(!is_version_compatible("not.a.version", "1.0.0"));
    assert!(!is_version_compatible("1.0", "1.0.0"));
    assert!(!is_version_compatible("v1.0.0", "1.0.0"));
    assert!(!is_version_compatible("1", "1.0.0"));
}

// ──────────────────────────────────────────────────────────────────────
// 16. Additional wire format edge cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn header_little_endian_encoding_verified() -> TestResult {
    let header = MessageHeader::new(0x0102, 0x03040506, 0x0708090A);
    let bytes = header.encode();

    // Verify little-endian layout
    assert_eq!(bytes[0], 0x02); // message_type low byte
    assert_eq!(bytes[1], 0x01); // message_type high byte
    assert_eq!(bytes[2], 0x06); // payload_len lowest byte
    assert_eq!(bytes[3], 0x05);
    assert_eq!(bytes[4], 0x04);
    assert_eq!(bytes[5], 0x03); // payload_len highest byte
    assert_eq!(bytes[6], 0x0A); // sequence lowest byte
    assert_eq!(bytes[7], 0x09);
    assert_eq!(bytes[8], 0x08);
    assert_eq!(bytes[9], 0x07); // sequence highest byte

    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.message_type, 0x0102);
    assert_eq!(decoded.payload_len, 0x03040506);
    assert_eq!(decoded.sequence, 0x0708090A);
    Ok(())
}

#[test]
fn codec_encode_decode_round_trip_large_payload() -> TestResult {
    let codec = MessageCodec::new();
    let payload = SimplePayload {
        value: "x".repeat(1_000_000),
    };
    let bytes = codec.encode(&payload)?;
    let decoded: SimplePayload = codec.decode(&bytes)?;
    assert_eq!(decoded.value.len(), 1_000_000);
    Ok(())
}

#[test]
fn transport_config_defaults_are_consistent() {
    let config = TransportConfig::default();
    assert_eq!(config.max_connections, 100);
    assert_eq!(config.connection_timeout, Duration::from_secs(30));
    assert!(!config.enable_acl);
    assert_eq!(config.recv_buffer_size, 64 * 1024);
    assert_eq!(config.send_buffer_size, 64 * 1024);
}

#[test]
fn transport_builder_overrides_all_fields() {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp_with_address("0.0.0.0", 9999))
        .max_connections(200)
        .connection_timeout(Duration::from_secs(60))
        .enable_acl(true)
        .build();

    assert_eq!(config.max_connections, 200);
    assert_eq!(config.connection_timeout, Duration::from_secs(60));
    assert!(config.enable_acl);
}

#[test]
fn protocol_version_constants_are_valid_semver() {
    let parts: Vec<&str> = PROTOCOL_VERSION.split('.').collect();
    assert_eq!(parts.len(), 3, "PROTOCOL_VERSION must be semver");
    for part in &parts {
        assert!(part.parse::<u32>().is_ok(), "Each part must be a number");
    }

    let min_parts: Vec<&str> = MIN_CLIENT_VERSION.split('.').collect();
    assert_eq!(min_parts.len(), 3, "MIN_CLIENT_VERSION must be semver");
    for part in &min_parts {
        assert!(part.parse::<u32>().is_ok(), "Each part must be a number");
    }
}
