//! IPC wire format validation tests.
//!
//! Covers:
//! - Wire format stability (encode → snapshot comparison)
//! - Message framing correctness for multi-message streams
//! - Large message handling and codec limits
//! - Concurrent message sequencing
//! - Error message formatting and classification
//! - Transport configuration validation
//! - Server lifecycle and feature negotiation

use std::collections::HashMap;
use std::time::Duration;

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
// Section 1: Wire format stability (encode → snapshot)
// =========================================================================

#[test]
fn header_wire_bytes_match_known_layout() -> Result<(), BoxErr> {
    // Known-good header: type=0x0001, len=256, seq=42, flags=0
    let header = MessageHeader::new(0x0001, 256, 42);
    let bytes = header.encode();

    // message_type: LE u16
    assert_eq!(bytes[0], 0x01);
    assert_eq!(bytes[1], 0x00);
    // payload_len: LE u32 = 256 = 0x00000100
    assert_eq!(bytes[2], 0x00);
    assert_eq!(bytes[3], 0x01);
    assert_eq!(bytes[4], 0x00);
    assert_eq!(bytes[5], 0x00);
    // sequence: LE u32 = 42 = 0x0000002A
    assert_eq!(bytes[6], 0x2A);
    assert_eq!(bytes[7], 0x00);
    assert_eq!(bytes[8], 0x00);
    assert_eq!(bytes[9], 0x00);
    // flags: LE u16 = 0
    assert_eq!(bytes[10], 0x00);
    assert_eq!(bytes[11], 0x00);
    Ok(())
}

#[test]
fn header_encode_decode_is_identity_for_all_message_types() -> Result<(), BoxErr> {
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

    for msg_type in types {
        let header = MessageHeader::new(msg_type, 512, 99);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.payload_len, 512);
        assert_eq!(decoded.sequence, 99);
    }
    Ok(())
}

#[test]
fn protobuf_encode_deterministic_across_calls() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let msg = prost_types::Duration {
        seconds: 42,
        nanos: 123456789,
    };

    let enc1 = MessageEncoder::encode(&codec, &msg)?;
    let enc2 = MessageEncoder::encode(&codec, &msg)?;
    let enc3 = MessageEncoder::encode(&codec, &msg)?;
    assert_eq!(enc1, enc2);
    assert_eq!(enc2, enc3);
    Ok(())
}

#[test]
fn protobuf_timestamp_field_values_preserved() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let msg = prost_types::Timestamp {
        seconds: 1_700_000_000,
        nanos: 999_999_999,
    };

    let bytes = MessageEncoder::encode(&codec, &msg)?;
    let decoded: prost_types::Timestamp = MessageDecoder::decode(&codec, &bytes)?;
    assert_eq!(decoded.seconds, 1_700_000_000);
    assert_eq!(decoded.nanos, 999_999_999);
    Ok(())
}

#[test]
fn header_max_field_values() -> Result<(), BoxErr> {
    let header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.message_type, u16::MAX);
    assert_eq!(decoded.payload_len, u32::MAX);
    assert_eq!(decoded.sequence, u32::MAX);
    Ok(())
}

#[test]
fn header_zero_field_values() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0, 0, 0);
    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.message_type, 0);
    assert_eq!(decoded.payload_len, 0);
    assert_eq!(decoded.sequence, 0);
    assert_eq!(decoded.flags, 0);
    Ok(())
}

// =========================================================================
// Section 2: Message framing correctness
// =========================================================================

#[test]
fn framing_three_consecutive_messages() -> Result<(), BoxErr> {
    let messages: Vec<(u16, &[u8])> = vec![
        (message_types::DEVICE, &[0x01, 0x02, 0x03]),
        (message_types::TELEMETRY, &[0xAA, 0xBB]),
        (message_types::HEALTH, &[0xFF]),
    ];

    let mut stream = Vec::new();
    for (seq, (msg_type, payload)) in messages.iter().enumerate() {
        let header = MessageHeader::new(*msg_type, payload.len() as u32, seq as u32);
        stream.extend_from_slice(&header.encode());
        stream.extend_from_slice(payload);
    }

    // Parse them back
    let mut offset = 0;
    for (seq, (msg_type, payload)) in messages.iter().enumerate() {
        let header = MessageHeader::decode(&stream[offset..offset + MessageHeader::SIZE])?;
        assert_eq!(header.message_type, *msg_type);
        assert_eq!(header.sequence, seq as u32);
        assert_eq!(header.payload_len, payload.len() as u32);

        offset += MessageHeader::SIZE;
        let p_end = offset + header.payload_len as usize;
        assert_eq!(&stream[offset..p_end], *payload);
        offset = p_end;
    }
    assert_eq!(offset, stream.len());
    Ok(())
}

#[test]
fn framing_empty_payload_messages() -> Result<(), BoxErr> {
    // Messages with zero-length payload (just headers)
    let mut stream = Vec::new();
    for i in 0..5 {
        let header = MessageHeader::new(message_types::HEALTH, 0, i);
        stream.extend_from_slice(&header.encode());
    }

    // Should be exactly 5 headers
    assert_eq!(stream.len(), 5 * MessageHeader::SIZE);

    // Parse all 5
    for i in 0..5u32 {
        let offset = (i as usize) * MessageHeader::SIZE;
        let header = MessageHeader::decode(&stream[offset..offset + MessageHeader::SIZE])?;
        assert_eq!(header.sequence, i);
        assert_eq!(header.payload_len, 0);
    }
    Ok(())
}

#[test]
fn framing_with_flags_preserved() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::DEVICE, 4, 1);
    header.set_flag(message_flags::COMPRESSED);
    header.set_flag(message_flags::REQUIRES_ACK);

    let mut stream = Vec::new();
    stream.extend_from_slice(&header.encode());
    stream.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);

    let decoded = MessageHeader::decode(&stream[..MessageHeader::SIZE])?;
    assert!(decoded.has_flag(message_flags::COMPRESSED));
    assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
    assert!(!decoded.has_flag(message_flags::IS_ERROR));
    Ok(())
}

#[test]
fn framing_header_only_no_payload_read() -> Result<(), BoxErr> {
    // Ensure we can parse just the header without reading any payload
    let header = MessageHeader::new(message_types::PROFILE, 1024 * 1024, 0);
    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;
    // We know there should be 1MB of payload, but we only decoded the header
    assert_eq!(decoded.payload_len, 1024 * 1024);
    Ok(())
}

// =========================================================================
// Section 3: Large message handling
// =========================================================================

#[test]
fn codec_rejects_zero_length_message() {
    let codec = MessageCodec::new();
    assert!(!codec.is_valid_size(0));
}

#[test]
fn codec_rejects_oversized_message() {
    let codec = MessageCodec::new();
    let too_big = codec.max_message_size() + 1;
    assert!(!codec.is_valid_size(too_big));
}

#[test]
fn codec_accepts_max_size_message() {
    let codec = MessageCodec::new();
    assert!(codec.is_valid_size(codec.max_message_size()));
}

#[test]
fn codec_custom_size_limit() {
    let codec = MessageCodec::with_max_size(1024);
    assert!(codec.is_valid_size(1024));
    assert!(!codec.is_valid_size(1025));
    assert_eq!(codec.max_message_size(), 1024);
}

#[test]
fn codec_encode_rejects_message_exceeding_limit() {
    let codec = MessageCodec::with_max_size(4);
    let msg = prost_types::Duration {
        seconds: 1_000_000_000,
        nanos: 999_999_999,
    };
    let result = MessageEncoder::encode(&codec, &msg);
    assert!(result.is_err());
}

#[test]
fn codec_decode_rejects_message_exceeding_limit() {
    let small_codec = MessageCodec::with_max_size(2);
    let big_bytes = vec![0x08, 0x01, 0x10, 0x02]; // small protobuf but > 2 bytes
    let result: IpcResult<prost_types::Duration> = MessageDecoder::decode(&small_codec, &big_bytes);
    assert!(result.is_err());
}

#[test]
fn codec_default_limit_is_16mb() {
    let codec = MessageCodec::new();
    assert_eq!(codec.max_message_size(), 16 * 1024 * 1024);
}

#[test]
fn encode_to_buffer_clears_and_fills() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let msg = prost_types::Duration {
        seconds: 10,
        nanos: 20,
    };

    let mut buf = vec![0xFF; 100]; // pre-filled
    MessageEncoder::encode_to_buffer(&codec, &msg, &mut buf)?;

    // Buffer should now contain only the encoded message
    assert!(buf.len() < 100);
    assert!(!buf.is_empty());

    let decoded: prost_types::Duration = MessageDecoder::decode(&codec, &buf)?;
    assert_eq!(decoded.seconds, 10);
    assert_eq!(decoded.nanos, 20);
    Ok(())
}

#[test]
fn encoded_len_matches_actual_encoding() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let msg = prost_types::Duration {
        seconds: 42,
        nanos: 99,
    };

    let predicted_len = MessageDecoder::encoded_len(&codec, &msg);
    let actual = MessageEncoder::encode(&codec, &msg)?;
    assert_eq!(predicted_len, actual.len());
    Ok(())
}

// =========================================================================
// Section 4: Concurrent message sequencing
// =========================================================================

#[test]
fn sequence_numbers_monotonic_in_stream() -> Result<(), BoxErr> {
    let mut stream = Vec::new();
    let count = 100u32;

    for seq in 0..count {
        let header = MessageHeader::new(message_types::TELEMETRY, 0, seq);
        stream.extend_from_slice(&header.encode());
    }

    let mut prev_seq = None;
    for i in 0..count {
        let offset = (i as usize) * MessageHeader::SIZE;
        let header = MessageHeader::decode(&stream[offset..offset + MessageHeader::SIZE])?;
        if let Some(prev) = prev_seq {
            assert!(
                header.sequence > prev,
                "sequence {} should be > {}",
                header.sequence,
                prev
            );
        }
        prev_seq = Some(header.sequence);
    }
    Ok(())
}

#[test]
fn interleaved_message_types_with_sequences() -> Result<(), BoxErr> {
    let mut stream = Vec::new();
    let msg_types = [
        message_types::DEVICE,
        message_types::TELEMETRY,
        message_types::HEALTH,
        message_types::PROFILE,
    ];

    for (seq, &msg_type) in msg_types.iter().enumerate() {
        let header = MessageHeader::new(msg_type, 0, seq as u32);
        stream.extend_from_slice(&header.encode());
    }

    for (i, &expected_type) in msg_types.iter().enumerate() {
        let offset = i * MessageHeader::SIZE;
        let header = MessageHeader::decode(&stream[offset..offset + MessageHeader::SIZE])?;
        assert_eq!(header.message_type, expected_type);
        assert_eq!(header.sequence, i as u32);
    }
    Ok(())
}

#[test]
fn sequence_number_wraps_at_u32_max() -> Result<(), BoxErr> {
    let header = MessageHeader::new(message_types::DEVICE, 0, u32::MAX);
    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;
    assert_eq!(decoded.sequence, u32::MAX);
    Ok(())
}

// =========================================================================
// Section 5: Error message formatting and classification
// =========================================================================

#[test]
fn all_error_variants_have_display() {
    let errors: Vec<IpcError> = vec![
        IpcError::TransportInit("init failed".to_string()),
        IpcError::ConnectionFailed("conn refused".to_string()),
        IpcError::EncodingFailed("encode err".to_string()),
        IpcError::DecodingFailed("decode err".to_string()),
        IpcError::VersionIncompatibility {
            client: "2.0.0".to_string(),
            server: "1.0.0".to_string(),
        },
        IpcError::FeatureNegotiation("feature xyz".to_string()),
        IpcError::ServerNotRunning,
        IpcError::ConnectionLimitExceeded { max: 100 },
        IpcError::Timeout { timeout_ms: 5000 },
        IpcError::Grpc("grpc error".to_string()),
        IpcError::InvalidConfig("bad config".to_string()),
        IpcError::PlatformNotSupported("platform".to_string()),
        IpcError::ShutdownRequested,
    ];

    for err in &errors {
        let msg = err.to_string();
        assert!(!msg.is_empty(), "error display should not be empty");
    }
}

#[test]
fn recoverable_errors_classified_correctly() {
    assert!(IpcError::ConnectionFailed("test".to_string()).is_recoverable());
    assert!(IpcError::Timeout { timeout_ms: 1000 }.is_recoverable());
    assert!(
        IpcError::VersionIncompatibility {
            client: "1.0.0".to_string(),
            server: "2.0.0".to_string()
        }
        .is_recoverable()
    );
    assert!(IpcError::FeatureNegotiation("test".to_string()).is_recoverable());

    // Non-recoverable
    assert!(!IpcError::TransportInit("test".to_string()).is_recoverable());
    assert!(!IpcError::EncodingFailed("test".to_string()).is_recoverable());
    assert!(!IpcError::ServerNotRunning.is_recoverable());
}

#[test]
fn fatal_errors_classified_correctly() {
    assert!(IpcError::TransportInit("test".to_string()).is_fatal());
    assert!(IpcError::ServerNotRunning.is_fatal());
    assert!(IpcError::ShutdownRequested.is_fatal());

    // Non-fatal
    assert!(!IpcError::ConnectionFailed("test".to_string()).is_fatal());
    assert!(!IpcError::Timeout { timeout_ms: 1000 }.is_fatal());
    assert!(!IpcError::EncodingFailed("test".to_string()).is_fatal());
}

#[test]
fn error_helpers_produce_correct_variants() {
    let timeout = IpcError::timeout(3000);
    assert!(matches!(timeout, IpcError::Timeout { timeout_ms: 3000 }));
    assert!(timeout.to_string().contains("3000"));

    let limit = IpcError::connection_limit(50);
    assert!(matches!(
        limit,
        IpcError::ConnectionLimitExceeded { max: 50 }
    ));
    assert!(limit.to_string().contains("50"));
}

#[test]
fn version_incompatibility_error_contains_versions() {
    let err = IpcError::VersionIncompatibility {
        client: "2.0.0".to_string(),
        server: "1.0.0".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("2.0.0"));
    assert!(msg.contains("1.0.0"));
}

#[test]
fn codec_encoding_error_contains_reason() {
    let codec = MessageCodec::with_max_size(1);
    let msg = prost_types::Duration {
        seconds: 999,
        nanos: 999,
    };
    let result = MessageEncoder::encode(&codec, &msg);
    assert!(result.is_err());
    let err_msg = format!(
        "{}",
        result.as_ref().err().unwrap_or(&IpcError::ServerNotRunning)
    );
    assert!(err_msg.contains("exceeds") || err_msg.contains("size"));
}

#[test]
fn decode_corrupted_protobuf_gives_error() {
    let codec = MessageCodec::new();
    let garbage = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x01, 0x02];
    let result: IpcResult<prost_types::Duration> = MessageDecoder::decode(&codec, &garbage);
    assert!(result.is_err());
}

// =========================================================================
// Section 6: Transport and configuration validation
// =========================================================================

#[test]
fn transport_type_tcp_defaults() {
    let tcp = TransportType::tcp();
    let desc = tcp.description();
    assert!(desc.contains("TCP"));
    assert!(desc.contains(&DEFAULT_TCP_PORT.to_string()));
}

#[test]
fn transport_type_tcp_custom_address() {
    let custom = TransportType::tcp_with_address("0.0.0.0", 8080);
    let desc = custom.description();
    assert!(desc.contains("0.0.0.0"));
    assert!(desc.contains("8080"));
}

#[test]
fn transport_config_defaults_are_reasonable() {
    let config = TransportConfig::default();
    assert_eq!(config.max_connections, 100);
    assert!(!config.enable_acl);
    assert!(config.recv_buffer_size > 0);
    assert!(config.send_buffer_size > 0);
}

#[test]
fn transport_builder_overrides_work() {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp())
        .max_connections(25)
        .connection_timeout(Duration::from_secs(60))
        .enable_acl(true)
        .build();

    assert_eq!(config.max_connections, 25);
    assert_eq!(config.connection_timeout, Duration::from_secs(60));
    assert!(config.enable_acl);
}

#[test]
fn ipc_config_default_values() {
    let config = IpcConfig::default();
    assert_eq!(config.server_name, "openracing-ipc");
    assert_eq!(config.health_buffer_size, 1000);
    assert!(config.enable_connection_logging);
}

#[test]
fn ipc_config_builder_methods() {
    let config = IpcConfig::with_transport(TransportType::tcp())
        .max_connections(10)
        .health_buffer_size(500);

    assert_eq!(config.transport.max_connections, 10);
    assert_eq!(config.health_buffer_size, 500);
}

// =========================================================================
// Section 7: Version compatibility
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
fn version_compat_different_major_rejected() {
    assert!(!is_version_compatible("2.0.0", "1.0.0"));
    assert!(!is_version_compatible("0.9.0", "1.0.0"));
}

#[test]
fn version_compat_lower_minor_rejected() {
    assert!(!is_version_compatible("1.0.0", "1.1.0"));
}

#[test]
fn version_compat_lower_patch_rejected() {
    assert!(!is_version_compatible("1.1.0", "1.1.1"));
}

#[test]
fn version_compat_malformed_rejected() {
    assert!(!is_version_compatible("abc", "1.0.0"));
    assert!(!is_version_compatible("1.0", "1.0.0"));
    assert!(!is_version_compatible("", "1.0.0"));
}

#[test]
fn current_protocol_compatible_with_min() {
    assert!(is_version_compatible(PROTOCOL_VERSION, MIN_CLIENT_VERSION));
}

// =========================================================================
// Section 8: Server lifecycle (async tests)
// =========================================================================

#[tokio::test]
async fn server_starts_and_stops_cleanly() -> IpcResult<()> {
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
async fn server_double_start_rejected() -> IpcResult<()> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.start().await;
    assert!(result.is_err(), "double start should fail");

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_stop_when_already_stopped_is_ok() -> IpcResult<()> {
    let server = IpcServer::new(IpcConfig::default());
    // Stop without start should be OK
    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    Ok(())
}

#[tokio::test]
async fn server_client_count_initially_zero() -> IpcResult<()> {
    let server = IpcServer::new(IpcConfig::default());
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

#[tokio::test]
async fn server_feature_negotiation_compatible_client() -> IpcResult<()> {
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
    assert_eq!(result.server_version, PROTOCOL_VERSION);
    assert!(
        result
            .enabled_features
            .contains(&"device_management".to_string())
    );
    assert!(
        result
            .enabled_features
            .contains(&"safety_control".to_string())
    );

    // Client should be registered
    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_feature_negotiation_incompatible_client() -> IpcResult<()> {
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

#[tokio::test]
async fn server_feature_negotiation_unsupported_feature_ignored() -> IpcResult<()> {
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
    assert!(
        result
            .enabled_features
            .contains(&"device_management".to_string())
    );
    assert!(
        !result
            .enabled_features
            .contains(&"nonexistent_feature".to_string())
    );

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_health_event_broadcast_and_receive() {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx = server.subscribe_health();

    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "device-1".to_string(),
        event_type: HealthEventType::Fault,
        message: "Overtemperature".to_string(),
        metadata: HashMap::new(),
    };

    server.broadcast_health_event(event);

    let received = rx.try_recv();
    assert!(received.is_ok());
    let evt = received.ok();
    assert!(evt.is_some());
}

#[tokio::test]
async fn server_register_and_unregister_client() -> IpcResult<()> {
    let server = IpcServer::new(IpcConfig::default());

    let client = ClientInfo {
        id: "test-client-1".to_string(),
        connected_at: std::time::Instant::now(),
        version: "1.0.0".to_string(),
        features: vec!["device_management".to_string()],
        peer_info: PeerInfo::default(),
    };

    server.register_client(client).await;
    assert_eq!(server.client_count().await, 1);

    server.unregister_client("test-client-1").await;
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

#[tokio::test]
async fn server_stop_clears_clients() -> IpcResult<()> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Negotiate adds a client
    server
        .negotiate_features("1.0.0", &["device_management".to_string()])
        .await?;
    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

// =========================================================================
// Section 9: Flag combination edge cases
// =========================================================================

#[test]
fn flag_response_plus_error_combination() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
    header.set_flag(message_flags::IS_RESPONSE);
    header.set_flag(message_flags::IS_ERROR);

    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;

    assert!(decoded.has_flag(message_flags::IS_RESPONSE));
    assert!(decoded.has_flag(message_flags::IS_ERROR));
    assert!(!decoded.has_flag(message_flags::COMPRESSED));
    Ok(())
}

#[test]
fn flag_streaming_compressed() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::TELEMETRY, 1024, 0);
    header.set_flag(message_flags::STREAMING);
    header.set_flag(message_flags::COMPRESSED);

    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;

    assert!(decoded.has_flag(message_flags::STREAMING));
    assert!(decoded.has_flag(message_flags::COMPRESSED));
    Ok(())
}

#[test]
fn setting_same_flag_twice_is_idempotent() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
    header.set_flag(message_flags::COMPRESSED);
    header.set_flag(message_flags::COMPRESSED);

    let bytes = header.encode();
    let decoded = MessageHeader::decode(&bytes)?;
    assert!(decoded.has_flag(message_flags::COMPRESSED));
    assert_eq!(decoded.flags, message_flags::COMPRESSED);
    Ok(())
}

// =========================================================================
// Section 10: Windows-specific transport
// =========================================================================

#[cfg(windows)]
#[test]
fn named_pipe_transport_description() {
    let pipe = TransportType::named_pipe(r"\\.\pipe\test-openracing");
    let desc = pipe.description();
    assert!(desc.contains("pipe"));
    assert!(desc.contains("test-openracing"));
}

#[cfg(windows)]
#[test]
fn platform_default_is_named_pipe() {
    let default = TransportType::platform_default();
    let desc = default.description();
    assert!(desc.contains("pipe"));
}

#[test]
fn transport_type_tcp_serde_roundtrip() -> Result<(), BoxErr> {
    let tcp = TransportType::tcp_with_address("10.0.0.1", 9999);
    let json = serde_json::to_string(&tcp)?;
    let restored: TransportType = serde_json::from_str(&json)?;
    let desc = restored.description();
    assert!(desc.contains("10.0.0.1"));
    assert!(desc.contains("9999"));
    Ok(())
}

#[test]
fn transport_config_serde_roundtrip() -> Result<(), BoxErr> {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp())
        .max_connections(42)
        .enable_acl(true)
        .build();

    let json = serde_json::to_string(&config)?;
    let restored: TransportConfig = serde_json::from_str(&json)?;
    assert_eq!(restored.max_connections, 42);
    assert!(restored.enable_acl);
    Ok(())
}
