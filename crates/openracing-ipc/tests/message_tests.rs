//! Comprehensive tests for IPC message types, serialization round-trips,
//! error propagation, connection multiplexing, and edge cases.

use std::collections::HashMap;
use std::time::Duration;

use openracing_ipc::codec::{
    MessageCodec, MessageDecoder, MessageHeader, message_flags, message_types,
};
use openracing_ipc::error::IpcError;
use openracing_ipc::server::{
    HealthEvent, HealthEventType, IpcConfig, IpcServer, ServerState, is_version_compatible,
};
use openracing_ipc::transport::{TransportBuilder, TransportConfig, TransportType};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// =========================================================================
// 1. All IPC message types
// =========================================================================

#[test]
fn message_type_constants_are_distinct() -> Result<(), BoxErr> {
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
    let mut seen = std::collections::HashSet::new();
    for t in &types {
        assert!(seen.insert(t), "duplicate message type: {t:#06x}");
    }
    assert_eq!(types.len(), 8, "expected 8 message types");
    Ok(())
}

#[test]
fn message_flags_are_non_overlapping() -> Result<(), BoxErr> {
    let flags = [
        message_flags::COMPRESSED,
        message_flags::REQUIRES_ACK,
        message_flags::IS_RESPONSE,
        message_flags::IS_ERROR,
        message_flags::STREAMING,
    ];
    for (i, a) in flags.iter().enumerate() {
        for b in &flags[i + 1..] {
            assert_eq!(a & b, 0, "flags {a:#06x} and {b:#06x} overlap");
        }
    }
    Ok(())
}

#[test]
fn message_header_all_types_roundtrip() -> Result<(), BoxErr> {
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
        let decoded = MessageHeader::decode(&encoded)?;
        assert_eq!(decoded.message_type, msg_type);
        assert_eq!(decoded.payload_len, 512);
        assert_eq!(decoded.sequence, 7);
    }
    Ok(())
}

#[test]
fn message_header_with_all_flags() -> Result<(), BoxErr> {
    let mut header = MessageHeader::new(message_types::HEALTH, 100, 1);
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

// =========================================================================
// 2. Serialization round-trips with serde
// =========================================================================

#[test]
fn ipc_config_serde_roundtrip() -> Result<(), BoxErr> {
    let original = IpcConfig::default();
    let json = serde_json::to_string_pretty(&original)?;
    let parsed: IpcConfig = serde_json::from_str(&json)?;

    assert_eq!(parsed.server_name, original.server_name);
    assert_eq!(parsed.health_buffer_size, original.health_buffer_size);
    assert_eq!(
        parsed.transport.max_connections,
        original.transport.max_connections
    );
    Ok(())
}

#[test]
fn transport_config_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportConfig::default();
    let json = serde_json::to_string(&original)?;
    let parsed: TransportConfig = serde_json::from_str(&json)?;

    assert_eq!(parsed.max_connections, original.max_connections);
    assert_eq!(parsed.enable_acl, original.enable_acl);
    assert_eq!(parsed.recv_buffer_size, original.recv_buffer_size);
    assert_eq!(parsed.send_buffer_size, original.send_buffer_size);
    Ok(())
}

#[test]
fn transport_type_tcp_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportType::tcp_with_address("10.0.0.1", 8888);
    let json = serde_json::to_string(&original)?;
    let parsed: TransportType = serde_json::from_str(&json)?;

    match parsed {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "10.0.0.1");
            assert_eq!(port, 8888);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("expected TCP transport".into()),
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn transport_type_named_pipe_serde_roundtrip() -> Result<(), BoxErr> {
    let original = TransportType::named_pipe(r"\\.\pipe\msg-test");
    let json = serde_json::to_string(&original)?;
    let parsed: TransportType = serde_json::from_str(&json)?;

    match parsed {
        TransportType::NamedPipe { pipe_name } => {
            assert_eq!(pipe_name, r"\\.\pipe\msg-test");
        }
        _ => return Err("expected NamedPipe transport".into()),
    }
    Ok(())
}

#[test]
fn ipc_config_builder_roundtrip() -> Result<(), BoxErr> {
    let config = IpcConfig::with_transport(TransportType::tcp())
        .max_connections(25)
        .health_buffer_size(512);

    let json = serde_json::to_string(&config)?;
    let parsed: IpcConfig = serde_json::from_str(&json)?;

    assert_eq!(parsed.transport.max_connections, 25);
    assert_eq!(parsed.health_buffer_size, 512);
    Ok(())
}

// =========================================================================
// 3. Error message propagation
// =========================================================================

#[test]
fn error_display_contains_context() -> Result<(), BoxErr> {
    let cases: Vec<(IpcError, &str)> = vec![
        (IpcError::TransportInit("bind failed".into()), "bind failed"),
        (
            IpcError::ConnectionFailed("refused".into()),
            "refused",
        ),
        (
            IpcError::EncodingFailed("too large".into()),
            "too large",
        ),
        (
            IpcError::DecodingFailed("corrupt".into()),
            "corrupt",
        ),
        (
            IpcError::VersionIncompatibility {
                client: "0.5.0".into(),
                server: "1.0.0".into(),
            },
            "0.5.0",
        ),
        (
            IpcError::FeatureNegotiation("unsupported".into()),
            "unsupported",
        ),
        (IpcError::ServerNotRunning, "not running"),
        (
            IpcError::ConnectionLimitExceeded { max: 42 },
            "42",
        ),
        (IpcError::Timeout { timeout_ms: 3000 }, "3000"),
        (IpcError::Grpc("status 14".into()), "status 14"),
        (
            IpcError::InvalidConfig("bad value".into()),
            "bad value",
        ),
        (
            IpcError::PlatformNotSupported("plan9".into()),
            "plan9",
        ),
        (IpcError::ShutdownRequested, "shutdown"),
    ];

    for (err, expected_substr) in &cases {
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains(&expected_substr.to_lowercase()),
            "error '{msg}' should contain '{expected_substr}'"
        );
    }
    Ok(())
}

#[test]
fn error_recoverable_classification() -> Result<(), BoxErr> {
    let recoverable = [
        IpcError::ConnectionFailed("x".into()),
        IpcError::Timeout { timeout_ms: 1 },
        IpcError::VersionIncompatibility {
            client: "a".into(),
            server: "b".into(),
        },
        IpcError::FeatureNegotiation("x".into()),
    ];
    for err in &recoverable {
        assert!(err.is_recoverable(), "{err:?} should be recoverable");
    }

    let non_recoverable = [
        IpcError::TransportInit("x".into()),
        IpcError::ServerNotRunning,
        IpcError::ShutdownRequested,
        IpcError::EncodingFailed("x".into()),
        IpcError::DecodingFailed("x".into()),
    ];
    for err in &non_recoverable {
        assert!(!err.is_recoverable(), "{err:?} should not be recoverable");
    }
    Ok(())
}

#[test]
fn error_fatal_classification() -> Result<(), BoxErr> {
    let fatal = [
        IpcError::TransportInit("x".into()),
        IpcError::ServerNotRunning,
        IpcError::ShutdownRequested,
    ];
    for err in &fatal {
        assert!(err.is_fatal(), "{err:?} should be fatal");
    }

    let non_fatal = [
        IpcError::ConnectionFailed("x".into()),
        IpcError::Timeout { timeout_ms: 1 },
        IpcError::EncodingFailed("x".into()),
    ];
    for err in &non_fatal {
        assert!(!err.is_fatal(), "{err:?} should not be fatal");
    }
    Ok(())
}

#[test]
fn error_io_conversion() -> Result<(), BoxErr> {
    let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broke");
    let ipc_err: IpcError = io_err.into();
    let msg = format!("{ipc_err}");
    assert!(msg.contains("pipe broke"));
    Ok(())
}

// =========================================================================
// 4. Connection multiplexing
// =========================================================================

#[tokio::test]
async fn server_multiple_health_subscribers() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let mut rx1 = server.subscribe_health();
    let mut rx2 = server.subscribe_health();
    let mut rx3 = server.subscribe_health();

    let event = HealthEvent {
        timestamp: std::time::SystemTime::now(),
        device_id: "mux-dev".to_string(),
        event_type: HealthEventType::Connected,
        message: "connected".to_string(),
        metadata: HashMap::new(),
    };
    server.broadcast_health_event(event);

    let e1 = rx1.try_recv();
    let e2 = rx2.try_recv();
    let e3 = rx3.try_recv();

    assert!(e1.is_ok(), "subscriber 1 should receive");
    assert!(e2.is_ok(), "subscriber 2 should receive");
    assert!(e3.is_ok(), "subscriber 3 should receive");

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_client_registration_and_counting() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    assert_eq!(server.client_count().await, 0);

    // Register via feature negotiation
    let result = server
        .negotiate_features("1.0.0", &["device_management".into()])
        .await?;
    assert!(result.compatible);
    assert_eq!(server.client_count().await, 1);

    // Second client
    let result2 = server
        .negotiate_features("1.0.0", &["profile_management".into()])
        .await?;
    assert!(result2.compatible);
    assert_eq!(server.client_count().await, 2);

    server.stop().await?;
    // After stop, clients should be cleared
    assert_eq!(server.client_count().await, 0);
    Ok(())
}

#[tokio::test]
async fn server_feature_negotiation_intersection() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    // Request features that exist and some that don't
    let result = server
        .negotiate_features(
            "1.0.0",
            &[
                "device_management".into(),
                "nonexistent_feature".into(),
                "health_monitoring".into(),
            ],
        )
        .await?;

    assert!(result.compatible);
    assert!(result.enabled_features.contains(&"device_management".to_string()));
    assert!(result.enabled_features.contains(&"health_monitoring".to_string()));
    assert!(!result.enabled_features.contains(&"nonexistent_feature".to_string()));

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_incompatible_version_rejected() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server
        .negotiate_features("0.1.0", &["device_management".into()])
        .await?;

    assert!(!result.compatible);
    // Incompatible clients should not be registered
    assert_eq!(server.client_count().await, 0);

    server.stop().await?;
    Ok(())
}

// =========================================================================
// 5. Edge cases: oversized messages, malformed data, boundary conditions
// =========================================================================

#[test]
fn codec_rejects_zero_size() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    assert!(!codec.is_valid_size(0));
    Ok(())
}

#[test]
fn codec_rejects_oversized_message() -> Result<(), BoxErr> {
    let codec = MessageCodec::with_max_size(1024);
    assert!(!codec.is_valid_size(1025));
    assert!(codec.is_valid_size(1024));
    assert!(codec.is_valid_size(1));
    Ok(())
}

#[test]
fn codec_decode_malformed_bytes() -> Result<(), BoxErr> {
    let codec = MessageCodec::new();
    // Feed garbage bytes — prost should fail to decode a valid message
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
    // This should not panic — it should return an Ok or Err (prost is lenient)
    let result: Result<prost_types::Timestamp, _> = codec.decode(&garbage);
    // We just verify it doesn't panic; the decode may succeed with default fields
    let _ = result;
    Ok(())
}

#[test]
fn header_decode_empty_bytes() -> Result<(), BoxErr> {
    let result = MessageHeader::decode(&[]);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn header_decode_partial_bytes() -> Result<(), BoxErr> {
    for len in 1..MessageHeader::SIZE {
        let bytes = vec![0u8; len];
        let result = MessageHeader::decode(&bytes);
        assert!(result.is_err(), "should fail with {len} bytes");
    }
    Ok(())
}

#[test]
fn header_decode_exact_size_boundary() -> Result<(), BoxErr> {
    let bytes = vec![0u8; MessageHeader::SIZE];
    let header = MessageHeader::decode(&bytes)?;
    assert_eq!(header.message_type, 0);
    assert_eq!(header.payload_len, 0);
    assert_eq!(header.sequence, 0);
    assert_eq!(header.flags, 0);
    Ok(())
}

#[test]
fn header_decode_extra_trailing_bytes() -> Result<(), BoxErr> {
    let mut bytes = vec![0u8; MessageHeader::SIZE + 100];
    // Set message_type to DEVICE in little-endian
    bytes[0] = message_types::DEVICE as u8;
    bytes[1] = (message_types::DEVICE >> 8) as u8;

    let header = MessageHeader::decode(&bytes)?;
    assert_eq!(header.message_type, message_types::DEVICE);
    Ok(())
}

#[test]
fn header_max_values() -> Result<(), BoxErr> {
    let header = MessageHeader::new(u16::MAX, u32::MAX, u32::MAX);
    let encoded = header.encode();
    let decoded = MessageHeader::decode(&encoded)?;

    assert_eq!(decoded.message_type, u16::MAX);
    assert_eq!(decoded.payload_len, u32::MAX);
    assert_eq!(decoded.sequence, u32::MAX);
    Ok(())
}

#[tokio::test]
async fn server_double_start_fails() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    server.start().await?;

    let result = server.start().await;
    assert!(result.is_err(), "double start should fail");

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn server_stop_when_already_stopped() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
    // Stop without starting — should be a no-op, not an error
    server.stop().await?;
    assert_eq!(server.state().await, ServerState::Stopped);
    Ok(())
}

#[tokio::test]
async fn server_start_after_stop_restart() -> Result<(), BoxErr> {
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

#[test]
fn version_compat_edge_cases() -> Result<(), BoxErr> {
    // Empty strings
    assert!(!is_version_compatible("", "1.0.0"));
    assert!(!is_version_compatible("1.0.0", ""));

    // Single segment
    assert!(!is_version_compatible("1", "1.0.0"));

    // Two segments
    assert!(!is_version_compatible("1.0", "1.0.0"));

    // Extra segments (should still work — only first 3 are used)
    assert!(is_version_compatible("1.0.0.0", "1.0.0"));

    // Non-numeric
    assert!(!is_version_compatible("a.b.c", "1.0.0"));
    Ok(())
}

#[test]
fn transport_builder_produces_valid_config() -> Result<(), BoxErr> {
    let config = TransportBuilder::new()
        .transport(TransportType::tcp_with_address("0.0.0.0", 9999))
        .max_connections(200)
        .connection_timeout(Duration::from_secs(60))
        .enable_acl(true)
        .build();

    assert_eq!(config.max_connections, 200);
    assert!(config.enable_acl);
    assert_eq!(config.connection_timeout, Duration::from_secs(60));

    match config.transport {
        TransportType::Tcp { address, port } => {
            assert_eq!(address, "0.0.0.0");
            assert_eq!(port, 9999);
        }
        #[allow(unreachable_patterns)]
        _ => return Err("expected TCP transport".into()),
    }
    Ok(())
}

#[tokio::test]
async fn health_event_all_types_broadcast() -> Result<(), BoxErr> {
    let server = IpcServer::new(IpcConfig::default());
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

    for et in &event_types {
        server.broadcast_health_event(HealthEvent {
            timestamp: std::time::SystemTime::now(),
            device_id: "ev-dev".to_string(),
            event_type: *et,
            message: format!("{et:?}"),
            metadata: HashMap::new(),
        });
    }

    // Verify all 9 events received
    for _ in 0..event_types.len() {
        let e = rx.try_recv();
        assert!(e.is_ok(), "should receive all health event types");
    }
    Ok(())
}
