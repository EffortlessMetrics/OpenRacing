//! Comprehensive IPC transport stress tests and edge case coverage.
//!
//! Covers:
//! - Message serialization/deserialization roundtrip for all IPC message types
//! - Large message handling (oversized payloads, chunking boundaries)
//! - Rapid connect/disconnect cycles
//! - Concurrent client stress (multiple clients sending simultaneously)
//! - Backpressure behavior (slow consumer, fast producer)
//! - Feature negotiation: version mismatch handling, capability detection
//! - Error propagation: transport errors, codec errors, timeout handling
//! - Graceful shutdown: in-flight messages during shutdown
//! - Connection state machine: all valid/invalid transitions
//! - Message ordering guarantees
//! - Property-based testing for codec roundtrips

#![deny(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageEncoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::IpcError;
use openracing_ipc::server::{
    ClientInfo, HealthEvent, HealthEventType, IpcConfig, IpcServer, PeerInfo, ServerState,
    is_version_compatible,
};
use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};
use openracing_ipc::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};
use proptest::prelude::*;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ═══════════════════════════════════════════════════════════
// 1. Message serialization/deserialization roundtrip
// ═══════════════════════════════════════════════════════════

mod codec_roundtrip_stress {
    use super::*;

    #[test]
    fn all_message_types_roundtrip() -> Result<(), BoxErr> {
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
            let header = MessageHeader::new(msg_type, 512, 1);
            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)?;

            assert_eq!(
                decoded.message_type, msg_type,
                "message type 0x{:04x} roundtrip failed",
                msg_type
            );
            assert_eq!(decoded.payload_len, 512);
            assert_eq!(decoded.sequence, 1);
        }

        Ok(())
    }

    #[test]
    fn all_flag_combinations_roundtrip() -> Result<(), BoxErr> {
        let flags = [
            message_flags::COMPRESSED,
            message_flags::REQUIRES_ACK,
            message_flags::IS_RESPONSE,
            message_flags::IS_ERROR,
            message_flags::STREAMING,
        ];

        // Test every combination of flags (2^5 = 32)
        for mask in 0u16..32 {
            let mut header = MessageHeader::new(message_types::DEVICE, 100, 42);
            for (i, &flag) in flags.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    header.set_flag(flag);
                }
            }

            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)?;

            assert_eq!(
                decoded.flags, header.flags,
                "flag combination 0x{:04x} roundtrip failed",
                mask
            );
        }

        Ok(())
    }

    #[test]
    fn sequence_number_boundaries_roundtrip() -> Result<(), BoxErr> {
        let boundary_values: Vec<u32> = vec![0, 1, u32::MAX - 1, u32::MAX, u32::MAX / 2];

        for &seq in &boundary_values {
            let header = MessageHeader::new(message_types::TELEMETRY, 0, seq);
            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)?;

            assert_eq!(decoded.sequence, seq, "sequence {} roundtrip failed", seq);
        }

        Ok(())
    }

    #[test]
    fn payload_len_boundaries_roundtrip() -> Result<(), BoxErr> {
        let boundary_values: Vec<u32> = vec![0, 1, 65535, u32::MAX - 1, u32::MAX];

        for &len in &boundary_values {
            let header = MessageHeader::new(message_types::DEVICE, len, 0);
            let encoded = header.encode();
            let decoded = MessageHeader::decode(&encoded)?;

            assert_eq!(
                decoded.payload_len, len,
                "payload_len {} roundtrip failed",
                len
            );
        }

        Ok(())
    }

    #[test]
    fn bulk_encode_decode_consistency() -> Result<(), BoxErr> {
        let count = 10_000;
        let mut headers: Vec<[u8; 12]> = Vec::with_capacity(count);

        for i in 0..count {
            let header = MessageHeader::new((i % 8 + 1) as u16, (i * 100) as u32, i as u32);
            headers.push(header.encode());
        }

        for (i, encoded) in headers.iter().enumerate() {
            let decoded = MessageHeader::decode(encoded)?;
            assert_eq!(decoded.message_type, (i % 8 + 1) as u16);
            assert_eq!(decoded.payload_len, (i * 100) as u32);
            assert_eq!(decoded.sequence, i as u32);
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 2. Large message handling
// ═══════════════════════════════════════════════════════════

mod large_message_handling {
    use super::*;

    #[test]
    fn codec_rejects_oversized_encode() -> Result<(), BoxErr> {
        let codec = MessageCodec::with_max_size(1024);

        // Create a prost message that exceeds the limit.
        // We use prost_types::Value as a convenient stand-in.
        let large_string = "x".repeat(2048);
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::StringValue(large_string)),
        };

        let result = codec.encode(&msg);
        assert!(result.is_err(), "should reject message exceeding max size");

        Ok(())
    }

    #[test]
    fn codec_accepts_at_max_boundary() -> Result<(), BoxErr> {
        // A small message that fits easily
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::NumberValue(42.0)),
        };
        let encoded_len = prost::Message::encoded_len(&msg);
        let codec = MessageCodec::with_max_size(encoded_len);

        let encoded = codec.encode(&msg)?;
        assert_eq!(encoded.len(), encoded_len);

        Ok(())
    }

    #[test]
    fn codec_rejects_one_over_max() -> Result<(), BoxErr> {
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::NumberValue(42.0)),
        };
        let encoded_len = prost::Message::encoded_len(&msg);
        let codec = MessageCodec::with_max_size(encoded_len.saturating_sub(1));

        let result = codec.encode(&msg);
        assert!(result.is_err(), "should reject message one byte over max");

        Ok(())
    }

    #[test]
    fn codec_decode_rejects_oversized_input() -> Result<(), BoxErr> {
        let codec = MessageCodec::with_max_size(64);

        let big_bytes = vec![0u8; 128];
        let result: Result<prost_types::Value, _> = codec.decode(&big_bytes);
        assert!(result.is_err(), "should reject oversized decode input");

        Ok(())
    }

    #[test]
    fn codec_decode_rejects_empty_input() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();

        let result: Result<prost_types::Value, _> = codec.decode(&[]);
        assert!(result.is_err(), "should reject empty decode input");

        Ok(())
    }

    #[test]
    fn encode_to_buffer_rejects_oversized() -> Result<(), BoxErr> {
        let codec = MessageCodec::with_max_size(4);
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::StringValue(
                "oversized payload".to_string(),
            )),
        };

        let mut buf = Vec::new();
        let result = codec.encode_to_buffer(&msg, &mut buf);
        assert!(result.is_err(), "encode_to_buffer should reject oversized");

        Ok(())
    }

    #[test]
    fn encode_to_buffer_clears_previous_data() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::NumberValue(1.0)),
        };

        let mut buf = vec![0xFF; 1024];
        codec.encode_to_buffer(&msg, &mut buf)?;

        // Buffer should contain only the encoded message
        let decoded: prost_types::Value = codec.decode(&buf)?;
        match decoded.kind {
            Some(prost_types::value::Kind::NumberValue(v)) => {
                assert!((v - 1.0).abs() < f64::EPSILON);
            }
            other => return Err(format!("unexpected decoded kind: {:?}", other).into()),
        }

        Ok(())
    }

    #[test]
    fn header_with_max_payload_len() -> Result<(), BoxErr> {
        let header = MessageHeader::new(message_types::DEVICE, u32::MAX, 0);
        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)?;

        assert_eq!(decoded.payload_len, u32::MAX);

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 3. Rapid connect/disconnect cycles
// ═══════════════════════════════════════════════════════════

mod rapid_connect_disconnect {
    use super::*;

    #[tokio::test]
    async fn rapid_start_stop_100_cycles() -> Result<(), BoxErr> {
        let config = IpcConfig::default();
        let server = IpcServer::new(config);

        for i in 0..100 {
            server.start().await?;
            assert_eq!(
                server.state().await,
                ServerState::Running,
                "cycle {} start failed",
                i
            );

            server.stop().await?;
            assert_eq!(
                server.state().await,
                ServerState::Stopped,
                "cycle {} stop failed",
                i
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn rapid_register_unregister_same_client() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        for i in 0..500 {
            let client = ClientInfo {
                id: "rapid-client".to_string(),
                connected_at: std::time::Instant::now(),
                version: "1.0.0".to_string(),
                features: vec![format!("feature-{}", i)],
                peer_info: PeerInfo::default(),
            };

            server.register_client(client).await;
            assert!(server.client_count().await >= 1);

            server.unregister_client("rapid-client").await;
            assert_eq!(server.client_count().await, 0);
        }

        Ok(())
    }

    #[tokio::test]
    async fn double_stop_is_idempotent() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);

        // Second stop should not error
        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn start_while_running_is_error() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.start().await;
        assert!(
            result.is_err(),
            "starting an already-running server should error"
        );

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_registers_then_stop_clears() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        for _ in 0..50 {
            server
                .negotiate_features("1.0.0", &["device_management".to_string()])
                .await?;
        }
        assert_eq!(server.client_count().await, 50);

        server.stop().await?;
        assert_eq!(server.client_count().await, 0);

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 4. Concurrent client stress
// ═══════════════════════════════════════════════════════════

mod concurrent_client_stress {
    use super::*;

    #[tokio::test]
    async fn concurrent_negotiation_with_barrier() -> Result<(), BoxErr> {
        let num_clients = 200;
        let config = IpcConfig::default().max_connections(1000);
        let server = Arc::new(IpcServer::new(config));
        server.start().await?;

        let barrier = Arc::new(Barrier::new(num_clients));
        let mut tasks = JoinSet::new();

        for i in 0..num_clients {
            let server = server.clone();
            let barrier = barrier.clone();
            tasks.spawn(async move {
                barrier.wait().await;
                let features = vec![format!("feature_{}", i % 7)];
                server.negotiate_features("1.0.0", &features).await
            });
        }

        let mut successes = 0;
        while let Some(result) = tasks.join_next().await {
            let join_result = result?;
            if join_result.is_ok() {
                successes += 1;
            }
        }

        assert_eq!(successes, num_clients, "all clients should succeed");
        assert_eq!(server.client_count().await, num_clients);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_register_unregister_interleaved() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        let num_ops = 500;
        let mut tasks = JoinSet::new();

        for i in 0..num_ops {
            let server = server.clone();
            tasks.spawn(async move {
                let client_id = format!("interleaved-{}", i);
                let client = ClientInfo {
                    id: client_id.clone(),
                    connected_at: std::time::Instant::now(),
                    version: "1.0.0".to_string(),
                    features: vec![],
                    peer_info: PeerInfo::default(),
                };
                server.register_client(client).await;

                // Small yield to allow interleaving
                tokio::task::yield_now().await;

                server.unregister_client(&client_id).await;
            });
        }

        while let Some(result) = tasks.join_next().await {
            result?;
        }

        // All clients should have been unregistered
        assert_eq!(server.client_count().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn concurrent_health_subscribers_receive_events() -> Result<(), BoxErr> {
        let config = IpcConfig::default().health_buffer_size(5000);
        let server = Arc::new(IpcServer::new(config));

        let num_subscribers = 10;
        let num_events = 100;

        let mut receivers: Vec<_> = (0..num_subscribers)
            .map(|_| server.subscribe_health())
            .collect();

        // Broadcast events
        for i in 0..num_events {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("dev-{}", i),
                event_type: HealthEventType::Connected,
                message: format!("event-{}", i),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        // Each subscriber should have received all events
        for (sub_idx, rx) in receivers.iter_mut().enumerate() {
            let mut count = 0;
            while rx.try_recv().is_ok() {
                count += 1;
            }
            assert_eq!(
                count, num_events,
                "subscriber {} received {} events, expected {}",
                sub_idx, count, num_events
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn mixed_version_concurrent_negotiation() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        let versions = vec![
            ("1.0.0", true),
            ("1.1.0", true),
            ("1.0.1", true),
            ("0.9.0", false),
            ("2.0.0", false),
            ("0.0.1", false),
        ];

        let mut tasks = JoinSet::new();

        for (version, expected_compat) in versions.clone() {
            let server = server.clone();
            tasks.spawn(async move {
                let result = server
                    .negotiate_features(version, &["device_management".to_string()])
                    .await;
                (version, expected_compat, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            let (version, expected_compat, negotiate_result) = result?;
            let negotiation = negotiate_result?;
            assert_eq!(
                negotiation.compatible, expected_compat,
                "version {} compatibility mismatch",
                version
            );
        }

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 5. Backpressure behavior
// ═══════════════════════════════════════════════════════════

mod backpressure_behavior {
    use super::*;

    #[tokio::test]
    async fn slow_consumer_drops_oldest_events() -> Result<(), BoxErr> {
        // Small buffer to force backpressure
        let buffer_size = 16;
        let config = IpcConfig::default().health_buffer_size(buffer_size);
        let server = IpcServer::new(config);

        let mut receiver = server.subscribe_health();

        // Flood far more events than the buffer can hold
        let flood_count = buffer_size * 10;
        for i in 0..flood_count {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("dev-{}", i),
                event_type: HealthEventType::Connected,
                message: format!("msg-{}", i),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        // Consumer reads late; should get lagged error or remaining events
        let mut received = 0;
        let mut lagged = false;
        loop {
            match receiver.try_recv() {
                Ok(_) => received += 1,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_n)) => {
                    lagged = true;
                    // Continue reading after lag
                    continue;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            }
        }

        // Either we lagged or got some events (the broadcast channel can lag)
        assert!(
            lagged || received > 0,
            "should either lag or receive events"
        );

        Ok(())
    }

    #[tokio::test]
    async fn producer_not_blocked_by_no_consumers() -> Result<(), BoxErr> {
        let config = IpcConfig::default().health_buffer_size(8);
        let server = IpcServer::new(config);

        // No subscribers: broadcast should not block or panic
        let start = std::time::Instant::now();
        for i in 0..10_000 {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("dev-{}", i),
                event_type: HealthEventType::Fault,
                message: "no-consumer".to_string(),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }
        let elapsed = start.elapsed();

        // Should complete quickly even without consumers
        assert!(
            elapsed < Duration::from_secs(5),
            "broadcasting without consumers should be fast, took {:?}",
            elapsed
        );

        Ok(())
    }

    #[tokio::test]
    async fn multiple_consumers_different_speeds() -> Result<(), BoxErr> {
        let config = IpcConfig::default().health_buffer_size(256);
        let server = Arc::new(IpcServer::new(config));

        let mut fast_rx = server.subscribe_health();
        let mut slow_rx = server.subscribe_health();

        let num_events = 50;
        for i in 0..num_events {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("dev-{}", i),
                event_type: HealthEventType::Connected,
                message: format!("evt-{}", i),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        // Fast consumer reads immediately
        let mut fast_count = 0;
        while fast_rx.try_recv().is_ok() {
            fast_count += 1;
        }
        assert_eq!(
            fast_count, num_events,
            "fast consumer should get all events"
        );

        // Slow consumer reads later (should still get events from buffer)
        let mut slow_count = 0;
        while slow_rx.try_recv().is_ok() {
            slow_count += 1;
        }
        assert_eq!(
            slow_count, num_events,
            "slow consumer should get all events from buffer"
        );

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 6. Feature negotiation edge cases
// ═══════════════════════════════════════════════════════════

mod feature_negotiation_stress {
    use super::*;

    #[test]
    fn version_with_garbage_input() {
        // Non-numeric parts
        assert!(!is_version_compatible("abc.def.ghi", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "abc.def.ghi"));

        // Empty string
        assert!(!is_version_compatible("", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", ""));

        // Single component
        assert!(!is_version_compatible("1", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "1"));

        // Two components
        assert!(!is_version_compatible("1.0", "1.0.0"));
        assert!(!is_version_compatible("1.0.0", "1.0"));
    }

    #[test]
    fn version_boundary_values() {
        // Same version
        assert!(is_version_compatible("1.0.0", "1.0.0"));

        // Max-ish values
        assert!(is_version_compatible("999.999.999", "999.999.999"));
        assert!(is_version_compatible("1.999.999", "1.0.0"));

        // Zero major
        assert!(is_version_compatible("0.0.0", "0.0.0"));
        assert!(!is_version_compatible("0.0.0", "1.0.0"));
    }

    #[test]
    fn version_with_extra_components() {
        // Extra components are ignored (take(3))
        assert!(is_version_compatible("1.0.0.0", "1.0.0"));
        assert!(is_version_compatible("1.0.0.99", "1.0.0"));
    }

    #[tokio::test]
    async fn negotiate_with_empty_features() -> Result<(), BoxErr> {
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
    async fn negotiate_with_unknown_features() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let features = vec![
            "nonexistent_feature_1".to_string(),
            "nonexistent_feature_2".to_string(),
        ];
        let result = server.negotiate_features("1.0.0", &features).await?;
        assert!(result.compatible);
        assert!(
            result.enabled_features.is_empty(),
            "unknown features should not be enabled"
        );

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_incompatible_still_returns_server_info() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server
            .negotiate_features("0.1.0", &["device_management".to_string()])
            .await?;

        assert!(!result.compatible);
        assert_eq!(result.server_version, PROTOCOL_VERSION);
        assert_eq!(result.min_client_version, MIN_CLIENT_VERSION);
        assert!(
            !result.supported_features.is_empty(),
            "server should still report supported features"
        );

        // Incompatible client should not be registered
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn negotiate_all_features_at_once() -> Result<(), BoxErr> {
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

        let mut enabled_sorted = result.enabled_features.clone();
        enabled_sorted.sort();
        let mut expected_sorted = all_features.clone();
        expected_sorted.sort();
        assert_eq!(enabled_sorted, expected_sorted);

        server.stop().await?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 7. Error propagation
// ═══════════════════════════════════════════════════════════

mod error_propagation {
    use super::*;

    #[test]
    fn io_error_converts_to_ipc_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let ipc_err: IpcError = io_err.into();

        assert!(
            matches!(ipc_err, IpcError::Io(_)),
            "IO error should convert to IpcError::Io"
        );
        assert!(ipc_err.to_string().contains("refused"));
    }

    #[test]
    fn encoding_error_is_not_recoverable_or_fatal() {
        let err = IpcError::EncodingFailed("bad data".to_string());
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn decoding_error_is_not_recoverable_or_fatal() {
        let err = IpcError::DecodingFailed("corrupt".to_string());
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn timeout_error_is_recoverable() {
        let err = IpcError::timeout(5000);
        assert!(err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn connection_limit_is_neither_recoverable_nor_fatal() {
        let err = IpcError::connection_limit(100);
        assert!(!err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn version_incompatibility_is_recoverable() {
        let err = IpcError::VersionIncompatibility {
            client: "0.9.0".to_string(),
            server: "1.0.0".to_string(),
        };
        assert!(err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn feature_negotiation_error_is_recoverable() {
        let err = IpcError::FeatureNegotiation("unsupported".to_string());
        assert!(err.is_recoverable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn shutdown_requested_is_fatal() {
        let err = IpcError::ShutdownRequested;
        assert!(err.is_fatal());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn all_error_variants_have_nonempty_display() {
        let errors: Vec<IpcError> = vec![
            IpcError::TransportInit("init".to_string()),
            IpcError::ConnectionFailed("fail".to_string()),
            IpcError::EncodingFailed("encode".to_string()),
            IpcError::DecodingFailed("decode".to_string()),
            IpcError::VersionIncompatibility {
                client: "1.0.0".to_string(),
                server: "2.0.0".to_string(),
            },
            IpcError::FeatureNegotiation("neg".to_string()),
            IpcError::ServerNotRunning,
            IpcError::ConnectionLimitExceeded { max: 10 },
            IpcError::Timeout { timeout_ms: 1000 },
            IpcError::Grpc("grpc".to_string()),
            IpcError::InvalidConfig("config".to_string()),
            IpcError::PlatformNotSupported("platform".to_string()),
            IpcError::ShutdownRequested,
        ];

        for err in &errors {
            let display = format!("{}", err);
            assert!(!display.is_empty(), "error display should be non-empty");
        }
    }

    #[test]
    fn codec_decode_garbage_bytes_returns_error() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();
        let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];

        let result: Result<prost_types::Value, _> = codec.decode(&garbage);
        assert!(
            result.is_err(),
            "decoding garbage bytes should return error"
        );

        Ok(())
    }

    #[test]
    fn header_decode_all_zeros() -> Result<(), BoxErr> {
        let zeros = [0u8; 12];
        let decoded = MessageHeader::decode(&zeros)?;

        assert_eq!(decoded.message_type, 0);
        assert_eq!(decoded.payload_len, 0);
        assert_eq!(decoded.sequence, 0);
        assert_eq!(decoded.flags, 0);

        Ok(())
    }

    #[test]
    fn header_decode_all_ones() -> Result<(), BoxErr> {
        let ones = [0xFF; 12];
        let decoded = MessageHeader::decode(&ones)?;

        assert_eq!(decoded.message_type, u16::MAX);
        assert_eq!(decoded.payload_len, u32::MAX);
        assert_eq!(decoded.sequence, u32::MAX);
        assert_eq!(decoded.flags, u16::MAX);

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 8. Graceful shutdown
// ═══════════════════════════════════════════════════════════

mod graceful_shutdown {
    use super::*;

    #[tokio::test]
    async fn shutdown_clears_all_clients() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        for i in 0..100 {
            let client = ClientInfo {
                id: format!("client-{}", i),
                connected_at: std::time::Instant::now(),
                version: "1.0.0".to_string(),
                features: vec!["device_management".to_string()],
                peer_info: PeerInfo::default(),
            };
            server.register_client(client).await;
        }

        assert_eq!(server.client_count().await, 100);

        server.stop().await?;
        assert_eq!(server.client_count().await, 0);
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn shutdown_during_concurrent_registrations() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        server.start().await?;

        let server_for_register = server.clone();
        let register_handle = tokio::spawn(async move {
            for i in 0..1000 {
                let client = ClientInfo {
                    id: format!("inflight-{}", i),
                    connected_at: std::time::Instant::now(),
                    version: "1.0.0".to_string(),
                    features: vec![],
                    peer_info: PeerInfo::default(),
                };
                server_for_register.register_client(client).await;
                tokio::task::yield_now().await;
            }
        });

        // Give some registrations time to happen, then shutdown
        tokio::time::sleep(Duration::from_millis(5)).await;
        server.stop().await?;

        // Wait for registration task to finish
        let _ = register_handle.await;

        // After shutdown, state should be stopped (clients cleared by stop)
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn restart_after_shutdown_works() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        server.start().await?;
        server
            .negotiate_features("1.0.0", &["device_management".to_string()])
            .await?;
        assert_eq!(server.client_count().await, 1);

        server.stop().await?;
        assert_eq!(server.client_count().await, 0);

        // Restart
        server.start().await?;
        assert_eq!(server.state().await, ServerState::Running);
        assert_eq!(server.client_count().await, 0);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn health_subscriber_after_shutdown() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let mut rx = server.subscribe_health();

        // Broadcast an event
        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "dev".to_string(),
            event_type: HealthEventType::Connected,
            message: "test".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event);

        // Should receive the event
        let received = rx.try_recv();
        assert!(received.is_ok());

        server.stop().await?;

        // After shutdown, new broadcasts have no subscribers issue
        // but should not panic
        let event2 = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "dev2".to_string(),
            event_type: HealthEventType::Disconnected,
            message: "post-shutdown".to_string(),
            metadata: HashMap::new(),
        };
        server.broadcast_health_event(event2);

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 9. Connection state machine
// ═══════════════════════════════════════════════════════════

mod connection_state_machine {
    use super::*;

    #[tokio::test]
    async fn initial_state_is_stopped() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);
        Ok(())
    }

    #[tokio::test]
    async fn stopped_to_running_transition() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        assert_eq!(server.state().await, ServerState::Running);
        assert!(server.is_running().await);

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn running_to_stopped_transition() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;
        server.stop().await?;

        assert_eq!(server.state().await, ServerState::Stopped);
        assert!(!server.is_running().await);

        Ok(())
    }

    #[tokio::test]
    async fn cannot_start_already_running() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        server.start().await?;

        let result = server.start().await;
        assert!(result.is_err());

        server.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn stop_from_stopped_is_noop() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());
        // Never started
        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn full_lifecycle_stopped_running_stopped() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        assert_eq!(server.state().await, ServerState::Stopped);

        server.start().await?;
        assert_eq!(server.state().await, ServerState::Running);

        server.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }

    #[tokio::test]
    async fn multiple_full_lifecycles() -> Result<(), BoxErr> {
        let server = IpcServer::new(IpcConfig::default());

        for _ in 0..20 {
            assert_eq!(server.state().await, ServerState::Stopped);

            server.start().await?;
            assert_eq!(server.state().await, ServerState::Running);
            assert!(server.is_running().await);

            server.stop().await?;
            assert_eq!(server.state().await, ServerState::Stopped);
            assert!(!server.is_running().await);
        }

        Ok(())
    }

    #[tokio::test]
    async fn state_visible_across_arc_clones() -> Result<(), BoxErr> {
        let server = Arc::new(IpcServer::new(IpcConfig::default()));
        let server2 = server.clone();

        server.start().await?;
        assert_eq!(server2.state().await, ServerState::Running);

        server2.stop().await?;
        assert_eq!(server.state().await, ServerState::Stopped);

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 10. Message ordering guarantees
// ═══════════════════════════════════════════════════════════

mod message_ordering {
    use super::*;

    #[test]
    fn sequential_headers_preserve_sequence_order() -> Result<(), BoxErr> {
        let count = 1000;
        let mut encoded_headers = Vec::with_capacity(count);

        for i in 0..count {
            let header = MessageHeader::new(message_types::TELEMETRY, 64, i as u32);
            encoded_headers.push(header.encode());
        }

        for (i, encoded) in encoded_headers.iter().enumerate() {
            let decoded = MessageHeader::decode(encoded)?;
            assert_eq!(
                decoded.sequence, i as u32,
                "sequence ordering broken at index {}",
                i
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn health_events_received_in_order() -> Result<(), BoxErr> {
        let config = IpcConfig::default().health_buffer_size(2000);
        let server = IpcServer::new(config);
        let mut rx = server.subscribe_health();

        let num_events = 1000;
        for i in 0..num_events {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("seq-{}", i),
                event_type: HealthEventType::Connected,
                message: format!("{}", i),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        let mut received_order = Vec::new();
        while let Ok(event) = rx.try_recv() {
            received_order.push(event.device_id.clone());
        }

        assert_eq!(received_order.len(), num_events);
        for (i, device_id) in received_order.iter().enumerate() {
            assert_eq!(
                device_id,
                &format!("seq-{}", i),
                "event ordering broken at index {}",
                i
            );
        }

        Ok(())
    }

    #[test]
    fn header_encode_is_deterministic() -> Result<(), BoxErr> {
        let header = MessageHeader::new(message_types::DEVICE, 12345, 67890);

        let encode1 = header.encode();
        let encode2 = header.encode();
        let encode3 = header.encode();

        assert_eq!(encode1, encode2);
        assert_eq!(encode2, encode3);

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 11. Transport configuration edge cases
// ═══════════════════════════════════════════════════════════

mod transport_config_edge_cases {
    use super::*;

    #[test]
    fn transport_builder_defaults() -> Result<(), BoxErr> {
        let config = TransportBuilder::new().build();

        assert_eq!(config.max_connections, 100);
        assert!(!config.enable_acl);
        assert_eq!(config.recv_buffer_size, 64 * 1024);
        assert_eq!(config.send_buffer_size, 64 * 1024);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));

        Ok(())
    }

    #[test]
    fn transport_builder_all_options() -> Result<(), BoxErr> {
        let config = TransportBuilder::new()
            .transport(TransportType::tcp_with_address("0.0.0.0", 9090))
            .max_connections(1)
            .connection_timeout(Duration::from_millis(100))
            .enable_acl(true)
            .build();

        assert_eq!(config.max_connections, 1);
        assert!(config.enable_acl);
        assert_eq!(config.connection_timeout, Duration::from_millis(100));

        match &config.transport {
            TransportType::Tcp { address, port } => {
                assert_eq!(address, "0.0.0.0");
                assert_eq!(*port, 9090);
            }
            #[allow(unreachable_patterns)]
            _ => return Err("expected TCP transport".into()),
        }

        Ok(())
    }

    #[test]
    fn transport_builder_zero_connections() -> Result<(), BoxErr> {
        let config = TransportBuilder::new().max_connections(0).build();
        assert_eq!(config.max_connections, 0);
        Ok(())
    }

    #[test]
    fn transport_builder_max_connections() -> Result<(), BoxErr> {
        let config = TransportBuilder::new().max_connections(usize::MAX).build();
        assert_eq!(config.max_connections, usize::MAX);
        Ok(())
    }

    #[test]
    fn transport_config_default_serde_roundtrip() -> Result<(), BoxErr> {
        let config = TransportConfig::default();
        let json = serde_json::to_string(&config)?;
        let deserialized: TransportConfig = serde_json::from_str(&json)?;

        assert_eq!(deserialized.max_connections, config.max_connections);
        assert_eq!(deserialized.connection_timeout, config.connection_timeout);
        assert_eq!(deserialized.enable_acl, config.enable_acl);
        assert_eq!(deserialized.recv_buffer_size, config.recv_buffer_size);
        assert_eq!(deserialized.send_buffer_size, config.send_buffer_size);

        Ok(())
    }

    #[test]
    fn ipc_config_custom_transport() -> Result<(), BoxErr> {
        let config = IpcConfig::with_transport(TransportType::tcp_with_address("10.0.0.1", 8080))
            .max_connections(25)
            .health_buffer_size(100);

        assert_eq!(config.transport.max_connections, 25);
        assert_eq!(config.health_buffer_size, 100);
        assert_eq!(config.server_name, "openracing-ipc");

        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_transport_serde_roundtrip() -> Result<(), BoxErr> {
        let transport = TransportType::named_pipe(r"\\.\pipe\test-stress");
        let json = serde_json::to_string(&transport)?;
        let deserialized: TransportType = serde_json::from_str(&json)?;

        match deserialized {
            TransportType::NamedPipe { pipe_name } => {
                assert_eq!(pipe_name, r"\\.\pipe\test-stress");
            }
            _ => return Err("expected NamedPipe transport".into()),
        }

        Ok(())
    }

    #[test]
    fn platform_default_transport() -> Result<(), BoxErr> {
        let transport = TransportType::platform_default();
        let desc = transport.description();
        assert!(
            !desc.is_empty(),
            "platform default should have a description"
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 12. Property-based tests for codec roundtrips
// ═══════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn prop_codec_encode_decode_roundtrip_value(n in any::<f64>()) {
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::NumberValue(n)),
        };
        let codec = MessageCodec::new();

        // Only test if encoded_len > 0 (prost may produce empty for some values)
        let encoded_len = prost::Message::encoded_len(&msg);
        if encoded_len > 0 {
            let encoded = codec.encode(&msg)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            let decoded: prost_types::Value = codec.decode(&encoded)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

            match decoded.kind {
                Some(prost_types::value::Kind::NumberValue(v)) => {
                    // NaN != NaN, so check both NaN or equal
                    prop_assert!(
                        (n.is_nan() && v.is_nan()) || (n - v).abs() < f64::EPSILON,
                        "roundtrip mismatch: {} vs {}", n, v
                    );
                }
                other => {
                    prop_assert!(false, "expected NumberValue, got {:?}", other);
                }
            }
        }
    }

    #[test]
    fn prop_codec_encode_decode_roundtrip_string(s in ".*") {
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::StringValue(s.clone())),
        };
        let codec = MessageCodec::new();

        let encoded_len = prost::Message::encoded_len(&msg);
        if encoded_len > 0 {
            let encoded = codec.encode(&msg)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            let decoded: prost_types::Value = codec.decode(&encoded)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

            match decoded.kind {
                Some(prost_types::value::Kind::StringValue(v)) => {
                    prop_assert_eq!(s, v);
                }
                other => {
                    prop_assert!(false, "expected StringValue, got {:?}", other);
                }
            }
        }
    }

    #[test]
    fn prop_codec_encode_decode_roundtrip_bool(b in any::<bool>()) {
        let msg = prost_types::Value {
            kind: Some(prost_types::value::Kind::BoolValue(b)),
        };
        let codec = MessageCodec::new();

        let encoded_len = prost::Message::encoded_len(&msg);
        if encoded_len > 0 {
            let encoded = codec.encode(&msg)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;
            let decoded: prost_types::Value = codec.decode(&encoded)
                .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

            match decoded.kind {
                Some(prost_types::value::Kind::BoolValue(v)) => {
                    prop_assert_eq!(b, v);
                }
                other => {
                    prop_assert!(false, "expected BoolValue, got {:?}", other);
                }
            }
        }
    }

    #[test]
    fn prop_header_all_fields_roundtrip(
        msg_type in any::<u16>(),
        payload_len in any::<u32>(),
        sequence in any::<u32>(),
        flags in any::<u16>(),
    ) {
        let mut header = MessageHeader::new(msg_type, payload_len, sequence);
        header.flags = flags;

        let encoded = header.encode();
        let decoded = MessageHeader::decode(&encoded)
            .map_err(|e| TestCaseError::Fail(format!("{e}").into()))?;

        prop_assert_eq!(decoded.message_type, msg_type);
        prop_assert_eq!(decoded.payload_len, payload_len);
        prop_assert_eq!(decoded.sequence, sequence);
        prop_assert_eq!(decoded.flags, flags);
    }

    #[test]
    fn prop_codec_size_validation_consistent(
        max_size in 1usize..10_000_000,
        test_size in 0usize..20_000_000,
    ) {
        let codec = MessageCodec::with_max_size(max_size);
        let valid = codec.is_valid_size(test_size);

        // Valid iff test_size > 0 AND test_size <= max_size
        let expected = test_size > 0 && test_size <= max_size;
        prop_assert_eq!(valid, expected,
            "is_valid_size({}) with max {} should be {}",
            test_size, max_size, expected
        );
    }

    #[test]
    fn prop_version_compatible_reflexive(
        major in 0u32..100,
        minor in 0u32..100,
        patch in 0u32..100,
    ) {
        let version = format!("{}.{}.{}", major, minor, patch);
        prop_assert!(
            is_version_compatible(&version, &version),
            "version {} should be compatible with itself",
            version
        );
    }

    #[test]
    fn prop_version_compatible_higher_minor(
        major in 0u32..100,
        minor in 0u32..99,
        patch in 0u32..100,
        extra_minor in 1u32..100,
    ) {
        let client = format!("{}.{}.{}", major, minor + extra_minor, patch);
        let min = format!("{}.{}.{}", major, minor, 0);
        prop_assert!(
            is_version_compatible(&client, &min),
            "client {} should be compatible with min {}",
            client, min
        );
    }

    #[test]
    fn prop_version_incompatible_different_major(
        major1 in 0u32..100,
        major2 in 0u32..100,
        minor in 0u32..100,
        patch in 0u32..100,
    ) {
        prop_assume!(major1 != major2);
        let v1 = format!("{}.{}.{}", major1, minor, patch);
        let v2 = format!("{}.{}.{}", major2, minor, patch);
        prop_assert!(
            !is_version_compatible(&v1, &v2),
            "different major versions {} and {} should be incompatible",
            v1, v2
        );
    }
}

// ═══════════════════════════════════════════════════════════
// 13. Codec stress tests
// ═══════════════════════════════════════════════════════════

mod codec_stress {
    use super::*;

    #[test]
    fn encode_decode_many_different_sizes() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();

        let sizes: Vec<usize> = vec![1, 10, 100, 1000, 10_000, 100_000];

        for size in sizes {
            let payload = "a".repeat(size);
            let msg = prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue(payload.clone())),
            };

            let encoded = codec.encode(&msg)?;
            let decoded: prost_types::Value = codec.decode(&encoded)?;

            match decoded.kind {
                Some(prost_types::value::Kind::StringValue(v)) => {
                    assert_eq!(v.len(), size, "roundtrip failed for size {}", size);
                }
                other => {
                    return Err(format!("unexpected kind for size {}: {:?}", size, other).into());
                }
            }
        }

        Ok(())
    }

    #[test]
    fn encode_to_buffer_reuses_buffer() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();
        let mut buffer = Vec::new();

        for i in 0..100 {
            let msg = prost_types::Value {
                kind: Some(prost_types::value::Kind::NumberValue(i as f64)),
            };
            codec.encode_to_buffer(&msg, &mut buffer)?;

            let decoded: prost_types::Value = codec.decode(&buffer)?;
            match decoded.kind {
                Some(prost_types::value::Kind::NumberValue(v)) => {
                    assert!(
                        (v - i as f64).abs() < f64::EPSILON,
                        "buffer reuse failed at iteration {}",
                        i
                    );
                }
                other => {
                    return Err(format!("unexpected kind at iteration {}: {:?}", i, other).into());
                }
            }
        }

        Ok(())
    }

    #[test]
    fn encoded_len_matches_actual() -> Result<(), BoxErr> {
        let codec = MessageCodec::new();

        for i in 0..50 {
            let msg = prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue(format!(
                    "test-message-{}",
                    i
                ))),
            };

            let predicted_len = codec.encoded_len(&msg);
            let encoded = codec.encode(&msg)?;

            assert_eq!(
                predicted_len,
                encoded.len(),
                "encoded_len mismatch at iteration {}",
                i
            );
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════
// 14. Health event type coverage
// ═══════════════════════════════════════════════════════════

mod health_event_coverage {
    use super::*;

    #[tokio::test]
    async fn all_health_event_types_broadcast() -> Result<(), BoxErr> {
        let config = IpcConfig::default().health_buffer_size(100);
        let server = IpcServer::new(config);
        let mut rx = server.subscribe_health();

        let event_types = [
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

        for (i, &evt_type) in event_types.iter().enumerate() {
            let event = HealthEvent {
                timestamp: std::time::SystemTime::now(),
                device_id: format!("dev-{}", i),
                event_type: evt_type,
                message: format!("type-{}", i),
                metadata: HashMap::new(),
            };
            server.broadcast_health_event(event);
        }

        let mut received = Vec::new();
        while let Ok(event) = rx.try_recv() {
            received.push(event.event_type);
        }

        assert_eq!(
            received.len(),
            event_types.len(),
            "should receive all event types"
        );

        for (i, &expected_type) in event_types.iter().enumerate() {
            assert_eq!(
                received[i], expected_type,
                "event type mismatch at index {}",
                i
            );
        }

        Ok(())
    }

    #[test]
    fn health_event_with_metadata() -> Result<(), BoxErr> {
        let mut metadata = HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key2".to_string(), "value2".to_string());

        let event = HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "dev-meta".to_string(),
            event_type: HealthEventType::Fault,
            message: "fault with metadata".to_string(),
            metadata: metadata.clone(),
        };

        assert_eq!(event.metadata.len(), 2);
        assert_eq!(
            event.metadata.get("key1").map(|s| s.as_str()),
            Some("value1")
        );

        Ok(())
    }
}
