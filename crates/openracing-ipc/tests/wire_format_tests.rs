//! Deep IPC wire format and message routing tests.
//!
//! Covers:
//! 1. All message types serialize to expected wire format
//! 2. Binary size stability (no unexpected growth)
//! 3. Message ID uniqueness across all types
//! 4. Request-response pairing correctness
//! 5. Error message encoding
//! 6. Streaming/chunked message handling
//! 7. Version negotiation messages
//! 8. Heartbeat/keepalive protocol
//! 9. Authentication/handshake sequence
//! 10. Message ordering guarantees
//! 11. Large payload segmentation
//! 12. Concurrent message interleaving

use std::collections::{HashMap, HashSet};

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::{IpcError, IpcResult};
use openracing_ipc::server::{
    ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, is_version_compatible,
};
use openracing_ipc::transport::TransportBuilder;
use openracing_ipc::{DEFAULT_TCP_PORT, MIN_CLIENT_VERSION, PROTOCOL_VERSION};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// All defined message type constants in a single place.
const ALL_MSG_TYPES: [u16; 8] = [
    message_types::DEVICE,
    message_types::PROFILE,
    message_types::SAFETY,
    message_types::HEALTH,
    message_types::FEATURE_NEGOTIATION,
    message_types::GAME,
    message_types::TELEMETRY,
    message_types::DIAGNOSTIC,
];

/// All defined flag constants.
const ALL_FLAGS: [u16; 5] = [
    message_flags::COMPRESSED,
    message_flags::REQUIRES_ACK,
    message_flags::IS_RESPONSE,
    message_flags::IS_ERROR,
    message_flags::STREAMING,
];

// =========================================================================
// 1. All message types serialize to expected wire format
// =========================================================================

#[test]
fn wire_format_header_is_little_endian() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0x0102, 0x03040506, 0x0708090A);
    let bytes = header.encode();

    // message_type: u16 LE → [0x02, 0x01]
    assert_eq!(bytes[0], 0x02);
    assert_eq!(bytes[1], 0x01);
    // payload_len: u32 LE → [0x06, 0x05, 0x04, 0x03]
    assert_eq!(bytes[2], 0x06);
    assert_eq!(bytes[3], 0x05);
    assert_eq!(bytes[4], 0x04);
    assert_eq!(bytes[5], 0x03);
    // sequence: u32 LE → [0x0A, 0x09, 0x08, 0x07]
    assert_eq!(bytes[6], 0x0A);
    assert_eq!(bytes[7], 0x09);
    assert_eq!(bytes[8], 0x08);
    assert_eq!(bytes[9], 0x07);
    Ok(())
}

#[test]
fn wire_format_each_message_type_encodes_at_correct_offset() -> Result<(), BoxErr> {
    for &msg_type in &ALL_MSG_TYPES {
        let header = MessageHeader::new(msg_type, 0, 0);
        let bytes = header.encode();
        let decoded_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(
            decoded_type, msg_type,
            "message type {msg_type:#06x} not at bytes [0..2]"
        );
    }
    Ok(())
}

#[test]
fn wire_format_flags_at_correct_byte_offset() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
    header.set_flag(message_flags::STREAMING);
    let bytes = header.encode();

    // flags occupy bytes [10..12]
    let wire_flags = u16::from_le_bytes([bytes[10], bytes[11]]);
    assert_eq!(wire_flags, message_flags::STREAMING);
    Ok(())
}

#[test]
fn wire_format_roundtrip_preserves_all_fields_for_every_type() -> Result<(), BoxErr> {
    for (i, &msg_type) in ALL_MSG_TYPES.iter().enumerate() {
        let payload_len = (i as u32 + 1) * 100;
        let seq = (i as u32 + 1) * 7;
        let mut header = MessageHeader::new(msg_type, payload_len, seq);
        // set two flags to confirm bitwise preservation
        header.set_flag(message_flags::COMPRESSED);
        header.set_flag(message_flags::REQUIRES_ACK);

        let decoded = MessageHeader::decode(&header.encode())?;
        assert_eq!(
            decoded.message_type, msg_type,
            "type mismatch for {msg_type:#06x}"
        );
        assert_eq!(decoded.payload_len, payload_len);
        assert_eq!(decoded.sequence, seq);
        assert!(decoded.has_flag(message_flags::COMPRESSED));
        assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
        assert!(!decoded.has_flag(message_flags::IS_ERROR));
    }
    Ok(())
}

#[test]
fn wire_format_zero_header_is_all_zeroes() -> Result<(), BoxErr> {
    let header = MessageHeader::new(0, 0, 0);
    let bytes = header.encode();
    assert!(
        bytes.iter().all(|&b| b == 0),
        "zero header should be all zero bytes"
    );
    Ok(())
}

// =========================================================================
// 2. Binary size stability (no unexpected growth)
// =========================================================================

#[test]
fn binary_size_header_is_exactly_12_bytes() -> Result<(), BoxErr> {
    assert_eq!(MessageHeader::SIZE, 12, "header size must remain 12 bytes");
    let header = MessageHeader::new(message_types::DEVICE, u32::MAX, u32::MAX);
    let bytes = header.encode();
    assert_eq!(bytes.len(), 12);
    Ok(())
}

#[test]
fn binary_size_header_independent_of_content() -> Result<(), BoxErr> {
    // Headers with different payloads all encode to the same size
    let sizes: Vec<usize> = ALL_MSG_TYPES
        .iter()
        .map(|&t| MessageHeader::new(t, 999_999, 42).encode().len())
        .collect();
    assert!(
        sizes.iter().all(|&s| s == MessageHeader::SIZE),
        "all headers must be {}: got {sizes:?}",
        MessageHeader::SIZE
    );
    Ok(())
}

#[test]
fn binary_size_codec_default_limit_is_16mb() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    assert_eq!(codec.max_message_size(), 16 * 1024 * 1024);
    Ok(())
}

#[test]
fn binary_size_prost_timestamp_encoded_len_is_bounded() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let ts = prost_types::Timestamp {
        seconds: i64::MAX,
        nanos: 999_999_999,
    };
    let len = codec.encoded_len(&ts);
    // A prost Timestamp with max values should be well under 30 bytes
    assert!(
        len <= 30,
        "Timestamp encoded len {len} exceeds expected bound"
    );
    Ok(())
}

#[test]
fn binary_size_codec_rejects_above_custom_limit() -> Result<(), BoxErr> {
    let codec = MessageCodec::with_max_size(64);
    // A tiny message should pass
    let small = prost_types::Timestamp {
        seconds: 1,
        nanos: 0,
    };
    let result = codec.encode(&small);
    assert!(result.is_ok(), "small message should encode");

    // Decoding oversized buffer should fail
    let big_buf = vec![0u8; 128];
    let decode_result: IpcResult<prost_types::Timestamp> = codec.decode(&big_buf);
    assert!(
        decode_result.is_err(),
        "oversized buffer should be rejected"
    );
    Ok(())
}

// =========================================================================
// 3. Message ID uniqueness across all types
// =========================================================================

#[test]
fn message_type_ids_are_all_unique() -> Result<(), BoxErr> {
    let mut set = HashSet::new();
    for &t in &ALL_MSG_TYPES {
        assert!(set.insert(t), "duplicate message type ID: {t:#06x}");
    }
    assert_eq!(set.len(), ALL_MSG_TYPES.len());
    Ok(())
}

#[test]
fn message_type_ids_are_nonzero() -> Result<(), BoxErr> {
    for &t in &ALL_MSG_TYPES {
        assert_ne!(t, 0, "message type 0x0000 is reserved / invalid");
    }
    Ok(())
}

#[test]
fn message_type_ids_fit_in_u16_range() -> Result<(), BoxErr> {
    // Confirm no truncation by checking encode/decode round-trip equality
    for &t in &ALL_MSG_TYPES {
        let header = MessageHeader::new(t, 0, 0);
        let decoded = MessageHeader::decode(&header.encode())?;
        assert_eq!(
            decoded.message_type, t,
            "type {t:#06x} truncated in wire format"
        );
    }
    Ok(())
}

#[test]
fn flag_ids_are_all_unique_powers_of_two() -> Result<(), BoxErr> {
    let mut set = HashSet::new();
    for &f in &ALL_FLAGS {
        assert!(f.is_power_of_two(), "flag {f:#06x} is not a power of two");
        assert!(set.insert(f), "duplicate flag ID: {f:#06x}");
    }
    Ok(())
}

#[test]
fn flag_ids_are_nonzero() -> Result<(), BoxErr> {
    for &f in &ALL_FLAGS {
        assert_ne!(f, 0, "flag 0x0000 would be a no-op");
    }
    Ok(())
}

#[test]
fn all_flags_combined_do_not_overflow_u16() -> Result<(), BoxErr> {
    let combined: u32 = ALL_FLAGS.iter().map(|&f| u32::from(f)).sum();
    assert!(
        combined <= u32::from(u16::MAX),
        "combined flags overflow u16"
    );
    // Also confirm OR-combination
    let ored: u16 = ALL_FLAGS.iter().fold(0u16, |acc, &f| acc | f);
    let decoded_header = {
        let mut h = MessageHeader::new(message_types::DEVICE, 0, 0);
        h.flags = ored;
        MessageHeader::decode(&h.encode())?
    };
    for &f in &ALL_FLAGS {
        assert!(
            decoded_header.has_flag(f),
            "flag {f:#06x} missing after OR-combine"
        );
    }
    Ok(())
}

// =========================================================================
// 4. Request-response pairing correctness
// =========================================================================

#[test]
fn request_response_pair_shares_sequence_number() -> Result<(), BoxErr> {
    let seq = 42u32;
    let request = MessageHeader::new(message_types::DEVICE, 128, seq);
    let mut response = MessageHeader::new(message_types::DEVICE, 64, seq);
    response.set_flag(message_flags::IS_RESPONSE);

    let req_decoded = MessageHeader::decode(&request.encode())?;
    let resp_decoded = MessageHeader::decode(&response.encode())?;

    assert_eq!(
        req_decoded.sequence, resp_decoded.sequence,
        "request/response sequence mismatch"
    );
    assert!(!req_decoded.has_flag(message_flags::IS_RESPONSE));
    assert!(resp_decoded.has_flag(message_flags::IS_RESPONSE));
    Ok(())
}

#[test]
fn request_response_pair_preserves_message_type() -> Result<(), BoxErr> {
    for &msg_type in &ALL_MSG_TYPES {
        let mut resp = MessageHeader::new(msg_type, 0, 1);
        resp.set_flag(message_flags::IS_RESPONSE);

        let decoded = MessageHeader::decode(&resp.encode())?;
        assert_eq!(
            decoded.message_type, msg_type,
            "response message type changed for {msg_type:#06x}"
        );
    }
    Ok(())
}

#[test]
fn error_response_has_both_response_and_error_flags() -> Result<(), BoxErr> {
    let seq = 99u32;
    let mut err_resp = MessageHeader::new(message_types::SAFETY, 32, seq);
    err_resp.set_flag(message_flags::IS_RESPONSE);
    err_resp.set_flag(message_flags::IS_ERROR);

    let decoded = MessageHeader::decode(&err_resp.encode())?;
    assert!(decoded.has_flag(message_flags::IS_RESPONSE));
    assert!(decoded.has_flag(message_flags::IS_ERROR));
    assert_eq!(decoded.sequence, seq);
    Ok(())
}

#[test]
fn multiple_request_response_pairs_maintain_sequence_isolation() -> Result<(), BoxErr> {
    let mut pairs = Vec::new();
    for seq in 0..100u32 {
        let req = MessageHeader::new(message_types::DEVICE, 64, seq);
        let mut resp = MessageHeader::new(message_types::DEVICE, 32, seq);
        resp.set_flag(message_flags::IS_RESPONSE);
        pairs.push((req, resp));
    }

    for (req, resp) in &pairs {
        let req_d = MessageHeader::decode(&req.encode())?;
        let resp_d = MessageHeader::decode(&resp.encode())?;
        assert_eq!(req_d.sequence, resp_d.sequence);
        assert!(!req_d.has_flag(message_flags::IS_RESPONSE));
        assert!(resp_d.has_flag(message_flags::IS_RESPONSE));
    }
    Ok(())
}

#[test]
fn requires_ack_flag_set_on_request_cleared_on_response() -> Result<(), BoxErr> {
    let mut request = MessageHeader::new(message_types::PROFILE, 256, 10);
    request.set_flag(message_flags::REQUIRES_ACK);

    let mut response = MessageHeader::new(message_types::PROFILE, 0, 10);
    response.set_flag(message_flags::IS_RESPONSE);
    // Response does not set REQUIRES_ACK

    let req_d = MessageHeader::decode(&request.encode())?;
    let resp_d = MessageHeader::decode(&response.encode())?;

    assert!(req_d.has_flag(message_flags::REQUIRES_ACK));
    assert!(!resp_d.has_flag(message_flags::REQUIRES_ACK));
    Ok(())
}

// =========================================================================
// 5. Error message encoding
// =========================================================================

#[test]
fn error_header_flag_encodes_correctly() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::DIAGNOSTIC, 64, 5);
    header.set_flag(message_flags::IS_ERROR);

    let bytes = header.encode();
    let flags_wire = u16::from_le_bytes([bytes[10], bytes[11]]);
    assert_ne!(flags_wire & message_flags::IS_ERROR, 0);

    let decoded = MessageHeader::decode(&bytes)?;
    assert!(decoded.has_flag(message_flags::IS_ERROR));
    Ok(())
}

#[test]
fn error_encoding_preserves_payload_length() -> Result<(), BoxErr> {
    let error_payload_len = 512u32;
    let mut header = MessageHeader::new(message_types::HEALTH, error_payload_len, 77);
    header.set_flag(message_flags::IS_ERROR);
    header.set_flag(message_flags::IS_RESPONSE);

    let decoded = MessageHeader::decode(&header.encode())?;
    assert_eq!(decoded.payload_len, error_payload_len);
    Ok(())
}

#[test]
fn error_variants_all_have_display_messages() -> Result<(), BoxErr> {
    let errors: Vec<IpcError> = vec![
        IpcError::TransportInit("test init".into()),
        IpcError::ConnectionFailed("test conn".into()),
        IpcError::EncodingFailed("test enc".into()),
        IpcError::DecodingFailed("test dec".into()),
        IpcError::VersionIncompatibility {
            client: "0.1.0".into(),
            server: "1.0.0".into(),
        },
        IpcError::FeatureNegotiation("test feat".into()),
        IpcError::ServerNotRunning,
        IpcError::ConnectionLimitExceeded { max: 10 },
        IpcError::Timeout { timeout_ms: 500 },
        IpcError::Grpc("test grpc".into()),
        IpcError::InvalidConfig("test cfg".into()),
        IpcError::PlatformNotSupported("test plat".into()),
        IpcError::ShutdownRequested,
    ];
    for err in &errors {
        let msg = format!("{err}");
        assert!(
            !msg.is_empty(),
            "error display should not be empty: {err:?}"
        );
    }
    Ok(())
}

#[test]
fn error_codec_encoding_failure_contains_size_info() -> Result<(), BoxErr> {
    let codec = MessageCodec::with_max_size(4);
    // prost_types::Timestamp with nonzero fields encodes to more than 4 bytes
    let ts = prost_types::Timestamp {
        seconds: 1_000_000,
        nanos: 500,
    };
    let result = codec.encode(&ts);
    assert!(result.is_err());
    if let Err(IpcError::EncodingFailed(msg)) = result {
        assert!(
            msg.contains("exceeds"),
            "error should mention exceeds: {msg}"
        );
    }
    Ok(())
}

#[test]
fn error_codec_decoding_failure_on_oversized_input() -> Result<(), BoxErr> {
    let codec = MessageCodec::with_max_size(8);
    let too_big = vec![0u8; 16];
    let result: IpcResult<prost_types::Timestamp> = codec.decode(&too_big);
    assert!(result.is_err());
    if let Err(IpcError::DecodingFailed(msg)) = result {
        assert!(
            msg.contains("exceeds"),
            "error should mention exceeds: {msg}"
        );
    }
    Ok(())
}

// =========================================================================
// 6. Streaming/chunked message handling
// =========================================================================

#[test]
fn streaming_flag_encodes_in_wire_format() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::TELEMETRY, 1024, 1);
    header.set_flag(message_flags::STREAMING);

    let decoded = MessageHeader::decode(&header.encode())?;
    assert!(decoded.has_flag(message_flags::STREAMING));
    assert!(!decoded.has_flag(message_flags::IS_RESPONSE));
    Ok(())
}

#[test]
fn streaming_flag_coexists_with_compressed_flag() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::HEALTH, 2048, 5);
    header.set_flag(message_flags::STREAMING);
    header.set_flag(message_flags::COMPRESSED);

    let decoded = MessageHeader::decode(&header.encode())?;
    assert!(decoded.has_flag(message_flags::STREAMING));
    assert!(decoded.has_flag(message_flags::COMPRESSED));
    Ok(())
}

#[test]
fn chunked_message_sequence_numbers_increment() -> Result<(), BoxErr> {
    // Simulate a chunked stream: multiple headers with incrementing sequence
    let chunk_count = 50u32;
    let mut decoded_seqs = Vec::with_capacity(chunk_count as usize);

    for seq in 0..chunk_count {
        let mut header = MessageHeader::new(message_types::TELEMETRY, 512, seq);
        header.set_flag(message_flags::STREAMING);
        let decoded = MessageHeader::decode(&header.encode())?;
        decoded_seqs.push(decoded.sequence);
    }

    // Verify monotonic sequence
    for window in decoded_seqs.windows(2) {
        assert_eq!(
            window[1],
            window[0] + 1,
            "chunk sequences must be monotonically increasing"
        );
    }
    Ok(())
}

#[test]
fn streaming_chunks_preserve_message_type_across_sequence() -> Result<(), BoxErr> {
    for seq in 0..20u32 {
        let mut header = MessageHeader::new(message_types::GAME, 256, seq);
        header.set_flag(message_flags::STREAMING);
        let decoded = MessageHeader::decode(&header.encode())?;
        assert_eq!(
            decoded.message_type,
            message_types::GAME,
            "chunk {seq} changed message type"
        );
    }
    Ok(())
}

#[tokio::test]
async fn streaming_health_events_arrive_in_order() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx = server.subscribe_health();

    let event_count = 20;
    for i in 0..event_count {
        server.broadcast_health_event(HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: format!("stream-dev-{i}"),
            event_type: HealthEventType::Connected,
            message: format!("event-{i}"),
            metadata: HashMap::new(),
        });
    }

    for i in 0..event_count {
        let evt = rx.try_recv();
        assert!(evt.is_ok(), "missing event {i}");
        let evt = evt?;
        assert_eq!(evt.device_id, format!("stream-dev-{i}"));
    }
    Ok(())
}

// =========================================================================
// 7. Version negotiation messages
// =========================================================================

#[test]
fn version_negotiation_type_has_dedicated_id() -> Result<(), BoxErr> {
    assert_eq!(message_types::FEATURE_NEGOTIATION, 0x0005);
    // Confirm it's distinct from all others
    for &t in &ALL_MSG_TYPES {
        if t == message_types::FEATURE_NEGOTIATION {
            continue;
        }
        assert_ne!(t, message_types::FEATURE_NEGOTIATION);
    }
    Ok(())
}

#[test]
fn protocol_version_is_semver() -> Result<(), BoxErr> {
    let parts: Vec<&str> = PROTOCOL_VERSION.split('.').collect();
    assert_eq!(parts.len(), 3, "PROTOCOL_VERSION must be semver");
    for part in &parts {
        let _: u32 = part.parse().map_err(|_| "non-numeric semver component")?;
    }
    Ok(())
}

#[test]
fn min_client_version_is_semver() -> Result<(), BoxErr> {
    let parts: Vec<&str> = MIN_CLIENT_VERSION.split('.').collect();
    assert_eq!(parts.len(), 3, "MIN_CLIENT_VERSION must be semver");
    for part in &parts {
        let _: u32 = part.parse().map_err(|_| "non-numeric semver component")?;
    }
    Ok(())
}

#[test]
fn protocol_version_is_compatible_with_min_client() -> Result<(), BoxErr> {
    assert!(
        is_version_compatible(PROTOCOL_VERSION, MIN_CLIENT_VERSION),
        "PROTOCOL_VERSION must be compatible with MIN_CLIENT_VERSION"
    );
    Ok(())
}

#[tokio::test]
async fn version_negotiation_compatible_client_gets_features() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features(
            PROTOCOL_VERSION,
            &[
                "device_management".into(),
                "safety_control".into(),
                "streaming_health".into(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert_eq!(result.server_version, PROTOCOL_VERSION);
    assert!(!result.supported_features.is_empty());
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
    assert!(
        result
            .enabled_features
            .contains(&"streaming_health".to_string())
    );

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn version_negotiation_incompatible_major_version() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("2.0.0", &["device_management".into()])
        .await?;

    assert!(!result.compatible);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn version_negotiation_returns_all_server_features() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.negotiate_features(PROTOCOL_VERSION, &[]).await?;

    // Server should advertise its full feature set regardless of client request
    assert!(
        result.supported_features.len() >= 5,
        "server should have at least 5 features"
    );
    assert!(
        result.enabled_features.is_empty(),
        "no features requested, none enabled"
    );

    server.stop().await?;
    Ok(())
}

#[test]
fn version_negotiation_header_encodes_correctly() -> Result<(), BoxErr> {
    let header = MessageHeader::new(message_types::FEATURE_NEGOTIATION, 128, 0);
    let decoded = MessageHeader::decode(&header.encode())?;
    assert_eq!(decoded.message_type, message_types::FEATURE_NEGOTIATION);
    assert_eq!(decoded.payload_len, 128);
    Ok(())
}

// =========================================================================
// 8. Heartbeat/keepalive protocol
// =========================================================================

#[test]
fn heartbeat_header_uses_health_type_with_zero_payload() -> Result<(), BoxErr> {
    // A keepalive can be modeled as a HEALTH message with zero payload
    let heartbeat = MessageHeader::new(message_types::HEALTH, 0, 0);
    let decoded = MessageHeader::decode(&heartbeat.encode())?;
    assert_eq!(decoded.message_type, message_types::HEALTH);
    assert_eq!(decoded.payload_len, 0);
    Ok(())
}

#[test]
fn heartbeat_ack_uses_response_flag() -> Result<(), BoxErr> {
    let mut ack = MessageHeader::new(message_types::HEALTH, 0, 0);
    ack.set_flag(message_flags::IS_RESPONSE);

    let decoded = MessageHeader::decode(&ack.encode())?;
    assert!(decoded.has_flag(message_flags::IS_RESPONSE));
    assert_eq!(decoded.payload_len, 0);
    Ok(())
}

#[test]
fn heartbeat_sequence_numbers_enable_round_trip_timing() -> Result<(), BoxErr> {
    // Sequential heartbeats with increasing sequence enable RTT measurement
    for seq in 0..100u32 {
        let hb = MessageHeader::new(message_types::HEALTH, 0, seq);
        let decoded = MessageHeader::decode(&hb.encode())?;
        assert_eq!(decoded.sequence, seq, "heartbeat seq mismatch at {seq}");
    }
    Ok(())
}

#[tokio::test]
async fn heartbeat_events_broadcast_to_subscribers() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx = server.subscribe_health();

    // Simulate heartbeat via health event
    for i in 0..5 {
        server.broadcast_health_event(HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "heartbeat".to_string(),
            event_type: HealthEventType::Connected,
            message: format!("keepalive-{i}"),
            metadata: HashMap::new(),
        });
    }

    for i in 0..5 {
        let evt = rx.try_recv();
        assert!(evt.is_ok(), "missed heartbeat event {i}");
    }
    Ok(())
}

// =========================================================================
// 9. Authentication/handshake sequence
// =========================================================================

#[tokio::test]
async fn handshake_registers_client_on_compatible_version() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    assert_eq!(server.client_count().await, 0);

    let result = server
        .negotiate_features(PROTOCOL_VERSION, &["device_management".into()])
        .await?;

    assert!(result.compatible);
    assert_eq!(server.client_count().await, 1);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn handshake_rejects_incompatible_client() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("0.5.0", &["device_management".into()])
        .await?;

    assert!(!result.compatible);
    assert_eq!(
        server.client_count().await,
        0,
        "rejected client must not be registered"
    );

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn handshake_client_gets_unique_id() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Connect multiple clients
    for _ in 0..5 {
        let _result = server
            .negotiate_features(PROTOCOL_VERSION, &["device_management".into()])
            .await?;
    }

    let clients = server.connected_clients().await;
    assert_eq!(clients.len(), 5);

    let ids: HashSet<&str> = clients.iter().map(|c| &*c.id).collect();
    assert_eq!(ids.len(), 5, "all client IDs must be unique");

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn handshake_manual_register_and_unregister() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let client = ClientInfo {
        id: "manual-client-1".into(),
        connected_at: std::time::Instant::now(),
        version: PROTOCOL_VERSION.into(),
        features: vec!["device_management".into()],
        peer_info: PeerInfo::default(),
    };

    server.register_client(client).await;
    assert_eq!(server.client_count().await, 1);

    server.unregister_client("manual-client-1").await;
    assert_eq!(server.client_count().await, 0);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn handshake_stop_clears_all_clients() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    for i in 0..3 {
        server
            .register_client(ClientInfo {
                id: format!("client-{i}"),
                connected_at: std::time::Instant::now(),
                version: PROTOCOL_VERSION.into(),
                features: vec![],
                peer_info: PeerInfo::default(),
            })
            .await;
    }
    assert_eq!(server.client_count().await, 3);

    server.stop().await?;
    assert_eq!(
        server.client_count().await,
        0,
        "stop must clear all clients"
    );
    Ok(())
}

// =========================================================================
// 10. Message ordering guarantees
// =========================================================================

#[test]
fn message_ordering_sequence_numbers_encode_in_order() -> Result<(), BoxErr> {
    let mut encoded_pairs = Vec::new();
    for seq in 0..200u32 {
        let header = MessageHeader::new(message_types::TELEMETRY, 64, seq);
        encoded_pairs.push((seq, header.encode()));
    }

    for (expected_seq, bytes) in &encoded_pairs {
        let decoded = MessageHeader::decode(bytes)?;
        assert_eq!(
            decoded.sequence, *expected_seq,
            "ordering: expected seq {expected_seq}, got {}",
            decoded.sequence
        );
    }
    Ok(())
}

#[test]
fn message_ordering_sequence_wraps_at_u32_max() -> Result<(), BoxErr> {
    let header_max = MessageHeader::new(message_types::DEVICE, 0, u32::MAX);
    let header_zero = MessageHeader::new(message_types::DEVICE, 0, 0);

    let dec_max = MessageHeader::decode(&header_max.encode())?;
    let dec_zero = MessageHeader::decode(&header_zero.encode())?;

    assert_eq!(dec_max.sequence, u32::MAX);
    assert_eq!(dec_zero.sequence, 0);
    Ok(())
}

#[tokio::test]
async fn message_ordering_health_events_fifo() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    let mut rx = server.subscribe_health();

    let count = 100;
    for i in 0..count {
        server.broadcast_health_event(HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: format!("order-{i}"),
            event_type: HealthEventType::Connected,
            message: format!("msg-{i}"),
            metadata: HashMap::new(),
        });
    }

    for i in 0..count {
        let evt = rx.try_recv()?;
        assert_eq!(
            evt.device_id,
            format!("order-{i}"),
            "FIFO order violated at {i}"
        );
    }
    Ok(())
}

#[test]
fn message_ordering_codec_encode_decode_is_stable() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let ts = prost_types::Timestamp {
        seconds: 1_718_000_000,
        nanos: 123_456_789,
    };

    // Encode multiple times and verify deterministic output
    let enc1 = codec.encode(&ts)?;
    let enc2 = codec.encode(&ts)?;
    assert_eq!(enc1, enc2, "codec encoding must be deterministic");

    let decoded: prost_types::Timestamp = codec.decode(&enc1)?;
    assert_eq!(decoded.seconds, ts.seconds);
    assert_eq!(decoded.nanos, ts.nanos);
    Ok(())
}

// =========================================================================
// 11. Large payload segmentation
// =========================================================================

#[test]
fn large_payload_header_supports_up_to_u32_max_length() -> Result<(), BoxErr> {
    let header = MessageHeader::new(message_types::TELEMETRY, u32::MAX, 1);
    let decoded = MessageHeader::decode(&header.encode())?;
    assert_eq!(decoded.payload_len, u32::MAX);
    Ok(())
}

#[test]
fn large_payload_codec_rejects_above_limit() -> Result<(), BoxErr> {
    let codec = MessageCodec::with_max_size(256);
    assert!(!codec.is_valid_size(257));
    assert!(codec.is_valid_size(256));
    assert!(codec.is_valid_size(1));
    Ok(())
}

#[test]
fn large_payload_segmentation_via_sequence_numbers() -> Result<(), BoxErr> {
    // Simulate segmenting a 1 MB payload into 1024-byte chunks
    let total_size: u32 = 1_048_576;
    let chunk_size: u32 = 1024;
    let chunk_count = total_size / chunk_size;

    let mut headers = Vec::with_capacity(chunk_count as usize);
    for seq in 0..chunk_count {
        let remaining = total_size - (seq * chunk_size);
        let this_chunk = remaining.min(chunk_size);
        let mut header = MessageHeader::new(message_types::TELEMETRY, this_chunk, seq);
        header.set_flag(message_flags::STREAMING);
        if seq < chunk_count - 1 {
            header.set_flag(message_flags::REQUIRES_ACK);
        }
        headers.push(header);
    }

    // Verify all encode/decode correctly
    let mut total_payload = 0u64;
    for (i, h) in headers.iter().enumerate() {
        let decoded = MessageHeader::decode(&h.encode())?;
        assert_eq!(decoded.sequence, i as u32);
        assert!(decoded.has_flag(message_flags::STREAMING));
        total_payload += u64::from(decoded.payload_len);
    }
    assert_eq!(
        total_payload,
        u64::from(total_size),
        "segmented payload sum must equal total"
    );
    Ok(())
}

#[test]
fn large_payload_encode_to_buffer_reuses_allocation() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    let mut buffer = Vec::with_capacity(256);

    for i in 1..11 {
        let ts = prost_types::Timestamp {
            seconds: i * 1000,
            nanos: 1,
        };
        codec.encode_to_buffer(&ts, &mut buffer)?;
        assert!(
            !buffer.is_empty(),
            "buffer should have content after encode {i}"
        );

        let decoded: prost_types::Timestamp = codec.decode(&buffer)?;
        assert_eq!(decoded.seconds, i * 1000);
    }
    Ok(())
}

#[test]
fn large_payload_codec_boundary_at_exact_limit() -> Result<(), BoxErr> {
    // Create a codec with a limit that exactly matches a message size
    let ts = prost_types::Timestamp {
        seconds: 100,
        nanos: 200,
    };
    let codec_measure = MessageCodec::new();
    let exact_len = codec_measure.encoded_len(&ts);

    // Codec with exact limit should accept
    let codec_exact = MessageCodec::with_max_size(exact_len);
    let result = codec_exact.encode(&ts);
    assert!(result.is_ok(), "exact-size message should encode");

    // Codec with limit - 1 should reject
    if exact_len > 1 {
        let codec_tight = MessageCodec::with_max_size(exact_len - 1);
        let result = codec_tight.encode(&ts);
        assert!(result.is_err(), "over-limit message should be rejected");
    }
    Ok(())
}

// =========================================================================
// 12. Concurrent message interleaving
// =========================================================================

#[tokio::test]
async fn concurrent_feature_negotiations_all_succeed() -> Result<(), BoxErr> {
    let server = std::sync::Arc::new(IpcServer::new(IpcConfig::default()));
    server.start().await?;

    let mut handles = Vec::new();
    for i in 0..10 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            srv.negotiate_features(PROTOCOL_VERSION, &[format!("feature_{i}")])
                .await
        }));
    }

    for handle in handles {
        let result = handle.await?;
        // All negotiations return Ok but features may not match server set
        assert!(result.is_ok());
    }

    // All 10 clients should be registered (all compatible)
    assert_eq!(server.client_count().await, 10);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_health_broadcasts_no_lost_events() -> Result<(), BoxErr> {
    let server = std::sync::Arc::new(IpcServer::new(IpcConfig::default()));
    let mut rx = server.subscribe_health();

    let event_count = 50;
    let mut handles = Vec::new();
    for i in 0..event_count {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            srv.broadcast_health_event(HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("concurrent-{i}"),
                event_type: HealthEventType::Connected,
                message: format!("evt-{i}"),
                metadata: HashMap::new(),
            });
        }));
    }

    for handle in handles {
        handle.await?;
    }

    // Collect all received events
    let mut received = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        received.push(evt.device_id);
    }
    assert_eq!(
        received.len(),
        event_count,
        "expected {event_count} events, got {}",
        received.len()
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_register_unregister_clients() -> Result<(), BoxErr> {
    let server = std::sync::Arc::new(IpcServer::new(IpcConfig::default()));
    server.start().await?;

    // Register 10 clients concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            srv.register_client(ClientInfo {
                id: format!("conc-client-{i}"),
                connected_at: std::time::Instant::now(),
                version: PROTOCOL_VERSION.into(),
                features: vec![],
                peer_info: PeerInfo::default(),
            })
            .await;
        }));
    }
    for handle in handles {
        handle.await?;
    }
    assert_eq!(server.client_count().await, 10);

    // Unregister 5 concurrently
    let mut handles = Vec::new();
    for i in 0..5 {
        let srv = server.clone();
        handles.push(tokio::spawn(async move {
            srv.unregister_client(&format!("conc-client-{i}")).await;
        }));
    }
    for handle in handles {
        handle.await?;
    }
    assert_eq!(server.client_count().await, 5);

    server.stop().await?;
    Ok(())
}

#[test]
fn concurrent_header_encoding_is_thread_safe() -> Result<(), BoxErr> {
    // Encode/decode from multiple threads to verify no data races
    let handles: Vec<_> = (0..8)
        .map(|i| {
            std::thread::spawn(move || -> Result<(), BoxErr> {
                for seq in 0..100u32 {
                    let msg_type = ALL_MSG_TYPES[i % ALL_MSG_TYPES.len()];
                    let header = MessageHeader::new(msg_type, seq * 10, seq);
                    let decoded = MessageHeader::decode(&header.encode())?;
                    assert_eq!(decoded.message_type, msg_type);
                    assert_eq!(decoded.payload_len, seq * 10);
                    assert_eq!(decoded.sequence, seq);
                }
                Ok(())
            })
        })
        .collect();

    for handle in handles {
        handle.join().map_err(|_| "thread panicked")??;
    }
    Ok(())
}

#[tokio::test]
async fn interleaved_message_types_decode_independently() -> Result<(), BoxErr> {
    // Simulate interleaved messages from different "streams"
    let mut messages = Vec::new();
    for seq in 0..50u32 {
        for &msg_type in &ALL_MSG_TYPES {
            let header = MessageHeader::new(msg_type, seq, seq);
            messages.push((msg_type, seq, header.encode()));
        }
    }

    // Decode in interleaved order and verify each is independent
    for (expected_type, expected_seq, bytes) in &messages {
        let decoded = MessageHeader::decode(bytes)?;
        assert_eq!(decoded.message_type, *expected_type);
        assert_eq!(decoded.sequence, *expected_seq);
    }
    Ok(())
}

// =========================================================================
// Additional wire protocol invariants
// =========================================================================

#[test]
fn default_tcp_port_is_documented_value() -> Result<(), BoxErr> {
    assert_eq!(DEFAULT_TCP_PORT, 50051);
    Ok(())
}

#[test]
fn transport_builder_defaults_match_transport_config_defaults() -> Result<(), BoxErr> {
    let builder_config = TransportBuilder::new().build();
    let direct_config = openracing_ipc::transport::TransportConfig::default();

    assert_eq!(
        builder_config.max_connections,
        direct_config.max_connections
    );
    assert_eq!(
        builder_config.connection_timeout,
        direct_config.connection_timeout
    );
    assert_eq!(builder_config.enable_acl, direct_config.enable_acl);
    assert_eq!(
        builder_config.recv_buffer_size,
        direct_config.recv_buffer_size
    );
    assert_eq!(
        builder_config.send_buffer_size,
        direct_config.send_buffer_size
    );
    Ok(())
}

#[test]
fn ipc_config_default_values_are_stable() -> Result<(), BoxErr> {
    let config = IpcConfig::default();
    assert_eq!(config.server_name, "openracing-ipc");
    assert_eq!(config.health_buffer_size, 1000);
    assert!(config.enable_connection_logging);
    assert_eq!(
        config.negotiation_timeout,
        std::time::Duration::from_secs(5)
    );
    Ok(())
}
