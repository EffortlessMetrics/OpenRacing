//! Deep IPC protocol tests for openracing-ipc.
//!
//! Covers:
//! 1. All IPC message types
//! 2. Request/response matching
//! 3. Streaming protocol
//! 4. Error responses
//! 5. Version negotiation
//! 6. Keepalive/heartbeat
//! 7. Message framing

#![deny(clippy::unwrap_used)]

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

// ═══════════════════════════════════════════════════════════
// 1. All IPC message types
// ═══════════════════════════════════════════════════════════

mod all_message_types {
    use super::*;

    const ALL_MSG_TYPES: [(u16, &str); 8] = [
        (message_types::DEVICE, "DEVICE"),
        (message_types::PROFILE, "PROFILE"),
        (message_types::SAFETY, "SAFETY"),
        (message_types::HEALTH, "HEALTH"),
        (message_types::FEATURE_NEGOTIATION, "FEATURE_NEGOTIATION"),
        (message_types::GAME, "GAME"),
        (message_types::TELEMETRY, "TELEMETRY"),
        (message_types::DIAGNOSTIC, "DIAGNOSTIC"),
    ];

    #[test]
    fn all_message_types_have_unique_values() {
        let mut seen = std::collections::HashSet::new();
        for (val, name) in &ALL_MSG_TYPES {
            assert!(seen.insert(val), "Duplicate message type value for {name}");
        }
    }

    #[test]
    fn all_message_types_are_nonzero() {
        for (val, name) in &ALL_MSG_TYPES {
            assert!(*val != 0, "Message type {name} should be nonzero");
        }
    }

    #[test]
    fn each_message_type_encodes_to_header() -> Result<(), BoxErr> {
        for (msg_type, _name) in &ALL_MSG_TYPES {
            let header = MessageHeader::new(*msg_type, 100, 1);
            let bytes = header.encode();
            let decoded = MessageHeader::decode(&bytes)?;
            assert_eq!(decoded.message_type, *msg_type);
        }
        Ok(())
    }

    #[test]
    fn message_type_values_are_sequential() {
        assert_eq!(message_types::DEVICE, 0x0001);
        assert_eq!(message_types::PROFILE, 0x0002);
        assert_eq!(message_types::SAFETY, 0x0003);
        assert_eq!(message_types::HEALTH, 0x0004);
        assert_eq!(message_types::FEATURE_NEGOTIATION, 0x0005);
        assert_eq!(message_types::GAME, 0x0006);
        assert_eq!(message_types::TELEMETRY, 0x0007);
        assert_eq!(message_types::DIAGNOSTIC, 0x0008);
    }

    #[test]
    fn header_with_max_payload_len() -> Result<(), BoxErr> {
        let header = MessageHeader::new(message_types::TELEMETRY, u32::MAX, 0);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.payload_len, u32::MAX);
        Ok(())
    }

    #[test]
    fn header_with_max_sequence() -> Result<(), BoxErr> {
        let header = MessageHeader::new(message_types::DEVICE, 0, u32::MAX);
        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.sequence, u32::MAX);
        Ok(())
    }

    #[test]
    fn header_size_is_12_bytes() {
        assert_eq!(MessageHeader::SIZE, 12);
        let header = MessageHeader::new(message_types::DEVICE, 0, 0);
        let bytes = header.encode();
        assert_eq!(bytes.len(), 12);
    }
}

// ═══════════════════════════════════════════════════════════
// 2. Request/response matching
// ═══════════════════════════════════════════════════════════

mod request_response_matching {
    use super::*;

    #[test]
    fn request_and_response_share_sequence_number() -> Result<(), BoxErr> {
        let seq = 42u32;
        let request = MessageHeader::new(message_types::PROFILE, 256, seq);
        let mut response = MessageHeader::new(message_types::PROFILE, 128, seq);
        response.set_flag(message_flags::IS_RESPONSE);

        let req_bytes = request.encode();
        let resp_bytes = response.encode();
        let req_decoded = MessageHeader::decode(&req_bytes)?;
        let resp_decoded = MessageHeader::decode(&resp_bytes)?;

        assert_eq!(req_decoded.sequence, resp_decoded.sequence);
        assert!(!req_decoded.has_flag(message_flags::IS_RESPONSE));
        assert!(resp_decoded.has_flag(message_flags::IS_RESPONSE));
        Ok(())
    }

    #[test]
    fn error_response_has_error_and_response_flags() -> Result<(), BoxErr> {
        let mut header = MessageHeader::new(message_types::DEVICE, 64, 10);
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
    fn ack_required_flag_round_trips() -> Result<(), BoxErr> {
        let mut header = MessageHeader::new(message_types::SAFETY, 32, 5);
        header.set_flag(message_flags::REQUIRES_ACK);

        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
        Ok(())
    }

    #[test]
    fn multiple_flags_compose_correctly() -> Result<(), BoxErr> {
        let mut header = MessageHeader::new(message_types::HEALTH, 512, 99);
        header.set_flag(message_flags::COMPRESSED);
        header.set_flag(message_flags::REQUIRES_ACK);
        header.set_flag(message_flags::STREAMING);

        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert!(decoded.has_flag(message_flags::COMPRESSED));
        assert!(decoded.has_flag(message_flags::REQUIRES_ACK));
        assert!(decoded.has_flag(message_flags::STREAMING));
        assert!(!decoded.has_flag(message_flags::IS_RESPONSE));
        assert!(!decoded.has_flag(message_flags::IS_ERROR));
        Ok(())
    }

    #[test]
    fn setting_same_flag_twice_is_idempotent() {
        let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
        header.set_flag(message_flags::COMPRESSED);
        header.set_flag(message_flags::COMPRESSED);
        assert!(header.has_flag(message_flags::COMPRESSED));
        assert_eq!(header.flags, message_flags::COMPRESSED);
    }
}

// ═══════════════════════════════════════════════════════════
// 3. Streaming protocol
// ═══════════════════════════════════════════════════════════

mod streaming_protocol {
    use super::*;

    #[test]
    fn streaming_flag_identifies_stream_messages() -> Result<(), BoxErr> {
        let mut header = MessageHeader::new(message_types::HEALTH, 128, 1);
        header.set_flag(message_flags::STREAMING);

        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert!(decoded.has_flag(message_flags::STREAMING));
        Ok(())
    }

    #[test]
    fn stream_sequence_numbers_are_monotonic() -> Result<(), BoxErr> {
        let mut headers = Vec::new();
        for seq in 0..10u32 {
            let mut h = MessageHeader::new(message_types::TELEMETRY, 64, seq);
            h.set_flag(message_flags::STREAMING);
            headers.push(h);
        }

        for window in headers.windows(2) {
            assert!(window[1].sequence > window[0].sequence);
        }
        Ok(())
    }

    #[test]
    fn streaming_with_compression() -> Result<(), BoxErr> {
        let mut header = MessageHeader::new(message_types::TELEMETRY, 1024, 5);
        header.set_flag(message_flags::STREAMING);
        header.set_flag(message_flags::COMPRESSED);

        let bytes = header.encode();
        let decoded = MessageHeader::decode(&bytes)?;
        assert!(decoded.has_flag(message_flags::STREAMING));
        assert!(decoded.has_flag(message_flags::COMPRESSED));
        Ok(())
    }

    #[tokio::test]
    async fn health_event_broadcast_stream() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let mut rx = server.subscribe_health();

        // Broadcast multiple events
        for i in 0..5 {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("device-{i}"),
                event_type: HealthEventType::Connected,
                message: format!("Event {i}"),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        // Receive and verify ordering
        let mut received = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            received.push(evt);
        }
        assert_eq!(received.len(), 5);
        for (i, evt) in received.iter().enumerate() {
            assert_eq!(evt.device_id, format!("device-{i}"));
        }
    }

    #[tokio::test]
    async fn multiple_health_subscribers() {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        let mut rx1 = server.subscribe_health();
        let mut rx2 = server.subscribe_health();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "wheel-1".to_string(),
            event_type: HealthEventType::Fault,
            message: "Over temp".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }
}

// ═══════════════════════════════════════════════════════════
// 4. Error responses
// ═══════════════════════════════════════════════════════════

mod error_responses {
    use super::*;

    #[test]
    fn connection_failed_is_recoverable() {
        let err = IpcError::ConnectionFailed("timeout".to_string());
        assert!(err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn timeout_is_recoverable() {
        let err = IpcError::timeout(5000);
        assert!(err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn version_incompatibility_is_recoverable() {
        let err = IpcError::VersionIncompatibility {
            client: "0.9.0".to_string(),
            server: "1.0.0".to_string(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn feature_negotiation_error_is_recoverable() {
        let err = IpcError::FeatureNegotiation("unsupported feature".to_string());
        assert!(err.is_recoverable());
    }

    #[test]
    fn transport_init_is_fatal() {
        let err = IpcError::TransportInit("port in use".to_string());
        assert!(err.is_fatal());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn server_not_running_is_fatal() {
        let err = IpcError::ServerNotRunning;
        assert!(err.is_fatal());
    }

    #[test]
    fn shutdown_requested_is_fatal() {
        let err = IpcError::ShutdownRequested;
        assert!(err.is_fatal());
    }

    #[test]
    fn encoding_failed_is_neither_recoverable_nor_fatal() {
        let err = IpcError::EncodingFailed("invalid proto".to_string());
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn decoding_failed_is_neither_recoverable_nor_fatal() {
        let err = IpcError::DecodingFailed("bad bytes".to_string());
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn connection_limit_error() {
        let err = IpcError::connection_limit(100);
        let msg = format!("{err}");
        assert!(msg.contains("100"));
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn timeout_error_message_includes_ms() {
        let err = IpcError::timeout(3000);
        let msg = format!("{err}");
        assert!(msg.contains("3000"));
    }

    #[test]
    fn io_error_converts_from_std_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let ipc_err: IpcError = io_err.into();
        assert!(matches!(ipc_err, IpcError::Io(_)));
        assert!(!ipc_err.is_recoverable());
        assert!(!ipc_err.is_fatal());
    }

    #[test]
    fn error_header_flags_distinguish_error_from_success() -> Result<(), BoxErr> {
        let mut err_header = MessageHeader::new(message_types::DEVICE, 64, 1);
        err_header.set_flag(message_flags::IS_RESPONSE);
        err_header.set_flag(message_flags::IS_ERROR);

        let mut ok_header = MessageHeader::new(message_types::DEVICE, 128, 1);
        ok_header.set_flag(message_flags::IS_RESPONSE);

        let err_bytes = err_header.encode();
        let ok_bytes = ok_header.encode();
        let err_decoded = MessageHeader::decode(&err_bytes)?;
        let ok_decoded = MessageHeader::decode(&ok_bytes)?;

        assert!(err_decoded.has_flag(message_flags::IS_ERROR));
        assert!(!ok_decoded.has_flag(message_flags::IS_ERROR));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 5. Version negotiation
// ═══════════════════════════════════════════════════════════

mod version_negotiation {
    use super::*;

    #[test]
    fn same_version_is_compatible() {
        assert!(is_version_compatible("1.0.0", "1.0.0"));
    }

    #[test]
    fn higher_minor_is_compatible() {
        assert!(is_version_compatible("1.1.0", "1.0.0"));
        assert!(is_version_compatible("1.5.0", "1.0.0"));
    }

    #[test]
    fn higher_patch_is_compatible() {
        assert!(is_version_compatible("1.0.1", "1.0.0"));
        assert!(is_version_compatible("1.0.99", "1.0.0"));
    }

    #[test]
    fn lower_minor_is_incompatible() {
        assert!(!is_version_compatible("1.0.0", "1.1.0"));
    }

    #[test]
    fn lower_patch_same_minor_is_incompatible() {
        assert!(!is_version_compatible("1.1.0", "1.1.1"));
    }

    #[test]
    fn different_major_is_incompatible() {
        assert!(!is_version_compatible("2.0.0", "1.0.0"));
        assert!(!is_version_compatible("0.9.0", "1.0.0"));
    }

    #[test]
    fn malformed_version_is_incompatible() {
        assert!(!is_version_compatible("abc", "1.0.0"));
        assert!(!is_version_compatible("1.0", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "abc"));
        assert!(!is_version_compatible("", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", ""));
    }

    #[tokio::test]
    async fn negotiate_compatible_client() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;

        assert!(result.compatible);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
        assert_eq!(result.server_version, PROTOCOL_VERSION);
        assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_incompatible_client() -> IpcResult<()> {
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
    async fn negotiate_unknown_features_filtered() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features(
                "1.0.0",
                &[
                    "device_management".to_string(),
                    "time_travel".to_string(),
                    "warp_drive".to_string(),
                ],
            )
            .await?;

        assert!(result.compatible);
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
        assert!(!result.enabled_features.contains(&"time_travel".to_string()));
        assert!(!result.enabled_features.contains(&"warp_drive".to_string()));

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_empty_features() -> IpcResult<()> {
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
    async fn negotiate_registers_client() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        assert_eq!(server.client_count().await, 0);
        server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        assert_eq!(server.client_count().await, 1);

        server.stop().await?;
        Ok(())
    }

    #[test]
    fn protocol_version_constant_is_valid() {
        assert!(is_version_compatible(PROTOCOL_VERSION, MIN_CLIENT_VERSION));
    }

    #[test]
    fn default_tcp_port_is_50051() {
        assert_eq!(DEFAULT_TCP_PORT, 50051);
    }
}

// ═══════════════════════════════════════════════════════════
// 6. Keepalive/heartbeat
// ═══════════════════════════════════════════════════════════

mod keepalive_heartbeat {
    use super::*;

    #[test]
    fn health_event_types_cover_all_variants() {
        let variants = [
            HealthEventType::Connected,
            HealthEventType::Disconnected,
            HealthEventType::Fault,
            HealthEventType::FaultCleared,
            HealthEventType::TemperatureWarning,
            HealthEventType::TemperatureCritical,
            HealthEventType::ProfileChanged,
            HealthEventType::HighTorqueEnabled,
            HealthEventType::EmergencyStop,
        ];
        // Each variant should have a distinct repr value
        let repr_vals: Vec<i32> = variants.iter().map(|v| *v as i32).collect();
        let unique: std::collections::HashSet<_> = repr_vals.iter().collect();
        assert_eq!(unique.len(), variants.len());
    }

    #[test]
    fn health_event_metadata_can_carry_diagnostics() {
        let mut metadata = HashMap::new();
        metadata.insert("temp_c".to_string(), "78".to_string());
        metadata.insert("voltage".to_string(), "24.1".to_string());

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "wheel-1".to_string(),
            event_type: HealthEventType::TemperatureWarning,
            message: "Temperature approaching limit".to_string(),
            metadata,
        };

        assert_eq!(event.metadata.len(), 2);
        assert_eq!(event.metadata.get("temp_c").map(String::as_str), Some("78"));
    }

    #[tokio::test]
    async fn server_state_transitions() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);

        server.start().await?;
        assert_eq!(server.state().await, ServerState::Running);
        assert!(server.is_running().await);

        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);
        Ok(())
    }

    #[tokio::test]
    async fn server_double_start_fails() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;
        let result = server.start().await;
        assert!(result.is_err());
        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn server_double_stop_is_ok() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;
        server.stop().await?;
        // Second stop should be a no-op
        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        Ok(())
    }

    #[tokio::test]
    async fn server_stop_clears_clients() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        // Register a client
        let client = ClientInfo {
            id: "test-client-1".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec!["device_management".to_string()],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;
        assert_eq!(server.client_count().await, 1);

        server.stop().await?;
        assert_eq!(server.client_count().await, 0);
        Ok(())
    }

    #[tokio::test]
    async fn register_and_unregister_client() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());

        let client = ClientInfo {
            id: "client-abc".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;
        assert_eq!(server.client_count().await, 1);

        server.unregister_client("client-abc").await;
        assert_eq!(server.client_count().await, 0);
        Ok(())
    }

    #[tokio::test]
    async fn connected_clients_returns_all() -> IpcResult<()> {
        let server = IpcServer::new(IpcConfig::default());

        for i in 0..3 {
            let client = ClientInfo {
                id: format!("client-{i}"),
                connected_at: Instant::now(),
                version: "1.0.0".to_string(),
                features: vec![],
                peer_info: PeerInfo::default(),
            };
            server.register_client(client).await;
        }

        let clients = server.connected_clients().await;
        assert_eq!(clients.len(), 3);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 7. Message framing
// ═══════════════════════════════════════════════════════════

mod message_framing {
    use super::*;

    #[test]
    fn header_is_little_endian() -> Result<(), BoxErr> {
        let header = MessageHeader::new(0x0102, 0x03040506, 0x0708090A);
        let bytes = header.encode();
        // message_type LE
        assert_eq!(bytes[0], 0x02);
        assert_eq!(bytes[1], 0x01);
        // payload_len LE
        assert_eq!(bytes[2], 0x06);
        assert_eq!(bytes[3], 0x05);
        assert_eq!(bytes[4], 0x04);
        assert_eq!(bytes[5], 0x03);
        // sequence LE
        assert_eq!(bytes[6], 0x0A);
        assert_eq!(bytes[7], 0x09);
        assert_eq!(bytes[8], 0x08);
        assert_eq!(bytes[9], 0x07);
        Ok(())
    }

    #[test]
    fn decode_insufficient_bytes_fails() {
        assert!(MessageHeader::decode(&[]).is_err());
        assert!(MessageHeader::decode(&[0u8; 4]).is_err());
        assert!(MessageHeader::decode(&[0u8; 11]).is_err());
    }

    #[test]
    fn decode_exact_12_bytes_succeeds() -> Result<(), BoxErr> {
        let header = MessageHeader::new(1, 2, 3);
        let bytes = header.encode();
        assert_eq!(bytes.len(), 12);
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, 1);
        assert_eq!(decoded.payload_len, 2);
        assert_eq!(decoded.sequence, 3);
        Ok(())
    }

    #[test]
    fn decode_extra_bytes_succeeds() -> Result<(), BoxErr> {
        let header = MessageHeader::new(1, 2, 3);
        let mut bytes = header.encode().to_vec();
        bytes.extend_from_slice(&[0xFF; 20]); // extra bytes
        let decoded = MessageHeader::decode(&bytes)?;
        assert_eq!(decoded.message_type, 1);
        assert_eq!(decoded.payload_len, 2);
        Ok(())
    }

    #[test]
    fn codec_default_max_size_is_16mb() {
        let codec = MessageCodec::new();
        assert_eq!(codec.max_message_size(), 16 * 1024 * 1024);
    }

    #[test]
    fn codec_custom_max_size() {
        let codec = MessageCodec::with_max_size(1024);
        assert_eq!(codec.max_message_size(), 1024);
    }

    #[test]
    fn codec_rejects_zero_size() {
        let codec = MessageCodec::new();
        assert!(!codec.is_valid_size(0));
    }

    #[test]
    fn codec_rejects_oversized() {
        let codec = MessageCodec::with_max_size(100);
        assert!(!codec.is_valid_size(101));
        assert!(!codec.is_valid_size(1000));
    }

    #[test]
    fn codec_accepts_valid_sizes() {
        let codec = MessageCodec::with_max_size(1000);
        assert!(codec.is_valid_size(1));
        assert!(codec.is_valid_size(500));
        assert!(codec.is_valid_size(1000));
    }

    #[test]
    fn codec_encode_decode_prost_message() -> Result<(), BoxErr> {
        // Use a simple prost message for testing
        #[derive(Clone, PartialEq, prost::Message)]
        struct TestMsg {
            #[prost(string, tag = "1")]
            value: String,
            #[prost(uint32, tag = "2")]
            count: u32,
        }

        let codec = MessageCodec::new();
        let msg = TestMsg {
            value: "hello".to_string(),
            count: 42,
        };

        let encoded: Vec<u8> = MessageEncoder::encode(&codec, &msg)?;
        assert!(!encoded.is_empty());

        let decoded: TestMsg = MessageDecoder::decode(&codec, &encoded)?;
        assert_eq!(decoded, msg);
        Ok(())
    }

    #[test]
    fn codec_encode_to_buffer_clears_and_fills() -> Result<(), BoxErr> {
        #[derive(Clone, PartialEq, prost::Message)]
        struct TestMsg {
            #[prost(uint32, tag = "1")]
            val: u32,
        }

        let codec = MessageCodec::new();
        let msg = TestMsg { val: 99 };

        let mut buf = vec![0xFFu8; 100]; // pre-filled
        MessageEncoder::encode_to_buffer(&codec, &msg, &mut buf)?;

        // Buffer should have been cleared and re-filled
        assert!(buf.len() < 100);
        let decoded: TestMsg = MessageDecoder::decode(&codec, &buf)?;
        assert_eq!(decoded.val, 99);
        Ok(())
    }

    #[test]
    fn codec_rejects_oversized_decode() {
        let codec = MessageCodec::with_max_size(4);
        let big_data = vec![0u8; 10];
        let result: IpcResult<prost_types::Timestamp> = MessageDecoder::decode(&codec, &big_data);
        assert!(result.is_err());
    }

    #[test]
    fn codec_encoded_len_is_accurate() {
        #[derive(Clone, PartialEq, prost::Message)]
        struct TestMsg {
            #[prost(string, tag = "1")]
            data: String,
        }

        let codec = MessageCodec::new();
        let msg = TestMsg {
            data: "test data".to_string(),
        };
        let len = MessageDecoder::encoded_len(&codec, &msg);
        assert!(len > 0);
    }

    #[test]
    fn all_flags_have_unique_bit_positions() {
        let all_flags = [
            message_flags::COMPRESSED,
            message_flags::REQUIRES_ACK,
            message_flags::IS_RESPONSE,
            message_flags::IS_ERROR,
            message_flags::STREAMING,
        ];
        for i in 0..all_flags.len() {
            for j in (i + 1)..all_flags.len() {
                assert_ne!(
                    all_flags[i], all_flags[j],
                    "Flags at index {i} and {j} share the same value"
                );
                // Each flag should be a single bit
                assert_eq!(
                    all_flags[i].count_ones(),
                    1,
                    "Flag at index {i} is not a single bit"
                );
            }
        }
    }

    #[test]
    fn flags_byte_layout_in_header() -> Result<(), BoxErr> {
        let mut header = MessageHeader::new(message_types::DEVICE, 0, 0);
        header.set_flag(message_flags::COMPRESSED); // 0x0001
        header.set_flag(message_flags::IS_ERROR); // 0x0008

        let bytes = header.encode();
        let flags_le = u16::from_le_bytes([bytes[10], bytes[11]]);
        assert_eq!(flags_le, 0x0001 | 0x0008);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// Additional: Transport and config
// ═══════════════════════════════════════════════════════════

mod transport_and_config {
    use super::*;

    #[test]
    fn transport_type_tcp_defaults() {
        let tcp = TransportType::tcp();
        let desc = tcp.description();
        assert!(desc.contains("TCP"));
        assert!(desc.contains("127.0.0.1"));
        assert!(desc.contains(&DEFAULT_TCP_PORT.to_string()));
    }

    #[test]
    fn transport_type_tcp_custom_address() {
        let tcp = TransportType::tcp_with_address("0.0.0.0", 8080);
        let desc = tcp.description();
        assert!(desc.contains("0.0.0.0"));
        assert!(desc.contains("8080"));
    }

    #[cfg(windows)]
    #[test]
    fn transport_type_named_pipe() {
        let pipe = TransportType::named_pipe(r"\\.\pipe\test");
        let desc = pipe.description();
        assert!(desc.contains("pipe"));
    }

    #[test]
    fn transport_config_defaults() {
        let config = TransportConfig::default();
        assert_eq!(config.max_connections, 100);
        assert!(!config.enable_acl);
        assert_eq!(config.recv_buffer_size, 64 * 1024);
        assert_eq!(config.send_buffer_size, 64 * 1024);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
    }

    #[test]
    fn transport_builder_chain() {
        let config = TransportBuilder::new()
            .transport(TransportType::tcp())
            .max_connections(25)
            .connection_timeout(Duration::from_secs(5))
            .enable_acl(true)
            .build();

        assert_eq!(config.max_connections, 25);
        assert!(config.enable_acl);
        assert_eq!(config.connection_timeout, Duration::from_secs(5));
    }

    #[test]
    fn ipc_config_defaults() {
        let config = IpcConfig::default();
        assert_eq!(config.server_name, "openracing-ipc");
        assert_eq!(config.health_buffer_size, 1000);
        assert!(config.enable_connection_logging);
        assert_eq!(config.negotiation_timeout, Duration::from_secs(5));
    }

    #[test]
    fn ipc_config_builder_methods() {
        let config = IpcConfig::with_transport(TransportType::tcp())
            .max_connections(50)
            .health_buffer_size(500);

        assert_eq!(config.transport.max_connections, 50);
        assert_eq!(config.health_buffer_size, 500);
    }

    #[test]
    fn ipc_config_serde_roundtrip() -> Result<(), BoxErr> {
        let config = IpcConfig::default()
            .max_connections(42)
            .health_buffer_size(200);

        let json = serde_json::to_string(&config)?;
        let restored: IpcConfig = serde_json::from_str(&json)?;

        assert_eq!(restored.server_name, "openracing-ipc");
        assert_eq!(restored.transport.max_connections, 42);
        assert_eq!(restored.health_buffer_size, 200);
        Ok(())
    }

    #[test]
    fn server_config_accessible() {
        let config = IpcConfig::default().max_connections(77);
        let server = IpcServer::new(config);
        assert_eq!(server.config().transport.max_connections, 77);
    }
}
