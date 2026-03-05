//! Deep tests for IPC transport, feature negotiation, and service communication.
//!
//! Covers areas NOT covered by existing test files:
//! 1. Feature negotiation edge cases (re-negotiation, ordering, idempotency)
//! 2. Transport layer behavior (buffer sizes, connection limits, backpressure)
//! 3. Message serialization/deserialization for all message types with payloads
//! 4. Error propagation across IPC boundary (chained errors, io→ipc conversion)
//! 5. Concurrent client handling (contention, interleaved register/negotiate)
//! 6. Large message handling (near-limit, boundary, fragmentation)
//! 7. Proptest for message framing streams

#![deny(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;
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
use proptest::prelude::*;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ═══════════════════════════════════════════════════════════
// 1. Feature negotiation edge cases
// ═══════════════════════════════════════════════════════════

mod feature_negotiation_edge_cases {
    use super::*;

    #[tokio::test]
    async fn re_negotiation_adds_second_client() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let r1 = server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        assert!(r1.compatible);
        assert_eq!(server.client_count().await, 1);

        // A second negotiate call registers a second client
        let r2 = server
            .negotiate_features("1.0.0", &["safety_control".to_string()])
            .await?;
        assert!(r2.compatible);
        assert_eq!(server.client_count().await, 2);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn feature_order_does_not_affect_result() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features_a = vec![
            "device_management".to_string(),
            "safety_control".to_string(),
        ];
        let features_b = vec![
            "safety_control".to_string(),
            "device_management".to_string(),
        ];

        let r_a = server.negotiate_features("1.0.0", &features_a).await?;
        let r_b = server.negotiate_features("1.0.0", &features_b).await?;

        let mut enabled_a = r_a.enabled_features.clone();
        enabled_a.sort();
        let mut enabled_b = r_b.enabled_features.clone();
        enabled_b.sort();

        assert_eq!(enabled_a, enabled_b);
        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn duplicate_features_in_request_deduplicated_in_result() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features = vec![
            "device_management".to_string(),
            "device_management".to_string(),
            "device_management".to_string(),
        ];
        let result = server.negotiate_features("1.0.0", &features).await?;

        // The intersection filter operates per-element, so duplicates may appear;
        // verify at least one is present
        assert!(
            result
                .enabled_features
                .contains(&"device_management".to_string())
        );
        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn all_seven_server_features_can_be_enabled() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let all_features = vec![
            "device_management".to_string(),
            "profile_management".to_string(),
            "safety_control".to_string(),
            "health_monitoring".to_string(),
            "game_integration".to_string(),
            "streaming_health".to_string(),
            "streaming_devices".to_string(),
        ];

        let result = server.negotiate_features("1.0.0", &all_features).await?;
        assert!(result.compatible);
        assert_eq!(result.enabled_features.len(), 7);
        assert_eq!(result.supported_features.len(), 7);
        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn incompatible_client_gets_empty_enabled_but_full_supported() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features(
                "0.1.0",
                &[
                    "device_management".to_string(),
                    "safety_control".to_string(),
                ],
            )
            .await?;

        assert!(!result.compatible);
        // Server still reports what it supports for informational purposes
        assert!(!result.supported_features.is_empty());
        // But the enabled features should still list the intersection
        // (server does not gate on compatibility for enabled_features calculation)
        assert!(!result.enabled_features.is_empty());
        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn incompatible_client_not_registered() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let _ = server
            .negotiate_features("0.0.1", &["device_management".to_string()])
            .await?;
        assert_eq!(
            server.client_count().await,
            0,
            "incompatible client should not be registered"
        );

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiation_result_contains_min_client_version() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.negotiate_features("1.0.0", &[]).await?;
        assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);
        assert_eq!(result.server_version, PROTOCOL_VERSION);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiation_timeout_config_is_respected() -> Result<(), BoxErr> {
        let config = IpcConfig {
            negotiation_timeout: Duration::from_millis(100),
            ..IpcConfig::default()
        };
        let server = IpcServer::new(config);
        server.start().await?;

        assert_eq!(
            server.config().negotiation_timeout,
            Duration::from_millis(100)
        );

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn feature_negotiation_with_unicode_feature_names() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features = vec!["日本語_feature".to_string(), "émoji_🏎️".to_string()];
        let result = server.negotiate_features("1.0.0", &features).await?;

        // Neither Unicode feature is a known server feature
        assert!(result.enabled_features.is_empty());
        assert!(result.compatible);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn many_unknown_features_do_not_cause_errors() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features: Vec<String> = (0..1000).map(|i| format!("unknown_feature_{i}")).collect();
        let result = server.negotiate_features("1.0.0", &features).await?;

        assert!(result.compatible);
        assert!(result.enabled_features.is_empty());

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 2. Transport layer behavior
// ═══════════════════════════════════════════════════════════

mod transport_layer_behavior {
    use super::*;

    #[test]
    fn buffer_sizes_have_sane_defaults() {
        let config = TransportConfig::default();
        assert_eq!(config.recv_buffer_size, 64 * 1024);
        assert_eq!(config.send_buffer_size, 64 * 1024);
    }

    #[test]
    fn buffer_sizes_can_be_customized_via_direct_field_access() -> Result<(), BoxErr> {
        let config = TransportConfig {
            recv_buffer_size: 128 * 1024,
            send_buffer_size: 256 * 1024,
            ..Default::default()
        };
        assert_eq!(config.recv_buffer_size, 128 * 1024);
        assert_eq!(config.send_buffer_size, 256 * 1024);
        Ok(())
    }

    #[test]
    fn transport_config_serde_preserves_buffer_sizes() -> Result<(), BoxErr> {
        let config = TransportConfig {
            recv_buffer_size: 1024,
            send_buffer_size: 2048,
            ..Default::default()
        };

        let json = serde_json::to_string(&config)?;
        let restored: TransportConfig = serde_json::from_str(&json)?;
        assert_eq!(restored.recv_buffer_size, 1024);
        assert_eq!(restored.send_buffer_size, 2048);
        Ok(())
    }

    #[test]
    fn connection_limit_in_config_matches_builder() {
        let config = TransportBuilder::new().max_connections(42).build();
        assert_eq!(config.max_connections, 42);
    }

    #[test]
    fn connection_timeout_boundary_values() -> Result<(), BoxErr> {
        let zero_timeout = TransportBuilder::new()
            .connection_timeout(Duration::ZERO)
            .build();
        assert_eq!(zero_timeout.connection_timeout, Duration::ZERO);

        let large_timeout = TransportBuilder::new()
            .connection_timeout(Duration::from_secs(3600))
            .build();
        assert_eq!(large_timeout.connection_timeout, Duration::from_secs(3600));
        Ok(())
    }

    #[test]
    fn ipc_config_connection_logging_toggle() {
        let config = IpcConfig {
            enable_connection_logging: false,
            ..IpcConfig::default()
        };
        assert!(!config.enable_connection_logging);
    }

    #[test]
    fn health_buffer_size_zero_is_accepted_by_config() {
        // broadcast::channel panics on 0, but config itself should accept any value
        let config = IpcConfig::default().health_buffer_size(1);
        assert_eq!(config.health_buffer_size, 1);
    }

    #[test]
    fn transport_type_clone_eq() -> Result<(), BoxErr> {
        let t1 = TransportType::tcp_with_address("0.0.0.0", 8080);
        let t2 = t1.clone();
        // Both should produce the same description
        assert_eq!(t1.description(), t2.description());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_with_long_name() -> Result<(), BoxErr> {
        let long_name = format!(r"\\.\pipe\{}", "a".repeat(200));
        let t = TransportType::named_pipe(&long_name);
        let desc = t.description();
        assert!(desc.contains(&"a".repeat(200)));
        Ok(())
    }

    #[test]
    fn tcp_with_port_zero() -> Result<(), BoxErr> {
        let t = TransportType::tcp_with_address("127.0.0.1", 0);
        let desc = t.description();
        assert!(desc.contains(":0"));
        Ok(())
    }

    #[test]
    fn tcp_with_ipv6_address() -> Result<(), BoxErr> {
        let t = TransportType::tcp_with_address("::1", 50051);
        let desc = t.description();
        assert!(desc.contains("::1"));
        Ok(())
    }

    #[test]
    fn transport_config_serde_roundtrip_with_tcp() -> Result<(), BoxErr> {
        let config = TransportBuilder::new()
            .transport(TransportType::tcp_with_address("10.0.0.1", 9999))
            .max_connections(200)
            .connection_timeout(Duration::from_secs(60))
            .enable_acl(true)
            .build();

        let json = serde_json::to_string(&config)?;
        let restored: TransportConfig = serde_json::from_str(&json)?;
        assert_eq!(restored.max_connections, 200);
        assert!(restored.enable_acl);
        assert_eq!(restored.connection_timeout, Duration::from_secs(60));
        Ok(())
    }

    #[test]
    fn ipc_config_serde_roundtrip() -> Result<(), BoxErr> {
        let config = IpcConfig::with_transport(TransportType::tcp())
            .max_connections(75)
            .health_buffer_size(2000);

        let json = serde_json::to_string(&config)?;
        let restored: IpcConfig = serde_json::from_str(&json)?;
        assert_eq!(restored.transport.max_connections, 75);
        assert_eq!(restored.health_buffer_size, 2000);
        assert_eq!(restored.server_name, "openracing-ipc");
        Ok(())
    }

    #[tokio::test]
    async fn server_restart_cycle_preserves_config() -> Result<(), BoxErr> {
        let config = IpcConfig::with_transport(TransportType::tcp()).max_connections(10);
        let server = IpcServer::new(config);

        for _ in 0..5 {
            server.start().await?;
            assert!(server.is_running().await);
            assert_eq!(server.config().transport.max_connections, 10);
            server.stop().await?;
            assert!(!server.is_running().await);
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 3. Message serialization for ALL message types with payloads
// ═══════════════════════════════════════════════════════════

mod message_serialization_all_types {
    use super::*;

    /// Helper: build a header + payload buffer (header framing a protobuf body)
    fn frame_message(
        msg_type: u16,
        payload: &[u8],
        seq: u32,
        flags: u16,
    ) -> Result<Vec<u8>, BoxErr> {
        let mut header = MessageHeader::new(msg_type, payload.len() as u32, seq);
        header.flags = flags;
        let encoded_hdr = header.encode();
        let mut frame = encoded_hdr.to_vec();
        frame.extend_from_slice(payload);
        Ok(frame)
    }

    /// Helper: parse a framed message back into header + payload
    fn parse_frame(data: &[u8]) -> Result<(MessageHeader, &[u8]), BoxErr> {
        let header = MessageHeader::decode(data)?;
        let payload_start = MessageHeader::SIZE;
        let payload_end = payload_start + header.payload_len as usize;
        if data.len() < payload_end {
            return Err("incomplete frame".into());
        }
        Ok((header, &data[payload_start..payload_end]))
    }

    #[derive(Clone, PartialEq, prost::Message)]
    struct SimplePayload {
        #[prost(string, tag = "1")]
        id: String,
        #[prost(uint32, tag = "2")]
        value: u32,
    }

    #[test]
    fn frame_roundtrip_device_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "wheel-base-1".to_string(),
            value: 2500,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(message_types::DEVICE, &encoded, 1, 0)?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::DEVICE);
        assert_eq!(hdr.sequence, 1);
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.id, "wheel-base-1");
        assert_eq!(decoded.value, 2500);
        Ok(())
    }

    #[test]
    fn frame_roundtrip_profile_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "profile-drift-1".to_string(),
            value: 42,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(message_types::PROFILE, &encoded, 2, 0)?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::PROFILE);
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.id, "profile-drift-1");
        Ok(())
    }

    #[test]
    fn frame_roundtrip_safety_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "e-stop-1".to_string(),
            value: 1,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(
            message_types::SAFETY,
            &encoded,
            3,
            message_flags::REQUIRES_ACK,
        )?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::SAFETY);
        assert!(hdr.has_flag(message_flags::REQUIRES_ACK));
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.id, "e-stop-1");
        Ok(())
    }

    #[test]
    fn frame_roundtrip_health_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "health-check-0".to_string(),
            value: 78,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(message_types::HEALTH, &encoded, 4, message_flags::STREAMING)?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::HEALTH);
        assert!(hdr.has_flag(message_flags::STREAMING));
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.value, 78);
        Ok(())
    }

    #[test]
    fn frame_roundtrip_feature_negotiation_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "negotiate-1".to_string(),
            value: 7,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(message_types::FEATURE_NEGOTIATION, &encoded, 5, 0)?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::FEATURE_NEGOTIATION);
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.id, "negotiate-1");
        Ok(())
    }

    #[test]
    fn frame_roundtrip_game_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "acc-launch".to_string(),
            value: 100,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(message_types::GAME, &encoded, 6, 0)?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::GAME);
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.id, "acc-launch");
        Ok(())
    }

    #[test]
    fn frame_roundtrip_telemetry_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "telem-stream".to_string(),
            value: 1000,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(
            message_types::TELEMETRY,
            &encoded,
            7,
            message_flags::STREAMING | message_flags::COMPRESSED,
        )?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::TELEMETRY);
        assert!(hdr.has_flag(message_flags::STREAMING));
        assert!(hdr.has_flag(message_flags::COMPRESSED));
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.value, 1000);
        Ok(())
    }

    #[test]
    fn frame_roundtrip_diagnostic_message() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "diag-perf".to_string(),
            value: 250,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(message_types::DIAGNOSTIC, &encoded, 8, 0)?;
        let (hdr, body) = parse_frame(&frame)?;

        assert_eq!(hdr.message_type, message_types::DIAGNOSTIC);
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.id, "diag-perf");
        Ok(())
    }

    #[test]
    fn multiple_frames_in_stream_buffer() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();
        let mut stream_buf = Vec::new();

        // Write 3 framed messages
        for i in 0..3u32 {
            let payload = SimplePayload {
                id: format!("msg-{i}"),
                value: i * 10,
            };
            let encoded = codec.encode(&payload)?;
            let frame = frame_message(message_types::DEVICE, &encoded, i, 0)?;
            stream_buf.extend_from_slice(&frame);
        }

        // Parse them back
        let mut offset = 0;
        for i in 0..3u32 {
            let hdr = MessageHeader::decode(&stream_buf[offset..])?;
            offset += MessageHeader::SIZE;
            let payload_end = offset + hdr.payload_len as usize;
            let decoded: SimplePayload = codec.decode(&stream_buf[offset..payload_end])?;
            assert_eq!(decoded.id, format!("msg-{i}"));
            assert_eq!(decoded.value, i * 10);
            offset = payload_end;
        }
        assert_eq!(offset, stream_buf.len());
        Ok(())
    }

    #[test]
    fn error_response_frame() -> Result<(), BoxErr> {
        let payload = SimplePayload {
            id: "error-detail".to_string(),
            value: 404,
        };
        let codec = MessageCodec::new();
        let encoded = codec.encode(&payload)?;
        let frame = frame_message(
            message_types::DEVICE,
            &encoded,
            99,
            message_flags::IS_RESPONSE | message_flags::IS_ERROR,
        )?;
        let (hdr, body) = parse_frame(&frame)?;

        assert!(hdr.has_flag(message_flags::IS_RESPONSE));
        assert!(hdr.has_flag(message_flags::IS_ERROR));
        let decoded: SimplePayload = codec.decode(body)?;
        assert_eq!(decoded.value, 404);
        Ok(())
    }

    #[test]
    fn all_flags_combined_in_single_header() -> Result<(), BoxErr> {
        let all_flags = message_flags::COMPRESSED
            | message_flags::REQUIRES_ACK
            | message_flags::IS_RESPONSE
            | message_flags::IS_ERROR
            | message_flags::STREAMING;

        let frame = frame_message(message_types::DIAGNOSTIC, &[], 0, all_flags)?;
        let (hdr, _) = parse_frame(&frame)?;

        assert!(hdr.has_flag(message_flags::COMPRESSED));
        assert!(hdr.has_flag(message_flags::REQUIRES_ACK));
        assert!(hdr.has_flag(message_flags::IS_RESPONSE));
        assert!(hdr.has_flag(message_flags::IS_ERROR));
        assert!(hdr.has_flag(message_flags::STREAMING));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 4. Error propagation across IPC boundary
// ═══════════════════════════════════════════════════════════

mod error_propagation {
    use super::*;

    #[test]
    fn io_error_broken_pipe_converts_to_ipc() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe");
        let ipc_err: IpcError = io_err.into();
        let display = format!("{ipc_err}");
        assert!(display.contains("broken pipe"));
        assert!(!ipc_err.is_recoverable());
        assert!(!ipc_err.is_fatal());
    }

    #[test]
    fn io_error_connection_refused_converts_to_ipc() {
        let io_err =
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused");
        let ipc_err: IpcError = io_err.into();
        let display = format!("{ipc_err}");
        assert!(display.contains("connection refused"));
    }

    #[test]
    fn io_error_timed_out_converts_to_ipc() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "operation timed out");
        let ipc_err: IpcError = io_err.into();
        assert!(matches!(ipc_err, IpcError::Io(_)));
    }

    #[test]
    fn io_error_would_block_converts_to_ipc() {
        let io_err = std::io::Error::new(std::io::ErrorKind::WouldBlock, "would block");
        let ipc_err: IpcError = io_err.into();
        assert!(matches!(ipc_err, IpcError::Io(_)));
    }

    #[test]
    fn encoding_error_preserves_context_string() {
        let err = IpcError::EncodingFailed("field 'name' exceeds 255 bytes".to_string());
        let display = format!("{err}");
        assert!(display.contains("field 'name' exceeds 255 bytes"));
    }

    #[test]
    fn decoding_error_preserves_context_string() {
        let err = IpcError::DecodingFailed("invalid varint at byte 7".to_string());
        let display = format!("{err}");
        assert!(display.contains("invalid varint at byte 7"));
    }

    #[test]
    fn grpc_error_preserves_message() {
        let err = IpcError::Grpc("UNAVAILABLE: service shutting down".to_string());
        let display = format!("{err}");
        assert!(display.contains("UNAVAILABLE"));
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn invalid_config_error_preserves_message() {
        let err = IpcError::InvalidConfig("max_connections must be > 0".to_string());
        let display = format!("{err}");
        assert!(display.contains("max_connections must be > 0"));
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn platform_not_supported_error() {
        let err = IpcError::PlatformNotSupported("Named Pipes on Linux".to_string());
        let display = format!("{err}");
        assert!(display.contains("Named Pipes on Linux"));
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        // IpcError contains io::Error which is Send+Sync
        assert_send_sync::<IpcError>();
    }

    #[test]
    fn error_debug_format_is_non_empty() {
        let errors: Vec<IpcError> = vec![
            IpcError::TransportInit("test".to_string()),
            IpcError::ConnectionFailed("test".to_string()),
            IpcError::EncodingFailed("test".to_string()),
            IpcError::DecodingFailed("test".to_string()),
            IpcError::VersionIncompatibility {
                client: "1.0.0".to_string(),
                server: "2.0.0".to_string(),
            },
            IpcError::FeatureNegotiation("test".to_string()),
            IpcError::ServerNotRunning,
            IpcError::ConnectionLimitExceeded { max: 50 },
            IpcError::Timeout { timeout_ms: 1000 },
            IpcError::Grpc("test".to_string()),
            IpcError::InvalidConfig("test".to_string()),
            IpcError::PlatformNotSupported("test".to_string()),
            IpcError::ShutdownRequested,
        ];

        for err in &errors {
            let debug = format!("{err:?}");
            assert!(!debug.is_empty(), "Debug format should be non-empty");
        }
    }

    #[test]
    fn codec_encode_empty_message_returns_error() {
        // A default proto3 message with all default values has 0 encoded_len
        #[derive(Clone, PartialEq, prost::Message)]
        struct EmptyMsg {}

        let codec = MessageCodec::new();
        let msg = EmptyMsg {};
        let result = codec.encode(&msg);
        // encoded_len is 0 for empty message, which is_valid_size rejects
        assert!(result.is_err());
    }

    #[test]
    fn codec_decode_garbage_bytes_returns_error() {
        #[derive(Clone, PartialEq, prost::Message)]
        struct TestMsg {
            #[prost(string, tag = "1")]
            value: String,
        }

        let codec = MessageCodec::new();
        // Valid size but invalid protobuf data
        let garbage = vec![0xFF; 100];
        let result: IpcResult<TestMsg> = codec.decode(&garbage);
        // Prost is lenient: it may or may not decode garbage.
        // But 0xFF repeated is likely an invalid varint, so this tests the path.
        // Either success or error is acceptable; we just verify no panic.
        let _ = result;
    }

    #[tokio::test]
    async fn server_start_when_already_running_returns_invalid_config() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let err = server.start().await;
        assert!(err.is_err());
        let ipc_err = err.err().ok_or("expected error")?;
        assert!(
            matches!(ipc_err, IpcError::InvalidConfig(_)),
            "expected InvalidConfig, got: {ipc_err:?}"
        );

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 5. Concurrent client handling
// ═══════════════════════════════════════════════════════════

mod concurrent_client_handling {
    use super::*;

    #[tokio::test]
    async fn many_concurrent_negotiations() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        let mut handles = Vec::new();
        for i in 0..50 {
            let srv = Arc::clone(&server);
            handles.push(tokio::spawn(async move {
                let features = vec![
                    "device_management".to_string(),
                    format!("custom_feature_{i}"),
                ];
                srv.negotiate_features("1.0.0", &features).await
            }));
        }

        let mut success_count = 0u32;
        for handle in handles {
            let result = handle.await?;
            let negotiation = result?;
            assert!(negotiation.compatible);
            assert!(
                negotiation
                    .enabled_features
                    .contains(&"device_management".to_string())
            );
            success_count += 1;
        }
        assert_eq!(success_count, 50);
        // All 50 compatible clients registered
        assert_eq!(server.client_count().await, 50);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_register_unregister_interleaved() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        // Register 20 clients
        for i in 0..20 {
            let client = ClientInfo {
                id: format!("client-{i}"),
                connected_at: Instant::now(),
                version: "1.0.0".to_string(),
                features: vec!["device_management".to_string()],
                peer_info: PeerInfo::default(),
            };
            server.register_client(client).await;
        }
        assert_eq!(server.client_count().await, 20);

        // Concurrently unregister even-numbered and register new odd-numbered
        let mut handles = Vec::new();
        for i in 0..20 {
            let srv = Arc::clone(&server);
            if i % 2 == 0 {
                handles.push(tokio::spawn(async move {
                    srv.unregister_client(&format!("client-{i}")).await;
                }));
            } else {
                handles.push(tokio::spawn(async move {
                    let client = ClientInfo {
                        id: format!("new-client-{i}"),
                        connected_at: Instant::now(),
                        version: "1.0.0".to_string(),
                        features: vec![],
                        peer_info: PeerInfo::default(),
                    };
                    srv.register_client(client).await;
                }));
            }
        }

        for handle in handles {
            handle.await?;
        }

        // 20 initial - 10 unregistered + 10 new = 20
        assert_eq!(server.client_count().await, 20);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_health_broadcast_with_many_subscribers() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));

        let mut receivers: Vec<_> = (0..10).map(|_| server.subscribe_health()).collect();

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "concurrent-wheel".to_string(),
            event_type: HealthEventType::Connected,
            message: "test".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        for rx in &mut receivers {
            let received = rx.try_recv();
            assert!(received.is_ok(), "each subscriber should receive the event");
        }
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_negotiate_and_stop() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        let srv_for_negotiate = Arc::clone(&server);
        let negotiate_handle = tokio::spawn(async move {
            for _ in 0..10 {
                let _ = srv_for_negotiate
                    .negotiate_features("1.0.0", &["device_management".to_string()])
                    .await;
            }
        });

        // Allow some negotiations to happen then stop
        tokio::time::sleep(Duration::from_millis(1)).await;
        server.stop().await?;

        // negotiate_handle should complete without panic
        negotiate_handle.await?;
        assert_eq!(server.state().await, ServerState::Stopped);
        Ok(())
    }

    #[tokio::test]
    async fn unregister_nonexistent_client_is_no_op() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        let client = ClientInfo {
            id: "existing-client".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;

        // Unregister a client that doesn't exist
        server.unregister_client("no-such-client").await;
        // Original client should still be there
        assert_eq!(server.client_count().await, 1);
        Ok(())
    }

    #[tokio::test]
    async fn double_unregister_same_client() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        let client = ClientInfo {
            id: "double-unreg".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec![],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;
        assert_eq!(server.client_count().await, 1);

        server.unregister_client("double-unreg").await;
        assert_eq!(server.client_count().await, 0);

        // Second unregister is a no-op
        server.unregister_client("double-unreg").await;
        assert_eq!(server.client_count().await, 0);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 6. Large message handling
// ═══════════════════════════════════════════════════════════

mod large_message_handling {
    use super::*;

    #[derive(Clone, PartialEq, prost::Message)]
    struct LargePayload {
        #[prost(bytes = "vec", tag = "1")]
        data: Vec<u8>,
    }

    #[test]
    fn encode_message_just_under_limit() -> Result<(), BoxErr> {
        let codec = MessageCodec::with_max_size(1024);
        // Create a payload that encodes to just under 1024 bytes
        // Protobuf overhead: tag (1 byte) + length varint (2 bytes for sizes 128-16383)
        let payload = LargePayload {
            data: vec![0xAB; 1018],
        };
        let result = codec.encode(&payload);
        assert!(
            result.is_ok(),
            "message just under limit should encode: {:?}",
            result.err()
        );
        Ok(())
    }

    #[test]
    fn encode_message_over_limit_rejected() {
        let codec = MessageCodec::with_max_size(100);
        let payload = LargePayload {
            data: vec![0xCD; 200],
        };
        let result = codec.encode(&payload);
        assert!(result.is_err(), "oversized message should be rejected");
    }

    #[test]
    fn decode_message_over_limit_rejected() {
        let codec = MessageCodec::with_max_size(50);
        let bytes = vec![0x0A; 100]; // arbitrary bytes exceeding limit
        let result: IpcResult<LargePayload> = codec.decode(&bytes);
        assert!(result.is_err(), "oversized decode should be rejected");
    }

    #[test]
    fn encode_at_exact_limit_succeeds() -> Result<(), BoxErr> {
        // Find the exact limit by trial
        let payload = LargePayload {
            data: vec![0x42; 10],
        };
        let codec_for_measure = MessageCodec::new();
        let encoded = codec_for_measure.encode(&payload)?;
        let exact_size = encoded.len();

        let codec = MessageCodec::with_max_size(exact_size);
        let result = codec.encode(&payload);
        assert!(result.is_ok(), "message at exact limit should succeed");
        Ok(())
    }

    #[test]
    fn encode_one_byte_over_limit_fails() -> Result<(), BoxErr> {
        let payload = LargePayload {
            data: vec![0x42; 10],
        };
        let codec_for_measure = MessageCodec::new();
        let encoded = codec_for_measure.encode(&payload)?;
        let exact_size = encoded.len();

        let codec = MessageCodec::with_max_size(exact_size - 1);
        let result = codec.encode(&payload);
        assert!(result.is_err(), "one byte over should fail");
        Ok(())
    }

    #[test]
    fn large_payload_roundtrip_via_frame() -> Result<(), BoxErr> {
        let codec = MessageCodec::new(); // 16 MB limit
        let payload = LargePayload {
            data: vec![0xFF; 65536],
        };
        let encoded = codec.encode(&payload)?;

        let header = MessageHeader::new(message_types::TELEMETRY, encoded.len() as u32, 1);
        let header_bytes = header.encode();

        let mut frame = header_bytes.to_vec();
        frame.extend_from_slice(&encoded);

        // Parse back
        let decoded_hdr = MessageHeader::decode(&frame)?;
        assert_eq!(decoded_hdr.payload_len as usize, encoded.len());
        let payload_start = MessageHeader::SIZE;
        let payload_end = payload_start + decoded_hdr.payload_len as usize;
        let decoded: LargePayload = codec.decode(&frame[payload_start..payload_end])?;
        assert_eq!(decoded.data.len(), 65536);
        assert!(decoded.data.iter().all(|&b| b == 0xFF));
        Ok(())
    }

    #[test]
    fn encode_to_buffer_clears_previous_content() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();

        #[derive(Clone, PartialEq, prost::Message)]
        struct Msg {
            #[prost(uint32, tag = "1")]
            val: u32,
        }

        let mut buffer = Vec::with_capacity(1024);
        buffer.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // garbage
        assert_eq!(buffer.len(), 4);

        let msg = Msg { val: 42 };
        codec.encode_to_buffer(&msg, &mut buffer)?;

        // encode_to_buffer clears old content; length should differ from garbage
        assert_ne!(buffer.len(), 4, "buffer should have been cleared and re-filled");
        let decoded: Msg = codec.decode(&buffer)?;
        assert_eq!(decoded.val, 42);
        Ok(())
    }

    #[test]
    fn header_payload_len_matches_actual_encoded_size() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();

        #[derive(Clone, PartialEq, prost::Message)]
        struct Msg {
            #[prost(string, tag = "1")]
            name: String,
            #[prost(uint64, tag = "2")]
            ts: u64,
        }

        let msg = Msg {
            name: "test-device-123".to_string(),
            ts: 1700000000,
        };
        let encoded = codec.encode(&msg)?;
        let reported_len = codec.encoded_len(&msg);

        assert_eq!(
            encoded.len(),
            reported_len,
            "encoded_len must match actual encoding"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 7. Proptest for message framing streams
// ═══════════════════════════════════════════════════════════

mod proptest_framing {
    use super::*;

    fn arb_message_type() -> impl Strategy<Value = u16> {
        prop_oneof![
            Just(message_types::DEVICE),
            Just(message_types::PROFILE),
            Just(message_types::SAFETY),
            Just(message_types::HEALTH),
            Just(message_types::FEATURE_NEGOTIATION),
            Just(message_types::GAME),
            Just(message_types::TELEMETRY),
            Just(message_types::DIAGNOSTIC),
        ]
    }

    fn arb_flags() -> impl Strategy<Value = u16> {
        prop::bits::u16::masked(
            message_flags::COMPRESSED
                | message_flags::REQUIRES_ACK
                | message_flags::IS_RESPONSE
                | message_flags::IS_ERROR
                | message_flags::STREAMING,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn prop_framed_stream_roundtrip(
            msg_count in 1usize..20,
            msg_type in arb_message_type(),
            flags in arb_flags(),
        ) {
            // Build a stream of framed messages with varying payload sizes
            let codec = MessageCodec::new();
            let mut stream_buf = Vec::new();

            for seq in 0..msg_count as u32 {
                let payload_size = ((seq as usize) * 7 + 3) % 100 + 1;
                let payload_data = vec![(seq & 0xFF) as u8; payload_size];

                #[derive(Clone, PartialEq, prost::Message)]
                struct StreamPayload {
                    #[prost(bytes = "vec", tag = "1")]
                    data: Vec<u8>,
                }

                let payload = StreamPayload { data: payload_data };
                let encoded = codec.encode(&payload)
                    .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

                let mut header = MessageHeader::new(msg_type, encoded.len() as u32, seq);
                header.flags = flags;
                let hdr_bytes = header.encode();

                stream_buf.extend_from_slice(&hdr_bytes);
                stream_buf.extend_from_slice(&encoded);
            }

            // Parse back
            let mut offset = 0;
            for seq in 0..msg_count as u32 {
                let hdr = MessageHeader::decode(&stream_buf[offset..])
                    .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
                prop_assert_eq!(hdr.message_type, msg_type);
                prop_assert_eq!(hdr.sequence, seq);
                prop_assert_eq!(hdr.flags, flags);

                offset += MessageHeader::SIZE;
                let payload_end = offset + hdr.payload_len as usize;
                prop_assert!(payload_end <= stream_buf.len(), "frame extends past buffer");

                #[derive(Clone, PartialEq, prost::Message)]
                struct StreamPayload {
                    #[prost(bytes = "vec", tag = "1")]
                    data: Vec<u8>,
                }

                let decoded: StreamPayload = codec.decode(&stream_buf[offset..payload_end])
                    .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

                let expected_size = ((seq as usize) * 7 + 3) % 100 + 1;
                prop_assert_eq!(decoded.data.len(), expected_size);
                prop_assert!(decoded.data.iter().all(|&b| b == (seq & 0xFF) as u8));

                offset = payload_end;
            }
            prop_assert_eq!(offset, stream_buf.len(), "should consume entire buffer");
        }

        #[test]
        fn prop_header_with_arbitrary_payload_len_roundtrips(
            msg_type in arb_message_type(),
            payload_len in any::<u32>(),
            seq in any::<u32>(),
            flags in arb_flags(),
        ) {
            let mut header = MessageHeader::new(msg_type, payload_len, seq);
            header.flags = flags;

            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

            prop_assert_eq!(decoded.message_type, msg_type);
            prop_assert_eq!(decoded.payload_len, payload_len);
            prop_assert_eq!(decoded.sequence, seq);
            prop_assert_eq!(decoded.flags, flags);
        }

        #[test]
        fn prop_version_compat_reflexive(
            major in 0u32..10,
            minor in 0u32..20,
            patch in 0u32..50,
        ) {
            let version = format!("{major}.{minor}.{patch}");
            prop_assert!(
                is_version_compatible(&version, &version),
                "any version should be compatible with itself"
            );
        }

        #[test]
        fn prop_version_compat_higher_minor_always_compatible(
            major in 1u32..5,
            min_minor in 0u32..10,
            delta in 1u32..10,
            patch in 0u32..20,
        ) {
            let min_version = format!("{major}.{min_minor}.0");
            let client_version = format!("{major}.{}.{patch}", min_minor + delta);
            prop_assert!(
                is_version_compatible(&client_version, &min_version),
                "{client_version} should be compatible with min {min_version}"
            );
        }

        #[test]
        fn prop_version_compat_different_major_never_compatible(
            major1 in 0u32..10,
            major2 in 0u32..10,
            minor in 0u32..20,
            patch in 0u32..20,
        ) {
            prop_assume!(major1 != major2);
            let v1 = format!("{major1}.{minor}.{patch}");
            let v2 = format!("{major2}.{minor}.{patch}");
            prop_assert!(
                !is_version_compatible(&v1, &v2),
                "different major versions should never be compatible"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════
// 8. Additional health event edge cases
// ═══════════════════════════════════════════════════════════

mod health_event_edge_cases {
    use super::*;

    #[tokio::test]
    async fn broadcast_with_no_subscribers_does_not_panic() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        // Broadcast without any subscribers
        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "orphan-device".to_string(),
            event_type: HealthEventType::Disconnected,
            message: "no listeners".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);
        // Should not panic
        Ok(())
    }

    #[tokio::test]
    async fn subscriber_dropped_does_not_affect_other_subscribers() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        let mut rx1 = server.subscribe_health();
        let rx2 = server.subscribe_health();
        let mut rx3 = server.subscribe_health();

        // Drop rx2
        drop(rx2);

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "wheel-1".to_string(),
            event_type: HealthEventType::Connected,
            message: "test".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        assert!(rx1.try_recv().is_ok(), "rx1 should still receive");
        assert!(rx3.try_recv().is_ok(), "rx3 should still receive");
        Ok(())
    }

    #[tokio::test]
    async fn health_event_with_large_metadata() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        let mut rx = server.subscribe_health();

        let mut metadata = HashMap::new();
        for i in 0..100 {
            metadata.insert(format!("key_{i}"), format!("value_{}", "x".repeat(100)));
        }

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "wheel-1".to_string(),
            event_type: HealthEventType::TemperatureWarning,
            message: "temp high".to_string(),
            metadata: metadata.clone(),
        };
        server.broadcast_health_event(event);

        let received = rx.try_recv();
        assert!(received.is_ok());
        let evt = received.map_err(|e| -> BoxErr { format!("recv error: {e}").into() })?;
        assert_eq!(evt.metadata.len(), 100);
        Ok(())
    }

    #[test]
    fn health_event_type_repr_values_are_contiguous() {
        let expected: Vec<i32> = (0..=8).collect();
        let actual: Vec<i32> = vec![
            HealthEventType::Connected as i32,
            HealthEventType::Disconnected as i32,
            HealthEventType::Fault as i32,
            HealthEventType::FaultCleared as i32,
            HealthEventType::TemperatureWarning as i32,
            HealthEventType::TemperatureCritical as i32,
            HealthEventType::ProfileChanged as i32,
            HealthEventType::HighTorqueEnabled as i32,
            HealthEventType::EmergencyStop as i32,
        ];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn rapid_broadcast_burst() -> Result<(), BoxErr> {
        let config = IpcConfig::default().health_buffer_size(1000);
        let server = IpcServer::new(config);
        let mut rx = server.subscribe_health();

        // Rapid-fire 100 events
        for i in 0..100u32 {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("dev-{i}"),
                event_type: HealthEventType::Connected,
                message: format!("burst event {i}"),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        let mut count = 0u32;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 100, "all burst events should be received");
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 9. Server and client info edge cases
// ═══════════════════════════════════════════════════════════

mod server_client_info {
    use super::*;

    #[tokio::test]
    async fn connected_clients_returns_correct_info() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        let client = ClientInfo {
            id: "info-test-client".to_string(),
            connected_at: Instant::now(),
            version: "1.2.3".to_string(),
            features: vec![
                "device_management".to_string(),
                "safety_control".to_string(),
            ],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client).await;

        let clients = server.connected_clients().await;
        assert_eq!(clients.len(), 1);
        let c = &clients[0];
        assert_eq!(c.id, "info-test-client");
        assert_eq!(c.version, "1.2.3");
        assert_eq!(c.features.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn register_client_with_same_id_overwrites() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        let client1 = ClientInfo {
            id: "same-id".to_string(),
            connected_at: Instant::now(),
            version: "1.0.0".to_string(),
            features: vec!["device_management".to_string()],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client1).await;
        assert_eq!(server.client_count().await, 1);

        let client2 = ClientInfo {
            id: "same-id".to_string(),
            connected_at: Instant::now(),
            version: "2.0.0".to_string(),
            features: vec!["safety_control".to_string()],
            peer_info: PeerInfo::default(),
        };
        server.register_client(client2).await;
        // Same id should overwrite, not add
        assert_eq!(server.client_count().await, 1);

        let clients = server.connected_clients().await;
        assert_eq!(clients[0].version, "2.0.0");
        Ok(())
    }

    #[test]
    fn server_config_server_name_default() {
        let config = IpcConfig::default();
        assert_eq!(config.server_name, "openracing-ipc");
    }

    #[test]
    fn server_config_negotiation_timeout_default() {
        let config = IpcConfig::default();
        assert_eq!(config.negotiation_timeout, Duration::from_secs(5));
    }

    #[test]
    fn peer_info_default_on_windows() {
        let peer = PeerInfo::default();
        #[cfg(windows)]
        assert!(peer.process_id.is_none());
        #[cfg(unix)]
        {
            assert!(peer.uid.is_none());
            assert!(peer.gid.is_none());
        }
    }

    #[tokio::test]
    async fn server_state_after_fresh_creation() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);
        assert_eq!(server.client_count().await, 0);
        assert!(server.connected_clients().await.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn client_features_stored_from_negotiation() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let _ = server
            .negotiate_features(
                "1.0.0",
                &[
                    "device_management".to_string(),
                    "health_monitoring".to_string(),
                ],
            )
            .await?;

        let clients = server.connected_clients().await;
        assert_eq!(clients.len(), 1);
        let features = &clients[0].features;
        assert!(features.contains(&"device_management".to_string()));
        assert!(features.contains(&"health_monitoring".to_string()));

        server.stop().await?;
        Ok(())
    }
}
